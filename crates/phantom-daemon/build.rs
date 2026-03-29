use std::path::Path;

fn main() {
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let out_path = Path::new(&out_dir);

    // OUT_DIR is: target/{profile}/build/phantom-daemon-HASH/out
    // parent(out) = phantom-daemon-HASH, parent(that) = build/
    if let Some(build_dir) = out_path.parent().and_then(|p| p.parent()) {
        if let Ok(entries) = std::fs::read_dir(build_dir) {
            for entry in entries.flatten() {
                let name_str = entry.file_name().to_string_lossy().to_string();
                if name_str.starts_with("libghostty-vt-sys-") {
                    let lib_dir = entry.path().join("out/ghostty-install/lib");
                    if lib_dir.exists() {
                        println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_dir.display());
                        return;
                    }
                }
            }
        }
    }
}
