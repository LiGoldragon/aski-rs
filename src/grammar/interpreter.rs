//! PEG interpreter — executes grammar rules against token streams.
//!
//! Ordered choice with backtracking. First arm that matches wins.
//! Built-in rules: `expression` (Pratt parser), `stmts` (statement list).
//! Newlines are skipped between pattern elements.

use crate::ast::*;
use crate::lexer::{self, Token};
use crate::engine::config::GrammarConfig;
use super::{ParseArm, PatElem, ResultSpec, ResultArg, Value, Bindings, RuleTable, ForeignBlockDecl};
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

    /// Try grammar-defined item rules.
    /// Iterates over all rules in the table; any that produce a Value::Item wins.
    /// This is the extension mechanism: new item forms defined in .aski grammar files.
    fn try_grammar_item(&self, tokens: &[SpannedToken], pos: usize) -> Option<(Value, usize)> {
        // Special case: {| starts an FFI block — try <ffiBlock> or similar rules
        // More generally: try any rule that can produce an item from this position.
        // We check rules that are known item-level non-terminals.
        let item_rules: Vec<&str> = self.rules.keys()
            .filter(|name| {
                // Convention: rules that produce items at top level
                // Currently: ffiBlock is the only one, but any rule that
                // the first token can match is a candidate
                name.ends_with("Block") || name.ends_with("Decl") || name.ends_with("Item")
            })
            .map(|s| s.as_str())
            .collect();

        for rule_name in item_rules {
            if let Some(rule) = self.rules.get(rule_name) {
                for arm in &rule.arms {
                    if let Ok((val, new_pos)) = self.try_arm(arm, tokens, pos) {
                        if matches!(&val, Value::Item(_)) {
                            return Some((val, new_pos));
                        }
                    }
                }
            }
        }

        // Also try: if the current token is TraitBoundOpen {|, look for ffi rules
        if pos < tokens.len() && tokens[pos].token == Token::TraitBoundOpen {
            // Try parsing as FFI block inline: {| Library <ffiFunctions> |}
            if let Ok((val, new_pos)) = self.parse_ffi_block(tokens, pos) {
                return Some((val, new_pos));
            }
        }

        None
    }

    /// Parse FFI block: {| Library funcDecl... |}
    /// Delegates to grammar rules for function declarations.
    fn parse_ffi_block(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos;
        expect_token(tokens, cur, &Token::TraitBoundOpen)?;
        cur += 1;
        cur = skip_newlines(tokens, cur);

        // Library name (PascalCase)
        let library = expect_ident(tokens, cur)?;
        cur += 1;
        cur = skip_newlines(tokens, cur);

        // Parse function declarations via grammar rules
        let (funcs_val, new_pos) = self.try_rule("ffiFunctions", tokens, cur)?;
        cur = skip_newlines(tokens, new_pos);

        expect_token(tokens, cur, &Token::TraitBoundClose)?;
        let span = start..tokens[cur].span.end;
        cur += 1;

        let functions = funcs_val.into_list()?
            .into_iter()
            .map(|v| v.as_foreign_function())
            .collect::<Result<Vec<_>, _>>()?;

        Ok((Value::Item(Spanned::new(
            Item::ForeignBlock(ForeignBlockDecl { library, functions, span: span.clone() }),
            span,
        )), cur))
    }

    // ── Core PEG engine ──────────────────────────────────────

    /// Try a grammar rule: ordered choice over arms.
    pub fn try_rule(&self, name: &str, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        // Built-in rules
        match name {
            "item" => return self.parse_item(tokens, pos),
            "expression" => return self.parse_expression(tokens, pos),
            "stmts" => return self.parse_stmt_list(tokens, pos),
            "matchMethodArms" => return self.parse_match_method_arm_list(tokens, pos),
            "matchExprArms" => return self.parse_match_expr_arm_list(tokens, pos),
            "methodSigs" => return self.parse_method_sig_list(tokens, pos),
            "typeImpls" => return self.parse_type_impl_list(tokens, pos),
            _ => {}
        }

        let rule = self.rules.get(name)
            .ok_or_else(|| format!("unknown grammar rule: <{}>", name))?;

        let mut last_err = String::new();
        for arm in &rule.arms {
            match self.try_arm(arm, tokens, pos) {
                Ok(result) => return Ok(result),
                Err(e) => last_err = e,
            }
        }

        Err(format!("rule <{}> failed at pos {}: {}", name, pos, last_err))
    }

    /// Try a single arm: match pattern, build result.
    fn try_arm(&self, arm: &ParseArm, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let mut bindings = Bindings::new();
        let mut cur = pos;

        for elem in &arm.pattern {
            cur = skip_newlines(tokens, cur);

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
                        .ok_or_else(|| format!("expected bindable token for @{}, got EOF", bind_name))?;
                    let value = match &tok.token {
                        Token::PascalIdent(s) => Value::Str(s.clone()),
                        Token::CamelIdent(s) => Value::Str(s.clone()),
                        Token::Integer(n) => Value::Int(*n),
                        Token::Float(s) => Value::Float(s.parse().unwrap_or(0.0)),
                        Token::StringLit(s) => Value::Str(s.clone()),
                        _ => return Err(format!("cannot bind @{} to {:?}", bind_name, tok.token)),
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

    // ── Built-in: item dispatcher ────────────────────────────

    fn parse_item(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let cur = skip_newlines(tokens, pos);
        if cur >= tokens.len() {
            return Err("expected item, got EOF".to_string());
        }

        // Try grammar-defined item rules FIRST.
        // This is how new syntax (FFI, macros, etc.) is defined in .aski files
        // without touching the Rust interpreter.
        if let Some((val, new_pos)) = self.try_grammar_item(tokens, cur) {
            return Ok((val, new_pos));
        }

        let start = tokens[cur].span.start;

        match &tokens[cur].token {
            // !Name Type {value} — constant
            Token::Bang => {
                let (cst, new_pos) = self.parse_const_decl(tokens, cur)?;
                let span = start..tokens[new_pos.saturating_sub(1)].span.end;
                Ok((Value::Item(Spanned::new(Item::Const(cst), span)), new_pos))
            }
            // <Name> [...] — grammar rule
            Token::LessThan => {
                let (rule, new_pos) = self.parse_grammar_rule_item(tokens, cur)?;
                let span = start..tokens[new_pos.saturating_sub(1)].span.end;
                Ok((Value::Item(Spanned::new(Item::GrammarRule(rule), span)), new_pos))
            }
            // PascalCase — domain, struct, main, type alias
            Token::PascalIdent(name) => {
                let name = name.clone();
                if name == "Main" {
                    // Main [body]
                    let (body, new_pos) = self.parse_body(tokens, cur + 1)?;
                    let span = start..tokens[new_pos.saturating_sub(1)].span.end;
                    return Ok((Value::Item(Spanned::new(
                        Item::Main(MainDecl { body, span: span.clone() }),
                        span,
                    )), new_pos));
                }
                let after_name = skip_newlines(tokens, cur + 1);
                if after_name >= tokens.len() {
                    return Err(format!("unexpected EOF after '{}'", name));
                }
                match &tokens[after_name].token {
                    // Domain: Name (variants)
                    Token::LParen => {
                        let mut inner = after_name + 1;
                        let (variants_val, new_pos) = self.try_rule("variants", tokens, inner)?;
                        inner = skip_newlines(tokens, new_pos);
                        expect_token(tokens, inner, &Token::RParen)?;
                        let variants = variants_val.into_list()?
                            .into_iter().map(|v| v.as_variant()).collect::<Result<Vec<_>, _>>()?;
                        let span = start..tokens[inner].span.end;
                        Ok((Value::Item(Spanned::new(
                            Item::Domain(DomainDecl { name, variants, span: span.clone() }),
                            span,
                        )), inner + 1))
                    }
                    // Struct: Name {fields}
                    Token::LBrace => {
                        let mut inner = after_name + 1;
                        let (fields_val, new_pos) = self.try_rule("fields", tokens, inner)?;
                        inner = skip_newlines(tokens, new_pos);
                        expect_token(tokens, inner, &Token::RBrace)?;
                        let fields = fields_val.into_list()?
                            .into_iter().map(|v| v.as_field()).collect::<Result<Vec<_>, _>>()?;
                        let span = start..tokens[inner].span.end;
                        Ok((Value::Item(Spanned::new(
                            Item::Struct(StructDecl { name, fields, span: span.clone() }),
                            span,
                        )), inner + 1))
                    }
                    // Type alias: Name TypeRef
                    _ => {
                        let (tr, new_pos) = self.try_rule("typeRef", tokens, after_name)?;
                        let span = start..tokens[new_pos.saturating_sub(1)].span.end;
                        Ok((Value::Item(Spanned::new(
                            Item::TypeAlias(TypeAliasDecl { name, target: tr.as_type_ref()?, span: span.clone() }),
                            span,
                        )), new_pos))
                    }
                }
            }
            // camelCase — trait decl or trait impl
            Token::CamelIdent(name) => {
                let name = name.clone();
                let after_name = skip_newlines(tokens, cur + 1);
                if after_name >= tokens.len() {
                    return Err(format!("unexpected EOF after '{}'", name));
                }
                match &tokens[after_name].token {
                    // Trait decl: name (supertraits...) ([methods])
                    Token::LParen => {
                        self.parse_trait_decl_item(tokens, cur, &name)
                    }
                    // Trait impl: name [type-impls]
                    Token::LBracket => {
                        let mut inner = after_name + 1;
                        let (impls_val, new_pos) = self.parse_type_impl_list(tokens, inner)?;
                        inner = skip_newlines(tokens, new_pos);
                        expect_token(tokens, inner, &Token::RBracket)?;
                        let impls = impls_val.into_list()?
                            .into_iter().map(|v| v.as_type_impl()).collect::<Result<Vec<_>, _>>()?;
                        let span = start..tokens[inner].span.end;
                        Ok((Value::Item(Spanned::new(
                            Item::TraitImpl(TraitImplDecl { trait_name: name, impls, span: span.clone() }),
                            span,
                        )), inner + 1))
                    }
                    other => Err(format!("expected ( or [ after trait name '{}', got {:?}", name, other)),
                }
            }
            other => Err(format!("expected item, got {:?} at pos {}", other, cur)),
        }
    }

    fn parse_trait_decl_item(&self, tokens: &[SpannedToken], pos: usize, name: &str) -> Result<(Value, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = skip_newlines(tokens, pos + 1); // after name

        // Parse supertraits: check for Ampersand-separated names before (
        let mut supertraits = Vec::new();
        // Supertraits come after the name: name &Super1 &Super2 ([methods])
        // Actually in aski: name (supertrait1 &supertrait2) ([methods])
        // Wait — let me check the actual syntax...
        // From the existing parser: supertraits are parsed before the paren group
        // In v0.10: traitName superTrait1 &superTrait2 ([methods])
        // Actually from the test: "classify ([element(:@Self) Element])"
        // And: supertraits test shows trait declarations with supertraits

        // Opening ( for trait body/supertraits
        expect_token(tokens, cur, &Token::LParen)?;
        cur += 1;
        cur = skip_newlines(tokens, cur);

        // Check if content starts with [ — pure method list, no supertraits
        if cur < tokens.len() && tokens[cur].token == Token::LBracket {
            // Parse method sigs inside [...]
            cur += 1;
            let (sigs_val, new_pos) = self.parse_method_sig_list(tokens, cur)?;
            cur = skip_newlines(tokens, new_pos);
            expect_token(tokens, cur, &Token::RBracket)?;
            cur += 1;
            cur = skip_newlines(tokens, cur);
            expect_token(tokens, cur, &Token::RParen)?;
            cur += 1;

            let members = match sigs_val {
                Value::List(items) => items,
                _ => vec![],
            };

            let mut method_sigs = Vec::new();
            let mut constants = Vec::new();
            for m in members {
                match m {
                    Value::MethodSig(s) => method_sigs.push(s),
                    Value::ConstDecl(c) => constants.push(c),
                    _ => {}
                }
            }

            let span = start..tokens[cur.saturating_sub(1)].span.end;
            return Ok((Value::Item(Spanned::new(
                Item::Trait(TraitDecl { name: name.to_string(), supertraits, methods: method_sigs, constants, span: span.clone() }),
                span,
            )), cur));
        }

        // Supertraits: space-separated camelCase names before [methods]
        // e.g., describe (display classify [describe(:@Self) String])
        // → supertraits = ["display", "classify"]
        loop {
            cur = skip_newlines(tokens, cur);
            if cur >= tokens.len() { break; }

            let st_name = match &tokens[cur].token {
                Token::CamelIdent(s) => Some(s.clone()),
                Token::PascalIdent(s) => Some(s.clone()),
                _ => None,
            };

            if let Some(name) = st_name {
                // Peek ahead: if followed by another camel/pascal or [, it's a supertrait
                let next = skip_newlines(tokens, cur + 1);
                if next < tokens.len() {
                    match &tokens[next].token {
                        Token::LBracket | Token::CamelIdent(_) | Token::PascalIdent(_) => {
                            supertraits.push(name);
                            cur += 1;
                            continue;
                        }
                        _ => {}
                    }
                }
                // Not followed by [ or another name — not a supertrait, break
                break;
            } else {
                break;
            }
        }

        // Now parse [methods] inside the parens
        cur = skip_newlines(tokens, cur);
        let mut method_sigs = Vec::new();
        let mut constants = Vec::new();
        if cur < tokens.len() && tokens[cur].token == Token::LBracket {
            cur += 1;
            let (sigs_val, new_pos) = self.parse_method_sig_list(tokens, cur)?;
            cur = skip_newlines(tokens, new_pos);
            expect_token(tokens, cur, &Token::RBracket)?;
            cur += 1;

            let members = match sigs_val {
                Value::List(items) => items,
                _ => vec![],
            };
            for m in members {
                match m {
                    Value::MethodSig(s) => method_sigs.push(s),
                    Value::ConstDecl(c) => constants.push(c),
                    _ => {}
                }
            }
        }

        cur = skip_newlines(tokens, cur);
        expect_token(tokens, cur, &Token::RParen)?;
        cur += 1;

        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((Value::Item(Spanned::new(
            Item::Trait(TraitDecl { name: name.to_string(), supertraits, methods: method_sigs, constants, span: span.clone() }),
            span,
        )), cur))
    }

    fn parse_grammar_rule_item(&self, tokens: &[SpannedToken], pos: usize) -> Result<(GrammarRule, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos;
        // <
        expect_token(tokens, cur, &Token::LessThan)?;
        cur += 1;
        let name = expect_ident(tokens, cur)?;
        cur += 1;
        expect_token(tokens, cur, &Token::GreaterThan)?;
        cur += 1;

        // [arms]
        cur = skip_newlines(tokens, cur);
        expect_token(tokens, cur, &Token::LBracket)?;
        cur += 1;

        let mut arms = Vec::new();
        loop {
            cur = skip_newlines(tokens, cur);
            if cur >= tokens.len() || tokens[cur].token == Token::RBracket { break; }
            // Each arm: [pattern | @Rest] result @Rest
            if tokens[cur].token != Token::LBracket { break; }
            let arm_start = tokens[cur].span.start;
            cur += 1;

            let mut pattern = Vec::new();
            while cur < tokens.len() && tokens[cur].token != Token::Pipe && tokens[cur].token != Token::RBracket {
                // Parse grammar element
                match &tokens[cur].token {
                    Token::LessThan => {
                        cur += 1;
                        let nt_name = expect_ident(tokens, cur)?;
                        cur += 1;
                        expect_token(tokens, cur, &Token::GreaterThan)?;
                        cur += 1;
                        pattern.push(GrammarElement::NonTerminal(nt_name));
                    }
                    Token::At => {
                        cur += 1;
                        let bind_name = expect_ident(tokens, cur)?;
                        cur += 1;
                        pattern.push(GrammarElement::Binding(bind_name));
                    }
                    Token::PascalIdent(s) => {
                        pattern.push(GrammarElement::Terminal(s.clone()));
                        cur += 1;
                    }
                    _ => {
                        cur += 1;
                    }
                }
            }

            // | @Rest
            if cur < tokens.len() && tokens[cur].token == Token::Pipe {
                cur += 1;
                if cur < tokens.len() && tokens[cur].token == Token::At {
                    cur += 1;
                    let rest_name = expect_ident(tokens, cur)?;
                    cur += 1;
                    pattern.push(GrammarElement::Rest(rest_name));
                }
            }

            // ]
            expect_token(tokens, cur, &Token::RBracket)?;
            cur += 1;

            // Result: collect expressions until next [ or ]
            let mut result = Vec::new();
            while cur < tokens.len() && tokens[cur].token != Token::LBracket && tokens[cur].token != Token::RBracket {
                cur = skip_newlines(tokens, cur);
                if cur >= tokens.len() || tokens[cur].token == Token::LBracket || tokens[cur].token == Token::RBracket { break; }
                let (expr, new_pos) = self.parse_expression(tokens, cur)?;
                result.push(expr.as_expr()?);
                cur = new_pos;
            }

            let arm_span = arm_start..tokens[cur.saturating_sub(1)].span.end;
            arms.push(GrammarArm { pattern, result, span: arm_span });
        }

        expect_token(tokens, cur, &Token::RBracket)?;
        cur += 1;

        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((GrammarRule { name, arms, span }, cur))
    }

    // ── Built-in: module header ──────────────────────────────

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

    // ── Built-in: Pratt expression parser ────────────────────

    fn parse_expression(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let (expr, new_pos) = self.parse_expr_bp(tokens, pos, 0)?;
        Ok((Value::Expr(expr), new_pos))
    }

    fn parse_expr_bp(&self, tokens: &[SpannedToken], pos: usize, min_bp: u8) -> Result<(Spanned<Expr>, usize), String> {
        let (mut lhs, mut cur) = self.parse_postfix(tokens, pos)?;

        loop {
            cur = skip_newlines(tokens, cur);
            if cur >= tokens.len() { break; }

            let tok = &tokens[cur].token;
            if let Some((op, bp)) = self.config.operator_bp(tok) {
                if bp.lbp < min_bp { break; }
                cur += 1;
                let (rhs, new_pos) = self.parse_expr_bp(tokens, cur, bp.rbp)?;
                let span = lhs.span.start..rhs.span.end;
                lhs = Spanned::new(Expr::BinOp(Box::new(lhs), op, Box::new(rhs)), span);
                cur = new_pos;
            } else {
                break;
            }
        }

        Ok((lhs, cur))
    }

    fn parse_postfix(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Spanned<Expr>, usize), String> {
        let (mut expr, mut cur) = self.parse_atom(tokens, pos)?;

        loop {
            // No newline skipping for postfix — must be on same line
            if cur >= tokens.len() { break; }

            match &tokens[cur].token {
                Token::Dot => {
                    cur += 1;
                    let member_pos = skip_newlines(tokens, cur);
                    if member_pos >= tokens.len() { break; }
                    match &tokens[member_pos].token {
                        Token::CamelIdent(name) => {
                            let name = name.clone();
                            cur = member_pos + 1;
                            // Check for method call: .name(args)
                            if cur < tokens.len() && tokens[cur].token == Token::LParen {
                                cur += 1;
                                let (args, new_pos) = self.parse_call_args(tokens, cur)?;
                                cur = new_pos;
                                let span = expr.span.start..tokens[cur - 1].span.end;
                                expr = Spanned::new(
                                    Expr::MethodCall(Box::new(expr), name, args),
                                    span,
                                );
                            } else {
                                let span = expr.span.start..tokens[member_pos].span.end;
                                expr = Spanned::new(
                                    Expr::Access(Box::new(expr), name),
                                    span,
                                );
                            }
                        }
                        Token::PascalIdent(name) => {
                            let name = name.clone();
                            cur = member_pos + 1;
                            let span = expr.span.start..tokens[member_pos].span.end;
                            expr = Spanned::new(
                                Expr::Access(Box::new(expr), name),
                                span,
                            );
                        }
                        _ => break,
                    }
                }
                Token::Question => {
                    let span = expr.span.start..tokens[cur].span.end;
                    expr = Spanned::new(Expr::ErrorProp(Box::new(expr)), span);
                    cur += 1;
                }
                Token::RangeExclusive => {
                    cur += 1;
                    let (end_expr, new_pos) = self.parse_atom(tokens, cur)?;
                    let span = expr.span.start..end_expr.span.end;
                    expr = Spanned::new(Expr::Range {
                        start: Box::new(expr),
                        end: Box::new(end_expr),
                        inclusive: false,
                    }, span);
                    cur = new_pos;
                }
                Token::RangeInclusive => {
                    cur += 1;
                    let (end_expr, new_pos) = self.parse_atom(tokens, cur)?;
                    let span = expr.span.start..end_expr.span.end;
                    expr = Spanned::new(Expr::Range {
                        start: Box::new(expr),
                        end: Box::new(end_expr),
                        inclusive: true,
                    }, span);
                    cur = new_pos;
                }
                _ => break,
            }
        }

        Ok((expr, cur))
    }

    fn parse_atom(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Spanned<Expr>, usize), String> {
        let cur = skip_newlines(tokens, pos);
        if cur >= tokens.len() {
            return Err("expected expression, got EOF".to_string());
        }

        let start = tokens[cur].span.start;

        match &tokens[cur].token {
            // ___ stub
            Token::Stub => {
                let span = tokens[cur].span.clone();
                Ok((Spanned::new(Expr::Stub, span), cur + 1))
            }
            // !Name — const ref
            Token::Bang => {
                let next = cur + 1;
                let name = expect_ident(tokens, next)?;
                let span = start..tokens[next].span.end;
                Ok((Spanned::new(Expr::ConstRef(name), span), next + 1))
            }
            // ^expr — return
            Token::Caret => {
                let (inner, new_pos) = self.parse_expression(tokens, cur + 1)?;
                let span = start..inner.as_expr()?.span.end;
                let inner_expr = inner.as_expr()?;
                Ok((Spanned::new(Expr::Return(Box::new(inner_expr)), span), new_pos))
            }
            // #expr — yield
            Token::Hash => {
                let (inner, new_pos) = self.parse_expression(tokens, cur + 1)?;
                let span = start..inner.as_expr()?.span.end;
                let inner_expr = inner.as_expr()?;
                Ok((Spanned::new(Expr::Yield(Box::new(inner_expr)), span), new_pos))
            }
            // @Name... — instance ref or binding
            Token::At => {
                let next = cur + 1;
                let name = expect_ident(tokens, next)?;
                let span = start..tokens[next].span.end;
                Ok((Spanned::new(Expr::InstanceRef(name), span), next + 1))
            }
            // (| ... |) — match expression
            Token::CompositionOpen => {
                self.parse_match_expr(tokens, cur)
            }
            // [stmts] — inline eval
            Token::LBracket => {
                let (stmts, new_pos) = self.parse_bracket_body(tokens, cur)?;
                let span = start..tokens[new_pos - 1].span.end;
                Ok((Spanned::new(Expr::InlineEval(stmts), span), new_pos))
            }
            // (expr) — group or struct construction or StdOut
            Token::LParen => {
                let (inner, new_pos) = self.parse_expression(tokens, cur + 1)?;
                let inner_expr = inner.as_expr()?;
                let close_pos = skip_newlines(tokens, new_pos);
                expect_token(tokens, close_pos, &Token::RParen)?;
                let span = start..tokens[close_pos].span.end;
                Ok((Spanned::new(Expr::Group(Box::new(inner_expr)), span), close_pos + 1))
            }
            // Float literal
            Token::Float(s) => {
                let val: f64 = s.parse().unwrap_or(0.0);
                let span = tokens[cur].span.clone();
                Ok((Spanned::new(Expr::FloatLit(val), span), cur + 1))
            }
            // Integer literal
            Token::Integer(n) => {
                let span = tokens[cur].span.clone();
                Ok((Spanned::new(Expr::IntLit(*n), span), cur + 1))
            }
            // String literal
            Token::StringLit(s) => {
                let val = s.clone();
                let span = tokens[cur].span.clone();
                Ok((Spanned::new(Expr::StringLit(val), span), cur + 1))
            }
            // PascalCase — type path, struct construction, StdOut, or bare name
            Token::PascalIdent(name) => {
                let name = name.clone();
                // StdOut special handling
                if name == "StdOut" {
                    let (inner, new_pos) = self.parse_expression(tokens, cur + 1)?;
                    let inner_expr = inner.as_expr()?;
                    let span = start..inner_expr.span.end;
                    return Ok((Spanned::new(Expr::StdOut(Box::new(inner_expr)), span), new_pos));
                }
                // True/False are boolean literals — never construction
                if name == "True" || name == "False" {
                    let span = tokens[cur].span.clone();
                    return Ok((Spanned::new(Expr::BareName(name), span), cur + 1));
                }
                // Type/member — type path: PascalCase / ident
                if cur + 2 < tokens.len() && tokens[cur + 1].token == Token::Slash {
                    match &tokens[cur + 2].token {
                        Token::PascalIdent(member) | Token::CamelIdent(member) => {
                            let member = member.clone();
                            let after_member = cur + 3;
                            // Type/method(args) — associated function call
                            if after_member < tokens.len() && tokens[after_member].token == Token::LParen {
                                let (args, new_pos) = self.parse_call_args(tokens, after_member + 1)?;
                                let span = start..tokens[new_pos - 1].span.end;
                                return Ok((Spanned::new(
                                    Expr::MethodCall(
                                        Box::new(Spanned::new(Expr::BareName(name), tokens[cur].span.clone())),
                                        member,
                                        args,
                                    ),
                                    span,
                                ), new_pos));
                            }
                            // Type/Variant or Type/member — bare access
                            let span = start..tokens[cur + 2].span.end;
                            return Ok((Spanned::new(
                                Expr::Access(
                                    Box::new(Spanned::new(Expr::BareName(name), tokens[cur].span.clone())),
                                    member,
                                ),
                                span,
                            ), after_member));
                        }
                        _ => {}  // fall through — not a type path
                    }
                }
                // Check for paren — struct/variant construction
                if cur + 1 < tokens.len() && tokens[cur + 1].token == Token::LParen {
                    return self.parse_construction(tokens, cur, &name);
                }
                let span = tokens[cur].span.clone();
                Ok((Spanned::new(Expr::BareName(name), span), cur + 1))
            }
            // camelCase — bare name or function call
            Token::CamelIdent(name) => {
                let name = name.clone();
                // camelCase(args) — free function call
                if cur + 1 < tokens.len() && tokens[cur + 1].token == Token::LParen {
                    let (args, new_pos) = self.parse_call_args(tokens, cur + 2)?;
                    let span = start..tokens[new_pos - 1].span.end;
                    return Ok((Spanned::new(Expr::FnCall(name, args), span), new_pos));
                }
                let span = tokens[cur].span.clone();
                Ok((Spanned::new(Expr::BareName(name), span), cur + 1))
            }
            // Minus as unary negation
            Token::Minus => {
                let (inner, new_pos) = self.parse_atom(tokens, cur + 1)?;
                let span = start..inner.span.end;
                let zero = Spanned::new(Expr::IntLit(0), start..start);
                Ok((Spanned::new(
                    Expr::BinOp(Box::new(zero), BinOp::Subtraction, Box::new(inner)),
                    span,
                ), new_pos))
            }
            other => Err(format!("unexpected token in expression: {:?}", other)),
        }
    }

    /// Parse Name(field_pairs_or_expr) — struct construction or variant construction.
    fn parse_construction(&self, tokens: &[SpannedToken], pos: usize, name: &str) -> Result<(Spanned<Expr>, usize), String> {
        let start = tokens[pos].span.start;
        let paren_pos = pos + 1; // the (
        let inner_start = paren_pos + 1;

        // Try field pairs first: Name(Field(expr) Field(expr) ...)
        if let Ok((fields, end)) = self.try_parse_field_pairs(tokens, inner_start) {
            if end < tokens.len() && tokens[end].token == Token::RParen {
                let span = start..tokens[end].span.end;
                return Ok((Spanned::new(
                    Expr::StructConstruct(name.to_string(), fields),
                    span,
                ), end + 1));
            }
        }

        // Not a struct construction — return just the bare name.
        // The `(` is left unconsumed for the caller to handle.
        // This matches the old engine behavior: Value(5.0) returns
        // BareName("Value"), and (5.0) is a separate group expression.
        let span = tokens[pos].span.clone();
        Ok((Spanned::new(Expr::BareName(name.to_string()), span), pos + 1))
    }

    /// Try to parse field pairs: PascalName(expr) PascalName(expr) ...
    fn try_parse_field_pairs(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Vec<(String, Spanned<Expr>)>, usize), String> {
        let mut fields = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() || tokens[pos].token == Token::RParen {
                break;
            }
            // Expect PascalIdent(expr)
            let fname = match &tokens[pos].token {
                Token::PascalIdent(s) => s.clone(),
                _ => return Err("expected field name".to_string()),
            };
            pos += 1;
            if pos >= tokens.len() || tokens[pos].token != Token::LParen {
                return Err("expected ( after field name".to_string());
            }
            pos += 1;
            let (val, new_pos) = self.parse_expression(tokens, pos)?;
            let val_expr = val.as_expr()?;
            pos = skip_newlines(tokens, new_pos);
            if pos >= tokens.len() || tokens[pos].token != Token::RParen {
                return Err("expected ) after field value".to_string());
            }
            pos += 1;
            fields.push((fname, val_expr));
        }
        if fields.is_empty() {
            return Err("no field pairs found".to_string());
        }
        Ok((fields, pos))
    }

    /// Parse (| targets arms |)
    fn parse_match_expr(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Spanned<Expr>, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos + 1; // skip (|

        // Parse targets: @Name expressions before the first arm
        let mut targets = Vec::new();
        loop {
            cur = skip_newlines(tokens, cur);
            if cur >= tokens.len() { break; }
            // Arms start with ( or [
            if matches!(tokens[cur].token, Token::LParen | Token::LBracket) { break; }
            if tokens[cur].token == Token::CompositionClose { break; }
            let (target, new_pos) = self.parse_expression(tokens, cur)?;
            targets.push(target.as_expr()?);
            cur = new_pos;
        }

        // Parse arms
        let (arms_val, new_pos) = self.parse_match_expr_arm_list(tokens, cur)?;
        cur = new_pos;
        let arms = value_to_match_arms(arms_val)?;

        // |)
        cur = skip_newlines(tokens, cur);
        expect_token(tokens, cur, &Token::CompositionClose)?;
        let span = start..tokens[cur].span.end;

        Ok((Spanned::new(Expr::MatchExpr(MatchExprData { targets, arms }), span), cur + 1))
    }

    /// Parse [stmts] — bracket-delimited body.
    fn parse_bracket_body(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Vec<Spanned<Expr>>, usize), String> {
        let mut cur = pos + 1; // skip [
        let (stmts_val, new_pos) = self.parse_stmt_list(tokens, cur)?;
        cur = skip_newlines(tokens, new_pos);
        expect_token(tokens, cur, &Token::RBracket)?;
        let stmts = value_to_exprs(stmts_val)?;
        Ok((stmts, cur + 1))
    }

    fn parse_call_args(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Vec<Spanned<Expr>>, usize), String> {
        let mut args = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() || tokens[pos].token == Token::RParen {
                break;
            }
            // Try field pair: PascalName(expr) — variant construction
            if let Token::PascalIdent(fname) = &tokens[pos].token {
                let fname = fname.clone();
                if fname != "True" && fname != "False" && fname != "StdOut"
                    && pos + 1 < tokens.len() && tokens[pos + 1].token == Token::LParen
                {
                    let inner_start = pos + 2;
                    if let Ok((inner, new_pos)) = self.parse_expression(tokens, inner_start) {
                        let inner_expr = inner.as_expr().ok();
                        let close = skip_newlines(tokens, new_pos);
                        if close < tokens.len() && tokens[close].token == Token::RParen {
                            if let Some(ie) = inner_expr {
                                let span = tokens[pos].span.start..tokens[close].span.end;
                                args.push(Spanned::new(
                                    Expr::StructConstruct(fname, vec![("_value".to_string(), ie)]),
                                    span,
                                ));
                                pos = close + 1;
                                continue;
                            }
                        }
                    }
                }
            }
            let (val, new_pos) = self.parse_expression(tokens, pos)?;
            args.push(val.as_expr()?);
            pos = new_pos;
        }
        expect_token(tokens, pos, &Token::RParen)?;
        Ok((args, pos + 1))
    }

    // ── Built-in: statement list ─────────────────────────────

    fn parse_stmt_list(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Value, usize), String> {
        let mut stmts = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() { break; }
            // Stop at closing delimiters
            if matches!(tokens[pos].token,
                Token::RBracket | Token::RParen | Token::RBrace
                | Token::CompositionClose | Token::IterClose) {
                break;
            }
            let (val, new_pos) = self.parse_statement(tokens, pos)?;
            stmts.push(val);
            pos = new_pos;
        }
        Ok((Value::List(stmts), pos))
    }

    fn parse_statement(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Value, usize), String> {
        let cur = skip_newlines(tokens, pos);
        if cur >= tokens.len() {
            return Err("expected statement".to_string());
        }

        let start = tokens[cur].span.start;

        match &tokens[cur].token {
            // ~@Name... — mutable set or mutable new
            Token::Tilde => {
                if cur + 1 < tokens.len() && tokens[cur + 1].token == Token::At {
                    let name = expect_ident(tokens, cur + 2)?;
                    let after_name = cur + 3;
                    // ~@Name.set(expr)
                    if after_name < tokens.len() && tokens[after_name].token == Token::Dot {
                        if let Token::CamelIdent(method) = &tokens.get(after_name + 1).map(|t| &t.token).unwrap_or(&Token::Newline) {
                            if method == "set" && after_name + 2 < tokens.len() && tokens[after_name + 2].token == Token::LParen {
                                let (val, new_pos) = self.parse_expression(tokens, after_name + 3)?;
                                let val_expr = val.as_expr()?;
                                let close = skip_newlines(tokens, new_pos);
                                expect_token(tokens, close, &Token::RParen)?;
                                let span = start..tokens[close].span.end;
                                return Ok((Value::Expr(Spanned::new(
                                    Expr::MutableSet(name, Box::new(val_expr)), span,
                                )), close + 1));
                            }
                        }
                    }
                    // ~@Name Type/new(args)
                    if after_name < tokens.len() {
                        if let Token::PascalIdent(type_name) = &tokens[after_name].token {
                            let type_ref = TypeRef::Named(type_name.clone());
                            if after_name + 1 < tokens.len() && tokens[after_name + 1].token == Token::Slash {
                                let (args, new_pos) = self.parse_new_call(tokens, after_name + 1)?;
                                let span = start..tokens[new_pos - 1].span.end;
                                return Ok((Value::Expr(Spanned::new(
                                    Expr::MutableNew(name, type_ref, args), span,
                                )), new_pos));
                            }
                        }
                    }
                }
            }
            // @Name... — same-type new, sub-type new, or instance ref
            Token::At => {
                let name = expect_ident(tokens, cur + 1)?;
                let after_name = cur + 2;
                // @Name/method(args) — type path instantiation
                // Surface: @Radius/new(Value(5.0))
                // Kernel:  @Radius Radius/new(Value(5.0))  →  let radius = Radius::new(...)
                if after_name + 2 < tokens.len()
                    && tokens[after_name].token == Token::Slash
                {
                    if let Token::CamelIdent(method) = &tokens[after_name + 1].token {
                        let method = method.clone();
                        let method_pos = after_name + 1;
                        if method_pos + 1 < tokens.len() && tokens[method_pos + 1].token == Token::LParen {
                            let (args, new_pos) = self.parse_call_args(tokens, method_pos + 2)?;
                            let span = start..tokens[new_pos - 1].span.end;
                            if method == "new" {
                                // @Name/new(args) → SameTypeNew
                                return Ok((Value::Expr(Spanned::new(
                                    Expr::SameTypeNew(name, args), span,
                                )), new_pos));
                            } else {
                                // @Name/method(args) → bind + type method call
                                // Kernel: @Name Name/method(args)
                                let type_call = Spanned::new(
                                    Expr::MethodCall(
                                        Box::new(Spanned::new(Expr::BareName(name.clone()), tokens[cur+1].span.clone())),
                                        method,
                                        args,
                                    ),
                                    tokens[after_name].span.start..tokens[new_pos - 1].span.end,
                                );
                                return Ok((Value::Expr(Spanned::new(
                                    Expr::SameTypeNew(name, vec![type_call]), span,
                                )), new_pos));
                            }
                        }
                    }
                }
                // @Name/new(args) — same-type new (legacy . syntax removed)
                if after_name + 1 < tokens.len()
                    && tokens[after_name].token == Token::Slash
                    && matches!(&tokens[after_name + 1].token, Token::CamelIdent(s) if s == "new")
                {
                    let (args, new_pos) = self.parse_new_call(tokens, after_name)?;
                    let span = start..tokens[new_pos - 1].span.end;
                    return Ok((Value::Expr(Spanned::new(
                        Expr::SameTypeNew(name, args), span,
                    )), new_pos));
                }
                // @Name Type/... — sub-type new or type-path call
                if after_name < tokens.len() {
                    if let Token::PascalIdent(type_name) = &tokens[after_name].token {
                        let type_ref = TypeRef::Named(type_name.clone());
                        // Check for Type/ (type path)
                        if after_name + 1 < tokens.len() && tokens[after_name + 1].token == Token::Slash {
                            if after_name + 2 < tokens.len() {
                                if let Token::CamelIdent(method) = &tokens[after_name + 2].token {
                                    if method == "new" {
                                        // @Name Type/new(args) — sub-type new
                                        let (args, new_pos) = self.parse_new_call(tokens, after_name + 1)?;
                                        let span = start..tokens[new_pos - 1].span.end;
                                        return Ok((Value::Expr(Spanned::new(
                                            Expr::SubTypeNew(name, type_ref, args), span,
                                        )), new_pos));
                                    } else {
                                        // @Name Type/method(args) — bind result of type method
                                        let (rhs, new_pos) = self.parse_expression(tokens, after_name)?;
                                        let rhs_expr = rhs.as_expr()?;
                                        let span = start..tokens[new_pos - 1].span.end;
                                        return Ok((Value::Expr(Spanned::new(
                                            Expr::SubTypeNew(name, type_ref, vec![rhs_expr]), span,
                                        )), new_pos));
                                    }
                                }
                            }
                        }
                        // @Name Type (no /) — deferred new / subtype decl
                        let span = start..tokens[after_name].span.end;
                        return Ok((Value::Expr(Spanned::new(
                            Expr::SubTypeDecl(name, type_ref), span,
                        )), after_name + 1));
                    }
                }
            }
            _ => {}
        }

        // PascalIdent PascalIdent — SubTypeDecl (local sub-type declaration)
        if let Token::PascalIdent(name) = &tokens[cur].token {
            let name = name.clone();
            let after = skip_newlines(tokens, cur + 1);
            if after < tokens.len() {
                if let Token::PascalIdent(_) = &tokens[after].token {
                    // Check it's not followed by ( which would be a struct construction
                    let after2 = skip_newlines(tokens, after + 1);
                    let is_construction = after2 < tokens.len() && tokens[after2].token == Token::LParen;
                    if !is_construction {
                        let (tr, new_pos) = self.try_rule("typeRef", tokens, after)?;
                        let type_ref = tr.as_type_ref()?;
                        let span = start..tokens[new_pos.saturating_sub(1)].span.end;
                        return Ok((Value::Expr(Spanned::new(
                            Expr::SubTypeDecl(name, type_ref), span,
                        )), new_pos));
                    }
                }
            }
        }

        // Fallback: parse as expression
        let (val, new_pos) = self.parse_expression(tokens, cur)?;
        Ok((val, new_pos))
    }

    /// Parse /new(args) call — expects tokens starting at the slash.
    fn parse_new_call(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Vec<Spanned<Expr>>, usize), String> {
        let mut cur = pos;
        expect_token(tokens, cur, &Token::Slash)?;
        cur += 1;
        // "new"
        match &tokens.get(cur).map(|t| &t.token) {
            Some(Token::CamelIdent(s)) if s == "new" => { cur += 1; }
            _ => return Err("expected /new".to_string()),
        }
        expect_token(tokens, cur, &Token::LParen)?;
        cur += 1;
        let (args, new_pos) = self.parse_call_args(tokens, cur)?;
        // parse_call_args already consumed the closing )
        Ok((args, new_pos))
    }

    // ── Built-in: match method arms ──────────────────────────

    fn parse_match_method_arm_list(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Value, usize), String> {
        let mut arms = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() { break; }
            if tokens[pos].token == Token::CompositionClose { break; }

            let _start = tokens[pos].span.start;

            match &tokens[pos].token {
                // Destructure arm: [elems | @Rest] result
                Token::LBracket => {
                    let (arm, new_pos) = self.parse_destructure_arm(tokens, pos)?;
                    arms.push(Value::MatchMethodArm(arm));
                    pos = new_pos;
                }
                // Backtrack arm: [patterns] result (no pipe)
                // Actually [ can be either — disambiguate by looking for |
                // Commit arm: (patterns) result
                Token::LParen => {
                    let (arm, new_pos) = self.parse_commit_arm(tokens, pos)?;
                    arms.push(Value::MatchMethodArm(arm));
                    pos = new_pos;
                }
                _ => break,
            }
        }
        Ok((Value::List(arms), pos))
    }

    fn parse_destructure_arm(&self, tokens: &[SpannedToken], pos: usize) -> Result<(MatchMethodArm, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos + 1; // skip [

        // Check if this has a pipe (destructure) or not (backtrack)
        let has_pipe = find_pipe_before_close(tokens, cur, &Token::RBracket);

        if has_pipe {
            // Destructure: [elems | @Rest] result
            let mut elements = Vec::new();
            while cur < tokens.len() && tokens[cur].token != Token::Pipe && tokens[cur].token != Token::RBracket {
                let elem = parse_destructure_element(tokens, &mut cur)?;
                elements.push(elem);
            }
            // |
            expect_token(tokens, cur, &Token::Pipe)?;
            cur += 1;
            // @Rest
            expect_token(tokens, cur, &Token::At)?;
            cur += 1;
            let rest_name = expect_ident(tokens, cur)?;
            cur += 1;
            // ]
            expect_token(tokens, cur, &Token::RBracket)?;
            cur += 1;

            // Result body
            let (body_stmts, new_pos) = self.parse_arm_body(tokens, cur)?;

            let span = start..tokens[new_pos - 1].span.end;
            Ok((MatchMethodArm {
                kind: ArmKind::Destructure,
                patterns: vec![],
                body: body_stmts,
                destructure: Some((elements, rest_name)),
                span,
            }, new_pos))
        } else {
            // Backtrack: [patterns] result
            let mut patterns = Vec::new();
            while cur < tokens.len() && tokens[cur].token != Token::RBracket {
                cur = skip_newlines(tokens, cur);
                if tokens[cur].token == Token::RBracket { break; }
                let (pat, new_pos) = self.parse_pattern(tokens, cur)?;
                patterns.push(pat);
                cur = new_pos;
            }
            expect_token(tokens, cur, &Token::RBracket)?;
            cur += 1;

            let (body_stmts, new_pos) = self.parse_arm_body(tokens, cur)?;

            let span = start..tokens[new_pos - 1].span.end;
            Ok((MatchMethodArm {
                kind: ArmKind::Backtrack,
                patterns,
                body: body_stmts,
                destructure: None,
                span,
            }, new_pos))
        }
    }

    fn parse_commit_arm(&self, tokens: &[SpannedToken], pos: usize) -> Result<(MatchMethodArm, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos + 1; // skip (

        let mut patterns = Vec::new();
        while cur < tokens.len() && tokens[cur].token != Token::RParen {
            cur = skip_newlines(tokens, cur);
            if tokens[cur].token == Token::RParen { break; }
            let (pat, new_pos) = self.parse_pattern(tokens, cur)?;
            patterns.push(pat);
            cur = new_pos;
        }
        expect_token(tokens, cur, &Token::RParen)?;
        cur += 1;

        let (body_stmts, new_pos) = self.parse_arm_body(tokens, cur)?;

        let span = start..tokens[new_pos - 1].span.end;
        Ok((MatchMethodArm {
            kind: ArmKind::Commit,
            patterns,
            body: body_stmts,
            destructure: None,
            span,
        }, new_pos))
    }

    fn parse_arm_body(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Vec<Spanned<Expr>>, usize), String> {
        let cur = skip_newlines(tokens, pos);
        let mut stmts = Vec::new();
        let (val, new_pos) = self.parse_expression(tokens, cur)?;
        stmts.push(val.as_expr()?);
        Ok((stmts, new_pos))
    }

    fn parse_pattern(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Pattern, usize), String> {
        let cur = skip_newlines(tokens, pos);
        if cur >= tokens.len() {
            return Err("expected pattern".to_string());
        }

        match &tokens[cur].token {
            Token::Underscore => Ok((Pattern::Wildcard, cur + 1)),
            Token::StringLit(s) => Ok((Pattern::StringLit(s.clone()), cur + 1)),
            Token::At => {
                let name = expect_ident(tokens, cur + 1)?;
                Ok((Pattern::InstanceBind(name), cur + 2))
            }
            // Paren-wrapped pattern: or-pattern (A | B) or destructure (@Head | @Rest)
            Token::LParen => {
                let mut inner_cur = cur + 1;
                let has_pipe = find_pipe_before_close(tokens, inner_cur, &Token::RParen);
                if has_pipe {
                    // Collect elements before the pipe
                    let mut parts = Vec::new();
                    while inner_cur < tokens.len()
                        && tokens[inner_cur].token != Token::Pipe
                        && tokens[inner_cur].token != Token::RParen
                    {
                        inner_cur = skip_newlines(tokens, inner_cur);
                        if tokens[inner_cur].token == Token::Pipe || tokens[inner_cur].token == Token::RParen { break; }
                        let (pat, new_pos) = self.parse_pattern(tokens, inner_cur)?;
                        parts.push(pat);
                        inner_cur = new_pos;
                    }
                    // |
                    expect_token(tokens, inner_cur, &Token::Pipe)?;
                    inner_cur += 1;
                    inner_cur = skip_newlines(tokens, inner_cur);
                    // After pipe: if @Name → destructure tail, else more or-parts
                    if inner_cur < tokens.len() && tokens[inner_cur].token == Token::At {
                        let rest_name = expect_ident(tokens, inner_cur + 1)?;
                        inner_cur += 2;
                        inner_cur = skip_newlines(tokens, inner_cur);
                        expect_token(tokens, inner_cur, &Token::RParen)?;
                        return Ok((Pattern::Destructure {
                            head: parts,
                            tail: Some(rest_name),
                        }, inner_cur + 1));
                    }
                    // Or-pattern: collect remaining variants after |
                    while inner_cur < tokens.len() && tokens[inner_cur].token != Token::RParen {
                        inner_cur = skip_newlines(tokens, inner_cur);
                        if tokens[inner_cur].token == Token::RParen { break; }
                        if tokens[inner_cur].token == Token::Pipe {
                            inner_cur += 1;
                            continue;
                        }
                        let (pat, new_pos) = self.parse_pattern(tokens, inner_cur)?;
                        parts.push(pat);
                        inner_cur = new_pos;
                    }
                    expect_token(tokens, inner_cur, &Token::RParen)?;
                    return Ok((Pattern::Or(parts), inner_cur + 1));
                }
                // Single pattern in parens
                let (inner_pat, new_pos) = self.parse_pattern(tokens, inner_cur)?;
                let close = skip_newlines(tokens, new_pos);
                expect_token(tokens, close, &Token::RParen)?;
                Ok((inner_pat, close + 1))
            }
            Token::PascalIdent(name) => {
                let name = name.clone();
                if name == "True" { return Ok((Pattern::BoolLit(true), cur + 1)); }
                if name == "False" { return Ok((Pattern::BoolLit(false), cur + 1)); }
                // DataCarrying: Name(inner)
                if cur + 1 < tokens.len() && tokens[cur + 1].token == Token::LParen {
                    let mut inner_cur = cur + 2;
                    // Check for destructure: (head | @Rest) or or-pattern: (A | B)
                    let has_pipe = find_pipe_before_close(tokens, inner_cur, &Token::RParen);
                    if has_pipe {
                        // Or-pattern or destructure
                        let mut parts = Vec::new();
                        while inner_cur < tokens.len() && tokens[inner_cur].token != Token::RParen {
                            if tokens[inner_cur].token == Token::Pipe {
                                inner_cur += 1;
                                continue;
                            }
                            inner_cur = skip_newlines(tokens, inner_cur);
                            if tokens[inner_cur].token == Token::At {
                                // Destructure tail: @Rest
                                let rest_name = expect_ident(tokens, inner_cur + 1)?;
                                inner_cur += 2;
                                // Consume until RParen
                                while inner_cur < tokens.len() && tokens[inner_cur].token != Token::RParen {
                                    inner_cur += 1;
                                }
                                expect_token(tokens, inner_cur, &Token::RParen)?;
                                return Ok((Pattern::Destructure {
                                    head: parts,
                                    tail: Some(rest_name),
                                }, inner_cur + 1));
                            }
                            let (pat, new_pos) = self.parse_pattern(tokens, inner_cur)?;
                            parts.push(pat);
                            inner_cur = new_pos;
                        }
                        expect_token(tokens, inner_cur, &Token::RParen)?;
                        // All parts are variants → or-pattern
                        return Ok((Pattern::Or(parts), inner_cur + 1));
                    }
                    // Single inner pattern: DataCarrying
                    let (inner_pat, new_pos) = self.parse_pattern(tokens, inner_cur)?;
                    let close = skip_newlines(tokens, new_pos);
                    expect_token(tokens, close, &Token::RParen)?;
                    return Ok((Pattern::DataCarrying(name, Box::new(inner_pat)), close + 1));
                }
                Ok((Pattern::Variant(name), cur + 1))
            }
            other => Err(format!("unexpected token in pattern: {:?}", other)),
        }
    }

    // ── Built-in: match expression arms ──────────────────────

    fn parse_match_expr_arm_list(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Value, usize), String> {
        let mut arms = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() { break; }
            if tokens[pos].token == Token::CompositionClose { break; }
            if !matches!(tokens[pos].token, Token::LParen | Token::LBracket) { break; }

            let start = tokens[pos].span.start;
            let (patterns, mut cur) = match &tokens[pos].token {
                Token::LParen => {
                    let mut pats = Vec::new();
                    let mut c = pos + 1;
                    while c < tokens.len() && tokens[c].token != Token::RParen {
                        c = skip_newlines(tokens, c);
                        if tokens[c].token == Token::RParen { break; }
                        let (pat, new_pos) = self.parse_pattern(tokens, c)?;
                        pats.push(pat);
                        c = new_pos;
                    }
                    expect_token(tokens, c, &Token::RParen)?;
                    (pats, c + 1)
                }
                Token::LBracket => {
                    let mut pats = Vec::new();
                    let mut c = pos + 1;
                    while c < tokens.len() && tokens[c].token != Token::RBracket {
                        c = skip_newlines(tokens, c);
                        if tokens[c].token == Token::RBracket { break; }
                        let (pat, new_pos) = self.parse_pattern(tokens, c)?;
                        pats.push(pat);
                        c = new_pos;
                    }
                    expect_token(tokens, c, &Token::RBracket)?;
                    (pats, c + 1)
                }
                _ => break,
            };

            // Body expression(s)
            let mut body = Vec::new();
            let (val, new_pos) = self.parse_expression(tokens, cur)?;
            body.push(val.as_expr()?);
            cur = new_pos;

            let span = start..tokens[cur.saturating_sub(1)].span.end;
            arms.push(Value::MatchArm(MatchArm { patterns, body, span }));
            pos = cur;
        }
        Ok((Value::List(arms), pos))
    }

    // ── Built-in: method signature list ──────────────────────

    fn parse_method_sig_list(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Value, usize), String> {
        let mut sigs = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() { break; }
            if matches!(tokens[pos].token, Token::RParen | Token::RBracket) { break; }

            if let Token::CamelIdent(_) = &tokens[pos].token {
                let (sig, new_pos) = self.parse_method_sig(tokens, pos)?;
                sigs.push(Value::MethodSig(sig));
                pos = new_pos;
            } else if tokens[pos].token == Token::Bang {
                // Associated constant in trait
                let (cst, new_pos) = self.parse_const_decl(tokens, pos)?;
                sigs.push(Value::ConstDecl(cst));
                pos = new_pos;
            } else {
                break;
            }
        }
        Ok((Value::List(sigs), pos))
    }

    fn parse_method_sig(&self, tokens: &[SpannedToken], pos: usize) -> Result<(MethodSig, usize), String> {
        let start = tokens[pos].span.start;
        let name = expect_camel(tokens, pos)?;
        let mut cur = pos + 1;

        // (params)
        expect_token(tokens, cur, &Token::LParen)?;
        cur += 1;
        let mut params = Vec::new();
        while cur < tokens.len() && tokens[cur].token != Token::RParen {
            cur = skip_newlines(tokens, cur);
            if tokens[cur].token == Token::RParen { break; }
            let (param, new_pos) = self.try_rule("param", tokens, cur)?;
            params.push(param.as_param()?);
            cur = new_pos;
        }
        expect_token(tokens, cur, &Token::RParen)?;
        cur += 1;

        // Optional return type
        cur = skip_newlines(tokens, cur);
        let output = if cur < tokens.len() {
            match &tokens[cur].token {
                Token::PascalIdent(_) | Token::Colon | Token::TraitBoundOpen => {
                    let (tr, new_pos) = self.try_rule("typeRef", tokens, cur)?;
                    cur = new_pos;
                    Some(tr.as_type_ref()?)
                }
                _ => None,
            }
        } else {
            None
        };

        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((MethodSig { name, params, output, span }, cur))
    }

    fn parse_const_decl(&self, tokens: &[SpannedToken], pos: usize) -> Result<(ConstDecl, usize), String> {
        let start = tokens[pos].span.start;
        let mut cur = pos;
        expect_token(tokens, cur, &Token::Bang)?;
        cur += 1;
        let name = expect_ident(tokens, cur)?;
        cur += 1;
        let (tr, new_pos) = self.try_rule("typeRef", tokens, cur)?;
        cur = new_pos;
        let type_ref = tr.as_type_ref()?;

        // Optional value: {expr}
        cur = skip_newlines(tokens, cur);
        let value = if cur < tokens.len() && tokens[cur].token == Token::LBrace {
            cur += 1;
            let mut stmts = Vec::new();
            while cur < tokens.len() && tokens[cur].token != Token::RBrace {
                cur = skip_newlines(tokens, cur);
                if tokens[cur].token == Token::RBrace { break; }
                let (val, new_pos) = self.parse_expression(tokens, cur)?;
                stmts.push(val.as_expr()?);
                cur = new_pos;
            }
            expect_token(tokens, cur, &Token::RBrace)?;
            cur += 1;
            Some(Body::Block(stmts))
        } else {
            None
        };

        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((ConstDecl { name, type_ref, value, span }, cur))
    }

    // ── Built-in: type implementation list ───────────────────

    fn parse_type_impl_list(&self, tokens: &[SpannedToken], mut pos: usize) -> Result<(Value, usize), String> {
        let mut impls = Vec::new();
        loop {
            pos = skip_newlines(tokens, pos);
            if pos >= tokens.len() { break; }
            if matches!(tokens[pos].token, Token::RBracket) { break; }

            if let Token::PascalIdent(_) = &tokens[pos].token {
                let (ti, new_pos) = self.parse_one_type_impl(tokens, pos)?;
                impls.push(Value::TypeImpl(ti));
                pos = new_pos;
            } else {
                break;
            }
        }
        Ok((Value::List(impls), pos))
    }

    fn parse_one_type_impl(&self, tokens: &[SpannedToken], pos: usize) -> Result<(TypeImpl, usize), String> {
        let start = tokens[pos].span.start;
        let target = expect_ident(tokens, pos)?;
        let mut cur = pos + 1;

        // [methods]
        cur = skip_newlines(tokens, cur);
        expect_token(tokens, cur, &Token::LBracket)?;
        cur += 1;

        let mut methods = Vec::new();
        let mut associated_types = Vec::new();
        let mut associated_constants = Vec::new();

        loop {
            cur = skip_newlines(tokens, cur);
            if cur >= tokens.len() || tokens[cur].token == Token::RBracket { break; }

            // Associated type: PascalName TypeRef
            if let Token::PascalIdent(tname) = &tokens[cur].token {
                let tname = tname.clone();
                if cur + 1 < tokens.len() {
                    if let Token::PascalIdent(_) = &tokens[cur + 1].token {
                        let (tr, new_pos) = self.try_rule("typeRef", tokens, cur + 1)?;
                        let aspan = tokens[cur].span.start..tokens[new_pos.saturating_sub(1)].span.end;
                        associated_types.push(AssociatedTypeDef {
                            name: tname,
                            concrete_type: tr.as_type_ref()?,
                            span: aspan,
                        });
                        cur = new_pos;
                        continue;
                    }
                }
            }

            // Associated constant: !Name Type {value}
            if tokens[cur].token == Token::Bang {
                let (cst, new_pos) = self.parse_const_decl(tokens, cur)?;
                associated_constants.push(cst);
                cur = new_pos;
                continue;
            }

            // Method def: camelName(params) [body]
            if let Token::CamelIdent(_) = &tokens[cur].token {
                let (md, new_pos) = self.parse_method_def(tokens, cur)?;
                methods.push(md);
                cur = new_pos;
                continue;
            }

            break;
        }

        expect_token(tokens, cur, &Token::RBracket)?;
        cur += 1;
        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((TypeImpl { target, methods, associated_types, associated_constants, span }, cur))
    }

    fn parse_method_def(&self, tokens: &[SpannedToken], pos: usize) -> Result<(MethodDef, usize), String> {
        let start = tokens[pos].span.start;
        let name = expect_camel(tokens, pos)?;
        let mut cur = pos + 1;

        // (params)
        expect_token(tokens, cur, &Token::LParen)?;
        cur += 1;
        let mut params = Vec::new();
        while cur < tokens.len() && tokens[cur].token != Token::RParen {
            cur = skip_newlines(tokens, cur);
            if tokens[cur].token == Token::RParen { break; }
            let (param, new_pos) = self.try_rule("param", tokens, cur)?;
            params.push(param.as_param()?);
            cur = new_pos;
        }
        expect_token(tokens, cur, &Token::RParen)?;
        cur += 1;

        // Optional return type
        cur = skip_newlines(tokens, cur);
        let output = if cur < tokens.len() {
            match &tokens[cur].token {
                Token::PascalIdent(s) if s != "Main" => {
                    // A PascalIdent here is a return type.
                    // It could be followed by:
                    //   - {params} for parameterized type (Vec{Sign})
                    //   - a body delimiter (this PascalIdent is the type, next is body)
                    //   - another PascalIdent (this is type, next starts body context)
                    let (tr, new_pos) = self.try_rule("typeRef", tokens, cur)?;
                    cur = new_pos;
                    Some(tr.as_type_ref()?)
                }
                Token::Colon | Token::TraitBoundOpen => {
                    let (tr, new_pos) = self.try_rule("typeRef", tokens, cur)?;
                    cur = new_pos;
                    Some(tr.as_type_ref()?)
                }
                _ => None,
            }
        } else {
            None
        };

        // Body
        cur = skip_newlines(tokens, cur);
        let body = self.parse_body(tokens, cur)?;
        cur = body.1;

        let span = start..tokens[cur.saturating_sub(1)].span.end;
        Ok((MethodDef { name, params, output, body: body.0, span }, cur))
    }

    fn parse_body(&self, tokens: &[SpannedToken], pos: usize) -> Result<(Body, usize), String> {
        let cur = skip_newlines(tokens, pos);
        if cur >= tokens.len() {
            return Err("expected body".to_string());
        }

        match &tokens[cur].token {
            // (| arms |) — matching body
            Token::CompositionOpen => {
                let mut inner = cur + 1;
                let (arms_val, new_pos) = self.parse_match_method_arm_list(tokens, inner)?;
                inner = skip_newlines(tokens, new_pos);
                expect_token(tokens, inner, &Token::CompositionClose)?;
                let arms = value_to_match_method_arms(arms_val)?;
                Ok((Body::MatchBody(arms), inner + 1))
            }
            // [| stmts |] — tail body
            Token::IterOpen => {
                let (stmts_val, new_pos) = self.parse_stmt_list(tokens, cur + 1)?;
                let close = skip_newlines(tokens, new_pos);
                expect_token(tokens, close, &Token::IterClose)?;
                let stmts = value_to_exprs(stmts_val)?;
                Ok((Body::TailBlock(stmts), close + 1))
            }
            // [___] — stub body
            Token::LBracket if cur + 1 < tokens.len() && tokens[cur + 1].token == Token::Stub => {
                let close = cur + 2;
                expect_token(tokens, close, &Token::RBracket)?;
                Ok((Body::Stub, close + 1))
            }
            // [stmts] — block body
            Token::LBracket => {
                let (stmts_val, new_pos) = self.parse_stmt_list(tokens, cur + 1)?;
                let close = skip_newlines(tokens, new_pos);
                expect_token(tokens, close, &Token::RBracket)?;
                let stmts = value_to_exprs(stmts_val)?;
                Ok((Body::Block(stmts), close + 1))
            }
            other => Err(format!("expected body, got {:?}", other)),
        }
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

fn expect_camel(tokens: &[SpannedToken], pos: usize) -> Result<String, String> {
    if pos >= tokens.len() {
        return Err("expected camelCase identifier, got EOF".to_string());
    }
    match &tokens[pos].token {
        Token::CamelIdent(s) => Ok(s.clone()),
        other => Err(format!("expected camelCase, got {:?}", other)),
    }
}

fn find_pipe_before_close(tokens: &[SpannedToken], start: usize, close: &Token) -> bool {
    let mut depth = 0;
    let mut pos = start;
    while pos < tokens.len() {
        if tokens[pos].token == Token::LParen || tokens[pos].token == Token::LBracket || tokens[pos].token == Token::LBrace {
            depth += 1;
        } else if tokens[pos].token == Token::RParen || tokens[pos].token == Token::RBracket || tokens[pos].token == Token::RBrace {
            if depth == 0 && std::mem::discriminant(&tokens[pos].token) == std::mem::discriminant(close) {
                return false;
            }
            depth -= 1;
        } else if tokens[pos].token == Token::Pipe && depth == 0 {
            return true;
        }
        pos += 1;
    }
    false
}

fn parse_destructure_element(tokens: &[SpannedToken], cur: &mut usize) -> Result<DestructureElement, String> {
    *cur = skip_newlines(tokens, *cur);
    if *cur >= tokens.len() {
        return Err("expected destructure element".to_string());
    }
    match &tokens[*cur].token {
        Token::At => {
            *cur += 1;
            let name = expect_ident(tokens, *cur)?;
            *cur += 1;
            Ok(DestructureElement::Binding(name))
        }
        Token::Underscore => {
            *cur += 1;
            Ok(DestructureElement::Wildcard)
        }
        Token::PascalIdent(s) => {
            let val = s.clone();
            *cur += 1;
            Ok(DestructureElement::ExactToken(val))
        }
        _other => {
            let name = token_variant_name(&tokens[*cur].token).to_string();
            *cur += 1;
            Ok(DestructureElement::ExactToken(name))
        }
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
        }
    }).collect::<Result<Vec<_>, _>>()?;

    builders::construct(&spec.constructor, &resolved, span)
}

/// Convert Value::List of MatchMethodArm to Vec.
fn value_to_match_method_arms(val: Value) -> Result<Vec<MatchMethodArm>, String> {
    match val {
        Value::List(items) => items.into_iter().map(|v| v.as_match_method_arm()).collect(),
        _ => Err("expected list of match method arms".to_string()),
    }
}

/// Convert Value::List of MatchArm to Vec.
fn value_to_match_arms(val: Value) -> Result<Vec<MatchArm>, String> {
    match val {
        Value::List(items) => items.into_iter().map(|v| v.as_match_arm()).collect(),
        _ => Err("expected list of match arms".to_string()),
    }
}

/// Convert Value::List of exprs to Vec.
fn value_to_exprs(val: Value) -> Result<Vec<Spanned<Expr>>, String> {
    match val {
        Value::List(items) => items.into_iter().map(|v| v.as_expr()).collect(),
        _ => Err("expected list of expressions".to_string()),
    }
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
        assert!(matches!(&items[0].node, Item::Main(_)));
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
                        Expr::MethodCall(base, method, args) => {
                            assert!(matches!(&base.node, Expr::BareName(n) if n == "Sign"));
                            assert_eq!(method, "fromDegree");
                            assert_eq!(args.len(), 1);
                        }
                        other => panic!("expected MethodCall, got {:?}", other),
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
