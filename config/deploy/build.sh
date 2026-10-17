#!/usr/bin/env bash
# Builds the Linux bundles (AppImage + deb). Output: <target>/release/bundle/
# Needs: Rust, `cargo install tauri-cli trunk`, rustup target wasm32-unknown-unknown,
# and WebKitGTK dev packages (see https://v2.tauri.app/start/prerequisites/#linux).
set -euo pipefail
unset NO_COLOR # trunk rejects NO_COLOR=1
cd "$(dirname "$0")/../../app"
cargo tauri build --bundles appimage,deb
