use std::io::Write;
use std::process::Command;

use aski_rs::ir;
use aski_rs::grammar;
use aski_rs::grammar::config as grammar_config;

fn read_aski(name: &str) -> String {
    let manifest = env!("CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{manifest}/tests/aski/{name}"))
        .unwrap_or_else(|e| panic!("read {name}: {e}"))
}

fn resolve(sources: &[(&str, &str)]) -> ir::World {
    let config = grammar_config::load_or_bootstrap();
    let mut world = ir::create_world();
    let mut id_offset: i64 = 0;
    for (filename, source) in sources {
        let sf = grammar::parse_source_file_with_config(source, &config)
            .unwrap_or_else(|e| panic!("parse {filename}: {e}"));
        let mut ids = ir::IdGen { next: id_offset + 1 };
        let scope_id = sf.header.as_ref().map(|h| ir::insert_module_header(&mut world, &mut ids, h));
        id_offset = ids.next - 1;
        let count = ir::insert_ast_with_offset(&mut world, &sf.items, id_offset, scope_id)
            .unwrap_or_else(|e| panic!("insert {filename}: {e}"));
        id_offset += count;
    }
    ir::run_rules(&mut world);
    world
}

fn compile_and_run(rust_code: &str, name: &str) -> String {
    let dir = std::env::temp_dir();
    let rs = dir.join(format!("aski_k_{name}.rs"));
    let bin = dir.join(format!("aski_k_{name}_bin"));
    std::fs::File::create(&rs).unwrap().write_all(rust_code.as_bytes()).unwrap();

    let out = Command::new("rustc").arg(&rs).arg("-o").arg(&bin).output();
    let out = match out {
        Ok(o) => o,
        Err(_) => { eprintln!("rustc unavailable"); return String::new(); }
    };
    assert!(out.status.success(),
        "rustc failed:\n{}\n\n{rust_code}", String::from_utf8_lossy(&out.stderr));

    let run = Command::new(&bin).output().expect("run");
    assert!(run.status.success(), "binary failed");
    let _ = std::fs::remove_file(&rs);
    let _ = std::fs::remove_file(&bin);
    String::from_utf8_lossy(&run.stdout).to_string()
}

// ═══════════════════════════════════════════════════════════════
// Step 2: Verify kernel relations have the right data
// ═══════════════════════════════════════════════════════════════

#[test]
fn kernel_struct_in_match_relations() {
    let source = read_aski("struct_in_match.aski");
    let world = resolve(&[("struct_in_match.aski", &source)]);

    // VariantOf: Fire belongs to Element
    let fire_domain = aski_core::query_variant_domain(&world, "Fire");
    assert_eq!(fire_domain.as_ref().map(|(d, _)| d.as_str()), Some("Element"),
        "Fire should belong to Element domain");

    // TypeKind: Element is domain, Color is struct
    assert_eq!(aski_core::query_type_kind(&world, "Element").as_deref(), Some("domain"));
    assert_eq!(aski_core::query_type_kind(&world, "Color").as_deref(), Some("struct"));

    // BindingInfo: the Main bindings should be parsed
    // Find the sub_type_new exprs in Main
    let main_id = world.nodes.iter()
        .find(|n| n.kind == aski_core::NodeKind::Main)
        .map(|n| n.id)
        .expect("Main not found");

    let main_exprs = aski_core::query_child_exprs(&world, main_id);
    eprintln!("Main exprs:");
    for (id, kind, ord, val) in &main_exprs {
        eprintln!("  id={id} kind={kind} ord={ord} val={val:?}");
        if let Some(bi) = aski_core::query_binding_info(&world, *id) {
            eprintln!("    → BindingInfo: var={} type={}", bi.0, bi.1);
        }
    }

    // @Element Element/new(Fire) → BindingInfo("Element", "Element") or sub_type_new
    // @BrightColor Color/new(@Element.bright) → BindingInfo("BrightColor", "Color")
    let bindings: Vec<_> = main_exprs.iter()
        .filter_map(|(id, _, _, _)| aski_core::query_binding_info(&world, *id))
        .collect();
    eprintln!("Bindings: {:?}", bindings);
    assert!(bindings.iter().any(|(_, t)| t == "Color"),
        "should have a binding with type Color");

    // MethodOnType: Element should have 'bright' via colorize impl
    let methods = aski_core::query_methods_on_type(&world, "Element");
    eprintln!("Methods on Element: {:?}", methods);
    assert!(methods.iter().any(|(m, _)| m == "bright"),
        "Element should have 'bright' method");

    // The match arm results should be struct_construct nodes for Color
    // Find the 'bright' method
    let bright_id = methods.iter().find(|(m, _)| m == "bright").map(|(_, id)| *id)
        .expect("bright method not found");
    let arms = aski_core::query_match_arms(&world, bright_id);
    eprintln!("Match arms for bright: {} arms", arms.len());
    assert_eq!(arms.len(), 4, "should have 4 match arms (Fire/Earth/Air/Water)");

    // Each arm body should be a struct_construct for Color
    for (ord, _pats, body_id, _kind) in &arms {
        if let Some(bid) = body_id {
            let (kind, val) = aski_core::query_expr_by_id(&world, *bid)
                .expect("arm body not found");
            eprintln!("  arm {ord}: kind={kind} val={val:?}");
            assert_eq!(kind, "struct_construct", "arm body should be struct_construct");
            assert_eq!(val.as_deref(), Some("Color"), "arm body should construct Color");
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Step 5: End-to-end — kernel → codegen v3 → compile → run
// (v3 doesn't exist yet — this test will be enabled when it does)
// ═══════════════════════════════════════════════════════════════

#[test]
fn e2e_struct_in_match() {
    let source = read_aski("struct_in_match.aski");
    let world = resolve(&[("struct_in_match.aski", &source)]);

    let rust = aski_rs::codegen_v3::generate(&world).expect("codegen_v3 failed");
    eprintln!("=== Generated ===\n{rust}");

    let stdout = compile_and_run(&rust, "struct_match");
    eprintln!("Output: {stdout}");
    // Color.R for Fire should be 0.9
    assert!(stdout.contains("0.9"), "expected 0.9 in output, got: {stdout}");
}
