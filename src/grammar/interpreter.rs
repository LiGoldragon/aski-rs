//! PEG interpreter — executes grammar rules against token streams.
//!
//! Ordered choice with backtracking. First arm that matches wins.
//! Built-in rules: `expression` (Pratt parser), `stmts` (statement list).
//! Newlines are skipped between pattern elements.

use crate::ast::*;
use crate::lexer::{self, Token};
use crate::engine::config::GrammarConfig;
use super::{ParseArm, PatElem, ResultSpec, ResultArg, Value, Bindings, RuleTable};
use super::builders;

/// Spanned token — re-uses the lexer's Spanned type.
pub type SpannedToken = lexer::Spanned;

/// Grammar parser — holds rule table and config, executes rules.
pub struct GrammarParser {
    pub rules: RuleTable,
    pub config: GrammarConfig,
}

impl GrammarParser {
    pub fn new(rules: RuleTable, config: GrammarConfig) -> Self {
        GrammarParser { rules, config }
    }

    /// Parse source text into a SourceFile (optional header + items).
    pub fn parse_source_file(&self, source: &str) -> Result<SourceFile, String> {
        let spanned = crate::lexer::lex(source).map_err(|errs| {
            errs.into_iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ")
        })?;
        let tokens = spanned;

        // Try to parse module header
        let mut pos = 0;
        pos = skip_newlines(&tokens, pos);

        let header = self.try_parse_header(&tokens, pos);
        let (header, pos) = match header {
            Ok((h, new_pos)) => (Some(h), new_pos),
            Err(_) => (None, pos),
        };

        let items = self.parse_items(&tokens, pos)?;

        Ok(SourceFile { header, items })
    }

    /// Parse source text into a list of items (no header).
    pub fn parse_source(&self, source: &str) -> Result<Vec<Spanned<Item>>, String> {
        let spanned = crate::lexer::lex(source).map_err(|errs| {
            errs.into_iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ")
        })?;
        self.parse_items(&spanned, 0)
    }

    /// Parse items from token stream starting at position.
    fn parse_items(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<Vec<Spanned<Item>>, String> {
        let mut items = Vec::new();
        pos = skip_newlines(tokens, pos);

        while pos < tokens.len() {
            let (value, new_pos) = self.try_rule("item", tokens, pos)
                .map_err(|e| format!("at position {}: {}", pos, e))?;
            items.push(value.as_item()?);
            pos = skip_newlines(tokens, new_pos);
        }

        Ok(items)
    }

    /// Try to parse a module header: (name exports) [imports] {constraints}
    fn try_parse_header(&self, tokens: &[SpannedToken], pos: usize) -> Result<(ModuleHeader, usize), String> {
        // Header starts with ( — check if it looks like a header (PascalCase name inside)
        let cur = skip_newlines(tokens, pos);
        if cur >= tokens.len() || tokens[cur].token != Token::LParen {
            return Err("no header".to_string());
        }
        self.parse_module_header_builtin(tokens, cur)
            .and_then(|(v, p)| {
                match v {
                    Value::ModuleHeader(h) => Ok((h, p)),
                    _ => Err("header rule did not produce ModuleHeader".to_string()),
                }
            })
    }

    /// Built-in: module header parser.
    fn parse_module_header_builtin(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos;

        // (Name Export1 Export2 ...)
        expect_token(tokens, cur, &Token::LParen)?;
        cur += 1;
        let name = expect_ident(tokens, cur)?;
        cur += 1;
        let mut exports = Vec::new();
        while cur < tokens.len() && tokens[cur].token != Token::RParen {
            cur = skip_newlines(tokens, cur);
            if tokens[cur].token == Token::RParen { break; }
            let export_name = expect_ident(tokens, cur)?;
            exports.push(export_name);
            cur += 1;
        }
        expect_token(tokens, cur, &Token::RParen)?;
        cur += 1;

        // Optional [imports]
        cur = skip_newlines(tokens, cur);
        let mut imports = Vec::new();
        if cur < tokens.len() && tokens[cur].token == Token::LBracket {
            cur += 1;
            while cur < tokens.len() && tokens[cur].token != Token::RBracket {
                cur = skip_newlines(tokens, cur);
                if tokens[cur].token == Token::RBracket { break; }
                // Module(items...)
                let module_name = expect_ident(tokens, cur)?;
                cur += 1;
                expect_token(tokens, cur, &Token::LParen)?;
                cur += 1;
                let mut items = Vec::new();
                let mut is_wildcard = false;
                while cur < tokens.len() && tokens[cur].token != Token::RParen {
                    cur = skip_newlines(tokens, cur);
                    if tokens[cur].token == Token::RParen { break; }
                    if tokens[cur].token == Token::Star {
                        is_wildcard = true;
                        cur += 1;
                    } else {
                        let item_name = expect_ident(tokens, cur)?;
                        items.push(item_name);
                        cur += 1;
                    }
                }
                expect_token(tokens, cur, &Token::RParen)?;
                cur += 1;
                let import_span = tokens[pos].span.start..tokens[cur.saturating_sub(1)].span.end;
                imports.push(ImportEntry {
                    module: module_name,
                    items: if is_wildcard { ImportItems::Wildcard } else { ImportItems::Named(items) },
                    span: import_span,
                });
            }
            expect_token(tokens, cur, &Token::RBracket)?;
            cur += 1;
        }

        // Optional {constraints}
        cur = skip_newlines(tokens, cur);
        let mut constraints = Vec::new();
        if cur < tokens.len() && tokens[cur].token == Token::LBrace {
            cur += 1;
            while cur < tokens.len() && tokens[cur].token != Token::RBrace {
                cur = skip_newlines(tokens, cur);
                if tokens[cur].token == Token::RBrace { break; }
                let constraint = expect_ident(tokens, cur)?;
                constraints.push(constraint);
                cur += 1;
            }
            expect_token(tokens, cur, &Token::RBrace)?;
            cur += 1;
        }

        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((Value::ModuleHeader(ModuleHeader { name, exports, imports, constraints, span }), cur))
    }

    // ── Core PEG engine ──────────────────────────────────────

    /// Try a grammar rule: ordered choice over arms.
    pub fn try_rule(&self, name: &str, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let pos = skip_newlines(tokens, pos);
        // Grammar rules take priority — try them first if defined
        if self.rules.contains_key(name) {
            let rule = &self.rules[name];
            let mut last_err = String::new();
            for arm in &rule.arms {
                match self.try_arm(arm, tokens, pos) {
                    Ok(result) => return Ok(result),
                    Err(e) => last_err = e,
                }
            }
            return Err(format!("rule <{}> failed at pos {}: {}", name, pos, last_err));
        }

        Err(format!("unknown grammar rule: <{}>", name))
    }

    /// Try a single arm: match pattern, build result.
    fn try_arm(&self, arm: &ParseArm, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let mut bindings = Bindings::new();
        let mut cur = pos;

        for elem in &arm.pattern {
            // Skip newlines only before rule calls — token/bind matching is position-exact.
            // This prevents `.method\n(nextArm)` from being parsed as `.method(nextArm)`.
            if matches!(elem, PatElem::Rule(_)) {
                cur = skip_newlines(tokens, cur);
            }
            match elem {
                PatElem::Tok(name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected token {}, got EOF", name))?;
                    if token_variant_name(&tok.token) != name.as_str() {
                        return Err(format!("expected {}, got {:?}", name, tok.token));
                    }
                    cur += 1;
                }
                PatElem::Lit(value) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected '{}', got EOF", value))?;
                    match &tok.token {
                        Token::PascalIdent(s) if s == value => { cur += 1; }
                        Token::CamelIdent(s) if s == value => { cur += 1; }
                        _ => return Err(format!("expected '{}', got {:?}", value, tok.token)),
                    }
                }
                PatElem::Rule(rule_name) => {
                    let (value, new_pos) = self.try_rule(rule_name, tokens, cur)?;
                    bindings.insert(rule_name.clone(), value);
                    cur = new_pos;
                }
                PatElem::Bind(bind_name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected identifier for @{}, got EOF", bind_name))?;
                    let value = match &tok.token {
                        Token::PascalIdent(s) => Value::Str(s.clone()),
                        Token::CamelIdent(s) => Value::Str(s.clone()),
                        _ => return Err(format!("cannot bind @{} to {:?} (expected identifier)", bind_name, tok.token)),
                    };
                    bindings.insert(bind_name.clone(), value);
                    cur += 1;
                }
                PatElem::BindLit(bind_name) => {
                    let tok = tokens.get(cur)
                        .ok_or_else(|| format!("expected literal for @{}, got EOF", bind_name))?;
                    let value = match &tok.token {
                        Token::Integer(n) => Value::Int(*n),
                        Token::Float(s) => Value::Float(s.parse().unwrap_or(0.0)),
                        Token::StringLit(s) => Value::Str(s.clone()),
                        _ => return Err(format!("cannot bind @{} to {:?} (expected literal)", bind_name, tok.token)),
                    };
                    bindings.insert(bind_name.clone(), value);
                    cur += 1;
                }
            }
        }

        let span = make_span(tokens, pos, cur);
        let value = build_result(&arm.result, &bindings, span)?;
        Ok((value, cur))
    }
}

// ── Helper functions ─────────────────────────────────────────

fn skip_newlines(tokens: &[SpannedToken], mut pos: usize) -> usize {
    while pos < tokens.len() && matches!(tokens[pos].token, Token::Newline | Token::Comment) {
        pos += 1;
    }
    pos
}

fn make_span(tokens: &[SpannedToken], start: usize, end: usize) -> Span {
    if start < tokens.len() && end > 0 && end <= tokens.len() {
        tokens[start].span.start..tokens[end.saturating_sub(1)].span.end
    } else if start < tokens.len() {
        tokens[start].span.clone()
    } else {
        0..0
    }
}

fn token_variant_name(token: &Token) -> &'static str {
    match token {
        Token::Plus => "Plus",
        Token::Minus => "Minus",
        Token::Star => "Star",
        Token::Slash => "Slash",
        Token::Percent => "Percent",
        Token::DoubleEquals => "DoubleEquals",
        Token::NotEqual => "NotEqual",
        Token::LessThan => "LessThan",
        Token::GreaterThan => "GreaterThan",
        Token::LessThanOrEqual => "LessThanOrEqual",
        Token::GreaterThanOrEqual => "GreaterThanOrEqual",
        Token::LogicalAnd => "LogicalAnd",
        Token::LogicalOr => "LogicalOr",
        Token::LParen => "LParen",
        Token::RParen => "RParen",
        Token::LBracket => "LBracket",
        Token::RBracket => "RBracket",
        Token::LBrace => "LBrace",
        Token::RBrace => "RBrace",
        Token::Dot => "Dot",
        Token::At => "At",
        Token::Dollar => "Dollar",
        Token::Caret => "Caret",
        Token::Ampersand => "Ampersand",
        Token::Tilde => "Tilde",
        Token::Question => "Question",
        Token::Bang => "Bang",
        Token::Hash => "Hash",
        Token::Pipe => "Pipe",
        Token::Tick => "Tick",
        Token::Colon => "Colon",
        Token::Comma => "Comma",
        Token::Underscore => "Underscore",
        Token::CompositionOpen => "CompositionOpen",
        Token::CompositionClose => "CompositionClose",
        Token::TraitBoundOpen => "TraitBoundOpen",
        Token::TraitBoundClose => "TraitBoundClose",
        Token::IterOpen => "IterOpen",
        Token::IterClose => "IterClose",
        Token::RangeInclusive => "RangeInclusive",
        Token::RangeExclusive => "RangeExclusive",
        Token::Stub => "Stub",
        Token::Newline => "Newline",
        Token::Equals => "Equals",
        _ => "Unknown",
    }
}

fn expect_token(tokens: &[SpannedToken], pos: usize, expected: &Token) -> Result<(), String> {
    if pos >= tokens.len() {
        return Err(format!("expected {:?}, got EOF", expected));
    }
    if std::mem::discriminant(&tokens[pos].token) != std::mem::discriminant(expected) {
        return Err(format!("expected {:?}, got {:?} at pos {}", expected, tokens[pos].token, pos));
    }
    Ok(())
}

fn expect_ident(tokens: &[SpannedToken], pos: usize) -> Result<String, String> {
    if pos >= tokens.len() {
        return Err("expected identifier, got EOF".to_string());
    }
    match &tokens[pos].token {
        Token::PascalIdent(s) | Token::CamelIdent(s) => Ok(s.clone()),
        other => Err(format!("expected identifier, got {:?}", other)),
    }
}

fn build_result(spec: &ResultSpec, bindings: &Bindings, span: Span) -> Result<Value, String> {
    let resolved: Vec<Value> = spec.args.iter().map(|arg| {
        match arg {
            ResultArg::Bound(name) => bindings.get(name)
                .cloned()
                .ok_or_else(|| format!("unbound: @{}", name)),
            ResultArg::RuleResult(name) => bindings.get(name)
                .cloned()
                .ok_or_else(|| format!("rule result not found: <{}>", name)),
            ResultArg::Nested(nested) => build_result(nested, bindings, span.clone()),
            ResultArg::Literal(s) => Ok(Value::Str(s.clone())),
        }
    }).collect::<Result<Vec<_>, _>>()?;

    builders::construct(&spec.constructor, &resolved, span)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::bootstrap;
    use crate::engine::config;

    fn make_parser() -> GrammarParser {
        let grammar_dir = config::find_grammar_dir().expect("grammar dir");
        let config = config::GrammarConfig::load_from_dir(&grammar_dir)
            .unwrap_or_else(|_| config::GrammarConfig::bootstrap());
        let rules = bootstrap::load_rules(&grammar_dir).unwrap_or_default();
        GrammarParser::new(rules, config)
    }

    #[test]
    fn grammar_parse_domain() {
        let parser = make_parser();
        let items = parser.parse_source("Element (Fire Earth Air Water)").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.name, "Element");
                assert_eq!(d.variants.len(), 4);
                assert_eq!(d.variants[0].name, "Fire");
                assert_eq!(d.variants[3].name, "Water");
            }
            other => panic!("expected Domain, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_struct() {
        let parser = make_parser();
        let items = parser.parse_source("Point { Horizontal F64 Vertical F64 }").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Struct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "Horizontal");
                assert!(matches!(&s.fields[0].type_ref, TypeRef::Named(n) if n == "F64"));
            }
            other => panic!("expected Struct, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_const() {
        let parser = make_parser();
        let items = parser.parse_source("!Pi F64 {3.14159265358979}").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Const(c) => {
                assert_eq!(c.name, "Pi");
                assert!(matches!(&c.type_ref, TypeRef::Named(n) if n == "F64"));
            }
            other => panic!("expected Const, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_main() {
        let parser = make_parser();
        let items = parser.parse_source("Main [ StdOut \"hello\" ]").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Main(m) => match &m.body {
                Body::Block(stmts) => match &stmts[0].node {
                    Expr::StdOut(inner) => {
                        assert!(matches!(&inner.node, Expr::StringLit(s) if s == "hello"),
                            "expected StringLit(\"hello\"), got {:?}", inner.node);
                    }
                    other => panic!("expected StdOut, got {:?}", other),
                }
                other => panic!("expected Block, got {:?}", other),
            }
            _ => panic!("expected Main"),
        }
    }

    #[test]
    fn grammar_parse_trait_decl() {
        let parser = make_parser();
        let items = parser.parse_source("classify ([element(:@Self) Element])").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::Trait(t) => {
                assert_eq!(t.name, "classify");
                assert_eq!(t.methods.len(), 1);
                assert_eq!(t.methods[0].name, "element");
            }
            other => panic!("expected Trait, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_trait_impl() {
        let parser = make_parser();
        let items = parser.parse_source(
            "compute [Addition [ add(:@Self) U32 [ ^(@Self.Left + @Self.Right) ] ]]"
        ).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.trait_name, "compute");
                assert_eq!(ti.impls.len(), 1);
                assert_eq!(ti.impls[0].target, "Addition");
            }
            other => panic!("expected TraitImpl, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_type_alias() {
        let parser = make_parser();
        let items = parser.parse_source("SignList Vec{Sign}").unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::TypeAlias(ta) => {
                assert_eq!(ta.name, "SignList");
                assert!(matches!(&ta.target, TypeRef::Parameterized(n, _) if n == "Vec"));
            }
            other => panic!("expected TypeAlias, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_inline_domain() {
        let parser = make_parser();
        let items = parser.parse_source("Polarity (Solar (Active Positive) Lunar (Receptive Negative))").unwrap();
        match &items[0].node {
            Item::Domain(d) => {
                assert_eq!(d.name, "Polarity");
                assert_eq!(d.variants.len(), 2);
                assert_eq!(d.variants[0].name, "Solar");
                assert!(d.variants[0].sub_variants.is_some());
                let sub = d.variants[0].sub_variants.as_ref().unwrap();
                assert_eq!(sub.len(), 2);
                assert_eq!(sub[0].name, "Active");
            }
            other => panic!("expected Domain, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_matching_body() {
        let parser = make_parser();
        let src = r#"describe [Element [
  describe(:@Self) Element (|
    (Fire)  Fire
    (Earth) Earth
    (Air)   Air
    (Water) Water
  |)
]]"#;
        let items = parser.parse_source(src).unwrap();
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
            other => panic!("expected TraitImpl, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_multi_method_match_body() {
        let parser = make_parser();
        let src = r#"transform [Tokens [
  advance(@Self) Tokens [ ^@Self ]
  skipNewlines(@Self) Tokens [
    @Done Bool/new(@Self.atEnd)
    ^(| @Done
      (True) @Self
      (False) @Self.skipOneNewline
    |)
  ]
  skipOneNewline(@Self) Tokens [ ^@Self ]
]]"#;
        let items = parser.parse_source(src).unwrap();
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.impls[0].methods.len(), 3);
            }
            other => panic!("expected TraitImpl, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_binding_new() {
        let parser = make_parser();
        let items = parser.parse_source("Main [ @Radius/new(Value(5.0)) ]").unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        assert!(matches!(&stmts[0].node, Expr::SameTypeNew(name, _) if name == "Radius"));
                    }
                    other => panic!("expected Block, got {:?}", other),
                }
            }
            _ => panic!("expected Main"),
        }
    }

    #[test]
    fn grammar_parse_multiple_items() {
        let parser = make_parser();
        let src = r#"
Element (Fire Earth Air Water)
Point { Horizontal F64 Vertical F64 }
!Pi F64 {3.14}
"#;
        let items = parser.parse_source(src).unwrap();
        assert_eq!(items.len(), 3);
        assert!(matches!(&items[0].node, Item::Domain(_)));
        assert!(matches!(&items[1].node, Item::Struct(_)));
        assert!(matches!(&items[2].node, Item::Const(_)));
    }

    #[test]
    fn grammar_parse_measure_impl() {
        let parser = make_parser();
        let src = r#"measure [Radius [
  area(:@Self) F64 [
    @Area F64/new([@Self.Value * @Self.Value * !Pi])
    ^@Area
  ]
]]"#;
        let items = parser.parse_source(src).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::TraitImpl(ti) => {
                assert_eq!(ti.trait_name, "measure");
                assert_eq!(ti.impls[0].target, "Radius");
                assert_eq!(ti.impls[0].methods[0].name, "area");
            }
            other => panic!("expected TraitImpl, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_simple_aski_file() {
        let parser = make_parser();
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/simple.aski")
        ).unwrap();
        let sf = parser.parse_source_file(&source).unwrap();
        // simple.aski has no module header
        assert!(sf.header.is_none());
        assert!(sf.items.len() >= 3); // at least domains + structs + traits
    }

    #[test]
    fn grammar_parse_module_header() {
        let parser = make_parser();
        let src = "(Chart Sign Planet)\n[Ephemeris(julianDay longitude)]\nSign (Aries Taurus)";
        let sf = parser.parse_source_file(src).unwrap();
        let header = sf.header.as_ref().expect("should have header");
        assert_eq!(header.name, "Chart");
        assert_eq!(header.exports, vec!["Sign", "Planet"]);
        assert_eq!(header.imports.len(), 1);
        assert_eq!(header.imports[0].module, "Ephemeris");
    }

    #[test]
    fn grammar_parse_data_driven_precedence() {
        let parser = make_parser();
        // 1 + 2 * 3 should parse as 1 + (2 * 3) due to precedence
        let items = parser.parse_source("Main [ ^(1 + 2 * 3) ]").unwrap();
        match &items[0].node {
            Item::Main(m) => {
                match &m.body {
                    Body::Block(stmts) => {
                        match &stmts[0].node {
                            Expr::Return(inner) => {
                                match &inner.node {
                                    Expr::Group(g) => {
                                        match &g.node {
                                            Expr::BinOp(_, BinOp::Addition, rhs) => {
                                                assert!(matches!(&rhs.node, Expr::BinOp(_, BinOp::Multiplication, _)));
                                            }
                                            _ => panic!("expected addition at top"),
                                        }
                                    }
                                    _ => panic!("expected group"),
                                }
                            }
                            _ => panic!("expected return"),
                        }
                    }
                    _ => panic!("expected block"),
                }
            }
            _ => panic!("expected Main"),
        }
    }

    #[test]
    fn grammar_parse_ffi_block() {
        let parser = make_parser();
        let src = r#"{| SwissEphemeris
  julianDay(@Year I32 @Month I32 @Day I32 @Hour F64 @Flag I32) F64 swe_julday
|}"#;
        let items = parser.parse_source(&src).unwrap();
        assert_eq!(items.len(), 1);
        match &items[0].node {
            Item::ForeignBlock(fb) => {
                assert_eq!(fb.library, "SwissEphemeris");
                assert_eq!(fb.functions.len(), 1);
                assert_eq!(fb.functions[0].name, "julianDay");
                assert_eq!(fb.functions[0].extern_name, "swe_julday");
                assert_eq!(fb.functions[0].params.len(), 5);
                assert!(matches!(&fb.functions[0].return_type, TypeRef::Named(n) if n == "F64"));
            }
            other => panic!("expected ForeignBlock, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_parse_ffi_multiple_functions() {
        let parser = make_parser();
        let src = r#"{| SwissEphemeris
  julianDay(@Year I32 @Month I32 @Day I32 @Hour F64 @Flag I32) F64 swe_julday
  calculate(@JulDay F64 @Planet I32 @Flag I64) F64 swe_calc_ut
|}"#;
        let items = parser.parse_source(&src).unwrap();
        match &items[0].node {
            Item::ForeignBlock(fb) => {
                assert_eq!(fb.functions.len(), 2);
                assert_eq!(fb.functions[0].name, "julianDay");
                assert_eq!(fb.functions[1].name, "calculate");
                assert_eq!(fb.functions[1].extern_name, "swe_calc_ut");
            }
            other => panic!("expected ForeignBlock, got {:?}", std::mem::discriminant(other)),
        }
    }

    #[test]
    fn grammar_ffi_codegen() {
        let parser = make_parser();
        let src = r#"{| swisseph
  julianDay(@Year I32 @Month I32 @Day I32 @Hour F64 @Flag I32) F64 swe_julday
|}"#;
        let sf = parser.parse_source_file(src).unwrap();
        let mut world = crate::ir::create_world();
        let mut ids = crate::ir::IdGen { next: 1 };
        for item in &sf.items {
            crate::ir::insert_item_pub(&mut world, &mut ids, &item.node, None, None).unwrap();
        }
        crate::ir::run_rules(&mut world);
        let config = crate::codegen::CodegenConfig { rkyv: false };
        let rust = crate::codegen::generate_rust_from_db_with_config(&world, &config).unwrap();
        assert!(rust.contains("swisseph::swe_julday"), "should call crate function:\n{}", rust);
        assert!(rust.contains("fn julian_day"), "should have wrapper:\n{}", rust);
        assert!(rust.contains("year: i32"), "should have typed params:\n{}", rust);
    }

    #[test]
    fn grammar_parse_bootstrap_tokens_aski() {
        let parser = make_parser();
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("bootstrap/tokens.aski")
        ).unwrap();
        let sf = parser.parse_source_file(&source).unwrap();
        assert!(sf.header.is_some(), "should have module header");
        assert!(sf.items.len() > 3, "should have struct + trait decls + impls");
    }

    #[test]
    fn grammar_type_path_variant() {
        let parser = make_parser();
        let items = parser.parse_source("Main [ ^Sign/Aries ]").unwrap();
        match &items[0].node {
            Item::Main(m) => match &m.body {
                Body::Block(stmts) => match &stmts[0].node {
                    Expr::Return(inner) => match &inner.node {
                        Expr::Access(base, member) => {
                            assert!(matches!(&base.node, Expr::BareName(n) if n == "Sign"));
                            assert_eq!(member, "Aries");
                        }
                        other => panic!("expected Access, got {:?}", other),
                    }
                    _ => panic!("expected Return"),
                }
                _ => panic!("expected Block"),
            }
            _ => panic!("expected Main"),
        }
    }

    #[test]
    fn grammar_type_path_method_call() {
        let parser = make_parser();
        let items = parser.parse_source("Main [ ^Sign/fromDegree(@Lon) ]").unwrap();
        match &items[0].node {
            Item::Main(m) => match &m.body {
                Body::Block(stmts) => match &stmts[0].node {
                    Expr::Return(inner) => match &inner.node {
                        Expr::FnCall(name, args) => {
                            assert_eq!(name, "Sign/fromDegree");
                            assert_eq!(args.len(), 1);
                        }
                        other => panic!("expected FnCall, got {:?}", other),
                    }
                    _ => panic!("expected Return"),
                }
                _ => panic!("expected Block"),
            }
            _ => panic!("expected Main"),
        }
    }

    #[test]
    fn grammar_at_slash_new() {
        let parser = make_parser();
        let items = parser.parse_source("Main [ @Radius/new(Value(5.0)) ]").unwrap();
        match &items[0].node {
            Item::Main(m) => match &m.body {
                Body::Block(stmts) => {
                    assert!(matches!(&stmts[0].node, Expr::SameTypeNew(name, _) if name == "Radius"));
                }
                _ => panic!("expected Block"),
            }
            _ => panic!("expected Main"),
        }
    }
}
