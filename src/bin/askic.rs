//! askic — aski compiler
//!
//! Primary pipeline: .aski → .sema + .aski-table.sema → .rs
//!
//! Commands:
//!   askic compile <file.aski>  — produce .sema + .aski-table.sema
//!   askic rust <file.sema>     — read .sema → Rust (auto-discovers .aski-table.sema)
//!   askic rust <file.aski>     — shorthand: compile + rust in one step
//!   askic deparse <file.sema>  — read .sema → aski text
//!   askic deparse <file.aski>  — parse + deparse (single file)

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
use aski_rs::engine::raise::Raise;
use aski_rs::engine::sema::*;
use aski_rs::engine::codegen::{CodegenContext, Codegen};
use aski_rs::engine::compiler::{AskiCompiler, Compiler, ResolveImports};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() < 2 {
        eprintln!("usage: askic <compile|rust|deparse> <file> [--synth-dir <path>]");
        std::process::exit(1);
    }

    let mode = &args[0];
    let file = &args[1];
    let synth_dir = args.iter().position(|a| a == "--synth-dir")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("source");
    let names_flag = args.iter().position(|a| a == "--names")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str());

    match mode.as_str() {
        "compile" => {
            // .aski → .sema + .aski-table.sema
            let dialects = load_dialects(synth_dir);
            let files = resolve(&dialects, file, synth_dir);
            let mut compiler = AskiCompiler::new(dialects, synth_dir);
            let result = compiler.compile_files(&files).unwrap_or_die("compile");

            let sema_path = to_sema_path(file);
            let table_path = to_table_path(file);

            let sema_bytes = result.sema.to_sema_bytes();
            write_file(&sema_path, &sema_bytes);

            let table = AskiNameTable::from_lower(&result.names, &result.exports);
            let table_bytes = table.to_bytes();
            write_file(&table_path, &table_bytes);

            eprintln!("{} ({} bytes) + {} ({} bytes)",
                sema_path, sema_bytes.len(), table_path, table_bytes.len());
        }
        "rust" => {
            if file.ends_with(".sema") {
                // Read from .sema artifact
                let sema_bytes = fs::read(file).unwrap_or_die("read sema");
                let sema = Sema::from_sema_bytes(&sema_bytes).unwrap_or_die("deserialize sema");

                let table_path = names_flag.map(String::from)
                    .unwrap_or_else(|| auto_discover_table(file));
                let table_bytes = fs::read(&table_path).unwrap_or_die("read name table");
                let table = AskiNameTable::from_bytes(&table_bytes).unwrap_or_die("deserialize names");

                let ctx = CodegenContext { sema: &sema, names: &table };
                print!("{}", ctx.codegen());
            } else {
                // Shorthand: .aski → compile → .sema → rust
                let dialects = load_dialects(synth_dir);
                let files = resolve(&dialects, file, synth_dir);
                let mut compiler = AskiCompiler::new(dialects, synth_dir);
                let result = compiler.compile_files(&files).unwrap_or_die("compile");

                // Write sema artifacts
                let sema_path = to_sema_path(file);
                let table_path = to_table_path(file);
                let sema_bytes = result.sema.to_sema_bytes();
                write_file(&sema_path, &sema_bytes);
                let table = AskiNameTable::from_lower(&result.names, &result.exports);
                write_file(&table_path, &table.to_bytes());

                // Read back from .sema (proves self-containment)
                let sema = Sema::from_sema_bytes(&sema_bytes).unwrap_or_die("deserialize");
                let ctx = CodegenContext { sema: &sema, names: &table };
                print!("{}", ctx.codegen());
            }
        }
        "deparse" => {
            if file.ends_with(".sema") {
                // Read .sema → raise → deparse
                let sema_bytes = fs::read(file).unwrap_or_die("read sema");
                let sema = Sema::from_sema_bytes(&sema_bytes).unwrap_or_die("deserialize sema");

                let table_path = names_flag.map(String::from)
                    .unwrap_or_else(|| auto_discover_table(file));
                let table_bytes = fs::read(&table_path).unwrap_or_die("read name table");
                let table = AskiNameTable::from_bytes(&table_bytes).unwrap_or_die("deserialize names");

                let dialects = load_dialects(synth_dir);
                let raised = AskiWorld::raise(&sema, &table, &table.exports, dialects);
                print!("{}", raised.deparse());
            } else {
                // Single-file parse + deparse
                let dialects = load_dialects(synth_dir);
                let source = fs::read_to_string(file).unwrap_or_die("read");
                let mut world = AskiWorld::new(dialects);
                if file.ends_with(".main") {
                    world.parse_main(file, &source)
                } else {
                    world.parse_file(file, &source)
                }.unwrap_or_die("parse");
                print!("{}", world.deparse());
            }
        }
        "roundtrip" => {
            // .aski → compile → .sema → deparse → .aski
            let dialects = load_dialects(synth_dir);
            let source = fs::read_to_string(file).unwrap_or_die("read");
            let mut world = AskiWorld::new(dialects.clone());
            if file.ends_with(".main") {
                world.parse_main(file, &source)
            } else {
                world.parse_file(file, &source)
            }.unwrap_or_die("parse");

            let result = world.lower();

            // Through .sema binary
            let sema_bytes = result.sema.to_sema_bytes();
            let sema = Sema::from_sema_bytes(&sema_bytes).unwrap_or_die("roundtrip deserialize");
            let table = AskiNameTable::from_lower(&result.names, &result.exports);

            let raised = AskiWorld::raise(&sema, &table, &result.exports, dialects);
            print!("{}", raised.deparse());
        }
        other => {
            eprintln!("askic: unknown mode '{}'. Use compile, rust, deparse, or roundtrip.", other);
            std::process::exit(1);
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

fn resolve(dialects: &HashMap<String, Dialect>, file: &str, synth_dir: &str) -> Vec<String> {
    AskiCompiler::resolve_imports(file, synth_dir, dialects)
        .unwrap_or_else(|e| { eprintln!("askic: resolve error: {}", e); std::process::exit(1); })
}

fn to_sema_path(aski_path: &str) -> String {
    aski_path.replace(".aski", ".sema").replace(".main", ".sema")
}

fn to_table_path(aski_path: &str) -> String {
    aski_path.replace(".aski", ".aski-table.sema").replace(".main", ".aski-table.sema")
}

fn auto_discover_table(sema_path: &str) -> String {
    sema_path.replace(".sema", ".aski-table.sema")
}

fn write_file(path: &str, bytes: &[u8]) {
    fs::File::create(path)
        .and_then(|mut f| f.write_all(bytes))
        .unwrap_or_else(|e| { eprintln!("askic: write {}: {}", path, e); std::process::exit(1); });
}

trait UnwrapOrDie<T> {
    fn unwrap_or_die(self, context: &str) -> T;
}

impl<T, E: std::fmt::Display> UnwrapOrDie<T> for Result<T, E> {
    fn unwrap_or_die(self, context: &str) -> T {
        self.unwrap_or_else(|e| { eprintln!("askic: {}: {}", context, e); std::process::exit(1); })
    }
}
