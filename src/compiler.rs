//! Multi-file compilation API.
//!
//! Compiles multiple .aski source files together, resolving imports
//! across module boundaries.
//!
//! Grammar configuration (operators, kernel primitives, token classes)
//! is loaded from grammar/*.aski files before parsing begins.

use crate::ast::SourceFile;
use crate::codegen::{CodegenConfig, generate_rust_from_db_with_config};
use crate::ir;
use crate::engine::{parse_source_file_with_config, config as grammar_config};

/// Compile multiple .aski source files into a single Rust output.
///
/// Each entry is `(filename, source_text)`.
/// Files may have module headers with imports/exports.
/// All items from all files are compiled together.
///
/// Grammar configuration is loaded from grammar/*.aski files before parsing.
pub fn compile_files(
    sources: &[(&str, &str)],
    config: &CodegenConfig,
) -> Result<String, String> {
    // Phase 0: Load grammar configuration from .aski data files
    let grammar = grammar_config::load_or_bootstrap();

    let mut all_files: Vec<SourceFile> = Vec::new();

    // Phase 1: Parse all files using data-driven grammar config
    for (filename, source) in sources {
        let sf = parse_source_file_with_config(source, &grammar)
            .map_err(|e| format!("error in {filename}: {e}"))?;
        all_files.push(sf);
    }

    // Phase 2: Insert all items into a single World
    // Use increasing ID offsets to avoid collisions between files.
    // Module headers create Scope nodes and populate Export/Import relations.
    let mut world = ir::create_world();
    let mut id_offset: i64 = 0;
    for sf in &all_files {
        let mut ids = ir::IdGen { next: id_offset + 1 };
        let scope_id = if let Some(ref header) = sf.header {
            Some(ir::insert_module_header(&mut world, &mut ids, header))
        } else {
            None
        };
        // Advance offset past any IDs used by the header
        id_offset = ids.next - 1;
        let count = ir::insert_ast_with_offset(&mut world, &sf.items, id_offset, scope_id)?;
        id_offset += count;
    }

    // Phase 3: Expand grammar rules (Surface -> Kernel)
    expand_grammar_rules(&mut world)?;

    // Phase 4: Run derived rules before codegen
    ir::run_rules(&mut world);

    // Phase 5: Generate Rust from the combined World
    generate_rust_from_db_with_config(&world, config)
}

/// Expand grammar rules -- Surface Aski -> Kernel Aski.
///
/// Reads all GrammarRule/GrammarArm relations from the World.
/// For the first pass, grammar rules are stored but expansion is a no-op:
/// the rules exist in the World for tooling to query, and kernel primitives
/// (truncate, fromOrdinal, sin, cos, etc.) are handled directly by codegen.
///
/// Future passes will pattern-match expressions against grammar rules and
/// replace them with expanded kernel forms.
fn expand_grammar_rules(world: &mut ir::World) -> Result<(), String> {
    // Collect grammar rules for validation
    let rules: Vec<(i64, String)> = world.GrammarRule.iter()
        .map(|(id, name)| (*id, name.clone()))
        .collect();

    // Log grammar rules found (useful for debugging)
    for (_id, name) in &rules {
        // Grammar rule registered: name
        // Future: match expressions against rule patterns and expand
        let _ = name;
    }

    // Validate: any stub expression in a method whose type has a grammar rule
    // should eventually be expanded. For now, stubs remain as todo!().

    Ok(())
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
