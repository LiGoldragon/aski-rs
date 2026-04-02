use std::io::Write;
use std::process::Command;

use aski_rs::codegen::{CodegenConfig, generate_rust_from_db_with_config};
use aski_rs::ir;
use aski_rs::parser::parse_source;

#[test]
fn simple_aski_parses_stores_generates_compiles() {
    // 1. Read simple.aski
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/encoder/design/v0.9/examples/simple.aski");
    let source = std::fs::read_to_string(&path)
        .expect(&format!("failed to read {path}"));

    // 2. Parse it
    let items = parse_source(&source).expect("failed to parse simple.aski");
    assert!(!items.is_empty(), "parsed zero items");

    // 3. Insert into World
    let mut world = ir::create_world();
    ir::insert_ast(&mut world, &items).expect("failed to insert AST into world");
    ir::run_rules(&mut world);

    // 4. Generate Rust code from World
    let config = CodegenConfig { rkyv: false };
    let rust_code = generate_rust_from_db_with_config(&world, &config).expect("failed to generate Rust from World");

    eprintln!("=== Generated Rust ===\n{rust_code}\n=== End ===");

    assert!(rust_code.contains("pub enum Element"));
    assert!(rust_code.contains("pub struct Point"));
    assert!(rust_code.contains("fn add("));
    assert!(rust_code.contains("fn main()"));

    // 5. Write the Rust to a temp file and compile with rustc (if available)
    let dir = std::env::temp_dir();
    let rs_path = dir.join("aski_simple_test.rs");
    let bin_path = dir.join("aski_simple_test_bin");
    {
        let mut f = std::fs::File::create(&rs_path).expect("failed to create temp rs file");
        f.write_all(rust_code.as_bytes()).expect("failed to write rs");
    }

    // 6. Compile with rustc — skip if not available (e.g. Nix sandbox)
    if let Ok(output) = Command::new("rustc")
        .arg(&rs_path)
        .arg("-o")
        .arg(&bin_path)
        .output()
    {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "rustc failed to compile generated Rust:\n{stderr}\n\nGenerated code:\n{rust_code}"
        );
    } else {
        eprintln!("rustc not available — skipping compilation check");
    }

    // Cleanup
    let _ = std::fs::remove_file(&rs_path);
    let _ = std::fs::remove_file(&bin_path);
}
