//! Build script — invokes the bootstrap askic compiler to generate
//! Rust from kernel.aski. The askic binary must be on PATH (provided
//! by Nix dev shell or the bootstrap derivation).

use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();
    let kernel_path = Path::new(&manifest_dir).join("source/kernel.aski");

    println!("cargo::rerun-if-changed=source/kernel.aski");

    // Bootstrap: if ASKI_BOOTSTRAP is set, use bootstrap/kernel.rs instead of askic
    let bootstrap_path = Path::new(&manifest_dir).join("bootstrap/kernel.rs");
    if std::env::var("ASKI_BOOTSTRAP").is_ok() && bootstrap_path.exists() {
        let dest = Path::new(&out_dir).join("kernel.rs");
        std::fs::copy(&bootstrap_path, &dest).expect("copy bootstrap/kernel.rs");
        return;
    }

    let output = Command::new("askic")
        .arg(&kernel_path)
        .output()
        .expect("askic not found — install the bootstrap compiler (nix develop)");

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!("askic failed:\n{}", stderr);
    }

    let generated = String::from_utf8(output.stdout).unwrap();
    let dest = Path::new(&out_dir).join("kernel.rs");
    std::fs::write(&dest, generated).unwrap();
}
