use std::io::Write;
use std::process::Command;

use aski_rs::codegen_v3::CodegenConfig;
use aski_rs::compiler::compile_directory;

#[test]
fn bootstrap_parser_compiles() {
    let config = CodegenConfig { rkyv: false };
    let rust_code = compile_directory(
        &[
            "bootstrap/token.aski",
            "bootstrap/tokens.aski",
            "bootstrap/parser.aski",
            "bootstrap/main.aski",
        ],
        &config,
    )
    .expect("failed to compile bootstrap parser");

    eprintln!("=== Generated Rust ({} lines) ===", rust_code.lines().count());

    // Add test that parses Element (Fire Earth Air Water)
    let rust_code = rust_code.replace(
        "fn main() {",
        r#"fn test_parse_domain() {
    let tokens = vec![
        Token::PascalIdent("Element".to_string()),
        Token::LParen,
        Token::PascalIdent("Fire".to_string()),
        Token::PascalIdent("Earth".to_string()),
        Token::PascalIdent("Air".to_string()),
        Token::PascalIdent("Water".to_string()),
        Token::RParen,
    ];
    let t = Tokens { stream: tokens, position: 0 };

    assert!(!t.at_end());
    assert!(t.peek_is_pascal());

    let result = t.parse_domain();
    match &result {
        ParseResult::Parsed => println!("  parseDomain: OK"),
        ParseResult::Failed => panic!("  parseDomain: FAILED"),
    }

    // Test struct: Point { Horizontal F64 Vertical F64 }
    let struct_tokens = vec![
        Token::PascalIdent("Point".to_string()),
        Token::LBrace,
        Token::PascalIdent("Horizontal".to_string()),
        Token::PascalIdent("F64".to_string()),
        Token::PascalIdent("Vertical".to_string()),
        Token::PascalIdent("F64".to_string()),
        Token::RBrace,
    ];
    let st = Tokens { stream: struct_tokens, position: 0 };
    let sresult = st.parse_struct();
    match &sresult {
        ParseResult::Parsed => println!("  parseStruct: OK"),
        ParseResult::Failed => panic!("  parseStruct: FAILED"),
    }
}

fn main() {"#,
    );
    let rust_code = rust_code.replace(
        r#"println!("aski bootstrap parser");"#,
        r#"test_parse_domain();
    println!("aski bootstrap parser");"#,
    );

    // Write and compile
    let dir = std::env::temp_dir();
    let rs_path = dir.join("aski_bootstrap_test.rs");
    let bin_path = dir.join("aski_bootstrap_test_bin");
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
            "rustc failed:\n{stderr}\n\nGenerated:\n{rust_code}"
        );
        eprintln!("Compiled!");

        if let Ok(run_output) = Command::new(&bin_path).output() {
            let stdout = String::from_utf8_lossy(&run_output.stdout);
            let stderr_run = String::from_utf8_lossy(&run_output.stderr);
            eprintln!("stdout: {stdout}");
            if !stderr_run.is_empty() {
                eprintln!("stderr: {stderr_run}");
            }
            assert!(run_output.status.success(), "runtime error: {stderr_run}");
            assert!(stdout.contains("aski bootstrap parser"), "should have main output");
        }
    }

    let _ = std::fs::remove_file(&rs_path);
    let _ = std::fs::remove_file(&bin_path);
}
