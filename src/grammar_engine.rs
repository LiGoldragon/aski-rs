//! Data-driven grammar engine for aski.
//!
//! Reads `.aski` grammar files and applies PEG-style rules to token streams.
//! Operates alongside (not replacing) the chumsky parser.

use crate::lexer::Token;
use std::collections::HashMap;

// ── Data structures ─────────────────────────────────────────────────

/// A grammar rule: `<name>` with one or more arms.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub arms: Vec<Arm>,
}

/// One arm of a grammar rule.
#[derive(Debug, Clone)]
pub struct Arm {
    pub pattern: Vec<PatternElement>,
    pub result: ResultExpr,
}

/// An element in a pattern.
#[derive(Debug, Clone)]
pub enum PatternElement {
    /// Match a specific token.
    Terminal(Token),
    /// Match any PascalCase identifier.
    AnyPascal,
    /// Match any camelCase identifier.
    AnyCamel,
    /// Match any integer literal.
    AnyInt,
    /// Match any float literal.
    AnyFloat,
    /// Match any string literal.
    AnyString,
    /// Call another rule: `<name>`.
    NonTerminal(String),
    /// Bind current token value: `@Name`.
    Bind(String),
    /// Bind remaining tokens: `| @Rest` (must be last before closing delimiter).
    Rest(String),
    /// Match and skip newlines.
    SkipNewlines,
    /// Repetition: match a sub-pattern zero or more times until a stop token.
    Repeat {
        element: Box<PatternElement>,
        bind_name: String,
    },
}

/// Result of a successful match — what the arm produces.
#[derive(Debug, Clone)]
pub enum ResultExpr {
    /// Construct a domain value: `Name(children...)`.
    Construct(String, Vec<ResultExpr>),
    /// Reference a bound value: `@Name`.
    BoundRef(String),
    /// Reference a non-terminal result: `<name>`.
    RuleRef(String),
    /// Literal token value.
    Literal(String),
    /// Collect all values bound under a repeat: `@Name*`.
    CollectRef(String),
}

/// The result of matching a rule against a token stream.
#[derive(Debug, Clone)]
pub enum MatchResult {
    /// A single matched value (token text).
    Value(String),
    /// A tree node with children.
    Node(String, Vec<MatchResult>),
    /// A position marker (used internally by Rest).
    Position(usize),
    /// A list of matched results (from repetition).
    List(Vec<MatchResult>),
}

// ── Grammar (rule collection) ───────────────────────────────────────

/// The grammar — all loaded rules.
pub struct Grammar {
    rules: HashMap<String, Rule>,
}

impl Grammar {
    pub fn new() -> Self {
        Grammar {
            rules: HashMap::new(),
        }
    }

    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.insert(rule.name.clone(), rule);
    }

    pub fn get_rule(&self, name: &str) -> Option<&Rule> {
        self.rules.get(name)
    }

    /// Apply a rule to a token stream starting at `pos`.
    /// Returns `(result, new_pos)` or `None` if no arm matches.
    pub fn apply(
        &self,
        rule_name: &str,
        tokens: &[Token],
        pos: usize,
    ) -> Option<(MatchResult, usize)> {
        let rule = self.rules.get(rule_name)?;
        for arm in &rule.arms {
            if let Some((result, new_pos)) = self.try_arm(arm, tokens, pos) {
                return Some((result, new_pos));
            }
        }
        None
    }

    /// Try to match a single arm against the token stream.
    fn try_arm(
        &self,
        arm: &Arm,
        tokens: &[Token],
        start: usize,
    ) -> Option<(MatchResult, usize)> {
        let mut pos = start;
        let mut bindings: HashMap<String, MatchResult> = HashMap::new();

        for elem in &arm.pattern {
            // Auto-skip newlines between pattern elements
            while pos < tokens.len() && tokens[pos] == Token::Newline {
                pos += 1;
            }

            match elem {
                PatternElement::Terminal(expected) => {
                    if pos < tokens.len() && &tokens[pos] == expected {
                        pos += 1;
                    } else {
                        return None;
                    }
                }
                PatternElement::AnyPascal => {
                    if pos < tokens.len() {
                        if let Token::PascalIdent(_) = &tokens[pos] {
                            pos += 1;
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                PatternElement::AnyCamel => {
                    if pos < tokens.len() {
                        if let Token::CamelIdent(_) = &tokens[pos] {
                            pos += 1;
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                PatternElement::AnyInt => {
                    if pos < tokens.len() {
                        if let Token::Integer(_) = &tokens[pos] {
                            pos += 1;
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                PatternElement::AnyFloat => {
                    if pos < tokens.len() {
                        if let Token::Float(_) = &tokens[pos] {
                            pos += 1;
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                PatternElement::AnyString => {
                    if pos < tokens.len() {
                        if let Token::StringLit(_) = &tokens[pos] {
                            pos += 1;
                        } else {
                            return None;
                        }
                    } else {
                        return None;
                    }
                }
                PatternElement::Bind(name) => {
                    if pos >= tokens.len() {
                        return None;
                    }
                    let value = match &tokens[pos] {
                        Token::PascalIdent(s) | Token::CamelIdent(s) => {
                            MatchResult::Value(s.clone())
                        }
                        Token::Integer(n) => MatchResult::Value(n.to_string()),
                        Token::Float(s) => MatchResult::Value(s.clone()),
                        Token::StringLit(s) => MatchResult::Value(s.clone()),
                        _ => return None,
                    };
                    bindings.insert(name.clone(), value);
                    pos += 1;
                }
                PatternElement::NonTerminal(rule_name) => {
                    let (result, new_pos) = self.apply(rule_name, tokens, pos)?;
                    bindings.insert(rule_name.clone(), result);
                    pos = new_pos;
                }
                PatternElement::Rest(name) => {
                    // Rest consumes everything until a matching closing delimiter.
                    // Bind the remaining position — the closing delimiter in the
                    // pattern will handle stopping.
                    bindings.insert(name.clone(), MatchResult::Position(pos));
                }
                PatternElement::SkipNewlines => {
                    while pos < tokens.len() && tokens[pos] == Token::Newline {
                        pos += 1;
                    }
                }
                PatternElement::Repeat {
                    element,
                    bind_name,
                } => {
                    let mut collected = Vec::new();
                    loop {
                        // Skip newlines between repeated elements
                        while pos < tokens.len() && tokens[pos] == Token::Newline {
                            pos += 1;
                        }
                        // Try matching the sub-element once
                        let mut sub_bindings: HashMap<String, MatchResult> = HashMap::new();
                        if let Some(new_pos) =
                            self.try_single_element(element, tokens, pos, &mut sub_bindings)
                        {
                            // Extract the value that was bound
                            if let Some(val) = sub_bindings.into_values().next() {
                                collected.push(val);
                            } else {
                                // Element matched but bound nothing — use token text
                                if pos < tokens.len() {
                                    collected
                                        .push(MatchResult::Value(tokens[pos].to_string()));
                                }
                            }
                            pos = new_pos;
                        } else {
                            break;
                        }
                    }
                    bindings.insert(bind_name.clone(), MatchResult::List(collected));
                }
            }
        }

        let result = self.build_result(&arm.result, &bindings);
        Some((result, pos))
    }

    /// Try to match a single pattern element, returning new position or None.
    fn try_single_element(
        &self,
        elem: &PatternElement,
        tokens: &[Token],
        pos: usize,
        bindings: &mut HashMap<String, MatchResult>,
    ) -> Option<usize> {
        if pos >= tokens.len() {
            return None;
        }
        match elem {
            PatternElement::Terminal(expected) => {
                if &tokens[pos] == expected {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            PatternElement::AnyPascal => {
                if let Token::PascalIdent(_) = &tokens[pos] {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            PatternElement::AnyCamel => {
                if let Token::CamelIdent(_) = &tokens[pos] {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            PatternElement::AnyInt => {
                if let Token::Integer(_) = &tokens[pos] {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            PatternElement::AnyFloat => {
                if let Token::Float(_) = &tokens[pos] {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            PatternElement::AnyString => {
                if let Token::StringLit(_) = &tokens[pos] {
                    Some(pos + 1)
                } else {
                    None
                }
            }
            PatternElement::Bind(name) => {
                let value = match &tokens[pos] {
                    Token::PascalIdent(s) | Token::CamelIdent(s) => {
                        MatchResult::Value(s.clone())
                    }
                    Token::Integer(n) => MatchResult::Value(n.to_string()),
                    Token::Float(s) => MatchResult::Value(s.clone()),
                    Token::StringLit(s) => MatchResult::Value(s.clone()),
                    _ => return None,
                };
                bindings.insert(name.clone(), value);
                Some(pos + 1)
            }
            PatternElement::NonTerminal(rule_name) => {
                let (result, new_pos) = self.apply(rule_name, tokens, pos)?;
                bindings.insert(rule_name.clone(), result);
                Some(new_pos)
            }
            _ => None,
        }
    }

    /// Build a MatchResult from a ResultExpr using captured bindings.
    fn build_result(
        &self,
        expr: &ResultExpr,
        bindings: &HashMap<String, MatchResult>,
    ) -> MatchResult {
        match expr {
            ResultExpr::BoundRef(name) => bindings
                .get(name)
                .cloned()
                .unwrap_or(MatchResult::Value(name.clone())),
            ResultExpr::RuleRef(name) => bindings
                .get(name)
                .cloned()
                .unwrap_or(MatchResult::Value(name.clone())),
            ResultExpr::Construct(name, children) => {
                let built: Vec<MatchResult> = children
                    .iter()
                    .map(|c| self.build_result(c, bindings))
                    .collect();
                MatchResult::Node(name.clone(), built)
            }
            ResultExpr::Literal(s) => MatchResult::Value(s.clone()),
            ResultExpr::CollectRef(name) => bindings
                .get(name)
                .cloned()
                .unwrap_or(MatchResult::List(Vec::new())),
        }
    }
}

// ── Bootstrap parser ────────────────────────────────────────────────
//
// Reads grammar rules from a token stream.  The bootstrap only knows
// how to parse `<Name> [ arms ]` — it is the seed that makes the rest
// of the grammar self-describing.

/// Parse grammar rules from a token stream.
pub fn parse_grammar_rules(tokens: &[Token]) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut pos = 0;

    while pos < tokens.len() {
        // Skip newlines
        while pos < tokens.len() && tokens[pos] == Token::Newline {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Look for `< Name > [`
        if tokens[pos] == Token::Lt {
            if let Some((rule, new_pos)) = parse_one_rule(tokens, pos) {
                rules.push(rule);
                pos = new_pos;
                continue;
            }
        }
        pos += 1; // skip unknown tokens
    }
    rules
}

fn parse_one_rule(tokens: &[Token], start: usize) -> Option<(Rule, usize)> {
    let mut pos = start;

    // `<`
    if tokens.get(pos)? != &Token::Lt {
        return None;
    }
    pos += 1;

    // Name (pascal or camel)
    let name = match tokens.get(pos)? {
        Token::PascalIdent(s) | Token::CamelIdent(s) => s.clone(),
        _ => return None,
    };
    pos += 1;

    // `>`
    if tokens.get(pos)? != &Token::Gt {
        return None;
    }
    pos += 1;

    // Skip newlines
    while pos < tokens.len() && tokens[pos] == Token::Newline {
        pos += 1;
    }

    // `[`
    if tokens.get(pos)? != &Token::LBracket {
        return None;
    }
    pos += 1;

    // Parse arms until `]`
    let mut arms = Vec::new();
    loop {
        while pos < tokens.len() && tokens[pos] == Token::Newline {
            pos += 1;
        }
        if pos >= tokens.len() {
            return None;
        }
        if tokens[pos] == Token::RBracket {
            pos += 1;
            break;
        }

        if let Some((arm, new_pos)) = parse_one_arm(tokens, pos) {
            arms.push(arm);
            pos = new_pos;
        } else {
            pos += 1; // skip unrecognized token
        }
    }

    Some((Rule { name, arms }, pos))
}

/// Parse one arm: `[ pattern_elements ] => result_expr`
///
/// Pattern syntax inside `[...]`:
///   - `PascalName`       → Terminal(Token::PascalIdent("PascalName"))
///                           BUT `PASCAL` (all-caps shorthand) → see below
///   - `(` `)` `{` `}` `[` `]`  → Terminal delimiters
///   - `<Name>`           → NonTerminal("Name")
///   - `@Name`            → Bind("Name")
///   - `@Name*`           → Repeat { Bind("Name") }
///   - `| @Name`          → Rest("Name")
///   - `_NL`              → SkipNewlines
///
/// Special terminal shorthands (inside patterns):
///   - `LPAREN` → Token::LParen    `RPAREN` → Token::RParen
///   - `LBRACE` → Token::LBrace    `RBRACE` → Token::RBrace
///   - `LBRACKET` → Token::LBracket  `RBRACKET` → Token::RBracket
///   - `BANG` → Token::Bang    `COLON` → Token::Colon
///   - `DOT` → Token::Dot     `EQ` → Token::Eq
///   - `PASCAL` → AnyPascal   `CAMEL` → AnyCamel
///   - `INT` → AnyInt  `FLOAT` → AnyFloat  `STRING` → AnyString
///
/// Result syntax after `=>`:
///   - `Name(children...)`  → Construct
///   - `@Name`              → BoundRef
///   - `<Name>`             → RuleRef
///   - `@Name*`             → CollectRef
///   - `"literal"`          → Literal
fn parse_one_arm(tokens: &[Token], start: usize) -> Option<(Arm, usize)> {
    let mut pos = start;

    // Skip newlines
    while pos < tokens.len() && tokens[pos] == Token::Newline {
        pos += 1;
    }

    // `[` — start of pattern
    if tokens.get(pos)? != &Token::LBracket {
        return None;
    }
    pos += 1;

    // Parse pattern elements until `]`
    let mut pattern = Vec::new();
    loop {
        while pos < tokens.len() && tokens[pos] == Token::Newline {
            pos += 1;
        }
        if pos >= tokens.len() {
            return None;
        }
        if tokens[pos] == Token::RBracket {
            pos += 1;
            break;
        }

        if let Some((elem, new_pos)) = parse_pattern_element(tokens, pos) {
            pattern.push(elem);
            pos = new_pos;
        } else {
            return None; // unrecognized pattern element
        }
    }

    // Skip newlines
    while pos < tokens.len() && tokens[pos] == Token::Newline {
        pos += 1;
    }

    // `=>` is represented as `=` `>` in token stream
    if pos + 1 < tokens.len() && tokens[pos] == Token::Eq && tokens[pos + 1] == Token::Gt {
        pos += 2;
    } else {
        return None;
    }

    // Skip newlines
    while pos < tokens.len() && tokens[pos] == Token::Newline {
        pos += 1;
    }

    // Parse result expression
    let (result, new_pos) = parse_result_expr(tokens, pos)?;
    pos = new_pos;

    Some((Arm { pattern, result }, pos))
}

/// Resolve a PascalCase name to a terminal token shorthand.
fn resolve_terminal_shorthand(name: &str) -> Option<PatternElement> {
    match name {
        "LPAREN" => Some(PatternElement::Terminal(Token::LParen)),
        "RPAREN" => Some(PatternElement::Terminal(Token::RParen)),
        "LBRACE" => Some(PatternElement::Terminal(Token::LBrace)),
        "RBRACE" => Some(PatternElement::Terminal(Token::RBrace)),
        "LBRACKET" => Some(PatternElement::Terminal(Token::LBracket)),
        "RBRACKET" => Some(PatternElement::Terminal(Token::RBracket)),
        "BANG" => Some(PatternElement::Terminal(Token::Bang)),
        "COLON" => Some(PatternElement::Terminal(Token::Colon)),
        "DOT" => Some(PatternElement::Terminal(Token::Dot)),
        "EQ" => Some(PatternElement::Terminal(Token::Eq)),
        "AT" => Some(PatternElement::Terminal(Token::At)),
        "CARET" => Some(PatternElement::Terminal(Token::Caret)),
        "PIPE" => Some(PatternElement::Terminal(Token::Pipe)),
        "TILDE" => Some(PatternElement::Terminal(Token::Tilde)),
        "STAR" => Some(PatternElement::Terminal(Token::Star)),
        "PLUS" => Some(PatternElement::Terminal(Token::Plus)),
        "MINUS" => Some(PatternElement::Terminal(Token::Minus)),
        "SLASH" => Some(PatternElement::Terminal(Token::Slash)),
        "PERCENT" => Some(PatternElement::Terminal(Token::Percent)),
        "COMMA" => Some(PatternElement::Terminal(Token::Comma)),
        "HASH" => Some(PatternElement::Terminal(Token::Hash)),
        "TICK" => Some(PatternElement::Terminal(Token::Tick)),
        "UNDERSCORE" => Some(PatternElement::Terminal(Token::Underscore)),
        "STUB" => Some(PatternElement::Terminal(Token::Stub)),
        "PASCAL" => Some(PatternElement::AnyPascal),
        "CAMEL" => Some(PatternElement::AnyCamel),
        "INT" => Some(PatternElement::AnyInt),
        "FLOAT" => Some(PatternElement::AnyFloat),
        "STRING" => Some(PatternElement::AnyString),
        _ => None,
    }
}

fn parse_pattern_element(tokens: &[Token], pos: usize) -> Option<(PatternElement, usize)> {
    let tok = tokens.get(pos)?;

    match tok {
        // `<Name>` — non-terminal
        Token::Lt => {
            let name = match tokens.get(pos + 1)? {
                Token::PascalIdent(s) | Token::CamelIdent(s) => s.clone(),
                _ => return None,
            };
            if tokens.get(pos + 2)? != &Token::Gt {
                return None;
            }
            Some((PatternElement::NonTerminal(name), pos + 3))
        }
        // `@Name` — bind (or `@Name*` — repeat-bind)
        Token::At => {
            let name = match tokens.get(pos + 1)? {
                Token::PascalIdent(s) | Token::CamelIdent(s) => s.clone(),
                _ => return None,
            };
            // Check for `*` suffix → repeat
            if pos + 2 < tokens.len() && tokens[pos + 2] == Token::Star {
                Some((
                    PatternElement::Repeat {
                        element: Box::new(PatternElement::Bind(name.clone())),
                        bind_name: name,
                    },
                    pos + 3,
                ))
            } else {
                Some((PatternElement::Bind(name), pos + 2))
            }
        }
        // `| @Name` — rest binding
        Token::Pipe => {
            if tokens.get(pos + 1)? != &Token::At {
                return None;
            }
            let name = match tokens.get(pos + 2)? {
                Token::PascalIdent(s) | Token::CamelIdent(s) => s.clone(),
                _ => return None,
            };
            Some((PatternElement::Rest(name), pos + 3))
        }
        // PascalCase — either a shorthand or a literal terminal
        Token::PascalIdent(s) => {
            if let Some(elem) = resolve_terminal_shorthand(s) {
                Some((elem, pos + 1))
            } else {
                // Literal PascalCase token — match exactly this identifier
                Some((
                    PatternElement::Terminal(Token::PascalIdent(s.clone())),
                    pos + 1,
                ))
            }
        }
        // camelCase — literal terminal
        Token::CamelIdent(s) => Some((
            PatternElement::Terminal(Token::CamelIdent(s.clone())),
            pos + 1,
        )),
        // Delimiter tokens as literals in patterns
        Token::LParen => Some((PatternElement::Terminal(Token::LParen), pos + 1)),
        Token::RParen => Some((PatternElement::Terminal(Token::RParen), pos + 1)),
        Token::LBrace => Some((PatternElement::Terminal(Token::LBrace), pos + 1)),
        Token::RBrace => Some((PatternElement::Terminal(Token::RBrace), pos + 1)),
        Token::Bang => Some((PatternElement::Terminal(Token::Bang), pos + 1)),
        Token::Colon => Some((PatternElement::Terminal(Token::Colon), pos + 1)),
        Token::Dot => Some((PatternElement::Terminal(Token::Dot), pos + 1)),
        Token::Eq => Some((PatternElement::Terminal(Token::Eq), pos + 1)),
        Token::Caret => Some((PatternElement::Terminal(Token::Caret), pos + 1)),
        Token::Tilde => Some((PatternElement::Terminal(Token::Tilde), pos + 1)),
        Token::Hash => Some((PatternElement::Terminal(Token::Hash), pos + 1)),
        Token::Underscore => Some((PatternElement::Terminal(Token::Underscore), pos + 1)),

        _ => None,
    }
}

fn parse_result_expr(tokens: &[Token], pos: usize) -> Option<(ResultExpr, usize)> {
    let tok = tokens.get(pos)?;

    match tok {
        // `@Name` — bound ref (or `@Name*` — collect ref)
        Token::At => {
            let name = match tokens.get(pos + 1)? {
                Token::PascalIdent(s) | Token::CamelIdent(s) => s.clone(),
                _ => return None,
            };
            if pos + 2 < tokens.len() && tokens[pos + 2] == Token::Star {
                Some((ResultExpr::CollectRef(name), pos + 3))
            } else {
                Some((ResultExpr::BoundRef(name), pos + 2))
            }
        }
        // `<Name>` — rule ref
        Token::Lt => {
            let name = match tokens.get(pos + 1)? {
                Token::PascalIdent(s) | Token::CamelIdent(s) => s.clone(),
                _ => return None,
            };
            if tokens.get(pos + 2)? != &Token::Gt {
                return None;
            }
            Some((ResultExpr::RuleRef(name), pos + 3))
        }
        // `"literal"` — literal string
        Token::StringLit(s) => Some((ResultExpr::Literal(s.clone()), pos + 1)),
        // `Name(children...)` — construct, or bare `Name` — literal
        Token::PascalIdent(name) => {
            // Check if followed by `(`
            if pos + 1 < tokens.len() && tokens[pos + 1] == Token::LParen {
                let mut children = Vec::new();
                let mut cpos = pos + 2;
                loop {
                    while cpos < tokens.len() && tokens[cpos] == Token::Newline {
                        cpos += 1;
                    }
                    if cpos >= tokens.len() {
                        return None;
                    }
                    if tokens[cpos] == Token::RParen {
                        cpos += 1;
                        break;
                    }
                    let (child, new_cpos) = parse_result_expr(tokens, cpos)?;
                    children.push(child);
                    cpos = new_cpos;
                }
                Some((ResultExpr::Construct(name.clone(), children), cpos))
            } else {
                Some((ResultExpr::Literal(name.clone()), pos + 1))
            }
        }
        Token::CamelIdent(name) => Some((ResultExpr::Literal(name.clone()), pos + 1)),
        _ => None,
    }
}

// ── Pratt expression parser ─────────────────────────────────────────

/// Operator entry in the binding power table.
#[derive(Debug, Clone)]
pub struct OpEntry {
    /// The token that represents this operator.
    pub token: Token,
    /// Left binding power (higher = tighter).
    pub lbp: u32,
    /// Right binding power (for right-associativity, set rbp < lbp).
    pub rbp: u32,
    /// The name to use in the result node.
    pub name: String,
}

/// A data-driven Pratt parser for expressions.
pub struct PrattParser {
    ops: Vec<OpEntry>,
}

impl PrattParser {
    pub fn new() -> Self {
        PrattParser { ops: Vec::new() }
    }

    pub fn add_op(&mut self, entry: OpEntry) {
        self.ops.push(entry);
    }

    /// Look up an operator by token.
    fn find_op(&self, token: &Token) -> Option<&OpEntry> {
        self.ops.iter().find(|e| &e.token == token)
    }

    /// Parse an expression from the token stream using Pratt precedence.
    /// `grammar` is used for parsing atoms (non-operator sub-expressions).
    /// `atom_rule` is the grammar rule name for atoms.
    pub fn parse_expr(
        &self,
        grammar: &Grammar,
        atom_rule: &str,
        tokens: &[Token],
        pos: usize,
        min_bp: u32,
    ) -> Option<(MatchResult, usize)> {
        // Skip newlines
        let mut pos = pos;
        while pos < tokens.len() && tokens[pos] == Token::Newline {
            pos += 1;
        }

        // Parse the left-hand side (atom)
        let (mut left, mut pos) = grammar.apply(atom_rule, tokens, pos)?;

        loop {
            // Skip newlines
            while pos < tokens.len() && tokens[pos] == Token::Newline {
                pos += 1;
            }

            if pos >= tokens.len() {
                break;
            }

            // Check if current token is an operator
            let op = match self.find_op(&tokens[pos]) {
                Some(op) => op.clone(),
                None => break,
            };

            if op.lbp < min_bp {
                break;
            }

            pos += 1; // consume the operator token

            let (right, new_pos) =
                self.parse_expr(grammar, atom_rule, tokens, pos, op.rbp)?;
            pos = new_pos;

            left = MatchResult::Node(op.name.clone(), vec![left, right]);
        }

        Some((left, pos))
    }
}

// ── Programmatic grammar builder ────────────────────────────────────
//
// For building grammars in code (useful for tests and for the initial
// bootstrap before .aski grammar files are self-hosting).

impl Grammar {
    /// Build a grammar for parsing domain declarations:
    /// `Name ( Variant1 Variant2 ... )`
    pub fn with_domain_rule(mut self) -> Self {
        self.add_rule(Rule {
            name: "domain".into(),
            arms: vec![Arm {
                pattern: vec![
                    PatternElement::Bind("Name".into()),
                    PatternElement::Terminal(Token::LParen),
                    PatternElement::Repeat {
                        element: Box::new(PatternElement::Bind("V".into())),
                        bind_name: "Variants".into(),
                    },
                    PatternElement::Terminal(Token::RParen),
                ],
                result: ResultExpr::Construct(
                    "Domain".into(),
                    vec![
                        ResultExpr::BoundRef("Name".into()),
                        ResultExpr::CollectRef("Variants".into()),
                    ],
                ),
            }],
        });
        self
    }

    /// Build a grammar for parsing struct declarations:
    /// `Name { Field1 Type1 Field2 Type2 ... }`
    pub fn with_struct_rule(mut self) -> Self {
        self.add_rule(Rule {
            name: "structDecl".into(),
            arms: vec![Arm {
                pattern: vec![
                    PatternElement::Bind("Name".into()),
                    PatternElement::Terminal(Token::LBrace),
                    PatternElement::Repeat {
                        element: Box::new(PatternElement::NonTerminal("field".into())),
                        bind_name: "Fields".into(),
                    },
                    PatternElement::Terminal(Token::RBrace),
                ],
                result: ResultExpr::Construct(
                    "Struct".into(),
                    vec![
                        ResultExpr::BoundRef("Name".into()),
                        ResultExpr::CollectRef("Fields".into()),
                    ],
                ),
            }],
        });

        // Field: PascalName PascalType
        self.add_rule(Rule {
            name: "field".into(),
            arms: vec![Arm {
                pattern: vec![
                    PatternElement::Bind("FieldName".into()),
                    PatternElement::Bind("FieldType".into()),
                ],
                result: ResultExpr::Construct(
                    "Field".into(),
                    vec![
                        ResultExpr::BoundRef("FieldName".into()),
                        ResultExpr::BoundRef("FieldType".into()),
                    ],
                ),
            }],
        });

        self
    }

    /// Build a grammar for parsing constant declarations:
    /// `!Name Type {value}`
    pub fn with_const_rule(mut self) -> Self {
        self.add_rule(Rule {
            name: "constDecl".into(),
            arms: vec![Arm {
                pattern: vec![
                    PatternElement::Terminal(Token::Bang),
                    PatternElement::Bind("Name".into()),
                    PatternElement::Bind("Type".into()),
                    PatternElement::Terminal(Token::LBrace),
                    PatternElement::Bind("Value".into()),
                    PatternElement::Terminal(Token::RBrace),
                ],
                result: ResultExpr::Construct(
                    "Const".into(),
                    vec![
                        ResultExpr::BoundRef("Name".into()),
                        ResultExpr::BoundRef("Type".into()),
                        ResultExpr::BoundRef("Value".into()),
                    ],
                ),
            }],
        });
        self
    }

    /// Build a grammar with a `sourceFile` rule that tries domain, struct,
    /// and const rules in sequence, repeated.
    pub fn with_source_file_rule(mut self) -> Self {
        // `item` tries each item type
        self.add_rule(Rule {
            name: "item".into(),
            arms: vec![
                Arm {
                    pattern: vec![PatternElement::NonTerminal("constDecl".into())],
                    result: ResultExpr::RuleRef("constDecl".into()),
                },
                Arm {
                    pattern: vec![PatternElement::NonTerminal("structDecl".into())],
                    result: ResultExpr::RuleRef("structDecl".into()),
                },
                Arm {
                    pattern: vec![PatternElement::NonTerminal("domain".into())],
                    result: ResultExpr::RuleRef("domain".into()),
                },
            ],
        });
        self
    }

    /// Load grammar rules from a .aski grammar file source string.
    /// Lexes the source and feeds it through the bootstrap parser.
    pub fn load_grammar_source(&mut self, source: &str) -> Result<usize, String> {
        let spanned = crate::lexer::lex(source).map_err(|errs| {
            errs.into_iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let tokens: Vec<Token> = spanned.into_iter().map(|s| s.token).collect();
        let rules = parse_grammar_rules(&tokens);
        let count = rules.len();
        for rule in rules {
            self.add_rule(rule);
        }
        Ok(count)
    }
}

// ── Convenience: parse all items from a token stream ────────────────

/// Parse all top-level items from a token stream using the grammar engine.
/// Returns a list of MatchResults, one per item.
pub fn parse_items(grammar: &Grammar, tokens: &[Token]) -> Vec<MatchResult> {
    let mut results = Vec::new();
    let mut pos = 0;

    while pos < tokens.len() {
        // Skip newlines
        while pos < tokens.len() && tokens[pos] == Token::Newline {
            pos += 1;
        }
        if pos >= tokens.len() {
            break;
        }

        // Try the `item` rule first, then individual rules
        if let Some((result, new_pos)) = grammar.apply("item", tokens, pos) {
            results.push(result);
            pos = new_pos;
        } else if let Some((result, new_pos)) = grammar.apply("domain", tokens, pos) {
            results.push(result);
            pos = new_pos;
        } else if let Some((result, new_pos)) = grammar.apply("structDecl", tokens, pos) {
            results.push(result);
            pos = new_pos;
        } else if let Some((result, new_pos)) = grammar.apply("constDecl", tokens, pos) {
            results.push(result);
            pos = new_pos;
        } else {
            pos += 1; // skip unrecognized
        }
    }

    results
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer;

    /// Helper: lex source and extract bare tokens.
    fn lex_tokens(source: &str) -> Vec<Token> {
        let spanned = lexer::lex(source).expect("lex failed");
        spanned.into_iter().map(|s| s.token).collect()
    }

    #[test]
    fn parse_domain_with_programmatic_grammar() {
        let grammar = Grammar::new().with_domain_rule();
        let tokens = lex_tokens("Element (Fire Earth Air Water)");

        let (result, consumed) = grammar
            .apply("domain", &tokens, 0)
            .expect("domain rule should match");

        // Should consume all 7 tokens
        assert_eq!(consumed, tokens.len());

        // Result should be Domain(Name, [Variants...])
        match result {
            MatchResult::Node(name, children) => {
                assert_eq!(name, "Domain");
                assert_eq!(children.len(), 2);

                // First child: domain name
                match &children[0] {
                    MatchResult::Value(s) => assert_eq!(s, "Element"),
                    other => panic!("expected Value(Element), got {:?}", other),
                }

                // Second child: variant list
                match &children[1] {
                    MatchResult::List(variants) => {
                        assert_eq!(variants.len(), 4);
                        let names: Vec<&str> = variants
                            .iter()
                            .map(|v| match v {
                                MatchResult::Value(s) => s.as_str(),
                                _ => panic!("expected Value"),
                            })
                            .collect();
                        assert_eq!(names, vec!["Fire", "Earth", "Air", "Water"]);
                    }
                    other => panic!("expected List, got {:?}", other),
                }
            }
            other => panic!("expected Node, got {:?}", other),
        }
    }

    #[test]
    fn parse_struct_with_programmatic_grammar() {
        let grammar = Grammar::new().with_struct_rule();
        let tokens = lex_tokens("Point { Horizontal F64 Vertical F64 }");

        let (result, consumed) = grammar
            .apply("structDecl", &tokens, 0)
            .expect("struct rule should match");

        assert_eq!(consumed, tokens.len());

        match result {
            MatchResult::Node(name, children) => {
                assert_eq!(name, "Struct");
                assert_eq!(children.len(), 2);
                match &children[0] {
                    MatchResult::Value(s) => assert_eq!(s, "Point"),
                    other => panic!("expected Value(Point), got {:?}", other),
                }
                match &children[1] {
                    MatchResult::List(fields) => {
                        assert_eq!(fields.len(), 2);
                    }
                    other => panic!("expected List, got {:?}", other),
                }
            }
            other => panic!("expected Node, got {:?}", other),
        }
    }

    #[test]
    fn parse_const_with_programmatic_grammar() {
        let grammar = Grammar::new().with_const_rule();
        let tokens = lex_tokens("!Pi F64 {3.14159265358979}");

        let (result, consumed) = grammar
            .apply("constDecl", &tokens, 0)
            .expect("const rule should match");

        assert_eq!(consumed, tokens.len());

        match result {
            MatchResult::Node(name, children) => {
                assert_eq!(name, "Const");
                assert_eq!(children.len(), 3);
                match &children[0] {
                    MatchResult::Value(s) => assert_eq!(s, "Pi"),
                    other => panic!("expected Value(Pi), got {:?}", other),
                }
                match &children[1] {
                    MatchResult::Value(s) => assert_eq!(s, "F64"),
                    other => panic!("expected Value(F64), got {:?}", other),
                }
            }
            other => panic!("expected Node, got {:?}", other),
        }
    }

    #[test]
    fn parse_multiple_items() {
        let grammar = Grammar::new()
            .with_domain_rule()
            .with_struct_rule()
            .with_const_rule()
            .with_source_file_rule();

        let source = "\
Element (Fire Earth Air Water)
Modality (Cardinal Fixed Mutable)
Point { Horizontal F64 Vertical F64 }
!Pi F64 {3.14159265358979}";

        let tokens = lex_tokens(source);
        let items = parse_items(&grammar, &tokens);

        assert_eq!(items.len(), 4, "should parse 4 items, got: {:?}", items);
    }

    #[test]
    fn bootstrap_parse_grammar_file() {
        // Grammar file syntax: <Name> [ [pattern] => result ]
        let grammar_source = r#"
<domain> [
  [@Name LPAREN @Variants* RPAREN] => Domain(@Name @Variants*)
]
"#;

        let tokens = lex_tokens(grammar_source);
        let rules = parse_grammar_rules(&tokens);

        assert_eq!(rules.len(), 1, "should parse 1 rule");
        assert_eq!(rules[0].name, "domain");
        assert_eq!(rules[0].arms.len(), 1, "should have 1 arm");
        assert_eq!(rules[0].arms[0].pattern.len(), 4); // @Name LPAREN @Variants* RPAREN
    }

    #[test]
    fn bootstrap_grammar_file_parses_domain() {
        // Load grammar rules from the file syntax
        let grammar_source = r#"
<domain> [
  [@Name LPAREN @Variants* RPAREN] => Domain(@Name @Variants*)
]
"#;

        let mut grammar = Grammar::new();
        let count = grammar
            .load_grammar_source(grammar_source)
            .expect("grammar load failed");
        assert_eq!(count, 1);

        // Now use the loaded grammar to parse actual aski
        let tokens = lex_tokens("Element (Fire Earth Air Water)");
        let (result, consumed) = grammar
            .apply("domain", &tokens, 0)
            .expect("domain rule should match");

        assert_eq!(consumed, tokens.len());

        match result {
            MatchResult::Node(name, children) => {
                assert_eq!(name, "Domain");
                assert_eq!(children.len(), 2);
            }
            other => panic!("expected Node, got {:?}", other),
        }
    }

    #[test]
    fn pratt_parser_basic() {
        let mut grammar = Grammar::new();
        // Atom rule: just matches an integer and binds it
        grammar.add_rule(Rule {
            name: "atom".into(),
            arms: vec![Arm {
                pattern: vec![PatternElement::Bind("V".into())],
                result: ResultExpr::BoundRef("V".into()),
            }],
        });

        let mut pratt = PrattParser::new();
        pratt.add_op(OpEntry {
            token: Token::Plus,
            lbp: 10,
            rbp: 11,
            name: "Add".into(),
        });
        pratt.add_op(OpEntry {
            token: Token::Star,
            lbp: 20,
            rbp: 21,
            name: "Mul".into(),
        });

        // Parse: 1 + 2 * 3
        let tokens = lex_tokens("1 + 2 * 3");
        let (result, _) = pratt
            .parse_expr(&grammar, "atom", &tokens, 0, 0)
            .expect("pratt parse should succeed");

        // Should be Add(1, Mul(2, 3)) due to precedence
        match result {
            MatchResult::Node(name, children) => {
                assert_eq!(name, "Add");
                assert_eq!(children.len(), 2);
                match &children[0] {
                    MatchResult::Value(s) => assert_eq!(s, "1"),
                    other => panic!("expected Value(1), got {:?}", other),
                }
                match &children[1] {
                    MatchResult::Node(name, _) => assert_eq!(name, "Mul"),
                    other => panic!("expected Mul node, got {:?}", other),
                }
            }
            other => panic!("expected Add node, got {:?}", other),
        }
    }
}
