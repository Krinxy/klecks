// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use klecks_settings::{PipelineSettings, PresetSettings, ReconstructSettings};

use crate::{
    CancelToken, ProgressEvent, ReconstructError, Reconstructor, ScanRequest, ScanResult,
};

const PROGRESS_PREFIX: &str = "@@";
const SCRIPT_NAME: &str = "room_scan.py";
const POLL: Duration = Duration::from_millis(100);
const LOG_TAIL_LINES: usize = 6;
const MAX_PARENT_LEVELS: usize = 6;
/// Share of the progress bar taken by the Python stage when a splat is trained afterwards
/// (a few minutes, more for a long walk-through, against about ten for the training).
const PYTHON_SHARE: f32 = 0.35;
/// Brush reports nothing while training; until its first export we assume this speed.
const FALLBACK_STEPS_PER_SECOND: f32 = 45.0;
const EXPORT_PREFIX: &str = "splat_";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Turns videos of a room into a 3D scene, in up to two stages:
///
/// 1. `lab/room_scan.py` in the conda lab (VGGT cameras and depth → point cloud, plus a COLMAP
///    dataset of the frames), and
/// 2. [Brush](https://github.com/ArthurBrussee/brush) trains a Gaussian splat on that dataset,
///    if a `brush` executable is configured.
///
/// Dev-only bridge: stage 1 needs a Python environment next to the program, so it can't be shipped.
/// An ONNX model run from Rust replaces it; the `Reconstructor` trait keeps the app unchanged.
pub struct PythonLabReconstructor {
    settings: ReconstructSettings,
    /// Training length and splat limit of the chosen quality preset.
    preset: PresetSettings,
    python: PathBuf,
    script: PathBuf,
    upscale_model: Option<PathBuf>,
    brush: Option<PathBuf>,
}

impl PythonLabReconstructor {
    /// `config_dir` is the shipped `config/` folder; the default script lives in `lab/` above it.
    pub fn new(pipeline: &PipelineSettings, config_dir: &Path) -> Result<Self, ReconstructError> {
        let settings = &pipeline.reconstruct;
        let preset = pipeline.active_preset().cloned().ok_or_else(|| {
            ReconstructError::NotConfigured(format!("quality preset '{}' does not exist", pipeline.preset))
        })?;
        if settings.backend != "python_lab" {
            return Err(ReconstructError::NotConfigured(format!(
                "backend '{}' is not available yet (use python_lab)",
                settings.backend
            )));
        }
        if settings.python.trim().is_empty() {
            return Err(ReconstructError::NotConfigured(
                "set pipeline.reconstruct.python (the lab's python.exe) in %APPDATA%\\klecks\\pipeline.yaml".into(),
            ));
        }
        let python = Self::existing_file(&settings.python, "python")?;
        let script = if settings.script.trim().is_empty() {
            Self::find_default_script(config_dir)
        } else {
            PathBuf::from(settings.script.trim())
        };
        if !script.is_file() {
            return Err(ReconstructError::NotConfigured(format!(
                "reconstruction script not found at {}",
                script.display()
            )));
        }
        Ok(Self {
            settings: settings.clone(),
            preset,
            python,
            script,
            upscale_model: Self::optional_file(&settings.upscale_model, "upscale model")?,
            brush: Self::optional_file(&settings.brush, "brush")?,
        })
    }

    fn existing_file(configured: &str, what: &str) -> Result<PathBuf, ReconstructError> {
        let path = PathBuf::from(configured.trim());
        if path.is_file() {
            Ok(path)
        } else {
            Err(ReconstructError::NotConfigured(format!(
                "{what} not found at {}",
                path.display()
            )))
        }
    }

    /// Empty means "not used"; a path that doesn't exist is a mistake worth telling about.
    fn optional_file(configured: &str, what: &str) -> Result<Option<PathBuf>, ReconstructError> {
        if configured.trim().is_empty() {
            Ok(None)
        } else {
            Self::existing_file(configured, what).map(Some)
        }
    }

    /// `lab/room_scan.py` next to `config/` or in a parent folder. Dev builds find `config/` as a
    /// copy inside `target/debug`, so the repo's `lab/` can be a few levels further up.
    fn find_default_script(config_dir: &Path) -> PathBuf {
        let candidates = || {
            config_dir
                .ancestors()
                .skip(1)
                .take(MAX_PARENT_LEVELS)
                .map(|dir| dir.join("lab").join(SCRIPT_NAME))
        };
        candidates()
            .find(|script| script.is_file())
            .or_else(|| candidates().next())
            .unwrap_or_default()
    }

    /// Parses a `@@{"stage":..,"pct":..,"message":..}` line; anything else is ignored.
    fn parse_progress(line: &str) -> Option<ProgressEvent> {
        let json = line.trim().strip_prefix(PROGRESS_PREFIX)?;
        serde_json::from_str(json).ok()
    }

    /// Last lines of a process log, to tell the user why it failed.
    fn log_tail(log: &Path) -> String {
        let text = std::fs::read_to_string(log).unwrap_or_default();
        let lines: Vec<&str> = text.lines().filter(|line| !line.trim().is_empty()).collect();
        let tail = &lines[lines.len().saturating_sub(LOG_TAIL_LINES)..];
        if tail.is_empty() {
            String::new()
        } else {
            format!("\n{}", tail.join("\n"))
        }
    }

    /// No console window, no stdin: these are background workers.
    fn worker(program: &Path) -> Command {
        let mut command = Command::new(program);
        command.stdin(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }

    /// Waits for a child while calling `tick` and honouring cancellation.
    fn supervise(
        child: &mut Child,
        cancel: &CancelToken,
        tick: &mut dyn FnMut(),
    ) -> Result<ExitStatus, ReconstructError> {
        loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ReconstructError::Cancelled);
            }
            tick();
            if let Some(status) = child.try_wait().map_err(ReconstructError::Spawn)? {
                return Ok(status);
            }
            std::thread::sleep(POLL);
        }
    }

    // ── stage 1: cameras, depth, point cloud, dataset ────────────────────────────────────────

    fn run_python(
        &self,
        request: &ScanRequest,
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<(), ReconstructError> {
        let log_path = request.scan_dir.join("log.txt");
        let log = File::create(&log_path).map_err(ReconstructError::ScanDir)?;
        let settings = &self.settings;

        let mut command = Self::worker(&self.python);
        command
            .arg("-u")
            .arg(&self.script)
            .args(&request.videos)
            .arg("--out")
            .arg(&request.scan_dir)
            .args(["--window", &settings.window.to_string()])
            .args(["--fps", &settings.frame_rate.to_string()])
            .args(["--max-frames", &settings.max_frames.to_string()])
            .args(["--conf-percentile", &settings.conf_percentile.to_string()])
            .args(["--view-points", &settings.view_points.to_string()])
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        if !settings.hf_home.trim().is_empty() {
            command.env("HF_HOME", settings.hf_home.trim());
        }
        if self.brush.is_some() {
            command.arg("--colmap");
            if let Some(model) = &self.upscale_model {
                command.arg("--train-upscale-model").arg(model);
            }
        }

        let mut child = command.spawn().map_err(ReconstructError::Spawn)?;
        let stdout = child.stdout.take().expect("stdout is piped");
        // stdout is read on its own thread so a quiet phase (model download) never blocks cancelling.
        let (sender, receiver) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(event) = Self::parse_progress(&line) {
                    if sender.send(event).is_err() {
                        break;
                    }
                }
            }
        });

        // With a splat to train afterwards this stage only fills the first part of the bar.
        let share = if self.brush.is_some() { PYTHON_SHARE } else { 1.0 };
        let forward = |receiver: &Receiver<ProgressEvent>, on_progress: &mut dyn FnMut(&ProgressEvent)| {
            while let Ok(mut event) = receiver.try_recv() {
                if event.stage == "done" && share < 1.0 {
                    continue; // the app announces "done" itself after the training
                }
                event.pct = (f32::from(event.pct) * share).round() as u8;
                on_progress(&event);
            }
        };

        let status = Self::supervise(&mut child, cancel, &mut || forward(&receiver, on_progress))?;
        let _ = reader.join();
        forward(&receiver, on_progress);
        if status.success() {
            Ok(())
        } else {
            Err(ReconstructError::Failed {
                code: status.code().unwrap_or(-1),
                detail: Self::log_tail(&log_path),
            })
        }
    }

    // ── stage 2: Gaussian splat training ─────────────────────────────────────────────────────

    /// Highest step among the `splat_<step>.ply` files Brush has exported so far.
    fn latest_export(dir: &Path) -> Option<u32> {
        std::fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                name.strip_prefix(EXPORT_PREFIX)?
                    .strip_suffix(".ply")?
                    .parse::<u32>()
                    .ok()
            })
            .max()
    }

    fn run_brush(
        &self,
        brush: &Path,
        request: &ScanRequest,
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<(), ReconstructError> {
        let dataset = request.scan_dir.join("colmap");
        if !dataset.join("sparse").is_dir() {
            return Err(ReconstructError::MissingOutput("the training dataset (colmap)".into()));
        }
        let work_dir = request.scan_dir.join("splat_work");
        std::fs::create_dir_all(&work_dir).map_err(ReconstructError::ScanDir)?;
        let log_path = request.scan_dir.join("brush.log");
        let log = File::create(&log_path).map_err(ReconstructError::ScanDir)?;
        let log_err = log.try_clone().map_err(ReconstructError::ScanDir)?;

        let steps = self.preset.train_iterations.max(1000);
        // Brush writes nothing while it trains, so progress is read from its periodic exports.
        let export_every = if steps.is_multiple_of(5) { steps / 5 } else { steps };
        let mut command = Self::worker(brush);
        command
            .arg(&dataset)
            .args(["--total-steps", &steps.to_string()])
            .arg("--export-path")
            .arg(&work_dir)
            .args(["--export-name", &format!("{EXPORT_PREFIX}{{iter}}.ply")])
            .args(["--export-every", &export_every.to_string()])
            .args(["--max-splats", &self.preset.max_splats.to_string()])
            .args(["--eval-every", "100000000"])
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err));
        let mut child = command.spawn().map_err(ReconstructError::Spawn)?;

        let mut base = (0u32, Instant::now()); // last known step and when we learned it
        let mut speed = FALLBACK_STEPS_PER_SECOND;
        let mut shown = 0.0f32; // the bar never goes backwards, even when a new export corrects the pace
        let mut last_pct = None;
        let mut tick = || {
            if let Some(step) = Self::latest_export(&work_dir) {
                if step > base.0 {
                    // pace between two exports; the first one also includes loading the dataset
                    speed = (step - base.0) as f32 / base.1.elapsed().as_secs_f32().max(1.0);
                    base = (step, Instant::now());
                }
            }
            let estimate = (base.0 as f32 + base.1.elapsed().as_secs_f32() * speed).min(steps as f32 - 1.0);
            let estimate = estimate.max(shown);
            shown = estimate;
            let start = PYTHON_SHARE * 100.0 + 1.0;
            let pct = (start + (99.0 - start) * estimate / steps as f32).round() as u8;
            if last_pct != Some(pct) {
                last_pct = Some(pct);
                on_progress(&ProgressEvent {
                    stage: "train".into(),
                    pct,
                    message: format!("Training the splats (step {} of {steps})", estimate as u32),
                });
            }
        };
        let status = Self::supervise(&mut child, cancel, &mut tick)?;

        // Prefer the export of the very last step; fall back to whatever Brush managed to write.
        let final_step = Self::latest_export(&work_dir);
        let Some(step) = final_step else {
            return Err(ReconstructError::Failed {
                code: status.code().unwrap_or(-1),
                detail: Self::log_tail(&log_path),
            });
        };
        std::fs::rename(
            work_dir.join(format!("{EXPORT_PREFIX}{step}.ply")),
            request.scan_dir.join("splat.ply"),
        )
        .map_err(ReconstructError::ScanDir)?;
        let _ = std::fs::remove_dir_all(&work_dir);
        Ok(())
    }
}

impl Reconstructor for PythonLabReconstructor {
    fn run(
        &self,
        request: &ScanRequest,
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<ScanResult, ReconstructError> {
        if request.videos.is_empty() {
            return Err(ReconstructError::NotConfigured("no video was chosen".into()));
        }
        std::fs::create_dir_all(&request.scan_dir).map_err(ReconstructError::ScanDir)?;

        let outcome = self.run_python(request, cancel, on_progress).and_then(|()| match &self.brush {
            Some(brush) => {
                let result = self.run_brush(brush, request, cancel, on_progress);
                on_progress(&ProgressEvent { stage: "done".into(), pct: 100, message: "Done".into() });
                result
            }
            None => Ok(()),
        });
        match outcome {
            Ok(()) => ScanResult::from_dir(&request.scan_dir)
                .ok_or_else(|| ReconstructError::MissingOutput("the scene".into())),
            Err(ReconstructError::Cancelled) => {
                // The folder was created for this run alone and holds nothing worth keeping.
                let _ = std::fs::remove_dir_all(&request.scan_dir);
                Err(ReconstructError::Cancelled)
            }
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline() -> PipelineSettings {
        let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        klecks_settings::SettingsLoader::new(config, None).load().unwrap().pipeline
    }

    #[test]
    fn parses_progress_lines_and_ignores_noise() {
        let line = r#"@@{"stage": "inference", "pct": 60, "message": "Estimating"}"#;
        let event = PythonLabReconstructor::parse_progress(line).unwrap();
        assert_eq!((event.stage.as_str(), event.pct), ("inference", 60));
        assert!(PythonLabReconstructor::parse_progress("Loading weights...").is_none());
        assert!(PythonLabReconstructor::parse_progress("@@not json").is_none());
    }

    #[test]
    fn refuses_to_start_without_a_configured_python() {
        let error = PythonLabReconstructor::new(&pipeline(), Path::new(".")).err().unwrap();
        assert!(matches!(error, ReconstructError::NotConfigured(_)));
    }

    #[test]
    fn finds_the_lab_script_above_a_config_copy_in_target() {
        let root = std::env::temp_dir().join("klecks-find-script-test");
        let _ = std::fs::remove_dir_all(&root);
        let config_copy = root.join("target").join("debug").join("config");
        std::fs::create_dir_all(&config_copy).unwrap();
        std::fs::create_dir_all(root.join("lab")).unwrap();
        std::fs::write(root.join("lab").join(SCRIPT_NAME), "").unwrap();
        let found = PythonLabReconstructor::find_default_script(&config_copy);
        assert_eq!(found, root.join("lab").join(SCRIPT_NAME));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_backend_is_not_configured() {
        let mut onnx = pipeline();
        onnx.reconstruct.backend = "onnx".into();
        assert!(PythonLabReconstructor::new(&onnx, Path::new(".")).is_err());
    }

    #[test]
    fn reads_the_latest_brush_export_step() {
        let dir = std::env::temp_dir().join("klecks-export-step-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["splat_6000.ply", "splat_12000.ply", "other.txt", "splat_x.ply"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        assert_eq!(PythonLabReconstructor::latest_export(&dir), Some(12000));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
