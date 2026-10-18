fn main() {
    // Which commit this program was built from, shown in the title bar so it is clear which version runs.
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    let commit = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|changes| !changes.is_empty());
    if !commit.is_empty() {
        println!("cargo:rustc-env=KLECKS_BUILD={}{}", commit, if dirty { "+" } else { "" });
    }
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/index");
    tauri_build::build()
}
