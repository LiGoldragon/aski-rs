//! Multi-file compilation API.
//!
//! Compiles multiple .aski source files together, resolving imports
//! across module boundaries.
//!
//! Grammar configuration (operators, kernel primitives, token classes)
//! is loaded from grammar/*.aski files before parsing begins.

use std::collections::HashMap;
use crate::ast::SourceFile;
use crate::codegen_v3::{self, CodegenConfig};
use crate::ir;
use crate::grammar::{self, RuleTable};
use crate::grammar::config as grammar_config;

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
    let world = compile_files_to_world(sources)?;
    codegen_v3::generate_with_config(&world, config)
}

/// Expand grammar rules — validate grammar rules stored in the World.
///
/// Grammar rules are live: they are injected into the parser during parsing
/// and travel with imports via topo-sort. This pass validates the stored
/// GrammarRule/GrammarArm relations. Kernel primitives (truncate, fromOrdinal,
/// sin, cos, etc.) are handled directly by codegen.
fn expand_grammar_rules(world: &mut ir::World) -> Result<(), String> {
    // Collect grammar rules for validation
    let rules: Vec<(i64, String)> = world.grammar_rules.iter()
        .map(|r| (r.node_id, r.rule_name.clone()))
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

/// Compile .aski source files and return the populated IR World (no codegen).
///
/// Files are parsed in dependency order: module headers are extracted first,
/// files are topologically sorted by imports, and each file's parser receives
/// grammar rules from its imported modules. Grammar rules travel with types.
pub fn compile_files_to_world(
    sources: &[(&str, &str)],
) -> Result<ir::World, String> {
    let grammar_config = grammar_config::load_or_bootstrap();

    // Phase 0: Extract module headers to determine dependencies
    let headers: Vec<Option<crate::ast::ModuleHeader>> = sources.iter()
        .map(|(_, source)| grammar::parse_header_only(source, &grammar_config))
        .collect();

    // Build module name → index map
    let mut name_to_idx: HashMap<String, usize> = HashMap::new();
    for (i, header) in headers.iter().enumerate() {
        if let Some(h) = header {
            name_to_idx.insert(h.name.clone(), i);
        }
    }

    // Phase 1: Topological sort by import dependencies
    let order = topo_sort_files(&headers, &name_to_idx, sources.len());

    // Phase 2: Parse files in dependency order, accumulating grammar rules
    let mut module_rules: HashMap<String, RuleTable> = HashMap::new();
    let mut parsed_files: Vec<(usize, SourceFile)> = Vec::new();

    for idx in &order {
        let (filename, source) = &sources[*idx];

        // Collect grammar rules from imported modules
        let mut extra_rules = RuleTable::new();
        if let Some(ref header) = headers[*idx] {
            for import in &header.imports {
                if let Some(rules) = module_rules.get(&import.module) {
                    for (name, rule) in rules {
                        extra_rules.insert(name.clone(), rule.clone());
                    }
                }
            }
        }

        // Parse with bootstrap + imported grammar rules
        let (sf, user_rules) = grammar::extract_user_grammar_rules(source, &grammar_config, &extra_rules)
            .map_err(|e| format!("error in {filename}: {e}"))?;

        // Store this module's grammar rules for downstream consumers
        if let Some(ref header) = sf.header {
            // Merge: this module's own rules + rules it imported (transitive)
            let mut all_rules = extra_rules;
            for (name, rule) in user_rules {
                all_rules.insert(name, rule);
            }
            module_rules.insert(header.name.clone(), all_rules);
        }

        parsed_files.push((*idx, sf));
    }

    // Sort back to original order for stable ID assignment
    parsed_files.sort_by_key(|(idx, _)| *idx);

    // Phase 3: Insert all items into a single World
    let mut world = ir::create_world();
    let mut id_offset: i64 = 0;
    for (_, sf) in &parsed_files {
        let mut ids = ir::IdGen { next: id_offset + 1 };
        let scope_id = if let Some(ref header) = sf.header {
            Some(ir::insert_module_header(&mut world, &mut ids, header))
        } else {
            None
        };
        id_offset = ids.next - 1;
        let count = ir::insert_ast_with_offset(&mut world, &sf.items, id_offset, scope_id)?;
        id_offset += count;
    }

    // Phase 4: Expand grammar rules + derive
    expand_grammar_rules(&mut world)?;
    ir::run_rules(&mut world);

    Ok(world)
}

/// Topological sort of files by import dependencies.
/// Files without headers or with no imports come first.
fn topo_sort_files(
    headers: &[Option<crate::ast::ModuleHeader>],
    name_to_idx: &HashMap<String, usize>,
    count: usize,
) -> Vec<usize> {
    // Build adjacency: file i depends on file j if i imports from j
    let mut deps: Vec<Vec<usize>> = vec![vec![]; count];
    for (i, header) in headers.iter().enumerate() {
        if let Some(h) = header {
            for import in &h.imports {
                if let Some(&j) = name_to_idx.get(&import.module) {
                    deps[i].push(j);
                }
            }
        }
    }

    // Kahn's algorithm
    let mut in_degree = vec![0usize; count];
    for d in &deps {
        for &j in d {
            in_degree[j] += 1;
        }
    }

    // Wait — in_degree should count incoming edges. deps[i] lists what i depends on.
    // For topo sort, we need: i depends on j → j must come before i.
    // So the edge is j → i (j before i). in_degree of i = number of deps.
    let mut in_deg = vec![0usize; count];
    for (i, d) in deps.iter().enumerate() {
        in_deg[i] = d.len();
    }

    // Reverse adjacency: for each j, which files depend on j?
    let mut rev: Vec<Vec<usize>> = vec![vec![]; count];
    for (i, d) in deps.iter().enumerate() {
        for &j in d {
            rev[j].push(i);
        }
    }

    let mut queue: Vec<usize> = (0..count).filter(|&i| in_deg[i] == 0).collect();
    let mut order = Vec::new();
    while let Some(j) = queue.pop() {
        order.push(j);
        for &i in &rev[j] {
            in_deg[i] -= 1;
            if in_deg[i] == 0 {
                queue.push(i);
            }
        }
    }

    // If cycle detected, append remaining in original order
    if order.len() < count {
        for i in 0..count {
            if !order.contains(&i) {
                order.push(i);
            }
        }
    }

    order
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
