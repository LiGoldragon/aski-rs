use std::io::Write;
use std::process::Command;

use aski_rs::codegen_v3::{self, CodegenConfig};
use aski_rs::ir;
use aski_rs::parser::parse_source;

#[test]
fn parser_aski_parses_and_generates() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/examples/parser.aski");
    let source = std::fs::read_to_string(&path).expect("failed to read parser.aski");

    // 1. Parse
    let items = match parse_source(&source) {
        Ok(items) => items,
        Err(e) => panic!("failed to parse parser.aski:\n{e}"),
    };
    eprintln!("Parsed {} items from parser.aski", items.len());
    assert!(!items.is_empty(), "parsed zero items");

    // 2. Insert into World
    let mut world = ir::create_world();
    ir::insert_ast(&mut world, &items).expect("failed to insert AST");
    ir::run_rules(&mut world);

    // 3. Generate Rust
    let config = CodegenConfig { rkyv: false };
    let rust_code = codegen_v3::generate_with_config(&world, &config).expect("failed to generate");
    eprintln!("=== Generated Rust ===\n{rust_code}\n=== End ===");

    // 4. Verify key types exist
    assert!(rust_code.contains("pub enum Token"), "should have Token enum");
    assert!(rust_code.contains("pub struct ParseState"), "should have ParseState struct");
    assert!(rust_code.contains("fn main()"), "should have main");

    // 5. Write and compile
    let dir = std::env::temp_dir();
    let rs_path = dir.join("aski_parser_test.rs");
    let bin_path = dir.join("aski_parser_test_bin");
    {
        let mut f = std::fs::File::create(&rs_path).expect("failed to create temp file");
        f.write_all(rust_code.as_bytes()).expect("failed to write");
    }

    if let Ok(output) = Command::new("rustc")
        .arg(&rs_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
    {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "rustc failed to compile parser.aski:\n{stderr}\n\nGenerated code:\n{rust_code}"
        );
        {
            eprintln!("parser.aski compiled successfully!");
        }
    }

    let _ = std::fs::remove_file(&rs_path);
    let _ = std::fs::remove_file(&bin_path);
}
