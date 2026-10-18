# Builds klecks and puts ONE file on the desktop: klecks.exe.
# The commit it was built from is shown in its title bar (a "+" behind it means uncommitted changes),
# so it is always clear which version is running.
#
#   .\scripts\build-desktop.ps1           release build (fast program, takes a few minutes)
#   .\scripts\build-desktop.ps1 -Debug    quick debug build
#
# The program finds config/, media/ and lab/ in this checkout (the path is built in), and your own settings
# in %APPDATA%\klecks. Set CARGO_TARGET_DIR to a drive with space first if you keep builds elsewhere.
param([switch]$Debug)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Remove-Item Env:NO_COLOR -ErrorAction SilentlyContinue   # Trunk stops on it

Push-Location (Join-Path $repo 'app')
try {
    if ($Debug) { cargo tauri build --debug --no-bundle } else { cargo tauri build --no-bundle }
    if ($LASTEXITCODE -ne 0) { throw 'The build failed, see the messages above.' }
} finally {
    Pop-Location
}

$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
$built = Join-Path $target ($(if ($Debug) { 'debug' } else { 'release' }) + '\klecks-app.exe')
$copy = Join-Path ([Environment]::GetFolderPath('Desktop')) 'klecks.exe'
try {
    Copy-Item $built $copy -Force
} catch {
    throw "Could not write $copy. Close klecks first if it is running, then run this again."
}

$commit = (git -C $repo rev-parse --short HEAD) + $(if (git -C $repo status --porcelain --untracked-files=no) { '+' } else { '' })
Write-Host "Done: $copy (commit $commit, $([math]::Round((Get-Item $copy).Length / 1MB)) MB)"
