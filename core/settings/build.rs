fn main() {
    // The folder of the checkout this was built from: a program built from a checkout still finds its
    // config/ (and media/, lab/) when it is copied somewhere else, for example to the desktop.
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let root = std::path::Path::new(&manifest).join("..").join("..");
    if let Ok(root) = std::fs::canonicalize(root) {
        let text = root.to_string_lossy().trim_start_matches(r"\\?\").to_string();
        println!("cargo:rustc-env=KLECKS_SOURCE_ROOT={text}");
    }
}
