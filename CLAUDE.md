# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

klecks reconstructs a room from phone video (no lidar): 3D Gaussian Splatting scene, a floor
plan derived from it, and a "what fits here" tool. Offline, Windows + Linux, one compiled program.

## Hard rules (owner mandates)

- **ML never runs on the CPU.** NVIDIA is the tested target; AMD stays untested until there is
  hardware. Any new ML code must go through the GPU guard in `core/gpu` and must not rely on a
  silent CPU fallback (ONNX Runtime and torch both fall back by default — disable it explicitly).
- **Shipped runtime is Rust.** No Python/conda in the installer. Python lives in `lab/` for
  experiments and ONNX export only. **Exception, dev only:** `core/reconstruct`'s
  `PythonLabReconstructor` runs `lab/room_scan.py` (then Brush) as a stopgap behind the `Reconstructor` trait; it
  is replaced by an ONNX model run from Rust and must not be shipped.
- **Offline by default.** Gemini is opt-in, never on the critical path, key from the environment
  only (never in YAML/git), and every call goes through the cache + budget in `config/gemini.yaml`.
- **All tunables are YAML in `config/`**, loaded by `core/settings`. No magic numbers in code.
- **OOP style:** structs with `impl`, dependency injection through constructors. `#[tauri::command]`
  and Leptos `#[component]` are the only free functions, kept as thin shims.
- **Every Rust file starts with the owner header** (Owner / Contributors / Last modified).
- Frameless window with our own min/max/close buttons. Design: gutla's marble black and glass,
  sage-green palette (`#A5B497` accent, `#7B8560` moss, `#85815B` olive), a field of drifting soft
  "splats" as the one memorable element. No grid/blueprint look (owner dislikes it), no ALL-CAPS
  eyebrows, no monospace data chips. Single work tree only (see global CLAUDE.md).
- **Work only in this checkout.** No git worktrees, no side branches.

## Layout

See README. Core crates in `core/`, Tauri shell in `app/`, Leptos frontend in `ui/` (own
workspace, wasm), lab in `lab/`.

## Commands

```
cargo test --workspace
cargo run -p klecks-gpu --example detect
cd app && cargo tauri dev
cd app && cargo tauri build --debug --no-bundle      # quick exe without installer
```

Set `CARGO_TARGET_DIR` to a drive with space (`D:\Projects\klecks-target`); unset `NO_COLOR` for Trunk.
Config lookup order: `KLECKS_CONFIG_DIR`, `config/` next to the exe, `config/` in a parent folder.
When running the built exe from a target dir, set `KLECKS_CONFIG_DIR`.

## Gotchas

- The CSP in `app/tauri.conf.json` must allow `connect-src 'self'` or the wasm fails to load and
  the window stays blank.
- Tauri on Linux uses WebKitGTK; WebGPU is unreliable there, so the 3D viewer targets WebGL2.
- DirectML is in maintenance and ROCm is deprecated in ONNX Runtime; prefer WinML / MIGraphX
  for AMD when that work starts.
- Brush v0.3.0 was verified on the RTX 4080 (about 10 minutes for 30,000 steps on 72 images); it is
  still a young project, so keep it behind the `PythonLabReconstructor` stage and pin the version.
- Machine-specific paths (lab python, HF cache, scans dir) live in `%APPDATA%\klecks\*.yaml`
  overrides, never in `config/`. The reconstruct backend refuses to start when they are missing.
- Dev builds find `config/` as a copy in `target/debug/config` (Tauri copies resources), so
  `lab/` is looked up in parent folders; a stale copy can hide new config keys until the next build.
- Dev shortcuts: `KLECKS_OPEN_SCENE=<scan dir>`, `KLECKS_AUTOSCAN=<video>`. The owner usually has
  `cargo tauri dev` running: never kill `klecks-app`/`trunk` by name, stop only PIDs you started.
- Brush (Gaussian splat trainer, Apache-2.0) is a separate exe that writes nothing to the console while
  training: progress comes from its periodic exports. It exits silently when given a `.ply` to view.
- Spark (splat renderer) starts Web Workers from `blob:` URLs: the CSP needs `worker-src 'self' blob:`.
  Three.js and Spark are vendored in `ui/public/vendor` with their bare module names rewritten to
  local files (no import map is allowed by the CSP).
- A locked PC makes screenshots of the screen show the lock screen. Check the UI over the WebView2
  DevTools port instead: start the exe with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333`.
- Keep animated CSS cheap: blurred layers and `backdrop-filter` over animated content cost GPU time
  in every frame, which competes with the ML workload.
- Measured: upscaling training images does not improve splats on pan-only footage (lab/README.md).
- The viewer reads scans through Tauri's asset protocol (`protocol-asset` feature, scope set at
  runtime to the scans dir); the CSP needs `asset:` and `http://asset.localhost` in `connect-src`.
- Phone videos carry a rotation tag (`rotation=-90`) that OpenCV ignores: always rotate frames
  explicitly when extracting (see `lab/room_scan.py`), or models get sideways images. VGGT's own loader
  also centre-crops portrait frames to a square; `room_scan.py` resizes the whole frame instead.
- VGGT weights are CC-BY-NC-4.0 (non-commercial). Keep that in mind before distributing klecks.
- Lab env lives at `D:\Projects\klecks-lab` (conda `-p`), run outputs in `D:\Projects\klecks-runs`;
  test videos in `media/` are git-ignored.
