//! askic — aski bootstrap compiler
//!
//! Compiles .aski source files to Rust and writes to stdout.
//! Used by build.rs in downstream crates (like aski-core) to
//! generate Rust from .aski without a proc-macro dependency.
//!
//! Modes:
//!   askic <files...>           — standard codegen (Rust structs/enums)
//!   askic --kernel <files...>  — kernel codegen (World + queries + derive)

use std::env;
use std::fs;
use std::process;

use aski_rs::codegen::CodegenConfig;
use aski_rs::codegen_kernel;
use aski_rs::compiler::{compile_files, compile_files_to_world};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("usage: askic [--kernel] <file.aski> [file2.aski ...]");
        process::exit(1);
    }

    let kernel_mode = args.iter().any(|a| a == "--kernel");
    let file_args: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();

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

    if kernel_mode {
        match compile_files_to_world(&refs) {
            Ok(world) => match codegen_kernel::generate_kernel(&world) {
                Ok(rust) => print!("{}", rust),
                Err(e) => {
                    eprintln!("askic: kernel codegen: {}", e);
                    process::exit(1);
                }
            },
            Err(e) => {
                eprintln!("askic: {}", e);
                process::exit(1);
            }
        }
    } else {
        let config = CodegenConfig { rkyv: false };
        match compile_files(&refs, &config) {
            Ok(rust) => print!("{}", rust),
            Err(e) => {
                eprintln!("askic: {}", e);
                process::exit(1);
            }
        }
    }
}
