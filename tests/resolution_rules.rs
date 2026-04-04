use std::io::Write;
use std::process::Command;

use aski_rs::ir;
use aski_rs::grammar;
use aski_rs::engine::config as grammar_config;
use aski_rs::codegen_v3;

fn read_aski(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = format!("{manifest}/tests/aski/{name}");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
}

fn resolve_sources(sources: &[(&str, &str)]) -> ir::World {
    let config = grammar_config::load_or_bootstrap();
    let mut world = ir::create_world();
    let mut id_offset: i64 = 0;

    for (filename, source) in sources {
        let sf = grammar::parse_source_file_with_config(source, &config)
            .unwrap_or_else(|e| panic!("parse error in {filename}: {e}"));

        let mut ids = ir::IdGen { next: id_offset + 1 };
        let scope_id = if let Some(ref header) = sf.header {
            Some(ir::insert_module_header(&mut world, &mut ids, header))
        } else {
            None
        };
        id_offset = ids.next - 1;
        let count = ir::insert_ast_with_offset(&mut world, &sf.items, id_offset, scope_id)
            .unwrap_or_else(|e| panic!("insert error in {filename}: {e}"));
        id_offset += count;
    }

    ir::run_rules(&mut world);
    world
}

fn find_node(world: &ir::World, kind: &str, name: &str) -> Option<i64> {
    world.Node.iter()
        .find(|(_, k, n, _, _, _, _)| k == kind && n == name)
        .map(|(id, _, _, _, _, _, _)| *id)
}

fn qualified_name(world: &ir::World, node_id: i64) -> Option<String> {
    world.QualifiedName.iter()
        .find(|(id, _)| *id == node_id)
        .map(|(_, qn)| qn.clone())
}

fn can_see(world: &ir::World, observer: i64, target: i64) -> bool {
    world.CanSee.contains(&(observer, target))
}

/// Validate resolution graph, then generate Rust via codegen_v2.
fn generate_with_resolution(world: &ir::World) -> String {
    // Verify every top-level node has a QualifiedName
    for (id, kind, name) in &ir::query_all_top_level_nodes(world).unwrap() {
        assert!(world.QualifiedName.iter().any(|(nid, _)| *nid == *id),
            "node '{}' (kind={}, id={}) has no QualifiedName", name, kind, id);
    }

    // Verify every method can see its return type
    for (id, kind, name, _, _, _, _) in world.Node.iter() {
        if kind != "method" { continue; }
        if let Some(ret_type) = ir::query_return_type(world, *id).unwrap() {
            if let Some((type_id, _, type_name, _, _, _, _)) = world.Node.iter()
                .find(|(_, k, n, _, _, _, _)| (k == "domain" || k == "struct") && *n == ret_type)
            {
                assert!(world.CanSee.contains(&(*id, *type_id)),
                    "method '{}' returns '{}' but cannot see type '{}' (id={})",
                    name, ret_type, type_name, type_id);
            }
        }
    }

    // Generate via v3 codegen — reads kernel relations only
    codegen_v3::generate(world).expect("codegen_v3 failed")
}

fn compile_and_run(rust_code: &str, test_name: &str) -> String {
    let dir = std::env::temp_dir();
    let rs_path = dir.join(format!("aski_{test_name}.rs"));
    let bin_path = dir.join(format!("aski_{test_name}_bin"));

    {
        let mut f = std::fs::File::create(&rs_path).expect("create rs");
        f.write_all(rust_code.as_bytes()).expect("write rs");
    }

    let compile = Command::new("rustc")
        .arg(&rs_path)
        .arg("-o")
        .arg(&bin_path)
        .output();

    let output = match compile {
        Ok(o) => o,
        Err(_) => {
            eprintln!("rustc not available — skipping");
            let _ = std::fs::remove_file(&rs_path);
            return String::new();
        }
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(),
        "rustc failed:\n{stderr}\n\nGenerated:\n{rust_code}");

    let run = Command::new(&bin_path).output().expect("run binary");
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    assert!(run.status.success(), "binary failed");

    let _ = std::fs::remove_file(&rs_path);
    let _ = std::fs::remove_file(&bin_path);

    stdout
}

// ═══════════════════════════════════════════════════════════════
// QualifiedName
// ═══════════════════════════════════════════════════════════════

#[test]
fn qualified_names_simple_aski() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let source = std::fs::read_to_string(format!("{manifest}/examples/simple.aski")).unwrap();
    let world = resolve_sources(&[("simple.aski", &source)]);

    let element_id = find_node(&world, "domain", "Element").expect("Element");
    let point_id = find_node(&world, "struct", "Point").expect("Point");

    assert_eq!(qualified_name(&world, element_id).as_deref(), Some("Element"));
    assert_eq!(qualified_name(&world, point_id).as_deref(), Some("Point"));
}

#[test]
fn qualified_names_scoped_modules() {
    let chart = read_aski("chart.aski");
    let world = resolve_sources(&[("chart.aski", &chart)]);

    let element_id = find_node(&world, "domain", "Element").expect("Element");
    let sign_id = find_node(&world, "domain", "Sign").expect("Sign");

    assert_eq!(qualified_name(&world, element_id).as_deref(), Some("Chart::Element"));
    assert_eq!(qualified_name(&world, sign_id).as_deref(), Some("Chart::Sign"));
}

// ═══════════════════════════════════════════════════════════════
// CanSee
// ═══════════════════════════════════════════════════════════════

#[test]
fn siblings_see_each_other() {
    let chart = read_aski("chart.aski");
    let world = resolve_sources(&[("chart.aski", &chart)]);

    let element_id = find_node(&world, "domain", "Element").expect("Element");
    let sign_id = find_node(&world, "domain", "Sign").expect("Sign");

    assert!(can_see(&world, element_id, sign_id), "Element should see Sign");
    assert!(can_see(&world, sign_id, element_id), "Sign should see Element");
}

#[test]
fn method_inherits_parent_visibility() {
    let chart = read_aski("chart.aski");
    let world = resolve_sources(&[("chart.aski", &chart)]);

    let element_id = find_node(&world, "domain", "Element").expect("Element");

    let method_nodes: Vec<i64> = world.Node.iter()
        .filter(|(_, k, n, _, _, _, _)| k == "method" && n == "element")
        .map(|(id, _, _, _, _, _, _)| *id)
        .collect();
    assert!(!method_nodes.is_empty());

    for mid in &method_nodes {
        assert!(can_see(&world, *mid, element_id),
            "element method should see Element domain");
    }
}

#[test]
fn cross_module_visibility_via_import() {
    let chart = read_aski("chart.aski");
    let main = read_aski("main.aski");
    let world = resolve_sources(&[("chart.aski", &chart), ("main.aski", &main)]);

    let sign_id = find_node(&world, "domain", "Sign").expect("Sign");
    let element_id = find_node(&world, "domain", "Element").expect("Element");
    let main_id = find_node(&world, "main", "main")
        .or_else(|| find_node(&world, "main", "Main"))
        .expect("Main");

    assert!(can_see(&world, main_id, sign_id), "Main should see Sign");
    assert!(can_see(&world, main_id, element_id), "Main should see Element");
}

// ═══════════════════════════════════════════════════════════════
// End-to-end: resolve → codegen → compile → run
// ═══════════════════════════════════════════════════════════════

#[test]
fn end_to_end_single_module() {
    let source = read_aski("single_module.aski");
    let world = resolve_sources(&[("single_module.aski", &source)]);
    let rust = generate_with_resolution(&world);
    let stdout = compile_and_run(&rust, "single_module");
    assert!(stdout.contains("Fire"), "expected Fire, got: {stdout}");
}

#[test]
fn end_to_end_multi_module() {
    let chart = read_aski("chart.aski");
    let main = read_aski("main.aski");
    let world = resolve_sources(&[("chart.aski", &chart), ("main.aski", &main)]);

    let sign_id = find_node(&world, "domain", "Sign").expect("Sign");
    assert_eq!(qualified_name(&world, sign_id).as_deref(), Some("Chart::Sign"));

    let main_id = find_node(&world, "main", "main")
        .or_else(|| find_node(&world, "main", "Main"))
        .expect("Main");
    assert!(can_see(&world, main_id, sign_id), "Main must see Sign");

    let rust = generate_with_resolution(&world);
    let stdout = compile_and_run(&rust, "multi_module");
    assert!(stdout.contains("Fire"), "Aries.element should be Fire, got: {stdout}");
}
