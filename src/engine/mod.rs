//! Full grammar engine parser for aski v0.10.
//!
//! Parses ALL aski syntax, producing the canonical AST types.
//! This is the PRIMARY (and only) parser for aski.
//!
//! Parser decisions (operator precedence, kernel primitives, token
//! classification) are DATA-DRIVEN: loaded from grammar/*.aski files.

mod state;
mod types;
mod expr;
mod pattern;
mod stmt;
mod items;
mod header;
pub mod config;

use crate::ast::*;
use crate::lexer::Token;
use state::{SpannedToken, ParseState};
use items::*;
use header::parse_module_header;
use config::GrammarConfig;

// ── Top-level item dispatcher ───────────────────────────────────────

fn parse_item(st: &mut ParseState) -> Result<Spanned<Item>, String> {
    let start = st.save();
    st.skip_newlines();

    if st.at_end() {
        return Err("unexpected end of input".to_string());
    }

    // Const: !Name Type {value}
    if st.peek() == Some(&Token::Bang) {
        let item = parse_const_decl(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(item, span));
    }

    // Grammar rule: <Name> [...]
    if st.peek() == Some(&Token::Lt) {
        let item = parse_grammar_rule(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(item, span));
    }

    // Main: Main [body]
    if let Some(Token::PascalIdent(s)) = st.peek() {
        if s == "Main" {
            st.advance();
            let item = parse_main_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
    }

    // Domain: PascalName (variants)
    if let Some(Token::PascalIdent(_)) = st.peek() {
        let save = st.save();
        // Peek: Pascal then (
        let (_, _) = st.eat_pascal().unwrap();
        if st.peek() == Some(&Token::LParen) {
            st.restore(save);
            let item = parse_domain_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        // Struct: PascalName { fields }
        if st.peek() == Some(&Token::LBrace) {
            st.restore(save);
            let item = parse_struct_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        // Type alias: PascalName TypeRef (fallback — must come after domain/struct)
        st.restore(save);
        let item = parse_type_alias(st)?;
        let span = st.span_from(start);
        return Ok(Spanned::new(item, span));
    }

    // Trait decl or impl: camelCase
    if let Some(Token::CamelIdent(_)) = st.peek() {
        let save = st.save();
        let (_name, _) = st.eat_camel().unwrap();
        st.skip_newlines();
        if st.peek() == Some(&Token::LParen) {
            // Trait declaration: name ([methods])
            st.restore(save);
            let item = parse_trait_decl(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        if st.peek() == Some(&Token::LBracket) {
            // Trait impl: name [Type [methods]]
            st.restore(save);
            let item = parse_impl_block(st)?;
            let span = st.span_from(start);
            return Ok(Spanned::new(item, span));
        }
        st.restore(save);
    }

    Err(format!(
        "expected item, got {:?} at position {}",
        st.peek(),
        st.pos
    ))
}

// ── Public API ──────────────────────────────────────────────────────

/// Parse aski source code, producing the canonical AST types.
pub fn parse_source(source: &str) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file(source)?;
    Ok(sf.items)
}

/// Parse aski source code with a specific grammar configuration.
pub fn parse_source_with_config(source: &str, config: &GrammarConfig) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file_with_config(source, config)?;
    Ok(sf.items)
}

/// Parse a full source file (with optional module header).
pub fn parse_source_file(source: &str) -> Result<SourceFile, String> {
    let config = config::load_or_bootstrap();
    parse_source_file_with_config(source, &config)
}

/// Parse a full source file with a specific grammar configuration.
pub fn parse_source_file_with_config(source: &str, config: &GrammarConfig) -> Result<SourceFile, String> {
    let spanned_tokens = crate::lexer::lex(source).map_err(|errs| {
        errs.into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let tokens: Vec<SpannedToken> = spanned_tokens
        .into_iter()
        .map(|s| SpannedToken {
            token: s.token,
            span: s.span,
        })
        .collect();

    let st = &mut ParseState::new(&tokens, config);
    st.skip_newlines();

    // Try parsing with module header first
    let header = {
        let save = st.save();
        if st.peek() == Some(&Token::LParen) {
            // Could be module header or domain with ( after something
            match parse_module_header(st) {
                Ok(h) => Some(h),
                Err(_) => {
                    st.restore(save);
                    None
                }
            }
        } else {
            None
        }
    };

    let mut items = Vec::new();
    loop {
        st.skip_newlines();
        if st.at_end() {
            break;
        }
        items.push(parse_item(st)?);
    }

    Ok(SourceFile { header, items })
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ge_parse_simple_domain() {
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
    fn ge_parse_simple_struct() {
        let items = parse_source("Point { Horizontal F64 Vertical F64 }").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Struct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "Horizontal");
            }
            other => panic!("expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_const_decl() {
        let items = parse_source("!Pi F64 {3.14159265358979}").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Const(c) => {
                assert_eq!(c.name, "Pi");
                assert!(matches!(&c.type_ref, TypeRef::Named(n) if n == "F64"));
            }
            other => panic!("expected Const, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_main() {
        let items = parse_source("Main [ StdOut \"hello\" ]").unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(&items[0].node, Item::Main(_)));
    }

    #[test]
    fn ge_parse_trait_decl() {
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
    fn ge_parse_trait_impl() {
        let src = "compute [Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]]";
        let items = parse_source(src).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.trait_name, "compute");
                assert_eq!(ti.impls.len(), 1);
                assert_eq!(ti.impls[0].target, "Addition");
                assert_eq!(ti.impls[0].methods.len(), 1);
                assert_eq!(ti.impls[0].methods[0].name, "add");
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_matching_method() {
        let src = r#"describe [Element [
  describe(:@Self) Element (|
    (Fire)  Fire
    (Earth) Earth
    (Air)   Air
    (Water) Water
  |)
]]"#;
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                match &ti.impls[0].methods[0].body {
                    Body::MatchBody(arms) => {
                        assert_eq!(arms.len(), 4);
                        assert!(matches!(arms[0].kind, ArmKind::Commit));
                    }
                    other => panic!("expected MatchBody, got {:?}", other),
                }
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_same_type_binding() {
        let src = "Main [ @Radius.new(Value(5.0)) ]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::SameTypeNew(name, _) if name == "Radius"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_subtype_binding() {
        let src = "Main [ @Area F64.new(42.0) ]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::SubTypeNew(name, _, _) if name == "Area"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_type_alias() {
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
    fn ge_parse_inline_domain() {
        let src = "Domain (One (A B C) Two)";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.variants.len(), 2);
                assert!(d.variants[0].sub_variants.is_some());
                let subs = d.variants[0].sub_variants.as_ref().unwrap();
                assert_eq!(subs.len(), 3);
            }
            other => panic!("expected Domain, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_struct_variant() {
        let src = "Shape (Circle (F64) Rectangle { Width F64 Height F64 })";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.variants.len(), 2);
                assert!(d.variants[0].wraps.is_some());
                assert!(d.variants[1].fields.is_some());
            }
            other => panic!("expected Domain, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_associated_type() {
        let src = r#"add [Point [
  Output Point
  add(:@Self @Rhs Point) Point [
    ^Point(Horizontal([@Self.Horizontal + @Rhs.Horizontal]) Vertical([@Self.Vertical + @Rhs.Vertical]))
  ]
]]"#;
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.impls[0].associated_types.len(), 1);
                assert_eq!(ti.impls[0].associated_types[0].name, "Output");
                assert_eq!(ti.impls[0].methods.len(), 1);
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_mutable_binding() {
        let src = "Main [ ~@Counter U32.new(0) ]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::MutableNew(name, _, _) if name == "Counter"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_simple_aski_file() {
        let src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/encoder/design/v0.9/examples/simple.aski"
        ))
        .unwrap();
        let items = parse_source(&src).unwrap();
        // simple.aski has many items — verify we parse a reasonable count
        assert!(
            items.len() >= 5,
            "expected at least 5 items from simple.aski, got {}",
            items.len(),
        );
    }

    #[test]
    fn ge_parse_module_header() {
        let src = "(Chart Sign Planet computeChart)\n[Core (ParseState Token)]\n\nSign (Aries Taurus)";
        let sf = parse_source_file(src).unwrap();
        let header = sf.header.unwrap();
        assert_eq!(header.name, "Chart");
        assert_eq!(header.exports, vec!["Sign", "Planet", "computeChart"]);
        assert_eq!(header.imports.len(), 1);
        assert_eq!(header.imports[0].module, "Core");
        assert_eq!(sf.items.len(), 1);
    }

    #[test]
    fn ge_parse_grammar_rule() {
        let src = "<Truncate> [\n  [@Value] @Value.truncate\n]";
        let items = parse_source(src).unwrap();
        assert!(matches!(&items[0].node, Item::GrammarRule(gr) if gr.name == "Truncate"));
    }

    #[test]
    fn ge_parse_destructure_arm() {
        let src = "parse [Tokens [\n  parse(:@Self) Option{Pair{Token Tokens}} (|\n    [PascalIdent LParen @Variants RParen | @Rest]  @Rest\n  |)\n]]";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                match &ti.impls[0].methods[0].body {
                    Body::MatchBody(arms) => {
                        assert!(matches!(arms[0].kind, ArmKind::Destructure));
                        let (elements, rest) = arms[0].destructure.as_ref().unwrap();
                        assert_eq!(elements.len(), 4);
                        assert_eq!(rest, "Rest");
                    }
                    other => panic!("expected MatchBody, got {:?}", other),
                }
            }
            other => panic!("expected TraitImpl, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_borrowed_field() {
        let src = "Excerpt { Text :String Page U32 }";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Struct(s) => {
                assert!(matches!(&s.fields[0].type_ref, TypeRef::Borrowed(_)));
            }
            other => panic!("expected Struct, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_supertraits() {
        let src = "describe (display classify [describe(:@Self) String])";
        let items = parse_source(src).unwrap();
        match &items[0].node {
            Item::Trait(t) => {
                assert_eq!(t.supertraits, vec!["display", "classify"]);
                assert_eq!(t.methods.len(), 1);
            }
            other => panic!("expected Trait, got {:?}", other),
        }
    }

    #[test]
    fn ge_parse_chart_aski_if_available() {
        let chart_src = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../astro-aski/aski/chart.aski"
        ));
        if let Ok(src) = chart_src {
            let sf = parse_source_file(&src).unwrap();
            assert!(sf.header.is_some(), "should have module header");
            assert!(
                sf.items.len() >= 10,
                "expected at least 10 items from chart.aski, got {}",
                sf.items.len(),
            );
        }
    }

    #[test]
    fn ge_data_driven_precedence() {
        // Verify the Pratt parser respects data-driven operator precedence.
        // With normal precedence: 1 + 2 * 3 = 1 + (2 * 3)
        let items = parse_source("Main [ ^(1 + 2 * 3) ]").unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        // The return expr should be: Add(1, Mul(2, 3))
                        match &stmts[0].node {
                            Expr::Return(inner) => {
                                match &inner.node {
                                    Expr::Group(g) => {
                                        match &g.node {
                                            Expr::BinOp(left, BinOp::Add, right) => {
                                                assert!(matches!(&left.node, Expr::IntLit(1)));
                                                assert!(matches!(&right.node, Expr::BinOp(_, BinOp::Mul, _)));
                                            }
                                            other => panic!("expected Add at top, got {:?}", other),
                                        }
                                    }
                                    other => panic!("expected Group, got {:?}", other),
                                }
                            }
                            other => panic!("expected Return, got {:?}", other),
                        }
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }

    #[test]
    fn ge_swapped_precedence_changes_parse() {
        // Create a config where + binds tighter than *
        let mut config = GrammarConfig::bootstrap();
        use config::BindingPower;
        config.set_operator("Plus", BinOp::Add, BindingPower { lbp: 40, rbp: 41 });
        config.set_operator("Star", BinOp::Mul, BindingPower { lbp: 30, rbp: 31 });

        let items = parse_source_with_config("Main [ ^(1 * 2 + 3) ]", &config).unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        match &stmts[0].node {
                            Expr::Return(inner) => {
                                match &inner.node {
                                    Expr::Group(g) => {
                                        // With swapped precedence: 1 * 2 + 3 = 1 * (2 + 3)
                                        // because + now binds tighter
                                        match &g.node {
                                            Expr::BinOp(_, BinOp::Mul, right) => {
                                                assert!(matches!(&right.node, Expr::BinOp(_, BinOp::Add, _)),
                                                    "expected Add inside Mul, got {:?}", right.node);
                                            }
                                            other => panic!("expected Mul at top with swapped prec, got {:?}", other),
                                        }
                                    }
                                    other => panic!("expected Group, got {:?}", other),
                                }
                            }
                            other => panic!("expected Return, got {:?}", other),
                        }
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            other => panic!("expected Main, got {:?}", other),
        }
    }
}
