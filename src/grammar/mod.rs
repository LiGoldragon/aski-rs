//! Grammar rule engine — aski defines its own syntax.
//!
//! Grammar rules in grammar/*.aski files define the parsing logic.
//! A bootstrap parser reads these files into a rule table.
//! A PEG interpreter executes rules against token streams.
//! Builder functions construct AST nodes from match results.

pub mod bootstrap;
pub mod config;
pub mod interpreter;
pub mod builders;

use std::collections::HashMap;
use crate::ast::*;

/// A grammar rule with ordered arms (PEG ordered choice).
#[derive(Debug, Clone)]
pub struct ParseRule {
    pub name: String,
    pub arms: Vec<ParseArm>,
}

/// A single arm: pattern → result.
#[derive(Debug, Clone)]
pub struct ParseArm {
    pub pattern: Vec<PatElem>,
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
#[derive(Debug, Clone)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    List(Vec<Value>),
    None,
    Param(Param),
    TypeRef(TypeRef),
    Item(Spanned<Item>),
    Expr(Spanned<Expr>),
    Pattern(Pattern),
    Body(Body),
    Variant(Variant),
    Field(Field),
    MethodSig(MethodSig),
    MethodDef(MethodDef),
    MatchArm(MatchArm),
    MatchMethodArm(MatchMethodArm),
    ModuleHeader(ModuleHeader),
    ImportEntry(ImportEntry),
    TypeImpl(TypeImpl),
    ConstDecl(ConstDecl),
    AssociatedTypeDef(AssociatedTypeDef),
    TraitBound(TraitBound),
    ForeignBlock(ForeignBlockDecl),
    ForeignFunction(ForeignFunction),
}

impl Value {
    pub fn as_str(&self) -> Result<String, String> {
        match self {
            Value::Str(s) => Ok(s.clone()),
            other => Err(format!("expected string, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_int(&self) -> Result<i64, String> {
        match self {
            Value::Int(n) => Ok(*n),
            other => Err(format!("expected int, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_float(&self) -> Result<f64, String> {
        match self {
            Value::Float(f) => Ok(*f),
            other => Err(format!("expected float, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_type_ref(&self) -> Result<TypeRef, String> {
        match self {
            Value::TypeRef(t) => Ok(t.clone()),
            other => Err(format!("expected TypeRef, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_param(&self) -> Result<Param, String> {
        match self {
            Value::Param(p) => Ok(p.clone()),
            other => Err(format!("expected Param, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_variant(&self) -> Result<Variant, String> {
        match self {
            Value::Variant(v) => Ok(v.clone()),
            other => Err(format!("expected Variant, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_field(&self) -> Result<Field, String> {
        match self {
            Value::Field(f) => Ok(f.clone()),
            other => Err(format!("expected Field, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_pattern(&self) -> Result<Pattern, String> {
        match self {
            Value::Pattern(p) => Ok(p.clone()),
            other => Err(format!("expected Pattern, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_body(&self) -> Result<Body, String> {
        match self {
            Value::Body(b) => Ok(b.clone()),
            other => Err(format!("expected Body, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_expr(&self) -> Result<Spanned<Expr>, String> {
        match self {
            Value::Expr(e) => Ok(e.clone()),
            other => Err(format!("expected Expr, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_item(&self) -> Result<Spanned<Item>, String> {
        match self {
            Value::Item(i) => Ok(i.clone()),
            other => Err(format!("expected Item, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_method_sig(&self) -> Result<MethodSig, String> {
        match self {
            Value::MethodSig(m) => Ok(m.clone()),
            other => Err(format!("expected MethodSig, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_method_def(&self) -> Result<MethodDef, String> {
        match self {
            Value::MethodDef(m) => Ok(m.clone()),
            other => Err(format!("expected MethodDef, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_match_method_arm(&self) -> Result<MatchMethodArm, String> {
        match self {
            Value::MatchMethodArm(m) => Ok(m.clone()),
            other => Err(format!("expected MatchMethodArm, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_match_arm(&self) -> Result<MatchArm, String> {
        match self {
            Value::MatchArm(m) => Ok(m.clone()),
            other => Err(format!("expected MatchArm, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_import_entry(&self) -> Result<ImportEntry, String> {
        match self {
            Value::ImportEntry(i) => Ok(i.clone()),
            other => Err(format!("expected ImportEntry, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_type_impl(&self) -> Result<TypeImpl, String> {
        match self {
            Value::TypeImpl(t) => Ok(t.clone()),
            other => Err(format!("expected TypeImpl, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_const_decl(&self) -> Result<ConstDecl, String> {
        match self {
            Value::ConstDecl(c) => Ok(c.clone()),
            other => Err(format!("expected ConstDecl, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_associated_type_def(&self) -> Result<AssociatedTypeDef, String> {
        match self {
            Value::AssociatedTypeDef(a) => Ok(a.clone()),
            other => Err(format!("expected AssociatedTypeDef, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_trait_bound(&self) -> Result<TraitBound, String> {
        match self {
            Value::TraitBound(b) => Ok(b.clone()),
            other => Err(format!("expected TraitBound, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn as_foreign_function(&self) -> Result<crate::ast::ForeignFunction, String> {
        match self {
            Value::ForeignFunction(f) => Ok(f.clone()),
            other => Err(format!("expected ForeignFunction, got {:?}", std::mem::discriminant(other))),
        }
    }

    pub fn into_list(self) -> Result<Vec<Value>, String> {
        match self {
            Value::List(v) => Ok(v),
            other => Err(format!("expected List, got {:?}", std::mem::discriminant(&other))),
        }
    }
}

/// Captured bindings from pattern matching.
#[derive(Debug, Clone, Default)]
pub struct Bindings {
    values: HashMap<String, Value>,
}

impl Bindings {
    pub fn new() -> Self { Self::default() }

    pub fn insert(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }
}

/// Table of all grammar rules, keyed by rule name.
pub type RuleTable = HashMap<String, ParseRule>;

// ── Public API ──────────────────────────────────────────────

use crate::ast::{SourceFile, ForeignBlockDecl, ForeignFunction};
use crate::grammar::config::{self as grammar_config, GrammarConfig};

/// Parse a full aski source file using the grammar engine.
pub fn parse_source_file(source: &str) -> Result<SourceFile, String> {
    let parser = make_parser()?;
    parser.parse_source_file(source)
}

/// Parse a full aski source file with a pre-loaded grammar config.
pub fn parse_source_file_with_config(source: &str, config: &GrammarConfig) -> Result<SourceFile, String> {
    let grammar_dir = grammar_config::find_grammar_dir();
    let rules = match grammar_dir {
        Some(ref dir) => bootstrap::load_rules(dir).unwrap_or_default(),
        None => RuleTable::new(),
    };
    let parser = interpreter::GrammarParser::new(rules, config.clone());
    parser.parse_source_file(source)
}

/// Create a GrammarParser from grammar directory files.
fn make_parser() -> Result<interpreter::GrammarParser, String> {
    let grammar_dir = grammar_config::find_grammar_dir()
        .ok_or_else(|| "grammar directory not found".to_string())?;
    let config = grammar_config::GrammarConfig::load_from_dir(&grammar_dir)
        .unwrap_or_else(|_| grammar_config::GrammarConfig::bootstrap());
    let rules = bootstrap::load_rules(&grammar_dir).unwrap_or_default();
    Ok(interpreter::GrammarParser::new(rules, config))
}
