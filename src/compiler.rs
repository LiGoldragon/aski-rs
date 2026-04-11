//! Multi-file compilation API.
//!
//! Compiles multiple .aski source files together, resolving imports
//! across module boundaries. Each file is parsed directly to a World.

use std::collections::HashMap;
use crate::emit::CodegenConfig;
use crate::grammar::{self, RuleTable, HeaderInfo};
use crate::grammar::config as grammar_config;

/// Compile multiple .aski source files into a single Rust output.
pub fn compile_files(
    sources: &[(&str, &str)],
    config: &CodegenConfig,
) -> Result<String, String> {
    let world = compile_files_to_world(sources)?;
    crate::emit::generate_with_config(&world, config)
}

/// Compile .aski source files and return the populated World (no codegen).
pub fn compile_files_to_world(
    sources: &[(&str, &str)],
) -> Result<aski_core::World, String> {
    let grammar_config = grammar_config::load_or_bootstrap();

    // Phase 0: Extract module headers to determine dependencies
    let headers: Vec<Option<HeaderInfo>> = sources.iter()
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
    let mut parsed_worlds: Vec<(usize, aski_core::World)> = Vec::new();

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

        // Parse with bootstrap + imported grammar rules, get World + user rules
        let (world, user_rules) = grammar::extract_user_grammar_rules_to_world(source, &grammar_config, &extra_rules)
            .map_err(|e| format!("error in {filename}: {e}"))?;

        // Store this module's grammar rules for downstream consumers
        if let Some(ref header) = headers[*idx] {
            let mut all_rules = extra_rules;
            for (name, rule) in user_rules {
                all_rules.insert(name, rule);
            }
            module_rules.insert(header.name.clone(), all_rules);
        }

        parsed_worlds.push((*idx, world));
    }

    // Sort back to original order for stable ID assignment
    parsed_worlds.sort_by_key(|(idx, _)| *idx);

    // Phase 3: Merge all Worlds into a single World with ID remapping
    let mut merged = aski_core::World::default();
    let mut id_offset: i64 = 0;

    for (_, mut file_world) in parsed_worlds {
        if id_offset > 0 {
            // Remap all IDs to avoid collisions between files
            for node in &mut file_world.parse_nodes {
                node.id += id_offset;
                if node.parent_id >= 0 { node.parent_id += id_offset; }
            }
            for child in &mut file_world.parse_children {
                child.parent_id += id_offset;
                child.child_id += id_offset;
            }
            for t in &mut file_world.types { t.id += id_offset; }
            for v in &mut file_world.variants { v.type_id += id_offset; }
            for f in &mut file_world.fields { f.type_id += id_offset; }
        }

        // Track max ID for next file's offset
        if let Some(max_id) = file_world.parse_nodes.iter().map(|n| n.id).max() {
            id_offset = max_id;
        }

        // Merge all relations
        merged.parse_nodes.extend(file_world.parse_nodes);
        merged.parse_children.extend(file_world.parse_children);
        merged.types.extend(file_world.types);
        merged.variants.extend(file_world.variants);
        merged.fields.extend(file_world.fields);
        merged.ffi_entries.extend(file_world.ffi_entries);
        merged.rules.extend(file_world.rules);
        merged.arms.extend(file_world.arms);
        merged.pat_elems.extend(file_world.pat_elems);
        merged.result_elems.extend(file_world.result_elems);
        merged.modules.extend(file_world.modules);
        merged.exports.extend(file_world.exports);
    }

    // Re-run derivation rules on the merged world
    aski_core::run_rules(&mut merged);

    Ok(merged)
}

/// Topological sort of files by import dependencies.
fn topo_sort_files(
    headers: &[Option<HeaderInfo>],
    name_to_idx: &HashMap<String, usize>,
    count: usize,
) -> Vec<usize> {
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

    let mut in_deg = vec![0usize; count];
    for (i, d) in deps.iter().enumerate() {
        in_deg[i] = d.len();
    }

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

    if order.len() < count {
        for i in 0..count {
            if !order.contains(&i) {
                order.push(i);
            }
        }
    }

    order
}

/// Compile multiple .aski source files from the filesystem.
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
