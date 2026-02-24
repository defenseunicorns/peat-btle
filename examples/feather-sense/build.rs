use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Select memory layout based on features
    let memory_file = if env::var("CARGO_FEATURE_SOFTDEVICE").is_ok() {
        "memory-softdevice.x"
    } else {
        "memory.x"  // Pure Rust (default)
    };

    // Copy the selected memory file to OUT_DIR as memory.x
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("memory.x");

    fs::copy(memory_file, &dest).expect("Failed to copy memory.x");

    // Tell linker where to find memory.x
    println!("cargo:rustc-link-search={}", out_dir.display());

    // Rerun if memory files change
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=memory-softdevice.x");
    println!("cargo:rerun-if-changed=build.rs");
}
