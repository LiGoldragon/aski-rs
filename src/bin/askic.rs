//! askic — aski compiler
//!
//! Modes:
//!   askic rust <file>         — compile to Rust (resolves imports)
//!   askic sema <file>         — write .sema binary
//!   askic deparse <file>      — parse then deparse (single file)
//!   askic roundtrip <file>    — parse → lower → raise → deparse

use std::env;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::collections::HashMap;

use aski_rs::synth::{loader, types::Dialect};
use aski_rs::engine::aski_world::AskiWorld;
use aski_rs::engine::parse::Parse;
use aski_rs::engine::deparse::Deparse;
use aski_rs::engine::lower::Lower;
use aski_rs::engine::sema::SemaSerialize;
use aski_rs::engine::raise::Raise;
use aski_rs::engine::compiler::{AskiCompiler, Compiler, ResolveImports};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() < 2 {
        eprintln!("usage: askic <rust|sema|deparse|roundtrip> <file> [--synth-dir <path>]");
        std::process::exit(1);
    }

    let mode = &args[0];
    let file = &args[1];
    let synth_dir = args.iter().position(|a| a == "--synth-dir")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("source");

    let dialects = load_dialects(synth_dir);

    match mode.as_str() {
        "rust" => {
            let files = AskiCompiler::resolve_imports(file, synth_dir, &dialects)
                .unwrap_or_else(|e| { eprintln!("askic: resolve error: {}", e); std::process::exit(1); });
            let mut compiler = AskiCompiler::new(dialects, synth_dir);
            let rust = compiler.compile_rust(&files)
                .unwrap_or_else(|e| { eprintln!("askic: compile error: {}", e); std::process::exit(1); });
            print!("{}", rust);
        }
        "sema" => {
            let files = AskiCompiler::resolve_imports(file, synth_dir, &dialects)
                .unwrap_or_else(|e| { eprintln!("askic: resolve error: {}", e); std::process::exit(1); });
            let mut compiler = AskiCompiler::new(dialects, synth_dir);
            let result = compiler.compile_files(&files)
                .unwrap_or_else(|e| { eprintln!("askic: compile error: {}", e); std::process::exit(1); });
            let bytes = result.sema.to_sema_bytes();
            let sema_path = file.replace(".aski", ".sema").replace(".main", ".sema");
            fs::File::create(&sema_path)
                .and_then(|mut f| f.write_all(&bytes))
                .unwrap_or_else(|e| { eprintln!("askic: write error: {}", e); std::process::exit(1); });
            eprintln!("wrote {} bytes to {}", bytes.len(), sema_path);
        }
        _ => {
            // Single-file modes (deparse, roundtrip)
            let source = fs::read_to_string(file)
                .unwrap_or_else(|e| { eprintln!("askic: {}: {}", file, e); std::process::exit(1); });
            let mut world = AskiWorld::new(dialects.clone());
            if file.ends_with(".main") {
                world.parse_main(file, &source)
            } else {
                world.parse_file(file, &source)
            }.unwrap_or_else(|e| { eprintln!("askic: parse error: {}", e); std::process::exit(1); });

            match mode.as_str() {
                "deparse" => print!("{}", world.deparse()),
                "roundtrip" => {
                    let result = world.lower();
                    let raised = AskiWorld::raise(&result.sema, &result.names, &result.exports, dialects);
                    print!("{}", raised.deparse());
                }
                other => {
                    eprintln!("askic: unknown mode '{}'. Use rust, sema, deparse, or roundtrip.", other);
                    std::process::exit(1);
                }
            }
        }
    }
}

fn load_dialects(dir: &str) -> HashMap<String, Dialect> {
    let path = Path::new(dir);
    if path.exists() {
        loader::load_all(path).unwrap_or_else(|e| {
            eprintln!("askic: synth load error: {}", e);
            HashMap::new()
        })
    } else {
        HashMap::new()
    }
}
