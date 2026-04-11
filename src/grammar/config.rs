//! Grammar configuration loaded from .aski data files.
//!
//! Parses grammar/operators.aski, grammar/kernel.aski, grammar/tokens.aski
//! and provides lookup functions used by the parser engine.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::lexer::Token;

/// Binary operator kinds — used for Pratt parser precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Addition,
    Subtraction,
    Multiplication,
    Division,
    Remainder,
    Equal,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    LogicalAnd,
    LogicalOr,
}

/// Binding power pair for a binary operator.
#[derive(Debug, Clone, Copy)]
pub struct BindingPower {
    pub lbp: u8,
    pub rbp: u8,
}

/// Token classification categories from tokens.aski.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenClass {
    Delimiter,
    Operator,
    Prefix,
    Compound,
}

/// Grammar configuration — loaded from .aski files.
/// Drives all data-dependent decisions in the parser engine.
#[derive(Debug, Clone)]
pub struct GrammarConfig {
    /// Token variant name -> (BinOp, BindingPower)
    pub operators: HashMap<String, (BinOp, BindingPower)>,
    /// Kernel primitive method names
    kernel_primitives: HashSet<String>,
    /// Token variant name -> set of classifications
    token_classes: HashMap<String, HashSet<TokenClass>>,
}

/// Error type for grammar loading failures.
#[derive(Debug)]
pub struct GrammarLoadError {
    pub file: String,
    pub message: String,
}

impl std::fmt::Display for GrammarLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "grammar load error in {}: {}", self.file, self.message)
    }
}

impl GrammarConfig {
    /// Load grammar configuration from .aski files in the given directory.
    pub fn load_from_dir(grammar_dir: &Path) -> Result<Self, GrammarLoadError> {
        let operators_path = grammar_dir.join("operators.aski");
        let kernel_path = grammar_dir.join("kernel.aski");
        let tokens_path = grammar_dir.join("tokens.aski");

        let operators = Self::parse_operators(&operators_path)?;
        let kernel_primitives = Self::parse_kernel(&kernel_path)?;
        let token_classes = Self::parse_tokens(&tokens_path)?;

        Ok(GrammarConfig {
            operators,
            kernel_primitives,
            token_classes,
        })
    }

    /// Build grammar configuration programmatically (for bootstrap / testing).
    pub fn bootstrap() -> Self {
        let mut operators = HashMap::new();
        let entries: Vec<(&str, BinOp, u8, u8)> = vec![
            ("LogicalOr", BinOp::LogicalOr, 5, 6),
            ("LogicalAnd", BinOp::LogicalAnd, 10, 11),
            ("DoubleEquals", BinOp::Equal, 20, 21),
            ("NotEqual", BinOp::NotEqual, 20, 21),
            ("LessThan", BinOp::LessThan, 25, 26),
            ("GreaterThan", BinOp::GreaterThan, 25, 26),
            ("LessThanOrEqual", BinOp::LessThanOrEqual, 25, 26),
            ("GreaterThanOrEqual", BinOp::GreaterThanOrEqual, 25, 26),
            ("Plus", BinOp::Addition, 30, 31),
            ("Minus", BinOp::Subtraction, 30, 31),
            ("Star", BinOp::Multiplication, 40, 41),
            ("Slash", BinOp::Division, 40, 41),
            ("Percent", BinOp::Remainder, 40, 41),
        ];
        for (name, op, lbp, rbp) in entries {
            operators.insert(name.to_string(), (op, BindingPower { lbp, rbp }));
        }

        let kernel_primitives: HashSet<String> = [
            "sin", "cos", "sqrt", "abs",
            "truncate", "toF32", "toU32", "toI64",
            "fromOrdinal",
            "len", "clone", "to_string", "is_empty", "unwrap",
            "toSnake", "toRustType", "toParamType", "stripVec", "allFieldsCopy", "needsPascalAlias",
            "withPush", "containsStr", "isLowerFirst",
        ].iter().map(|s| s.to_string()).collect();

        let mut token_classes = HashMap::new();
        let class_entries: Vec<(TokenClass, Vec<&str>)> = vec![
            (TokenClass::Delimiter, vec!["LParen", "RParen", "LBracket", "RBracket", "LBrace", "RBrace"]),
            (TokenClass::Operator, vec!["Plus", "Minus", "Star", "Slash", "Percent", "DoubleEquals", "NotEqual", "LessThan", "GreaterThan", "LessThanOrEqual", "GreaterThanOrEqual", "LogicalAnd", "LogicalOr"]),
            (TokenClass::Prefix, vec!["Caret", "Hash", "Bang", "At", "Colon", "Tilde"]),
            (TokenClass::Compound, vec!["CompositionOpen", "CompositionClose", "TraitBoundOpen", "TraitBoundClose", "IterOpen", "IterClose"]),
        ];
        for (class, names) in class_entries {
            for name in names {
                token_classes.entry(name.to_string())
                    .or_insert_with(HashSet::new)
                    .insert(class);
            }
        }

        GrammarConfig {
            operators,
            kernel_primitives,
            token_classes,
        }
    }

    /// Set an operator entry (for testing / dynamic reconfiguration).
    pub fn set_operator(&mut self, token_name: &str, op: BinOp, bp: BindingPower) {
        self.operators.insert(token_name.to_string(), (op, bp));
    }

    // ── Operator queries ────────────────────────────────────────

    /// Look up the binding power for a token. Returns None if not an operator.
    pub fn operator_bp(&self, token: &Token) -> Option<(BinOp, BindingPower)> {
        let name = token_variant_name(token);
        self.operators.get(name).cloned()
    }

    /// Get the minimum binding power (for the Pratt parser entry point).
    /// Returns 0 — callers use this as the initial min_bp.
    pub fn min_bp(&self) -> u8 {
        0
    }

    // ── Kernel primitive queries ────────────────────────────────

    /// Check if a name is a kernel primitive.
    pub fn is_kernel_primitive(&self, name: &str) -> bool {
        self.kernel_primitives.contains(name)
    }

    /// Get all kernel primitives as a sorted vec (for deterministic output).
    pub fn kernel_primitives(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.kernel_primitives.iter().map(|s| s.as_str()).collect();
        v.sort();
        v
    }

    // ── Token classification queries ────────────────────────────

    /// Check if a token variant name has a given classification.
    pub fn has_class(&self, token_name: &str, class: TokenClass) -> bool {
        self.token_classes
            .get(token_name)
            .map_or(false, |classes| classes.contains(&class))
    }

    /// Check if a token is classified as an operator.
    pub fn is_operator_token(&self, token: &Token) -> bool {
        let name = token_variant_name(token);
        self.has_class(name, TokenClass::Operator)
    }

    // ── Grammar file parsers ────────────────────────────────────

    fn parse_operators(path: &Path) -> Result<HashMap<String, (BinOp, BindingPower)>, GrammarLoadError> {
        let content = read_grammar_file(path, "operators.aski")?;

        // Parse to World, then extract constants from ParseNodes
        let world = parse_grammar_source_to_world(&content).map_err(|e| GrammarLoadError {
            file: "operators.aski".to_string(),
            message: format!("aski parse failed: {}", e),
        })?;

        // Extract constants: Const nodes with IntLit children
        // Body can be: direct IntLit child, or Block/TailBlock wrapping IntLit
        let mut constants: HashMap<String, u8> = HashMap::new();
        for node in &world.parse_nodes {
            if node.constructor == "Const" {
                let name = node.text.clone();
                let children = aski_core::query_parse_children(&world, node.id);
                for child in &children {
                    match child.constructor.as_str() {
                        "IntLit" => {
                            if let Ok(v) = child.text.parse::<i64>() {
                                constants.insert(name.clone(), v as u8);
                            }
                        }
                        "FloatLit" => {
                            if let Ok(v) = child.text.parse::<f64>() {
                                constants.insert(name.clone(), v as u8);
                            }
                        }
                        "Block" | "TailBlock" => {
                            let body_children = aski_core::query_parse_children(&world, child.id);
                            for bc in &body_children {
                                if bc.constructor == "IntLit" {
                                    if let Ok(v) = bc.text.parse::<i64>() {
                                        constants.insert(name.clone(), v as u8);
                                    }
                                } else if bc.constructor == "FloatLit" {
                                    if let Ok(v) = bc.text.parse::<f64>() {
                                        constants.insert(name.clone(), v as u8);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let op_groups: Vec<(&str, &str, BinOp)> = vec![
            ("LogicalOr",  "LogicalOr",        BinOp::LogicalOr),
            ("LogicalAnd", "LogicalAnd",        BinOp::LogicalAnd),
            ("Equal",      "DoubleEquals",      BinOp::Equal),
            ("Comparison", "LessThan",          BinOp::LessThan),
            ("Addition",   "Plus",              BinOp::Addition),
            ("Multiplication", "Star",          BinOp::Multiplication),
        ];

        let mut operators = HashMap::new();

        for (prefix, _token, _op) in &op_groups {
            let left_key = format!("{}Left", prefix);
            let right_key = format!("{}Right", prefix);
            if let (Some(&lbp), Some(&rbp)) = (constants.get(&left_key), constants.get(&right_key)) {
                match *prefix {
                    "LogicalOr" => {
                        operators.insert("LogicalOr".to_string(), (BinOp::LogicalOr, BindingPower { lbp, rbp }));
                    }
                    "LogicalAnd" => {
                        operators.insert("LogicalAnd".to_string(), (BinOp::LogicalAnd, BindingPower { lbp, rbp }));
                    }
                    "Equal" => {
                        operators.insert("DoubleEquals".to_string(), (BinOp::Equal, BindingPower { lbp, rbp }));
                        operators.insert("NotEqual".to_string(), (BinOp::NotEqual, BindingPower { lbp, rbp }));
                    }
                    "Comparison" => {
                        operators.insert("LessThan".to_string(), (BinOp::LessThan, BindingPower { lbp, rbp }));
                        operators.insert("GreaterThan".to_string(), (BinOp::GreaterThan, BindingPower { lbp, rbp }));
                        operators.insert("LessThanOrEqual".to_string(), (BinOp::LessThanOrEqual, BindingPower { lbp, rbp }));
                        operators.insert("GreaterThanOrEqual".to_string(), (BinOp::GreaterThanOrEqual, BindingPower { lbp, rbp }));
                    }
                    "Addition" => {
                        operators.insert("Plus".to_string(), (BinOp::Addition, BindingPower { lbp, rbp }));
                        operators.insert("Minus".to_string(), (BinOp::Subtraction, BindingPower { lbp, rbp }));
                    }
                    "Multiplication" => {
                        operators.insert("Star".to_string(), (BinOp::Multiplication, BindingPower { lbp, rbp }));
                        operators.insert("Slash".to_string(), (BinOp::Division, BindingPower { lbp, rbp }));
                        operators.insert("Percent".to_string(), (BinOp::Remainder, BindingPower { lbp, rbp }));
                    }
                    _ => {}
                }
            }
        }

        if operators.is_empty() {
            return Err(GrammarLoadError {
                file: "operators.aski".to_string(),
                message: "no operators defined".to_string(),
            });
        }

        Ok(operators)
    }

    fn parse_kernel(path: &Path) -> Result<HashSet<String>, GrammarLoadError> {
        let content = read_grammar_file(path, "kernel.aski")?;

        let world = parse_grammar_source_to_world(&content).map_err(|e| GrammarLoadError {
            file: "kernel.aski".to_string(),
            message: format!("aski parse failed: {}", e),
        })?;

        let mut primitives = HashSet::new();
        let pascal_to_method: HashMap<&str, &str> = [
            ("Sin", "sin"), ("Cos", "cos"), ("Sqrt", "sqrt"), ("Abs", "abs"),
            ("Truncate", "truncate"), ("ToF32", "toF32"), ("ToU32", "toU32"), ("ToI64", "toI64"),
            ("FromOrdinal", "fromOrdinal"),
            ("Len", "len"), ("Clone", "clone"), ("ToString", "to_string"),
            ("IsEmpty", "is_empty"), ("Unwrap", "unwrap"),
            ("ToSnake", "toSnake"), ("ToRustType", "toRustType"), ("ToParamType", "toParamType"),
            ("StripVec", "stripVec"), ("AllFieldsCopy", "allFieldsCopy"), ("NeedsPascalAlias", "needsPascalAlias"),
            ("WithPush", "withPush"), ("ContainsStr", "containsStr"), ("IsLowerFirst", "isLowerFirst"),
        ].iter().cloned().collect();

        // Find Domain named "KernelPrimitive" and extract variant names
        let variants = aski_core::query_domain_variants(&world, "KernelPrimitive");
        for (_, variant_name, _) in &variants {
            let name = pascal_to_method
                .get(variant_name.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| variant_name.clone());
            primitives.insert(name);
        }

        if primitives.is_empty() {
            return Err(GrammarLoadError {
                file: "kernel.aski".to_string(),
                message: "no kernel primitives defined".to_string(),
            });
        }

        Ok(primitives)
    }

    fn parse_tokens(path: &Path) -> Result<HashMap<String, HashSet<TokenClass>>, GrammarLoadError> {
        let content = read_grammar_file(path, "tokens.aski")?;

        let world = parse_grammar_source_to_world(&content).map_err(|e| GrammarLoadError {
            file: "tokens.aski".to_string(),
            message: format!("aski parse failed: {}", e),
        })?;

        let mut classes: HashMap<String, HashSet<TokenClass>> = HashMap::new();
        // Check each domain by name
        for domain_name in &["Delimiter", "Operator", "Prefix", "Compound"] {
            let class = match *domain_name {
                "Delimiter" => TokenClass::Delimiter,
                "Operator" => TokenClass::Operator,
                "Prefix" => TokenClass::Prefix,
                "Compound" => TokenClass::Compound,
                _ => continue,
            };
            let variants = aski_core::query_domain_variants(&world, domain_name);
            for (_, variant_name, _) in &variants {
                classes
                    .entry(variant_name.clone())
                    .or_insert_with(HashSet::new)
                    .insert(class);
            }
        }

        Ok(classes)
    }
}

// ── Helper functions ────────────────────────────────────────────

fn read_grammar_file(path: &Path, name: &str) -> Result<String, GrammarLoadError> {
    std::fs::read_to_string(path).map_err(|e| GrammarLoadError {
        file: name.to_string(),
        message: format!("failed to read {}: {}", path.display(), e),
    })
}

/// Parse aski source using bootstrap config to avoid infinite recursion.
/// Returns a World with ParseNodes and schema relations.
fn parse_grammar_source_to_world(source: &str) -> Result<aski_core::World, String> {
    let config = GrammarConfig::bootstrap();
    super::parse_to_world_with_config(source, &config)
}


/// Map a Token variant to its grammar name string.
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
        Token::Caret => "Caret",
        Token::Hash => "Hash",
        Token::Bang => "Bang",
        Token::At => "At",
        Token::Colon => "Colon",
        Token::Tilde => "Tilde",
        Token::CompositionOpen => "CompositionOpen",
        Token::CompositionClose => "CompositionClose",
        Token::TraitBoundOpen => "TraitBoundOpen",
        Token::TraitBoundClose => "TraitBoundClose",
        Token::IterOpen => "IterOpen",
        Token::IterClose => "IterClose",
        _ => "Unknown",
    }
}

/// aski-rs's own grammar directory, resolved at compile time.
const ASKI_RS_GRAMMAR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/grammar");

/// Resolve the grammar directory, searching multiple locations.
pub fn find_grammar_dir() -> Option<PathBuf> {
    // 1. Env var override
    if let Ok(dir) = std::env::var("ASKI_GRAMMAR_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() {
            return Some(p);
        }
    }

    // 2. Relative to current working directory
    let cwd = PathBuf::from("grammar");
    if cwd.is_dir() {
        return Some(cwd);
    }

    // 3. Relative to CARGO_MANIFEST_DIR of the calling crate
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest).join("grammar");
        if p.is_dir() {
            return Some(p);
        }
    }

    // 4. aski-rs's own grammar directory (always exists)
    let aski_rs = PathBuf::from(ASKI_RS_GRAMMAR_DIR);
    if aski_rs.is_dir() {
        return Some(aski_rs);
    }

    None
}

/// Load grammar config, falling back to bootstrap if files are unavailable.
pub fn load_or_bootstrap() -> GrammarConfig {
    match find_grammar_dir() {
        Some(dir) => match GrammarConfig::load_from_dir(&dir) {
            Ok(config) => config,
            Err(e) => {
                eprintln!("warning: grammar load failed ({}), using bootstrap", e);
                GrammarConfig::bootstrap()
            }
        },
        None => GrammarConfig::bootstrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_bootstrap_has_all_operators() {
        let config = GrammarConfig::bootstrap();
        assert!(config.operator_bp(&Token::Plus).is_some());
        assert!(config.operator_bp(&Token::Minus).is_some());
        assert!(config.operator_bp(&Token::Star).is_some());
        assert!(config.operator_bp(&Token::Slash).is_some());
        assert!(config.operator_bp(&Token::Percent).is_some());
        assert!(config.operator_bp(&Token::DoubleEquals).is_some());
        assert!(config.operator_bp(&Token::NotEqual).is_some());
        assert!(config.operator_bp(&Token::LessThan).is_some());
        assert!(config.operator_bp(&Token::GreaterThan).is_some());
        assert!(config.operator_bp(&Token::LessThanOrEqual).is_some());
        assert!(config.operator_bp(&Token::GreaterThanOrEqual).is_some());
        assert!(config.operator_bp(&Token::LogicalAnd).is_some());
        assert!(config.operator_bp(&Token::LogicalOr).is_some());
    }

    #[test]
    fn config_bootstrap_precedence_order() {
        let config = GrammarConfig::bootstrap();
        let (_, or_bp) = config.operator_bp(&Token::LogicalOr).unwrap();
        let (_, and_bp) = config.operator_bp(&Token::LogicalAnd).unwrap();
        let (_, eq_bp) = config.operator_bp(&Token::DoubleEquals).unwrap();
        let (_, lt_bp) = config.operator_bp(&Token::LessThan).unwrap();
        let (_, add_bp) = config.operator_bp(&Token::Plus).unwrap();
        let (_, mul_bp) = config.operator_bp(&Token::Star).unwrap();

        assert!(or_bp.lbp < and_bp.lbp);
        assert!(and_bp.lbp < eq_bp.lbp);
        assert!(eq_bp.lbp < lt_bp.lbp);
        assert!(lt_bp.lbp < add_bp.lbp);
        assert!(add_bp.lbp < mul_bp.lbp);
    }

    #[test]
    fn config_bootstrap_kernel_primitives() {
        let config = GrammarConfig::bootstrap();
        assert!(config.is_kernel_primitive("sin"));
        assert!(config.is_kernel_primitive("truncate"));
        assert!(config.is_kernel_primitive("len"));
        assert!(!config.is_kernel_primitive("foobar"));
    }

    #[test]
    fn config_loads_from_grammar_dir() {
        let dir = find_grammar_dir().expect("grammar dir should be findable in test");
        let config = GrammarConfig::load_from_dir(&dir).expect("grammar files should parse");

        assert!(config.operator_bp(&Token::Plus).is_some());
        assert!(config.operator_bp(&Token::Star).is_some());
        let (op, bp) = config.operator_bp(&Token::Plus).unwrap();
        assert_eq!(op, BinOp::Addition);
        assert_eq!(bp.lbp, 30);
        assert_eq!(bp.rbp, 31);

        assert!(config.is_kernel_primitive("sin"));
        assert!(config.is_kernel_primitive("unwrap"));

        assert!(config.has_class("LParen", TokenClass::Delimiter));
        assert!(config.has_class("Plus", TokenClass::Operator));
        assert!(config.has_class("Caret", TokenClass::Prefix));
    }

    #[test]
    fn config_file_and_bootstrap_agree() {
        let dir = find_grammar_dir().expect("grammar dir should be findable in test");
        let file_config = GrammarConfig::load_from_dir(&dir).expect("grammar files should parse");
        let boot_config = GrammarConfig::bootstrap();

        let test_tokens = [
            Token::Plus, Token::Minus, Token::Star, Token::Slash, Token::Percent,
            Token::DoubleEquals, Token::NotEqual, Token::LessThan, Token::GreaterThan, Token::LessThanOrEqual, Token::GreaterThanOrEqual,
            Token::LogicalAnd, Token::LogicalOr,
        ];
        for token in &test_tokens {
            let file_entry = file_config.operator_bp(token);
            let boot_entry = boot_config.operator_bp(token);
            assert!(file_entry.is_some(), "file missing operator {:?}", token);
            assert!(boot_entry.is_some(), "bootstrap missing operator {:?}", token);
            let (fop, fbp) = file_entry.unwrap();
            let (bop, bbp) = boot_entry.unwrap();
            assert_eq!(fop, bop, "BinOp mismatch for {:?}", token);
            assert_eq!(fbp.lbp, bbp.lbp, "lbp mismatch for {:?}", token);
            assert_eq!(fbp.rbp, bbp.rbp, "rbp mismatch for {:?}", token);
        }

        for prim in boot_config.kernel_primitives() {
            assert!(file_config.is_kernel_primitive(prim),
                "file config missing kernel primitive '{}'", prim);
        }
    }

    #[test]
    fn config_changing_operator_table_changes_parse() {
        let mut config = GrammarConfig::bootstrap();
        config.operators.insert("Plus".to_string(), (BinOp::Addition, BindingPower { lbp: 40, rbp: 41 }));
        config.operators.insert("Star".to_string(), (BinOp::Multiplication, BindingPower { lbp: 30, rbp: 31 }));

        let (_, add_bp) = config.operator_bp(&Token::Plus).unwrap();
        let (_, mul_bp) = config.operator_bp(&Token::Star).unwrap();
        assert!(add_bp.lbp > mul_bp.lbp);
    }
}
