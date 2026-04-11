//! Grammar rule engine — aski defines its own syntax.
//!
//! Grammar rules in grammar/*.aski files define the parsing logic.
//! A bootstrap parser reads these files into a rule table.
//! A PEG interpreter executes rules against token streams.
//! ParseNodes are written directly to a World — no AST layer.

pub mod bootstrap;
pub mod config;
pub mod interpreter;

use std::collections::HashMap;

/// A grammar rule with ordered arms (PEG ordered choice).
#[derive(Debug, Clone)]
pub struct ParseRule {
    pub name: String,
    pub arms: Vec<ParseArm>,
}

/// A grammar guard — condition between pattern and result.
/// Evaluated after pattern matches. If false, arm fails (PEG tries next arm).
/// Syntax in grammar files: `[pattern / @Rest] {@World.IsFfi(@Name)} Constructor @Rest`
#[derive(Debug, Clone)]
pub struct ParseGuard {
    /// The guard expression text, e.g. "@World.IsFfi(@Name)" or "@World.Context(Expr)"
    pub condition: String,
}

/// A single arm: pattern → optional guard → result.
#[derive(Debug, Clone)]
pub struct ParseArm {
    pub pattern: Vec<PatElem>,
    pub guard: Option<ParseGuard>,
    pub result: ResultSpec,
}

/// What to match in the token stream.
#[derive(Debug, Clone)]
pub enum PatElem {
    /// Match a specific token by variant name (e.g., "Colon" → Token::Colon).
    Tok(String),
    /// Match an identifier with an exact value (e.g., "Self", "Main", "True").
    Lit(String),
    /// Call another grammar rule (e.g., "typeRef" from <typeRef>).
    Rule(String),
    /// Bind the next identifier token to a name.
    /// Matches: PascalIdent, CamelIdent.
    Bind(String),
    /// Bind the next PascalCase identifier only.
    /// Matches: PascalIdent. Does NOT match CamelIdent.
    /// Used for type names where camelCase would be ambiguous.
    BindType(String),
    /// Bind the next literal token to a name.
    /// Matches: Integer, Float, StringLit.
    BindLit(String),
}

/// What to produce from a matched arm.
#[derive(Debug, Clone)]
pub struct ResultSpec {
    pub constructor: String,
    pub args: Vec<ResultArg>,
}

/// An argument in a result specification.
#[derive(Debug, Clone)]
pub enum ResultArg {
    /// Reference to a bound value: @Name.
    Bound(String),
    /// Result from a non-terminal call: <rule>.
    RuleResult(String),
    /// Nested constructor: Name(args...).
    Nested(ResultSpec),
    /// String literal value: "text".
    Literal(String),
}

/// Dynamic value produced during rule execution.
/// Minimal: no AST types. Values are either text, numbers, node references, or lists thereof.
#[derive(Debug, Clone)]
pub enum PegValue {
    /// Text (identifier names, operator strings, string literals).
    Text(String),
    /// Integer literal.
    Int(i64),
    /// Float literal.
    Float(f64),
    /// Reference to a ParseNode in the World.
    NodeId(i64),
    /// List of ParseNode IDs (from Cons/Nil).
    NodeList(Vec<i64>),
    /// Tagged list: [tag, ...values] for FoldPost ops.
    TaggedList(Vec<PegValue>),
    /// Nothing.
    None,
}

impl PegValue {
    pub fn as_text(&self) -> Result<String, String> {
        match self {
            PegValue::Text(s) => Ok(s.clone()),
            other => Err(format!("expected Text, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_int(&self) -> Result<i64, String> {
        match self {
            PegValue::Int(n) => Ok(*n),
            other => Err(format!("expected Int, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_float(&self) -> Result<f64, String> {
        match self {
            PegValue::Float(f) => Ok(*f),
            other => Err(format!("expected Float, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_node_id(&self) -> Result<i64, String> {
        match self {
            PegValue::NodeId(id) => Ok(*id),
            other => Err(format!("expected NodeId, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_node_list(&self) -> Result<Vec<i64>, String> {
        match self {
            PegValue::NodeList(ids) => Ok(ids.clone()),
            // Single node → singleton list
            PegValue::NodeId(id) => Ok(vec![*id]),
            other => Err(format!("expected NodeList, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_tagged_list(&self) -> Result<Vec<PegValue>, String> {
        match self {
            PegValue::TaggedList(v) => Ok(v.clone()),
            other => Err(format!("expected TaggedList, got {:?}", std::mem::discriminant(other))),
        }
    }
}

/// Captured bindings from pattern matching.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    values: HashMap<String, PegValue>,
}

impl Bindings {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, name: String, value: PegValue) {
        self.values.insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<&PegValue> {
        self.values.get(name)
    }
}

/// Table of all grammar rules, keyed by rule name.
pub type RuleTable = HashMap<String, ParseRule>;

// ── Public API ──────────────────────────────────────────────

use crate::grammar::config::GrammarConfig;

/// Parse only the module header from source text (no items).
/// Returns (module_name, exports, imports) extracted from ParseNodes.
pub fn parse_header_only(source: &str, config: &GrammarConfig) -> Option<HeaderInfo> {
    let tokens = crate::lexer::lex(source).ok()?;
    let grammar_dir = config::find_grammar_dir();
    let rules = match grammar_dir {
        Some(ref dir) => bootstrap::load_rules(dir).unwrap_or_default(),
        None => RuleTable::new(),
    };
    let mut parser = interpreter::GrammarParser::new(rules, config.clone());
    let pos = interpreter::skip_newlines_pub(&tokens, 0);
    if pos < tokens.len() && tokens[pos].token == crate::lexer::Token::LParen {
        match parser.try_rule("header", &tokens, pos) {
            Ok((PegValue::NodeId(id), _)) => {
                // Extract header info from the parsed node
                let node = parser.world.parse_nodes.iter().find(|n| n.id == id)?;
                if node.constructor != "ModuleHeader" { return None; }
                let name = node.text.clone();
                let children = aski_core::query_parse_children(&parser.world, id);
                let mut exports = Vec::new();
                let mut imports = Vec::new();
                for child in &children {
                    match child.constructor.as_str() {
                        "ExportName" => exports.push(child.text.clone()),
                        "NamedImport" => {
                            let import_children = aski_core::query_parse_children(&parser.world, child.id);
                            let items: Vec<String> = import_children.iter().map(|c| c.text.clone()).collect();
                            imports.push(ImportInfo {
                                module: child.text.clone(),
                                items,
                                wildcard: false,
                            });
                        }
                        "WildcardImport" => {
                            imports.push(ImportInfo {
                                module: child.text.clone(),
                                items: vec![],
                                wildcard: true,
                            });
                        }
                        _ => {}
                    }
                }
                Some(HeaderInfo { name, exports, imports })
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Lightweight header info — replaces ast::ModuleHeader for header-only parsing.
#[derive(Debug, Clone)]
pub struct HeaderInfo {
    pub name: String,
    pub exports: Vec<String>,
    pub imports: Vec<ImportInfo>,
}

/// Import entry info — replaces ast::ImportEntry.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub module: String,
    pub items: Vec<String>,
    pub wildcard: bool,
}

/// Parse a full aski source file and return the populated World.
pub fn parse_to_world(source: &str) -> Result<aski_core::World, String> {
    let mut parser = make_parser()?;
    parser.parse_to_world(source)
}

/// Parse a full aski source file with a pre-loaded grammar config, returning the World.
pub fn parse_to_world_with_config(source: &str, config: &GrammarConfig) -> Result<aski_core::World, String> {
    let grammar_dir = config::find_grammar_dir();
    let rules = match grammar_dir {
        Some(ref dir) => bootstrap::load_rules(dir).unwrap_or_default(),
        None => RuleTable::new(),
    };
    let mut parser = interpreter::GrammarParser::new(rules, config.clone());
    parser.parse_to_world(source)
}

/// Create a GrammarParser from grammar directory files.
fn make_parser() -> Result<interpreter::GrammarParser, String> {
    let grammar_dir = config::find_grammar_dir()
        .ok_or_else(|| "grammar directory not found".to_string())?;
    let config = config::GrammarConfig::load_from_dir(&grammar_dir)
        .unwrap_or_else(|_| config::GrammarConfig::bootstrap());
    let rules = bootstrap::load_rules(&grammar_dir).unwrap_or_default();
    Ok(interpreter::GrammarParser::new(rules, config))
}

/// Parse a source file with additional grammar rules injected (from imports).
/// Returns the populated World.
pub fn parse_to_world_with_extra_rules(
    source: &str,
    config: &GrammarConfig,
    extra_rules: &RuleTable,
) -> Result<aski_core::World, String> {
    let grammar_dir = config::find_grammar_dir();
    let mut rules = match grammar_dir {
        Some(ref dir) => bootstrap::load_rules(dir).unwrap_or_default(),
        None => RuleTable::new(),
    };
    for (name, rule) in extra_rules {
        rules.insert(name.clone(), rule.clone());
    }
    let mut parser = interpreter::GrammarParser::new(rules, config.clone());
    parser.parse_to_world(source)
}

/// Parse and extract user-defined grammar rules (for cross-module import).
/// Returns (World, user_rules).
pub fn extract_user_grammar_rules_to_world(
    source: &str,
    config: &GrammarConfig,
    extra_rules: &RuleTable,
) -> Result<(aski_core::World, RuleTable), String> {
    let grammar_dir = config::find_grammar_dir();
    let bootstrap_rules = match grammar_dir {
        Some(ref dir) => bootstrap::load_rules(dir).unwrap_or_default(),
        None => RuleTable::new(),
    };
    let mut rules = bootstrap_rules.clone();
    for (name, rule) in extra_rules {
        rules.insert(name.clone(), rule.clone());
    }
    let bootstrap_keys: std::collections::HashSet<String> = rules.keys().cloned().collect();
    let mut parser = interpreter::GrammarParser::new(rules, config.clone());
    let world = parser.parse_to_world(source)?;
    // Collect rules added during parsing (not in bootstrap + extras)
    let mut user_rules = RuleTable::new();
    for (name, rule) in &parser.rules {
        if !bootstrap_keys.contains(name) {
            user_rules.insert(name.clone(), rule.clone());
        }
    }
    Ok((world, user_rules))
}

/// Convert an in-source grammar rule (parsed as a ParseNode) into a live ParseRule.
/// This reads the GrammarRule/RuleArm/etc nodes from the World to reconstruct
/// the PEG rule for the interpreter.
pub fn grammar_node_to_parse_rule(world: &aski_core::World, node_id: i64) -> Option<ParseRule> {
    let node = aski_core::find_parse_node(world, node_id)?;
    if node.constructor != "GrammarRule" { return None; }
    let name = node.text.clone();
    let children = aski_core::query_parse_children(world, node_id);
    let arms: Vec<ParseArm> = children.iter().filter_map(|arm_node| {
        if arm_node.constructor != "RuleArm" { return None; }
        let arm_children = aski_core::query_parse_children(world, arm_node.id);
        let mut pattern = Vec::new();
        let mut result = ResultSpec { constructor: "Nil".into(), args: vec![] };
        let known = bootstrap::known_tokens();
        for child in &arm_children {
            match child.constructor.as_str() {
                "PatternGroup" => {
                    let pat_children = aski_core::query_parse_children(world, child.id);
                    for pc in pat_children {
                        match pc.constructor.as_str() {
                            "Terminal" => {
                                if known.contains(pc.text.as_str()) {
                                    pattern.push(PatElem::Tok(pc.text.clone()));
                                } else {
                                    pattern.push(PatElem::Lit(pc.text.clone()));
                                }
                            }
                            "NonTerminal" => pattern.push(PatElem::Rule(pc.text.clone())),
                            "Binding" => {
                                if pc.text == "Lit" {
                                    pattern.push(PatElem::BindLit(pc.text.clone()));
                                } else if pc.text == "Type" {
                                    pattern.push(PatElem::BindType(pc.text.clone()));
                                } else {
                                    pattern.push(PatElem::Bind(pc.text.clone()));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "ResultGroup" => {
                    let res_children = aski_core::query_parse_children(world, child.id);
                    if let Some(first) = res_children.first() {
                        result = node_to_result_spec(world, first);
                    }
                }
                _ => {}
            }
        }
        Some(ParseArm { pattern, guard: None, result })
    }).collect();
    Some(ParseRule { name, arms })
}

/// Convert a ParseNode representing a result expression into a ResultSpec.
fn node_to_result_spec(world: &aski_core::World, node: &aski_core::ParseNode) -> ResultSpec {
    match node.constructor.as_str() {
        "BareName" => ResultSpec {
            constructor: node.text.clone(),
            args: vec![],
        },
        "InstanceRef" => ResultSpec {
            constructor: "Passthrough".into(),
            args: vec![ResultArg::Bound(node.text.clone())],
        },
        "StructConstruct" => {
            let children = aski_core::query_parse_children(world, node.id);
            let args = children.iter().map(|c| {
                let field_children = aski_core::query_parse_children(world, c.id);
                if let Some(val) = field_children.first() {
                    ResultArg::Nested(node_to_result_spec(world, val))
                } else {
                    ResultArg::Literal(c.text.clone())
                }
            }).collect();
            ResultSpec { constructor: node.text.clone(), args }
        },
        "StringLit" => ResultSpec {
            constructor: "Passthrough".into(),
            args: vec![ResultArg::Literal(node.text.clone())],
        },
        "IntLit" => ResultSpec {
            constructor: "Passthrough".into(),
            args: vec![ResultArg::Literal(node.text.clone())],
        },
        "FloatLit" => ResultSpec {
            constructor: "Passthrough".into(),
            args: vec![ResultArg::Literal(node.text.clone())],
        },
        _ => ResultSpec { constructor: "Nil".into(), args: vec![] },
    }
}
