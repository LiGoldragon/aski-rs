use crate::ast::*;
use super::parse_source;

#[test]
fn parse_simple_domain() {
    let items = parse_source("Element (Fire Earth Air Water)").unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Domain(d) => {
            assert_eq!(d.name, "Element");
            assert_eq!(d.variants.len(), 4);
            assert_eq!(d.variants[0].name, "Fire");
            assert_eq!(d.variants[3].name, "Water");
        }
        other => panic!("expected Domain, got {:?}", other),
    }
}

#[test]
fn parse_simple_struct() {
    let items = parse_source("Point { X F64 Y F64 }").unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Struct(s) => {
            assert_eq!(s.name, "Point");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "X");
            assert_eq!(s.fields[1].name, "Y");
        }
        other => panic!("expected Struct, got {:?}", other),
    }
}

#[test]
fn parse_inherent_method() {
    let src = "Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::InherentImpl(ii) => {
            assert_eq!(ii.type_name, "Addition");
            assert_eq!(ii.methods.len(), 1);
            assert_eq!(ii.methods[0].name, "add");
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_const_decl() {
    let items = parse_source("!Pi F64 {3.14159265358979}").unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Const(c) => {
            assert_eq!(c.name, "Pi");
        }
        other => panic!("expected Const, got {:?}", other),
    }
}

#[test]
fn parse_main() {
    let items = parse_source("Main [ StdOut \"hello\" ]").unwrap();
    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0].node, Item::Main(_)));
}

#[test]
fn parse_trait_decl() {
    let src = "classify ([element(:@Self) Element])";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Trait(t) => {
            assert_eq!(t.name, "classify");
            assert_eq!(t.methods.len(), 1);
            assert_eq!(t.methods[0].name, "element");
            assert!(matches!(&t.methods[0].params[0], Param::BorrowSelf));
        }
        other => panic!("expected Trait, got {:?}", other),
    }
}

#[test]
fn parse_multiple_items() {
    let src = "Element (Fire Earth Air Water)\nModality (Cardinal Fixed Mutable)";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 2);
}

// === v0.8 tests ===

#[test]
fn parse_matching_method() {
    let src = r#"Element [
  describe(:@Self) String (|
    (Fire)  "passionate"
    (Earth) "grounded"
    (Air)   "intellectual"
    (Water) "intuitive"
  |)
]"#;
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::InherentImpl(ii) => {
            assert_eq!(ii.type_name, "Element");
            assert_eq!(ii.methods.len(), 1);
            assert_eq!(ii.methods[0].name, "describe");
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 4);
                    assert!(matches!(arms[0].kind, ArmKind::Commit));
                    assert_eq!(arms[0].patterns.len(), 1);
                    assert!(matches!(&arms[0].patterns[0], Pattern::Variant(v) if v == "Fire"));
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_same_type_binding() {
    let src = "Main [ @Radius.new(Value(5.0)) ]";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Main(m) => {
            match &m.body {
                Body::Block(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    assert!(matches!(&stmts[0].node, Expr::SameTypeNew(name, _) if name == "Radius"));
                }
                other => panic!("expected Block, got {:?}", other),
            }
        }
        other => panic!("expected Main, got {:?}", other),
    }
}

#[test]
fn parse_subtype_binding() {
    let src = "Main [ @Area F64.new(42.0) ]";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Main(m) => {
            match &m.body {
                Body::Block(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    assert!(matches!(&stmts[0].node, Expr::SubTypeNew(name, _, _) if name == "Area"));
                }
                other => panic!("expected Block, got {:?}", other),
            }
        }
        other => panic!("expected Main, got {:?}", other),
    }
}

#[test]
fn parse_matching_method_with_wildcard() {
    let src = r#"Element [
  isFire(:@Self) Bool (|
    (Fire) True
    (_)    False
  |)
]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 2);
                    assert!(matches!(&arms[1].patterns[0], Pattern::Wildcard));
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_multi_value_matching() {
    let src = r#"Element [
  classify(:@Self @Modality) String (|
    (Fire Cardinal) "initiator"
    (_ _)           "other"
  |)
]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms[0].patterns.len(), 2);
                    assert!(matches!(&arms[0].patterns[0], Pattern::Variant(v) if v == "Fire"));
                    assert!(matches!(&arms[0].patterns[1], Pattern::Variant(v) if v == "Cardinal"));
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_inline_domain_variant() {
    let src = "Domain (One (A B C) Two)";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Domain(d) => {
            assert_eq!(d.name, "Domain");
            assert_eq!(d.variants.len(), 2);
            assert_eq!(d.variants[0].name, "One");
            assert!(d.variants[0].sub_variants.is_some());
            let subs = d.variants[0].sub_variants.as_ref().unwrap();
            assert_eq!(subs.len(), 3);
            assert_eq!(subs[0].name, "A");
            assert_eq!(subs[1].name, "B");
            assert_eq!(subs[2].name, "C");
            assert_eq!(d.variants[1].name, "Two");
            assert!(d.variants[1].sub_variants.is_none());
        }
        other => panic!("expected Domain, got {:?}", other),
    }
}

#[test]
fn parse_struct_variant() {
    let src = "Shape (Circle (F64) Rectangle { Width F64 Height F64 })";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Domain(d) => {
            assert_eq!(d.name, "Shape");
            assert_eq!(d.variants.len(), 2);
            assert_eq!(d.variants[0].name, "Circle");
            assert!(d.variants[0].wraps.is_some());
            assert_eq!(d.variants[1].name, "Rectangle");
            assert!(d.variants[1].fields.is_some());
            let fields = d.variants[1].fields.as_ref().unwrap();
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name, "Width");
            assert_eq!(fields[1].name, "Height");
        }
        other => panic!("expected Domain, got {:?}", other),
    }
}

#[test]
fn parse_subtype_decl_in_body() {
    // SubTypeDecl (Name Type) parses as a statement — used for local sub-types
    let src = "Main [ Result String ]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Main(m) => {
            match &m.body {
                Body::Block(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    assert!(matches!(&stmts[0].node, Expr::SubTypeDecl(name, _) if name == "Result"));
                }
                other => panic!("expected Block, got {:?}", other),
            }
        }
        other => panic!("expected Main, got {:?}", other),
    }
}

#[test]
fn parse_mutable_binding() {
    let src = "Main [ ~@Counter U32.new(0) ]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Main(m) => {
            match &m.body {
                Body::Block(stmts) => {
                    assert_eq!(stmts.len(), 1);
                    assert!(matches!(&stmts[0].node, Expr::MutableNew(name, _, _) if name == "Counter"));
                }
                other => panic!("expected Block, got {:?}", other),
            }
        }
        other => panic!("expected Main, got {:?}", other),
    }
}

// === v0.9 tests ===

// ;; STATUS: removed from spec — comprehension parser deleted, tests will fail until removed
#[test]
#[ignore]
fn parse_comprehension_source_only() {
    // Simplest comprehension — just source, no output, no guard
    let src = "Main [ [| @AllSigns |] ]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Main(m) => match &m.body {
            Body::Block(stmts) => {
                assert!(matches!(&stmts[0].node, Expr::Comprehension { .. }));
            }
            other => panic!("expected Block, got {:?}", other),
        },
        other => panic!("expected Main, got {:?}", other),
    }
}

// ;; STATUS: removed from spec
#[test]
#[ignore]
fn parse_comprehension_filter() {
    let src = "Main [ [| @AllSigns {@Sign.element == Fire} |] ]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Main(m) => match &m.body {
            Body::Block(stmts) => {
                assert!(matches!(&stmts[0].node, Expr::Comprehension { .. }));
            }
            other => panic!("expected Block, got {:?}", other),
        },
        other => panic!("expected Main, got {:?}", other),
    }
}

// ;; STATUS: removed from spec
#[test]
#[ignore]
fn parse_comprehension_map() {
    let src = "Main [ [| @AllSigns @Sign.element |] ]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Main(m) => match &m.body {
            Body::Block(stmts) => {
                match &stmts[0].node {
                    Expr::Comprehension { source: _, output, guard } => {
                        assert!(output.is_some());
                        assert!(guard.is_none());
                    }
                    other => panic!("expected Comprehension, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        },
        other => panic!("expected Main, got {:?}", other),
    }
}

// ;; STATUS: removed from spec
#[test]
#[ignore]
fn parse_comprehension_filter_map() {
    let src = "Main [ [| @AllSigns @Sign.element {@Sign.modality == Cardinal} |] ]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Main(m) => match &m.body {
            Body::Block(stmts) => {
                match &stmts[0].node {
                    Expr::Comprehension { source: _, output, guard } => {
                        assert!(output.is_some());
                        assert!(guard.is_some());
                    }
                    other => panic!("expected Comprehension, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        },
        other => panic!("expected Main, got {:?}", other),
    }
}

#[test]
fn parse_parameterized_struct_field() {
    let src = "ParseState { Tokens Vec{Token} Position U32 }";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Struct(s) => {
            assert_eq!(s.name, "ParseState");
            assert_eq!(s.fields.len(), 2);
            assert_eq!(s.fields[0].name, "Tokens");
            assert!(matches!(&s.fields[0].type_ref, TypeRef::Parameterized(n, _) if n == "Vec"));
            assert_eq!(s.fields[1].name, "Position");
        }
        other => panic!("expected Struct, got {:?}", other),
    }
}

// === v0.9 pattern tests ===

#[test]
fn parse_or_pattern() {
    let src = r#"Element [
  isFire(:@Self) Bool (|
    ((Fire | Air)) True
    (_) False
  |)
]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 2);
                    match &arms[0].patterns[0] {
                        Pattern::Or(variants) => {
                            assert_eq!(variants.len(), 2);
                            assert!(matches!(&variants[0], Pattern::Variant(v) if v == "Fire"));
                            assert!(matches!(&variants[1], Pattern::Variant(v) if v == "Air"));
                        }
                        other => panic!("expected Or pattern, got {:?}", other),
                    }
                    assert!(matches!(&arms[1].patterns[0], Pattern::Wildcard));
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_nested_pattern() {
    let src = r#"Option [
  unwrap(:@Self) String (|
    (Some(@Value)) @Value
    (_) "none"
  |)
]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 2);
                    match &arms[0].patterns[0] {
                        Pattern::DataCarrying(name, inner) => {
                            assert_eq!(name, "Some");
                            assert!(matches!(inner.as_ref(), Pattern::InstanceBind(n) if n == "Value"));
                        }
                        other => panic!("expected DataCarrying pattern, got {:?}", other),
                    }
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_instance_bind_pattern() {
    let src = r#"Wrapper [
  extract(:@Self) String (|
    (@Inner) @Inner
    (_) "empty"
  |)
]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 2);
                    assert!(matches!(&arms[0].patterns[0], Pattern::InstanceBind(n) if n == "Inner"));
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

// === v0.9 sequence destructure and associated type tests ===

#[test]
fn parse_sequence_destructure() {
    let src = r#"List [
  head(:@Self) String (|
    ((@Head | @Rest)) @Head
    (_) "empty"
  |)
]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 2);
                    match &arms[0].patterns[0] {
                        Pattern::Destructure { head, tail } => {
                            assert_eq!(head.len(), 1);
                            assert!(matches!(&head[0], Pattern::InstanceBind(n) if n == "Head"));
                            assert_eq!(tail.as_deref(), Some("Rest"));
                        }
                        other => panic!("expected Destructure pattern, got {:?}", other),
                    }
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_associated_type_in_impl() {
    // Associated type `Output Point` should be parsed and stored;
    // the method `add` should still be parsed correctly.
    let src = r#"add [Point [
  Output Point
  add(:@Self @Rhs Point) Point [
    ^Point(X([@Self.X + @Rhs.X]) Y([@Self.Y + @Rhs.Y]))
  ]
]]"#;
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::TraitImpl(ti) => {
            assert_eq!(ti.trait_name, "add");
            assert_eq!(ti.impls.len(), 1);
            assert_eq!(ti.impls[0].target, "Point");
            // Associated type is now stored
            assert_eq!(ti.impls[0].associated_types.len(), 1);
            assert_eq!(ti.impls[0].associated_types[0].name, "Output");
            assert!(matches!(&ti.impls[0].associated_types[0].concrete_type, TypeRef::Named(n) if n == "Point"));
            // Method remains
            assert_eq!(ti.impls[0].methods.len(), 1);
            assert_eq!(ti.impls[0].methods[0].name, "add");
        }
        other => panic!("expected TraitImpl, got {:?}", other),
    }
}

#[test]
fn parse_owned_self_param() {
    let src = "String [\n  process(@Self) Bool [\n    ^@Self.len > 0\n  ]\n]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            assert!(matches!(&ii.methods[0].params[0], Param::OwnedSelf));
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn parse_borrowed_struct_field() {
    let src = "Excerpt { Text :String Page U32 }";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Struct(s) => {
            assert_eq!(s.fields.len(), 2);
            assert!(matches!(&s.fields[0].type_ref, TypeRef::Borrowed(_)));
            assert_eq!(s.fields[0].name, "Text");
        }
        other => panic!("expected Struct, got {:?}", other),
    }
}

#[test]
fn parse_supertraits() {
    let src = "describe (display classify [describe(:@Self) String])";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Trait(t) => {
            assert_eq!(t.name, "describe");
            assert_eq!(t.supertraits, vec!["display", "classify"]);
            assert_eq!(t.methods.len(), 1);
        }
        other => panic!("expected Trait, got {:?}", other),
    }
}

#[test]
fn parse_type_alias() {
    let src = "SignList Vec{Sign}";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::TypeAlias(a) => {
            assert_eq!(a.name, "SignList");
            assert!(matches!(&a.target, TypeRef::Parameterized(n, _) if n == "Vec"));
        }
        other => panic!("expected TypeAlias, got {:?}", other),
    }
}

#[test]
fn parse_chart_aski_multi_module() {
    let chart_src = std::fs::read_to_string(
        concat!(env!("CARGO_MANIFEST_DIR"), "/../astro-aski/aski/chart.aski")
    );
    if let Ok(src) = chart_src {
        let sf = crate::parser::parse_source_file(&src).unwrap();
        eprintln!("Header: {:?}", sf.header.as_ref().map(|h| &h.name));
        eprintln!("Items: {}", sf.items.len());
        for item in &sf.items {
            eprintln!("  {:?}", std::mem::discriminant(&item.node));
        }
        assert!(sf.header.is_some(), "should have module header");
        assert!(sf.items.len() >= 10, "should have at least 10 items, got {}", sf.items.len());
    }
}

#[test]
fn parse_module_header() {
    let src = "(Chart Sign Planet computeChart)\n[Core (ParseState Token)]\n\nSign (Aries Taurus)";
    let sf = crate::parser::parse_source_file(src).unwrap();
    let header = sf.header.unwrap();
    assert_eq!(header.name, "Chart");
    assert_eq!(header.exports, vec!["Sign", "Planet", "computeChart"]);
    assert_eq!(header.imports.len(), 1);
    assert_eq!(header.imports[0].module, "Core");
    match &header.imports[0].items {
        crate::ast::ImportItems::Named(items) => {
            assert_eq!(items, &vec!["ParseState", "Token"]);
        }
        _ => panic!("expected Named imports"),
    }
    assert_eq!(sf.items.len(), 1); // Sign domain
}

#[test]
fn parse_headerless_file() {
    let src = "Element (Fire Earth Air Water)";
    let sf = crate::parser::parse_source_file(src).unwrap();
    assert!(sf.header.is_none());
    assert_eq!(sf.items.len(), 1);
}

#[test]
fn parse_destructure_arm() {
    use crate::ast::*;
    // Destructure arm uses PascalCase token variant names — [] delimiters (not {})
    let src = "Tokens [\n  parse(:@Self) Option{Pair{Token Tokens}} (|\n    [PascalIdent LParen @Variants RParen | @Rest]  @Rest\n  |)\n]";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::InherentImpl(ii) => {
            assert_eq!(ii.methods.len(), 1);
            match &ii.methods[0].body {
                Body::MatchBody(arms) => {
                    assert_eq!(arms.len(), 1);
                    assert!(matches!(arms[0].kind, ArmKind::Destructure));
                    let (elements, rest) = arms[0].destructure.as_ref().unwrap();
                    assert_eq!(elements.len(), 4); // PascalIdent LParen @Variants RParen
                    assert_eq!(rest, "Rest");
                }
                other => panic!("expected MatchBody, got {:?}", other),
            }
        }
        other => panic!("expected InherentImpl, got {:?}", other),
    }
}

#[test]
fn debug_header_inline_match() {
    let src = "(Parser ItemResult)\n[Token (Token)\n Tokens (_)]\n\nItemResult (Parsed (Tokens) Failed (Tokens))\n\nTokens [\n  parseItem(@Self) ItemResult [\n    @Done Bool.new(@Self.atEnd)\n    ^(| @Done\n      (True) Failed(@Self)\n      (False) Parsed(@Self.advance)\n    |)\n  ]\n]\n";
    let sf = crate::parser::parse_source_file(src);
    match sf {
        Ok(sf) => {
            eprintln!("Header: {:?}", sf.header.as_ref().map(|h| &h.name));
            eprintln!("Items: {}", sf.items.len());
        }
        Err(e) => eprintln!("PARSE ERROR: {}", e),
    }
}

#[test]
fn debug_no_header_inline_match() {
    let src = "ItemResult (Parsed (Tokens) Failed (Tokens))\n\nTokens [\n  parseItem(@Self) ItemResult [\n    @Done Bool.new(@Self.atEnd)\n    ^(| @Done\n      (True) Failed(@Self)\n      (False) Parsed(@Self.advance)\n    |)\n  ]\n]\n";
    let items = crate::parser::parse_source(src);
    match items {
        Ok(items) => eprintln!("OK: {} items", items.len()),
        Err(e) => eprintln!("PARSE ERROR: {}", e),
    }
}

#[test]
fn debug_inline_match_simple_return() {
    let src = "Tokens { Pos U32 }\n\nTokens [\n  test(@Self) Bool [\n    @Done Bool.new(@Self.Pos >= 10)\n    ^(| @Done\n      (True) True\n      (False) False\n    |)\n  ]\n]\n";
    let items = crate::parser::parse_source(src);
    match items {
        Ok(items) => eprintln!("OK: {} items", items.len()),
        Err(e) => eprintln!("PARSE ERROR: {}", e),
    }
}

#[test]
fn parse_trait_bound_type() {
    // {|display|} as a type reference (single bound)
    let src = "!Test {|display|} {___}";
    let items = parse_source(src).unwrap();
    assert_eq!(items.len(), 1);
    match &items[0].node {
        Item::Const(c) => {
            assert_eq!(c.name, "Test");
            match &c.type_ref {
                TypeRef::Bound(tb) => {
                    assert_eq!(tb.bounds, vec!["display"]);
                    assert_eq!(tb.name, "display");
                }
                other => panic!("expected Bound type, got {:?}", other),
            }
        }
        other => panic!("expected Const, got {:?}", other),
    }
}

#[test]
fn parse_trait_bound_compound() {
    // {|sort&display|} as a type reference (compound bounds)
    let src = "!Test {|sort&display|} {___}";
    let items = parse_source(src).unwrap();
    match &items[0].node {
        Item::Const(c) => {
            match &c.type_ref {
                TypeRef::Bound(tb) => {
                    assert_eq!(tb.bounds, vec!["sort", "display"]);
                    assert_eq!(tb.name, "sort&display");
                }
                other => panic!("expected Bound type, got {:?}", other),
            }
        }
        other => panic!("expected Const, got {:?}", other),
    }
}
