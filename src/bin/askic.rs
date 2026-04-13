//! askic — aski compiler
//!
//! Modes:
//!   askic deparse <file.aski>     — parse then deparse (round-trip text)
//!   askic sema <file.aski>        — parse then lower to SemaWorld (dump)
//!   askic roundtrip <file.aski>   — parse → lower → raise → deparse

use std::env;
use std::fs;
use std::path::Path;
use std::collections::HashMap;

use aski_rs::synth::{loader, types::Dialect};
use aski_rs::engine::aski_world::AskiWorld;
use aski_rs::engine::parse::Parse;
use aski_rs::engine::deparse::Deparse;
use aski_rs::engine::lower::Lower;
use aski_rs::engine::raise::Raise;
use aski_rs::engine::codegen::Codegen;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.len() < 2 {
        eprintln!("usage: askic <rust|deparse|sema|roundtrip> <file.aski> [--synth-dir <path>]");
        std::process::exit(1);
    }

    let mode = &args[0];
    let file = &args[1];
    let synth_dir = args.iter().position(|a| a == "--synth-dir")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("source");

    let source = fs::read_to_string(file).unwrap_or_else(|e| {
        eprintln!("askic: {}: {}", file, e);
        std::process::exit(1);
    });

    let dialects = load_dialects(synth_dir);
    let mut world = AskiWorld::new(dialects.clone());

    let is_main = file.ends_with(".main");
    let parse_result = if is_main {
        world.parse_main(file, &source)
    } else {
        world.parse_file(file, &source)
    };
    parse_result.unwrap_or_else(|e| {
        eprintln!("askic: parse error: {}", e);
        std::process::exit(1);
    });

    match mode.as_str() {
        "deparse" => {
            print!("{}", world.deparse());
        }
        "sema" => {
            let sema = world.lower();
            println!("types: {:?}", sema.type_names);
            println!("variants: {:?}", sema.variant_names);
            println!("fields: {:?}", sema.field_names);
            for t in &sema.types {
                println!("  type[{}] = {:?}", sema.type_names[t.name as usize], t.form);
            }
            for v in &sema.variants {
                println!("  variant[{}] of type[{}] ordinal={}",
                    sema.variant_names[v.name as usize],
                    sema.type_names[v.type_id as usize],
                    v.ordinal);
            }
            for f in &sema.fields {
                println!("  field[{}] of type[{}] ordinal={}",
                    sema.field_names[f.name as usize],
                    sema.type_names[f.type_id as usize],
                    f.ordinal);
            }
            println!("trait_decls: {:?}", sema.trait_names);
            for d in &sema.trait_decls {
                println!("  trait[{}] sigs={}",
                    sema.trait_names[d.name as usize], d.method_sigs.len());
            }
            for i in &sema.trait_impls {
                println!("  impl {} for {} methods={}",
                    sema.trait_names[i.trait_id as usize],
                    sema.type_names[i.type_id as usize],
                    i.methods.len());
            }
            for m in &sema.modules {
                println!("module[{}] file={} exports={:?} imports={}",
                    sema.module_names[m.name as usize],
                    m.file_path,
                    m.exports,
                    m.imports.len());
                for imp in &m.imports {
                    println!("  import {} {:?}", imp.module_name, imp.names);
                }
            }
        }
        "rust" => {
            let sema = world.lower();
            print!("{}", sema.codegen());
        }
        "roundtrip" => {
            let sema = world.lower();
            let raised = AskiWorld::raise(&sema, dialects);
            print!("{}", raised.deparse());
        }
        _ => {
            eprintln!("askic: unknown mode '{}'. Use deparse, sema, or roundtrip.", mode);
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
