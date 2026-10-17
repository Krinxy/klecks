// ---
// Owner: Krinxy <dominic.bechtold@icloud.com>
// Contributors: Krinxy <dominic.bechtold@icloud.com>
// Last modified: 2026-10-17
// ---
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use klecks_settings::ReconstructSettings;

use crate::{
    CancelToken, ProgressEvent, ReconstructError, Reconstructor, ScanRequest, ScanResult,
};

const PROGRESS_PREFIX: &str = "@@";
const POLL: Duration = Duration::from_millis(100);
const LOG_TAIL_LINES: usize = 6;
const MAX_PARENT_LEVELS: usize = 6;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `lab/vggt_try.py` in the conda lab as a child process.
///
/// Dev-only bridge: it needs a Python environment next to the program, so it can't be shipped.
/// It exists so the whole pipeline can be used today; an ONNX backend replaces it later.
pub struct PythonLabReconstructor {
    settings: ReconstructSettings,
    python: PathBuf,
    script: PathBuf,
}

impl PythonLabReconstructor {
    /// `config_dir` is the shipped `config/` folder; the default script lives in `../lab/`.
    pub fn new(settings: &ReconstructSettings, config_dir: &Path) -> Result<Self, ReconstructError> {
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
        let python = PathBuf::from(settings.python.trim());
        if !python.is_file() {
            return Err(ReconstructError::NotConfigured(format!(
                "python not found at {}",
                python.display()
            )));
        }
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
            python,
            script,
        })
    }

    /// `lab/vggt_try.py` next to `config/` or in a parent folder. Dev builds find `config/` as a
    /// copy inside `target/debug`, so the repo's `lab/` can be a few levels further up.
    fn find_default_script(config_dir: &Path) -> PathBuf {
        let candidates = || {
            config_dir
                .ancestors()
                .skip(1)
                .take(MAX_PARENT_LEVELS)
                .map(|dir| dir.join("lab").join("vggt_try.py"))
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

    /// Last lines of the process log, to tell the user why it failed.
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

    fn command(&self, request: &ScanRequest, log: File) -> Command {
        let mut command = Command::new(&self.python);
        command
            .arg("-u")
            .arg(&self.script)
            .arg(&request.video)
            .arg("--out")
            .arg(&request.scan_dir)
            .args(["--frames", &self.settings.frames.to_string()])
            .args(["--conf-percentile", &self.settings.conf_percentile.to_string()])
            .args(["--view-points", &self.settings.view_points.to_string()])
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        if !self.settings.hf_home.trim().is_empty() {
            command.env("HF_HOME", self.settings.hf_home.trim());
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }
}

impl Reconstructor for PythonLabReconstructor {
    fn run(
        &self,
        request: &ScanRequest,
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(&ProgressEvent),
    ) -> Result<ScanResult, ReconstructError> {
        std::fs::create_dir_all(&request.scan_dir).map_err(ReconstructError::ScanDir)?;
        let log_path = request.scan_dir.join("log.txt");
        let log = File::create(&log_path).map_err(ReconstructError::ScanDir)?;

        let mut child = self
            .command(request, log)
            .spawn()
            .map_err(ReconstructError::Spawn)?;
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

        loop {
            if cancel.is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                // The folder was created for this run alone and holds nothing worth keeping.
                let _ = std::fs::remove_dir_all(&request.scan_dir);
                return Err(ReconstructError::Cancelled);
            }
            match receiver.recv_timeout(POLL) {
                Ok(event) => on_progress(&event),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        let status = child.wait().map_err(ReconstructError::Spawn)?;
        let _ = reader.join();

        if !status.success() {
            return Err(ReconstructError::Failed {
                code: status.code().unwrap_or(-1),
                detail: Self::log_tail(&log_path),
            });
        }
        ScanResult::from_dir(&request.scan_dir)
            .ok_or_else(|| ReconstructError::MissingOutput("scene.ply".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> ReconstructSettings {
        ReconstructSettings {
            backend: "python_lab".into(),
            python: String::new(),
            script: String::new(),
            hf_home: String::new(),
            frames: 32,
            conf_percentile: 40.0,
            view_points: 1_000_000,
        }
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
        let error = PythonLabReconstructor::new(&settings(), Path::new(".")).err().unwrap();
        assert!(matches!(error, ReconstructError::NotConfigured(_)));
    }

    #[test]
    fn finds_the_lab_script_above_a_config_copy_in_target() {
        let root = std::env::temp_dir().join("klecks-find-script-test");
        let _ = std::fs::remove_dir_all(&root);
        let config_copy = root.join("target").join("debug").join("config");
        std::fs::create_dir_all(&config_copy).unwrap();
        std::fs::create_dir_all(root.join("lab")).unwrap();
        std::fs::write(root.join("lab").join("vggt_try.py"), "").unwrap();
        let found = PythonLabReconstructor::find_default_script(&config_copy);
        assert_eq!(found, root.join("lab").join("vggt_try.py"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_backend_is_not_configured() {
        let mut onnx = settings();
        onnx.backend = "onnx".into();
        assert!(PythonLabReconstructor::new(&onnx, Path::new(".")).is_err());
    }
}
