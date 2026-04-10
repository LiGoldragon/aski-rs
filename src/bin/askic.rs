//! askic — aski bootstrap compiler
//!
//! Compiles .aski source files to Rust and writes to stdout.
//! One compiler, one codegen, one grammar.

use std::env;
use std::fs;
use std::process;

use aski_rs::codegen_v3::{self, CodegenConfig};
use aski_rs::compiler::compile_files_to_world;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("usage: askic [--grammar-dir <path>] <file.aski> [file2.aski ...]");
        process::exit(1);
    }

    // Extract --grammar-dir <path> and set ASKI_GRAMMAR_DIR
    for (i, arg) in args.iter().enumerate() {
        if arg == "--grammar-dir" {
            if let Some(dir) = args.get(i + 1) {
                env::set_var("ASKI_GRAMMAR_DIR", dir);
            } else {
                eprintln!("askic: --grammar-dir requires an argument");
                process::exit(1);
            }
        }
    }

    let file_args: Vec<&String> = args.iter().enumerate()
        .filter(|(i, a)| {
            if *a == "--grammar-dir" { return false; }
            if *i > 0 && args[i - 1] == "--grammar-dir" { return false; }
            true
        })
        .map(|(_, a)| a)
        .collect();

    if file_args.is_empty() {
        eprintln!("askic: no input files");
        process::exit(1);
    }

    let mut sources: Vec<(String, String)> = Vec::new();
    for path in &file_args {
        match fs::read_to_string(path) {
            Ok(s) => sources.push((path.to_string(), s)),
            Err(e) => {
                eprintln!("askic: {}: {}", path, e);
                process::exit(1);
            }
        }
    }

    let refs: Vec<(&str, &str)> = sources.iter()
        .map(|(p, s)| (p.as_str(), s.as_str()))
        .collect();

    let config = CodegenConfig { rkyv: false };
    match compile_files_to_world(&refs) {
        Ok(world) => {
            // Load FFI registry from parsed foreign blocks
            let ffi_reg = codegen_v3::FfiRegistry::load_from_world(&world);
            codegen_v3::set_ffi_registry(ffi_reg);

            match codegen_v3::generate_with_config(&world, &config) {
                Ok(rust) => print!("{}", rust),
                Err(e) => {
                    eprintln!("askic: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("askic: {}", e);
            process::exit(1);
        }
    }
}
