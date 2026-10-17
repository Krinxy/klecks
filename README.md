# klecks

Film your room with a phone, get a walkable 3D copy and a floor plan, then find out what fits
in that corner. Offline, GPU-only, one program for Windows and Linux.

> Status: **phase 1 scaffold** — window, config, GPU guard and the video drop zone work.
> The reconstruction pipeline is not built yet; see [Roadmap](#roadmap).

## How it works

```
phone video ──► frames ──► camera poses ──► 3D Gaussian Splatting ──► walkable 3D scene
                                                    │
                                                    ├──► floor plan (slice of the 3D scene)
                                                    └──► objects ──► "does it fit?"
```

- **Rust runtime, no Python in the shipped program.** Training runs through
  [Brush](https://github.com/ArthurBrussee/brush) (Burn + wgpu: Vulkan/DX12, any vendor),
  models run through ONNX Runtime. Python/conda is only the lab in `lab/`.
- **Never ML on the CPU.** At startup `klecks-gpu` looks for a real GPU from `gpu.allowed_vendors`;
  software adapters (WARP, llvmpipe) are rejected. No GPU, no pipeline, and the UI says why.
  Video decoding and file IO are CPU by nature and not covered by that rule.
- **NVIDIA is the tested target.** AMD is allowed by config but untested (no AMD machine).

## Layout

```
config/        YAML settings (klecks, capture, pipeline, gemini) + deploy scripts
core/settings  YAML loading, user overrides, platform paths
core/gpu       GPU detection and the never-on-CPU guard
app/           Tauri shell: window, commands, installer config
ui/            Leptos (wasm) frontend, built with Trunk
lab/           Python/conda lab for experiments and ONNX export (not shipped)
docs/          capture guide
```

## Where things live on a user's machine

| What | Windows | Linux |
|---|---|---|
| Program (NSIS, per-user) | `%LOCALAPPDATA%\Programs\klecks` | AppImage / `.deb` |
| User config overrides | `%APPDATA%\klecks\*.yaml` | `~/.config/klecks/*.yaml` |
| Models, scans, cache | `%LOCALAPPDATA%\klecks\{models,scans,cache}` | `~/.local/share/klecks/…` |

Defaults ship in `config/` next to the program. A file with the same name in the user config
dir overrides single keys. Move the big folders with `paths.*_dir` in `klecks.yaml`.

## Develop

Needs Rust, `cargo install tauri-cli trunk`, and the `wasm32-unknown-unknown` target.

```powershell
cargo test --workspace                 # settings + gpu
cargo run -p klecks-gpu --example detect
cd app; cargo tauri dev                # window with hot-reloading UI
```

Build outputs are large; point them off the system drive with `CARGO_TARGET_DIR`.
If `trunk` complains about `--no-color`, unset the `NO_COLOR` environment variable.

## Ship

```powershell
./config/deploy/build.ps1              # NSIS + MSI on Windows
./config/deploy/build.sh               # AppImage + deb on Linux
```

Models are several GB, so the result is an installer plus a models folder, not a single exe.
For "carry it around": copy the program folder and the `models` folder; it runs offline.

## Roadmap

| Phase | Content |
|---|---|
| 1 | Scaffold, config, GPU guard, frameless window, video drop zone **(this)** |
| 0 | Spikes that decide the engine: Brush as a library, pose model via ONNX on GPU, Spark viewer on Linux |
| 2 | Capture guide, frame extraction, quality check |
| 3 | Poses + Brush training + 3D fly-through viewer |
| 4 | Metric scale + 2D floor plan |
| 5 | Box furniture, drag and drop, "does it fit in this corner" |
| 6 | Real furniture from the scan (segmentation), optional Gemini labelling |
| 7 | Inpainting where moved furniture leaves a hole |
| 8 | Installers, AppImage, portable folder |

Gemini is optional and off by default; the key comes from an environment variable and requests
are capped by `config/gemini.yaml`.
