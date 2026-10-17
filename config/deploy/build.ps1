# Builds the Windows installers (NSIS per-user + MSI). Output: <target>\release\bundle\
# Needs: Rust, `cargo install tauri-cli trunk`, rustup target wasm32-unknown-unknown.
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue   # trunk rejects NO_COLOR=1
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
Set-Location (Join-Path $root 'app')
cargo tauri build --bundles nsis,msi
# Check the exit code instead of $ErrorActionPreference: cargo writes progress to stderr.
if ($LASTEXITCODE -ne 0) { throw "cargo tauri build failed with exit code $LASTEXITCODE" }
