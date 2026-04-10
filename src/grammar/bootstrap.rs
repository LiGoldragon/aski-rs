//! Bootstrap parser — reads grammar/*.aski rule files into a RuleTable.
//!
//! This is the ONLY Rust code that knows grammar file syntax.
//! It parses the minimal subset: <name> { [pattern | @Rest] (Result @Rest) }
//! Everything else is defined BY the grammar rules it loads.

use std::path::Path;
use std::collections::HashSet;
use crate::lexer::{self, Token};
use super::{ParseRule, ParseArm, PatElem, ResultSpec, ResultArg, RuleTable};

/// Known token variant names — used to distinguish terminals from literals.
pub fn known_tokens() -> HashSet<&'static str> {
    [
        "LParen", "RParen", "LBracket", "RBracket", "LBrace", "RBrace",
        "Dot", "At", "Dollar", "Caret", "GreaterThan", "LessThan",
        "Ampersand", "Tilde", "Question", "Bang", "Hash", "Pipe",
        "Tick", "Colon", "Plus", "Minus", "Star", "Slash", "Percent",
        "DoubleEquals", "Equals", "NotEqual", "GreaterThanOrEqual",
        "LessThanOrEqual", "LogicalAnd", "LogicalOr", "Comma",
        "Underscore", "CompositionOpen", "CompositionClose",
        "TraitBoundOpen", "TraitBoundClose", "IterOpen", "IterClose",
        "RangeInclusive", "RangeExclusive", "Stub", "Newline",
    ].into_iter().collect()
}

/// Load all grammar rule files from a directory.
pub fn load_rules(grammar_dir: &Path) -> Result<RuleTable, String> {
    let mut table = RuleTable::new();
    let known = known_tokens();

    // Scan for .aski files that contain grammar rules (<name> [...])
    let entries = std::fs::read_dir(grammar_dir)
        .map_err(|e| format!("cannot read grammar dir: {}", e))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("readdir error: {}", e))?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "aski") {
            let source = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
            let rules = parse_rules_from_source(&source, &known)
                .map_err(|e| format!("in {}: {}", path.display(), e))?;
            for rule in rules {
                table.insert(rule.name.clone(), rule);
            }
        }
    }

    Ok(table)
}

/// Parse grammar rules from source text.
pub fn parse_rules_from_source(source: &str, known: &HashSet<&str>) -> Result<Vec<ParseRule>, String> {
    let spanned = lexer::lex(source).map_err(|errs| {
        errs.into_iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ")
    })?;

    let tokens: Vec<&Token> = spanned.iter()
        .map(|s| &s.token)
        .filter(|t| !matches!(t, Token::Newline | Token::Comment))
        .collect();

    let mut rules = Vec::new();
    let mut pos = 0;

    while pos < tokens.len() {
        if tokens[pos] == &Token::LessThan {
            let (rule, new_pos) = parse_one_rule(&tokens, pos, known)?;
            rules.push(rule);
            pos = new_pos;
        } else {
            // Skip non-rule tokens (comments, data definitions used by config.rs)
            pos += 1;
        }
    }

    Ok(rules)
}

/// Parse a single grammar rule: <name> { arms }
fn parse_one_rule(tokens: &[&Token], pos: usize, known: &HashSet<&str>) -> Result<(ParseRule, usize), String> {
    let mut cur = pos;

    // <
    expect_tok(tokens, &mut cur, &Token::LessThan, "<")?;

    // Rule name (PascalCase or camelCase)
    let name = eat_ident(tokens, &mut cur)?;

    // >
    expect_tok(tokens, &mut cur, &Token::GreaterThan, ">")?;

    // {
    expect_tok(tokens, &mut cur, &Token::LBrace, "{")?;

    // Arms until }
    let mut arms = Vec::new();
    while cur < tokens.len() && tokens[cur] != &Token::RBrace {
        let arm = parse_arm(tokens, &mut cur, known)?;
        arms.push(arm);
    }

    // }
    expect_tok(tokens, &mut cur, &Token::RBrace, "}")?;

    Ok((ParseRule { name, arms }, cur))
}

/// Parse a single arm: [pattern | @Rest] Result @Rest
fn parse_arm(tokens: &[&Token], cur: &mut usize, known: &HashSet<&str>) -> Result<ParseArm, String> {
    // [ or [| (IterOpen — epsilon arm)
    if *cur < tokens.len() && tokens[*cur] == &Token::IterOpen {
        // [| was lexed as IterOpen — this is an epsilon arm
        *cur += 1;
        // Parse @Rest after the implicit |
        let pattern = Vec::new();
        if *cur < tokens.len() && tokens[*cur] == &Token::At {
            *cur += 1;
            let _rest_name = eat_ident(tokens, cur)?;
        }
        // ] — but note IterOpen consumed [|, so we might see |] (IterClose) or just ]
        if *cur < tokens.len() && tokens[*cur] == &Token::RBracket {
            *cur += 1;
        } else if *cur < tokens.len() && tokens[*cur] == &Token::IterClose {
            // |] was lexed as IterClose
            *cur += 1;
        } else {
            return Err(format!("expected ] after epsilon arm, got {:?}",
                tokens.get(*cur).map(|t| format!("{:?}", t)).unwrap_or("EOF".to_string())));
        }
        // Result
        let result = parse_result_spec(tokens, cur, known)?;
        // Skip trailing @Rest on result side
        if *cur < tokens.len() && tokens[*cur] == &Token::At {
            *cur += 1;
            let _rest = eat_ident(tokens, cur)?;
        }
        return Ok(ParseArm { pattern, result });
    }

    // Normal arm: [
    expect_tok(tokens, cur, &Token::LBracket, "[")?;

    // Pattern elements until / or ]
    let mut pattern = Vec::new();
    while *cur < tokens.len()
        && tokens[*cur] != &Token::Slash
        && tokens[*cur] != &Token::RBracket
    {
        let elem = parse_pattern_elem(tokens, cur, known)?;
        pattern.push(elem);
    }

    // Optional: / @Rest
    if *cur < tokens.len() && tokens[*cur] == &Token::Slash {
        *cur += 1; // skip /
        // @Rest binding — consume but don't store (position is implicit)
        if *cur < tokens.len() && tokens[*cur] == &Token::At {
            *cur += 1; // skip @
            let _rest_name = eat_ident(tokens, cur)?;
        }
    }

    // ]
    expect_tok(tokens, cur, &Token::RBracket, "]")?;

    // Result: constructor with optional args, until next [ or }
    let result = parse_result_spec(tokens, cur, known)?;

    // Skip trailing @Rest on result side
    if *cur < tokens.len() && tokens[*cur] == &Token::At {
        *cur += 1;
        let _rest = eat_ident(tokens, cur)?;
    }

    Ok(ParseArm { pattern, result })
}

/// Parse a single pattern element.
fn parse_pattern_elem(tokens: &[&Token], cur: &mut usize, known: &HashSet<&str>) -> Result<PatElem, String> {
    if *cur >= tokens.len() {
        return Err("unexpected end in pattern".to_string());
    }

    match tokens[*cur] {
        // <rule> — non-terminal
        Token::LessThan => {
            *cur += 1;
            let name = eat_ident(tokens, cur)?;
            expect_tok(tokens, cur, &Token::GreaterThan, ">")?;
            Ok(PatElem::Rule(name))
        }
        // @Name — identifier binding; @Lit — literal binding; @Type — PascalCase only
        Token::At => {
            *cur += 1;
            let name = eat_ident(tokens, cur)?;
            if name == "Lit" {
                Ok(PatElem::BindLit(name))
            } else if name == "Type" {
                Ok(PatElem::BindType(name))
            } else {
                Ok(PatElem::Bind(name))
            }
        }
        // "literal" — match identifier with exact value
        Token::StringLit(s) => {
            let val = s.clone();
            *cur += 1;
            Ok(PatElem::Lit(val))
        }
        // PascalCase — could be token name (terminal) or literal
        Token::PascalIdent(s) => {
            let name = s.clone();
            *cur += 1;
            if known.contains(name.as_str()) {
                Ok(PatElem::Tok(name))
            } else {
                // Treat as a literal identifier match
                Ok(PatElem::Lit(name))
            }
        }
        // CamelCase — literal identifier match
        Token::CamelIdent(s) => {
            let val = s.clone();
            *cur += 1;
            Ok(PatElem::Lit(val))
        }
        other => {
            Err(format!("unexpected token in pattern: {:?}", other))
        }
    }
}

/// Parse result specification: Constructor(args...) or <rule> passthrough
fn parse_result_spec(tokens: &[&Token], cur: &mut usize, _known: &HashSet<&str>) -> Result<ResultSpec, String> {
    // Passthrough: <rule> — result is the sub-rule's result directly
    if *cur < tokens.len() && tokens[*cur] == &Token::LessThan {
        *cur += 1;
        let name = eat_ident(tokens, cur)?;
        expect_tok(tokens, cur, &Token::GreaterThan, ">")?;
        // Optional (base) for dotChain-style calls
        let mut args = vec![ResultArg::RuleResult(name)];
        if *cur < tokens.len() && tokens[*cur] == &Token::LParen {
            *cur += 1;
            while *cur < tokens.len() && tokens[*cur] != &Token::RParen {
                args.push(parse_result_arg(tokens, cur)?);
            }
            expect_tok(tokens, cur, &Token::RParen, ")")?;
        }
        return Ok(ResultSpec { constructor: "Passthrough".to_string(), args });
    }

    // Constructor name
    let constructor = eat_ident(tokens, cur)?;

    // Optional (args...)
    let mut args = Vec::new();
    if *cur < tokens.len() && tokens[*cur] == &Token::LParen {
        *cur += 1; // skip (
        while *cur < tokens.len() && tokens[*cur] != &Token::RParen {
            let arg = parse_result_arg(tokens, cur)?;
            args.push(arg);
        }
        expect_tok(tokens, cur, &Token::RParen, ")")?;
    }

    Ok(ResultSpec { constructor, args })
}

/// Parse a single result argument.
fn parse_result_arg(tokens: &[&Token], cur: &mut usize) -> Result<ResultArg, String> {
    if *cur >= tokens.len() {
        return Err("unexpected end in result args".to_string());
    }

    match tokens[*cur] {
        // @Name — bound reference
        Token::At => {
            *cur += 1;
            let name = eat_ident(tokens, cur)?;
            Ok(ResultArg::Bound(name))
        }
        // <rule> — non-terminal result
        Token::LessThan => {
            *cur += 1;
            let name = eat_ident(tokens, cur)?;
            expect_tok(tokens, cur, &Token::GreaterThan, ">")?;
            Ok(ResultArg::RuleResult(name))
        }
        // PascalCase — nested constructor
        Token::PascalIdent(s) => {
            let name = s.clone();
            *cur += 1;
            // Check for nested (args...)
            let mut args = Vec::new();
            if *cur < tokens.len() && tokens[*cur] == &Token::LParen {
                *cur += 1;
                while *cur < tokens.len() && tokens[*cur] != &Token::RParen {
                    args.push(parse_result_arg(tokens, cur)?);
                }
                expect_tok(tokens, cur, &Token::RParen, ")")?;
            }
            if args.is_empty() {
                // Could be a literal string value passed as arg
                Ok(ResultArg::Bound(name))
            } else {
                Ok(ResultArg::Nested(ResultSpec { constructor: name, args }))
            }
        }
        // CamelCase — bound reference by name
        Token::CamelIdent(s) => {
            let name = s.clone();
            *cur += 1;
            Ok(ResultArg::Bound(name))
        }
        // "string" — literal string value
        Token::StringLit(s) => {
            let val = s.clone();
            *cur += 1;
            Ok(ResultArg::Literal(val))
        }
        other => Err(format!("unexpected token in result args: {:?}", other)),
    }
}

// ── Helpers ──────────────────────────────────────────────

fn eat_ident(tokens: &[&Token], cur: &mut usize) -> Result<String, String> {
    if *cur >= tokens.len() {
        return Err("expected identifier, got end of input".to_string());
    }
    match tokens[*cur] {
        Token::PascalIdent(s) => {
            let name = s.clone();
            *cur += 1;
            Ok(name)
        }
        Token::CamelIdent(s) => {
            let name = s.clone();
            *cur += 1;
            Ok(name)
        }
        other => Err(format!("expected identifier, got {:?}", other)),
    }
}

fn expect_tok(tokens: &[&Token], cur: &mut usize, expected: &Token, label: &str) -> Result<(), String> {
    if *cur >= tokens.len() {
        return Err(format!("expected '{}', got end of input", label));
    }
    if std::mem::discriminant(tokens[*cur]) != std::mem::discriminant(expected) {
        return Err(format!("expected '{}', got {:?}", label, tokens[*cur]));
    }
    *cur += 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_parse_simple_rule() {
        let source = r#"
            <param> {
                [Colon At "Self" | @Rest]  BorrowSelf @Rest
                [At @Name | @Rest]         Owned(@Name) @Rest
            }
        "#;
        let known = known_tokens();
        let rules = parse_rules_from_source(source, &known).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "param");
        assert_eq!(rules[0].arms.len(), 2);

        // First arm: [Colon At "Self" | @Rest] BorrowSelf @Rest
        let arm0 = &rules[0].arms[0];
        assert_eq!(arm0.pattern.len(), 3);
        assert!(matches!(&arm0.pattern[0], PatElem::Tok(s) if s == "Colon"));
        assert!(matches!(&arm0.pattern[1], PatElem::Tok(s) if s == "At"));
        assert!(matches!(&arm0.pattern[2], PatElem::Lit(s) if s == "Self"));
        assert_eq!(arm0.result.constructor, "BorrowSelf");
        assert!(arm0.result.args.is_empty());

        // Second arm: [At @Name | @Rest] Owned(@Name) @Rest
        let arm1 = &rules[0].arms[1];
        assert_eq!(arm1.pattern.len(), 2);
        assert!(matches!(&arm1.pattern[0], PatElem::Tok(s) if s == "At"));
        assert!(matches!(&arm1.pattern[1], PatElem::Bind(s) if s == "Name"));
        assert_eq!(arm1.result.constructor, "Owned");
        assert_eq!(arm1.result.args.len(), 1);
        assert!(matches!(&arm1.result.args[0], ResultArg::Bound(s) if s == "Name"));
    }

    #[test]
    fn bootstrap_parse_nonterminal_in_pattern() {
        let source = r#"
            <param> {
                [At @Name <typeRef> | @Rest]  Named(@Name <typeRef>) @Rest
            }
        "#;
        let known = known_tokens();
        let rules = parse_rules_from_source(source, &known).unwrap();
        let arm = &rules[0].arms[0];
        assert_eq!(arm.pattern.len(), 3);
        assert!(matches!(&arm.pattern[2], PatElem::Rule(s) if s == "typeRef"));
        assert_eq!(arm.result.args.len(), 2);
        assert!(matches!(&arm.result.args[1], ResultArg::RuleResult(s) if s == "typeRef"));
    }

    #[test]
    fn bootstrap_parse_epsilon_arm() {
        let source = r#"
            <variants> {
                [<variant> <variants> | @Rest]  Cons(<variant> <variants>) @Rest
                [| @Rest]                       Nil @Rest
            }
        "#;
        let known = known_tokens();
        let rules = parse_rules_from_source(source, &known).unwrap();
        assert_eq!(rules[0].arms.len(), 2);
        // Second arm has empty pattern (epsilon)
        assert!(rules[0].arms[1].pattern.is_empty());
        assert_eq!(rules[0].arms[1].result.constructor, "Nil");
    }

    #[test]
    fn bootstrap_load_from_grammar_dir() {
        let grammar_dir = crate::grammar::config::find_grammar_dir()
            .expect("grammar dir should exist");
        let table = load_rules(&grammar_dir).unwrap();
        // Should load rules from any grammar rule files that exist
        // (operators.aski, kernel.aski, tokens.aski have no rules, so may be empty
        //  unless grammar rule files are present)
        let _ = table;
    }
}
