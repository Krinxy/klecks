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
  experiments and ONNX export only.
- **Offline by default.** Gemini is opt-in, never on the critical path, key from the environment
  only (never in YAML/git), and every call goes through the cache + budget in `config/gemini.yaml`.
- **All tunables are YAML in `config/`**, loaded by `core/settings`. No magic numbers in code.
- **OOP style:** structs with `impl`, dependency injection through constructors. `#[tauri::command]`
  and Leptos `#[component]` are the only free functions, kept as thin shims.
- **Every Rust file starts with the owner header** (Owner / Contributors / Last modified).
- Frameless window with our own min/max/close buttons; design is its own thing (survey blueprint),
  not a copy of gutla.

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
- Brush is still a proof of concept; phase 0 verifies it before anything depends on it.
