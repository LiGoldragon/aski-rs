//! Grammar configuration loaded from .aski data files.
//!
//! Parses grammar/operators.aski, grammar/kernel.aski, grammar/tokens.aski
//! and provides lookup functions used by the parser engine.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::BinOp;
use crate::lexer::Token;

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
    operators: HashMap<String, (BinOp, BindingPower)>,
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
            ("Or", BinOp::Or, 5, 6),
            ("And", BinOp::And, 10, 11),
            ("DoubleEq", BinOp::Eq, 20, 21),
            ("Neq", BinOp::Neq, 20, 21),
            ("Lt", BinOp::Lt, 25, 26),
            ("Gt", BinOp::Gt, 25, 26),
            ("Lte", BinOp::Lte, 25, 26),
            ("Gte", BinOp::Gte, 25, 26),
            ("Plus", BinOp::Add, 30, 31),
            ("Minus", BinOp::Sub, 30, 31),
            ("Star", BinOp::Mul, 40, 41),
            ("Slash", BinOp::Div, 40, 41),
            ("Percent", BinOp::Rem, 40, 41),
        ];
        for (name, op, lbp, rbp) in entries {
            operators.insert(name.to_string(), (op, BindingPower { lbp, rbp }));
        }

        let kernel_primitives: HashSet<String> = [
            "sin", "cos", "sqrt", "abs",
            "truncate", "toF32", "toU32", "toI64",
            "fromOrdinal",
            "len", "clone", "to_string", "is_empty", "unwrap",
        ].iter().map(|s| s.to_string()).collect();

        let mut token_classes = HashMap::new();
        let class_entries: Vec<(TokenClass, Vec<&str>)> = vec![
            (TokenClass::Delimiter, vec!["LParen", "RParen", "LBracket", "RBracket", "LBrace", "RBrace"]),
            (TokenClass::Operator, vec!["Plus", "Minus", "Star", "Slash", "Percent", "DoubleEq", "Neq", "Lt", "Gt", "Lte", "Gte", "And", "Or"]),
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
        let mut operators = HashMap::new();

        for line in content.lines() {
            let line = strip_comment(line).trim();
            if line.is_empty() {
                continue;
            }

            // Format: TokenName BinOp (lbp rbp)
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                return Err(GrammarLoadError {
                    file: "operators.aski".to_string(),
                    message: format!("invalid operator line: {}", line),
                });
            }

            let token_name = parts[0].to_string();
            let binop_name = parts[1];

            // Parse (lbp rbp) — strip parens
            let lbp_str = parts[2].trim_start_matches('(');
            let rbp_str = parts[3].trim_end_matches(')');

            let lbp: u8 = lbp_str.parse().map_err(|_| GrammarLoadError {
                file: "operators.aski".to_string(),
                message: format!("invalid lbp '{}' in line: {}", lbp_str, line),
            })?;
            let rbp: u8 = rbp_str.parse().map_err(|_| GrammarLoadError {
                file: "operators.aski".to_string(),
                message: format!("invalid rbp '{}' in line: {}", rbp_str, line),
            })?;

            let binop = parse_binop_name(binop_name).ok_or_else(|| GrammarLoadError {
                file: "operators.aski".to_string(),
                message: format!("unknown BinOp '{}' in line: {}", binop_name, line),
            })?;

            operators.insert(token_name, (binop, BindingPower { lbp, rbp }));
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
        let mut primitives = HashSet::new();

        // Find the KernelPrimitive ( ... ) block
        let mut in_block = false;
        for line in content.lines() {
            let line = strip_comment(line).trim().to_string();
            if line.is_empty() {
                continue;
            }

            if line.starts_with("KernelPrimitive") {
                in_block = true;
                // Handle inline: KernelPrimitive (name1 name2)
                if let Some(rest) = line.strip_prefix("KernelPrimitive") {
                    let rest = rest.trim();
                    if let Some(inner) = rest.strip_prefix('(') {
                        for word in inner.split_whitespace() {
                            let word = word.trim_end_matches(')');
                            if !word.is_empty() {
                                primitives.insert(word.to_string());
                            }
                        }
                        if rest.contains(')') {
                            in_block = false;
                        }
                    }
                }
                continue;
            }

            if in_block {
                for word in line.split_whitespace() {
                    let word = word.trim_end_matches(')');
                    if !word.is_empty() && word != "(" {
                        primitives.insert(word.to_string());
                    }
                }
                if line.contains(')') {
                    in_block = false;
                }
            }
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
        let mut classes: HashMap<String, HashSet<TokenClass>> = HashMap::new();

        for line in content.lines() {
            let line = strip_comment(line).trim().to_string();
            if line.is_empty() {
                continue;
            }

            // Format: ClassName (Token1 Token2 ...)
            let paren_pos = match line.find('(') {
                Some(p) => p,
                None => continue,
            };
            let class_name = line[..paren_pos].trim();
            let class = match class_name {
                "Delimiter" => TokenClass::Delimiter,
                "Operator" => TokenClass::Operator,
                "Prefix" => TokenClass::Prefix,
                "Compound" => TokenClass::Compound,
                _ => continue, // unknown class, skip
            };

            let inner = &line[paren_pos + 1..];
            let inner = inner.trim_end_matches(')');
            for token_name in inner.split_whitespace() {
                classes
                    .entry(token_name.to_string())
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

fn strip_comment(line: &str) -> &str {
    match line.find(";;") {
        Some(pos) => &line[..pos],
        None => line,
    }
}

fn parse_binop_name(name: &str) -> Option<BinOp> {
    match name {
        "Add" => Some(BinOp::Add),
        "Sub" => Some(BinOp::Sub),
        "Mul" => Some(BinOp::Mul),
        "Div" => Some(BinOp::Div),
        "Rem" => Some(BinOp::Rem),
        "Eq" => Some(BinOp::Eq),
        "Neq" => Some(BinOp::Neq),
        "Lt" => Some(BinOp::Lt),
        "Gt" => Some(BinOp::Gt),
        "Lte" => Some(BinOp::Lte),
        "Gte" => Some(BinOp::Gte),
        "And" => Some(BinOp::And),
        "Or" => Some(BinOp::Or),
        _ => None,
    }
}

/// Map a Token variant to its grammar name string.
/// This is the bridge between lexer tokens and grammar data tables.
fn token_variant_name(token: &Token) -> &'static str {
    match token {
        Token::Plus => "Plus",
        Token::Minus => "Minus",
        Token::Star => "Star",
        Token::Slash => "Slash",
        Token::Percent => "Percent",
        Token::DoubleEq => "DoubleEq",
        Token::Neq => "Neq",
        Token::Lt => "Lt",
        Token::Gt => "Gt",
        Token::Lte => "Lte",
        Token::Gte => "Gte",
        Token::And => "And",
        Token::Or => "Or",
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

/// Resolve the grammar directory, searching standard locations.
/// Order: ASKI_GRAMMAR_DIR env var, then ./grammar/, then CARGO_MANIFEST_DIR/grammar/.
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

    // 3. Relative to CARGO_MANIFEST_DIR (for tests and cargo builds)
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        let p = PathBuf::from(manifest).join("grammar");
        if p.is_dir() {
            return Some(p);
        }
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
        // All 13 operators present
        assert!(config.operator_bp(&Token::Plus).is_some());
        assert!(config.operator_bp(&Token::Minus).is_some());
        assert!(config.operator_bp(&Token::Star).is_some());
        assert!(config.operator_bp(&Token::Slash).is_some());
        assert!(config.operator_bp(&Token::Percent).is_some());
        assert!(config.operator_bp(&Token::DoubleEq).is_some());
        assert!(config.operator_bp(&Token::Neq).is_some());
        assert!(config.operator_bp(&Token::Lt).is_some());
        assert!(config.operator_bp(&Token::Gt).is_some());
        assert!(config.operator_bp(&Token::Lte).is_some());
        assert!(config.operator_bp(&Token::Gte).is_some());
        assert!(config.operator_bp(&Token::And).is_some());
        assert!(config.operator_bp(&Token::Or).is_some());
    }

    #[test]
    fn config_bootstrap_precedence_order() {
        let config = GrammarConfig::bootstrap();
        let (_, or_bp) = config.operator_bp(&Token::Or).unwrap();
        let (_, and_bp) = config.operator_bp(&Token::And).unwrap();
        let (_, eq_bp) = config.operator_bp(&Token::DoubleEq).unwrap();
        let (_, lt_bp) = config.operator_bp(&Token::Lt).unwrap();
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
        // Uses CARGO_MANIFEST_DIR which points to aski-rs root
        let dir = find_grammar_dir().expect("grammar dir should be findable in test");
        let config = GrammarConfig::load_from_dir(&dir).expect("grammar files should parse");

        // Verify operators loaded from file match bootstrap
        assert!(config.operator_bp(&Token::Plus).is_some());
        assert!(config.operator_bp(&Token::Star).is_some());
        let (op, bp) = config.operator_bp(&Token::Plus).unwrap();
        assert_eq!(op, BinOp::Add);
        assert_eq!(bp.lbp, 30);
        assert_eq!(bp.rbp, 31);

        // Verify kernel primitives
        assert!(config.is_kernel_primitive("sin"));
        assert!(config.is_kernel_primitive("unwrap"));

        // Verify token classes
        assert!(config.has_class("LParen", TokenClass::Delimiter));
        assert!(config.has_class("Plus", TokenClass::Operator));
        assert!(config.has_class("Caret", TokenClass::Prefix));
    }

    #[test]
    fn config_file_and_bootstrap_agree() {
        let dir = find_grammar_dir().expect("grammar dir should be findable in test");
        let file_config = GrammarConfig::load_from_dir(&dir).expect("grammar files should parse");
        let boot_config = GrammarConfig::bootstrap();

        // Every operator in bootstrap should be in file config with same values
        let test_tokens = [
            Token::Plus, Token::Minus, Token::Star, Token::Slash, Token::Percent,
            Token::DoubleEq, Token::Neq, Token::Lt, Token::Gt, Token::Lte, Token::Gte,
            Token::And, Token::Or,
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

        // Every kernel primitive in bootstrap should be in file config
        for prim in boot_config.kernel_primitives() {
            assert!(file_config.is_kernel_primitive(prim),
                "file config missing kernel primitive '{}'", prim);
        }
    }

    #[test]
    fn config_changing_operator_table_changes_parse() {
        // Verify that operator precedence is actually data-driven:
        // Create a config where + has HIGHER precedence than *
        let mut config = GrammarConfig::bootstrap();
        // Swap: make Plus have mul's bp (40,41) and Star have add's bp (30,31)
        config.operators.insert("Plus".to_string(), (BinOp::Add, BindingPower { lbp: 40, rbp: 41 }));
        config.operators.insert("Star".to_string(), (BinOp::Mul, BindingPower { lbp: 30, rbp: 31 }));

        let (_, add_bp) = config.operator_bp(&Token::Plus).unwrap();
        let (_, mul_bp) = config.operator_bp(&Token::Star).unwrap();
        // Now + binds tighter than *
        assert!(add_bp.lbp > mul_bp.lbp);
    }
}
