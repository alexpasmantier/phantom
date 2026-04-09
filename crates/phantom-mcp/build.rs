// Embed an rpath to the vendored libghostty-vt dylib so the phantom-mcp
// binary can locate it at runtime. Mirrors the logic in phantom-daemon's
// build.rs — both binaries link transitively against libghostty-vt.

use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // OUT_DIR layout: target/{profile}/build/phantom-mcp-HASH/out
    // We want to find target/{profile}/build/libghostty-vt-sys-HASH/out/ghostty-install/lib
    if let Some(build_dir) = out_path.parent().and_then(|p| p.parent())
        && let Ok(entries) = std::fs::read_dir(build_dir)
    {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("libghostty-vt-sys-") {
                let lib_dir = entry.path().join("out/ghostty-install/lib");
                if lib_dir.exists() {
                    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
                    return;
                }
            }
        }
    }
}
