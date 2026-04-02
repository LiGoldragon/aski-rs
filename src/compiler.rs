//! Multi-file compilation API.
//!
//! Compiles multiple .aski source files together, resolving imports
//! across module boundaries.

use crate::ast::SourceFile;
use crate::codegen::{CodegenConfig, generate_rust_from_db_with_config};
use crate::ir;
use crate::parser::parse_source_file;

/// Compile multiple .aski source files into a single Rust output.
///
/// Each entry is `(filename, source_text)`.
/// Files may have module headers with imports/exports.
/// All items from all files are compiled together.
pub fn compile_files(
    sources: &[(&str, &str)],
    config: &CodegenConfig,
) -> Result<String, String> {
    let mut all_files: Vec<SourceFile> = Vec::new();

    // Phase 1: Parse all files
    for (filename, source) in sources {
        let sf = parse_source_file(source)
            .map_err(|e| format!("error in {filename}: {e}"))?;
        all_files.push(sf);
    }

    // Phase 2: Insert all items into a single World
    // Use increasing ID offsets to avoid collisions between files
    let mut world = ir::create_world();
    let mut id_offset: i64 = 0;
    for sf in &all_files {
        let count = ir::insert_ast_with_offset(&mut world, &sf.items, id_offset)?;
        id_offset += count;
    }

    // Phase 3: Run derived rules before codegen
    ir::run_rules(&mut world);

    // Phase 4: Generate Rust from the combined World
    generate_rust_from_db_with_config(&world, config)
}

/// Compile multiple .aski source files, reading from the filesystem.
pub fn compile_directory(
    paths: &[&str],
    config: &CodegenConfig,
) -> Result<String, String> {
    let sources: Vec<(String, String)> = paths
        .iter()
        .map(|path| {
            let source = std::fs::read_to_string(path)
                .map_err(|e| format!("failed to read {path}: {e}"))?;
            Ok((path.to_string(), source))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let refs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(name, src)| (name.as_str(), src.as_str()))
        .collect();

    compile_files(&refs, config)
}
