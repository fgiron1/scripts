use std::env;
use std::path::PathBuf;

fn main() {
    // Only apply the manifest for the main executable (not for build scripts or dependencies)
    if env::var("CARGO_BIN_NAME").is_ok() {
        let manifest_path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("Ps4MidiMapper.exe.manifest");  // Match your manifest filename

        println!("cargo:rerun-if-changed={}", manifest_path.display());

        let mut res = winres::WindowsResource::new();
        res.set_manifest_file(&manifest_path.to_string_lossy());
        res.compile().unwrap();
    }
}