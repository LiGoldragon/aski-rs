//! askic — aski bootstrap compiler
//!
//! Compiles .aski source files to Rust and writes to stdout.

use std::env;
use std::fs;
use std::process;

use aski_rs::emit::CodegenConfig;
use aski_rs::compiler::compile_files_to_world;

fn main() {
    // Spawn with larger stack for deeply nested PEG parsing
    let builder = std::thread::Builder::new().stack_size(256 * 1024 * 1024);
    let handler = builder.spawn(real_main).expect("thread spawn failed");
    handler.join().expect("thread panic");
}

fn real_main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("usage: askic [--grammar-dir <path>] [--rkyv] <file.aski> [file2.aski ...]");
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
            if *a == "--grammar-dir" || *a == "--rkyv" { return false; }
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

    let rkyv = args.contains(&"--rkyv".to_string());
    let config = CodegenConfig { rkyv };

    match compile_files_to_world(&refs) {
        Ok(mut world) => {
            aski_core::run_rules(&mut world);
            match aski_rs::emit::generate_with_config(&world, &config) {
                Ok(rust) => print!("{}", rust),
                Err(e) => {
                    eprintln!("askic: codegen: {}", e);
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
