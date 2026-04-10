//! askic — aski bootstrap compiler
//!
//! Lexer is Rust (logos). Codegen is compiled from aski.
//! Parser is Rust (kernel-specific, to be replaced by aski PEG).

use std::env;
use std::fs;
use std::process;

use aski_rs::kernel_parser;
use aski_rs::codegen_gen::Generate;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        eprintln!("usage: askic <file.aski>");
        process::exit(1);
    }

    let path = &args[args.len() - 1];
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("askic: {}: {}", path, e);
            process::exit(1);
        }
    };

    let world = match kernel_parser::parse(&source) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("askic: parse error: {}", e);
            process::exit(1);
        }
    };

    let output = world.generate();
    print!("{}", output);
}
