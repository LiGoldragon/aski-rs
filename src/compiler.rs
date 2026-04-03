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

    // Phase 3: Expand grammar rules (Surface → Kernel)
    expand_grammar_rules(&mut world)?;

    // Phase 4: Run derived rules before codegen
    ir::run_rules(&mut world);

    // Phase 5: Generate Rust from the combined World
    generate_rust_from_db_with_config(&world, config)
}

/// Expand grammar rules — Surface Aski → Kernel Aski.
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

/// Compile source files using the data-driven grammar engine.
///
/// This is an ALTERNATIVE code path — the chumsky parser remains the default.
/// Grammar files are loaded first, then source files are parsed using the
/// grammar engine instead of chumsky.
///
/// Returns a list of `MatchResult` trees (one per top-level item) for
/// inspection and testing.  Full codegen integration will come later.
pub fn parse_with_grammar(
    grammar_files: &[&str],
    source_files: &[(&str, &str)],
) -> Result<Vec<crate::grammar_engine::MatchResult>, String> {
    use crate::grammar_engine::{Grammar, parse_items};
    use crate::lexer::Token;

    // Phase 1: Load grammar rules from .aski grammar files
    let mut grammar = Grammar::new();
    for gf in grammar_files {
        let source = std::fs::read_to_string(gf)
            .map_err(|e| format!("failed to read grammar file {gf}: {e}"))?;
        let count = grammar.load_grammar_source(&source)?;
        if count == 0 {
            return Err(format!("no grammar rules found in {gf}"));
        }
    }

    // Phase 2: Parse source files using the grammar engine
    let mut all_items = Vec::new();
    for (filename, source) in source_files {
        let spanned = crate::lexer::lex(source).map_err(|errs| {
            format!(
                "lex error in {filename}: {}",
                errs.into_iter()
                    .map(|e| e.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        let tokens: Vec<Token> = spanned.into_iter().map(|s| s.token).collect();
        let items = parse_items(&grammar, &tokens);
        all_items.extend(items);
    }

    Ok(all_items)
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
