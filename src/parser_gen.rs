#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum TokenKind {
    PascalIdent,
    CamelIdent,
    Integer,
    Float,
    StringLit,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    DoubleEquals,
    NotEqual,
    LessThan,
    GreaterThan,
    LessThanOrEqual,
    GreaterThanOrEqual,
    LogicalAnd,
    LogicalOr,
    Dot,
    At,
    Dollar,
    Caret,
    Ampersand,
    Tilde,
    Question,
    Bang,
    Hash,
    Pipe,
    Tick,
    Colon,
    Comma,
    Underscore,
    Equals,
    CompositionOpen,
    CompositionClose,
    TraitBoundOpen,
    TraitBoundClose,
    IterOpen,
    IterClose,
    RangeInclusive,
    RangeExclusive,
    Stub,
    Newline,
    Comment,
    EOF,
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl TokenKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pascal_ident" => Some(Self::PascalIdent),
            "PascalIdent" => Some(Self::PascalIdent),
            "camel_ident" => Some(Self::CamelIdent),
            "CamelIdent" => Some(Self::CamelIdent),
            "integer" => Some(Self::Integer),
            "float" => Some(Self::Float),
            "string_lit" => Some(Self::StringLit),
            "StringLit" => Some(Self::StringLit),
            "l_paren" => Some(Self::LParen),
            "LParen" => Some(Self::LParen),
            "r_paren" => Some(Self::RParen),
            "RParen" => Some(Self::RParen),
            "l_bracket" => Some(Self::LBracket),
            "LBracket" => Some(Self::LBracket),
            "r_bracket" => Some(Self::RBracket),
            "RBracket" => Some(Self::RBracket),
            "l_brace" => Some(Self::LBrace),
            "LBrace" => Some(Self::LBrace),
            "r_brace" => Some(Self::RBrace),
            "RBrace" => Some(Self::RBrace),
            "plus" => Some(Self::Plus),
            "minus" => Some(Self::Minus),
            "star" => Some(Self::Star),
            "slash" => Some(Self::Slash),
            "percent" => Some(Self::Percent),
            "double_equals" => Some(Self::DoubleEquals),
            "DoubleEquals" => Some(Self::DoubleEquals),
            "not_equal" => Some(Self::NotEqual),
            "NotEqual" => Some(Self::NotEqual),
            "less_than" => Some(Self::LessThan),
            "LessThan" => Some(Self::LessThan),
            "greater_than" => Some(Self::GreaterThan),
            "GreaterThan" => Some(Self::GreaterThan),
            "less_than_or_equal" => Some(Self::LessThanOrEqual),
            "LessThanOrEqual" => Some(Self::LessThanOrEqual),
            "greater_than_or_equal" => Some(Self::GreaterThanOrEqual),
            "GreaterThanOrEqual" => Some(Self::GreaterThanOrEqual),
            "logical_and" => Some(Self::LogicalAnd),
            "LogicalAnd" => Some(Self::LogicalAnd),
            "logical_or" => Some(Self::LogicalOr),
            "LogicalOr" => Some(Self::LogicalOr),
            "dot" => Some(Self::Dot),
            "at" => Some(Self::At),
            "dollar" => Some(Self::Dollar),
            "caret" => Some(Self::Caret),
            "ampersand" => Some(Self::Ampersand),
            "tilde" => Some(Self::Tilde),
            "question" => Some(Self::Question),
            "bang" => Some(Self::Bang),
            "hash" => Some(Self::Hash),
            "pipe" => Some(Self::Pipe),
            "tick" => Some(Self::Tick),
            "colon" => Some(Self::Colon),
            "comma" => Some(Self::Comma),
            "underscore" => Some(Self::Underscore),
            "equals" => Some(Self::Equals),
            "composition_open" => Some(Self::CompositionOpen),
            "CompositionOpen" => Some(Self::CompositionOpen),
            "composition_close" => Some(Self::CompositionClose),
            "CompositionClose" => Some(Self::CompositionClose),
            "trait_bound_open" => Some(Self::TraitBoundOpen),
            "TraitBoundOpen" => Some(Self::TraitBoundOpen),
            "trait_bound_close" => Some(Self::TraitBoundClose),
            "TraitBoundClose" => Some(Self::TraitBoundClose),
            "iter_open" => Some(Self::IterOpen),
            "IterOpen" => Some(Self::IterOpen),
            "iter_close" => Some(Self::IterClose),
            "IterClose" => Some(Self::IterClose),
            "range_inclusive" => Some(Self::RangeInclusive),
            "RangeInclusive" => Some(Self::RangeInclusive),
            "range_exclusive" => Some(Self::RangeExclusive),
            "RangeExclusive" => Some(Self::RangeExclusive),
            "stub" => Some(Self::Stub),
            "newline" => Some(Self::Newline),
            "comment" => Some(Self::Comment),
            "e_o_f" => Some(Self::EOF),
            "EOF" => Some(Self::EOF),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::PascalIdent => "pascal_ident",
            Self::CamelIdent => "camel_ident",
            Self::Integer => "integer",
            Self::Float => "float",
            Self::StringLit => "string_lit",
            Self::LParen => "l_paren",
            Self::RParen => "r_paren",
            Self::LBracket => "l_bracket",
            Self::RBracket => "r_bracket",
            Self::LBrace => "l_brace",
            Self::RBrace => "r_brace",
            Self::Plus => "plus",
            Self::Minus => "minus",
            Self::Star => "star",
            Self::Slash => "slash",
            Self::Percent => "percent",
            Self::DoubleEquals => "double_equals",
            Self::NotEqual => "not_equal",
            Self::LessThan => "less_than",
            Self::GreaterThan => "greater_than",
            Self::LessThanOrEqual => "less_than_or_equal",
            Self::GreaterThanOrEqual => "greater_than_or_equal",
            Self::LogicalAnd => "logical_and",
            Self::LogicalOr => "logical_or",
            Self::Dot => "dot",
            Self::At => "at",
            Self::Dollar => "dollar",
            Self::Caret => "caret",
            Self::Ampersand => "ampersand",
            Self::Tilde => "tilde",
            Self::Question => "question",
            Self::Bang => "bang",
            Self::Hash => "hash",
            Self::Pipe => "pipe",
            Self::Tick => "tick",
            Self::Colon => "colon",
            Self::Comma => "comma",
            Self::Underscore => "underscore",
            Self::Equals => "equals",
            Self::CompositionOpen => "composition_open",
            Self::CompositionClose => "composition_close",
            Self::TraitBoundOpen => "trait_bound_open",
            Self::TraitBoundClose => "trait_bound_close",
            Self::IterOpen => "iter_open",
            Self::IterClose => "iter_close",
            Self::RangeInclusive => "range_inclusive",
            Self::RangeExclusive => "range_exclusive",
            Self::Stub => "stub",
            Self::Newline => "newline",
            Self::Comment => "comment",
            Self::EOF => "e_o_f",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum TypeForm {
    Domain,
    Struct,
}

impl std::fmt::Display for TypeForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl TypeForm {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "domain" => Some(Self::Domain),
            "struct" => Some(Self::Struct),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Struct => "struct",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum RustSpan {
    Cast,
    MethodCall,
    FreeCall,
    BlockExpr,
    IndexAccess,
}

impl std::fmt::Display for RustSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl RustSpan {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "cast" => Some(Self::Cast),
            "method_call" => Some(Self::MethodCall),
            "MethodCall" => Some(Self::MethodCall),
            "free_call" => Some(Self::FreeCall),
            "FreeCall" => Some(Self::FreeCall),
            "block_expr" => Some(Self::BlockExpr),
            "BlockExpr" => Some(Self::BlockExpr),
            "index_access" => Some(Self::IndexAccess),
            "IndexAccess" => Some(Self::IndexAccess),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Cast => "cast",
            Self::MethodCall => "method_call",
            Self::FreeCall => "free_call",
            Self::BlockExpr => "block_expr",
            Self::IndexAccess => "index_access",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ParamKind {
    BorrowSelf,
    MutBorrowSelf,
    OwnedSelf,
    Named,
}

impl std::fmt::Display for ParamKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ParamKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "borrow_self" => Some(Self::BorrowSelf),
            "BorrowSelf" => Some(Self::BorrowSelf),
            "mut_borrow_self" => Some(Self::MutBorrowSelf),
            "MutBorrowSelf" => Some(Self::MutBorrowSelf),
            "owned_self" => Some(Self::OwnedSelf),
            "OwnedSelf" => Some(Self::OwnedSelf),
            "named" => Some(Self::Named),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::BorrowSelf => "borrow_self",
            Self::MutBorrowSelf => "mut_borrow_self",
            Self::OwnedSelf => "owned_self",
            Self::Named => "named",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum BodyKind {
    Block,
    TailBlock,
    MatchBody,
}

impl std::fmt::Display for BodyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl BodyKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "block" => Some(Self::Block),
            "tail_block" => Some(Self::TailBlock),
            "TailBlock" => Some(Self::TailBlock),
            "match_body" => Some(Self::MatchBody),
            "MatchBody" => Some(Self::MatchBody),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::TailBlock => "tail_block",
            Self::MatchBody => "match_body",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ExprKind {
    StringLit,
    IntLit,
    BareName,
    InstanceRef,
    Return,
    Access,
    MethodCall,
    BinOp,
    InlineEval,
    Group,
    MutableNew,
    MutableSet,
    SameTypeNew,
    SubTypeNew,
    StructConstruct,
    StructField,
    Match,
    MatchArm,
    Yield,
    StdOut,
}

impl std::fmt::Display for ExprKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ExprKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "string_lit" => Some(Self::StringLit),
            "StringLit" => Some(Self::StringLit),
            "int_lit" => Some(Self::IntLit),
            "IntLit" => Some(Self::IntLit),
            "bare_name" => Some(Self::BareName),
            "BareName" => Some(Self::BareName),
            "instance_ref" => Some(Self::InstanceRef),
            "InstanceRef" => Some(Self::InstanceRef),
            "return" => Some(Self::Return),
            "access" => Some(Self::Access),
            "method_call" => Some(Self::MethodCall),
            "MethodCall" => Some(Self::MethodCall),
            "bin_op" => Some(Self::BinOp),
            "BinOp" => Some(Self::BinOp),
            "inline_eval" => Some(Self::InlineEval),
            "InlineEval" => Some(Self::InlineEval),
            "group" => Some(Self::Group),
            "mutable_new" => Some(Self::MutableNew),
            "MutableNew" => Some(Self::MutableNew),
            "mutable_set" => Some(Self::MutableSet),
            "MutableSet" => Some(Self::MutableSet),
            "same_type_new" => Some(Self::SameTypeNew),
            "SameTypeNew" => Some(Self::SameTypeNew),
            "sub_type_new" => Some(Self::SubTypeNew),
            "SubTypeNew" => Some(Self::SubTypeNew),
            "struct_construct" => Some(Self::StructConstruct),
            "StructConstruct" => Some(Self::StructConstruct),
            "struct_field" => Some(Self::StructField),
            "StructField" => Some(Self::StructField),
            "match" => Some(Self::Match),
            "match_arm" => Some(Self::MatchArm),
            "MatchArm" => Some(Self::MatchArm),
            "yield" => Some(Self::Yield),
            "std_out" => Some(Self::StdOut),
            "StdOut" => Some(Self::StdOut),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::StringLit => "string_lit",
            Self::IntLit => "int_lit",
            Self::BareName => "bare_name",
            Self::InstanceRef => "instance_ref",
            Self::Return => "return",
            Self::Access => "access",
            Self::MethodCall => "method_call",
            Self::BinOp => "bin_op",
            Self::InlineEval => "inline_eval",
            Self::Group => "group",
            Self::MutableNew => "mutable_new",
            Self::MutableSet => "mutable_set",
            Self::SameTypeNew => "same_type_new",
            Self::SubTypeNew => "sub_type_new",
            Self::StructConstruct => "struct_construct",
            Self::StructField => "struct_field",
            Self::Match => "match",
            Self::MatchArm => "match_arm",
            Self::Yield => "yield",
            Self::StdOut => "std_out",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TypeEntry {
    pub id: i64,
    pub name: String,
    pub form: TypeForm,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct VariantDef {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub contains_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FieldDef {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub field_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FfiEntry {
    pub library: String,
    pub aski_name: String,
    pub rust_name: String,
    pub span: RustSpan,
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Param {
    pub kind: ParamKind,
    pub name: String,
    pub param_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Expr {
    pub id: i64,
    pub kind: ExprKind,
    pub name: String,
    pub value: String,
    pub children: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MethodDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
    pub body_kind: BodyKind,
    pub body: Vec<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TraitDecl {
    pub name: String,
    pub methods: Vec<MethodSig>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TraitImpl {
    pub trait_name: String,
    pub target_type: String,
    pub methods: Vec<MethodDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct CodeWorld {
    pub types: Vec<TypeEntry>,
    pub variants: Vec<VariantDef>,
    pub fields: Vec<FieldDef>,
    pub ffi_entries: Vec<FfiEntry>,
    pub trait_decls: Vec<TraitDecl>,
    pub trait_impls: Vec<TraitImpl>,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ParseState {
    pub tokens: Vec<Token>,
    pub pos: i64,
    pub next_id: i64,
    pub types: Vec<TypeEntry>,
    pub variants: Vec<VariantDef>,
    pub fields: Vec<FieldDef>,
    pub ffi_entries: Vec<FfiEntry>,
    pub trait_decls: Vec<TraitDecl>,
    pub trait_impls: Vec<TraitImpl>,
    pub expr_stack: Vec<Expr>,
}

pub trait Derive {
    fn derive(&mut self);
    fn derive_variant_of(&mut self);
    fn derive_type_kind(&mut self);
    fn derive_contained_type(&mut self);
    fn derive_recursive_type(&mut self);
}

impl World {
    pub fn derive(&mut self) {
        self.derive_variant_of();
        self.derive_type_kind();
        self.derive_contained_type();
        self.derive_recursive_type_fixpoint();
    }

    fn derive_variant_of(&mut self) {
        let mut results = Vec::new();
        for type_entry in &self.types {
            if type_entry.form == TypeForm::Domain {
                for variant in &self.variants {
                    if variant.type_id == type_entry.id {
                        results.push(VariantOf { variant_name: variant.name.clone(), type_name: type_entry.name.clone(), type_id: type_entry.id });
                    }
                }
            }
        }
        self.variant_ofs = results;
    }

    fn derive_type_kind(&mut self) {
        let mut results = Vec::new();
        for type_entry in &self.types {
            results.push(TypeKind { type_name: type_entry.name.clone(), category: type_entry.form });
        }
        self.type_kinds = results;
    }

    fn derive_contained_type(&mut self) {
        let mut results = Vec::new();
        for type_entry in &self.types {
            if type_entry.form == TypeForm::Struct {
                for field in &self.fields {
                    if field.type_id == type_entry.id {
                        results.push(ContainedType { parent_type: type_entry.name.clone(), child_type: field.field_type.clone() });
                    }
                }
            }
        }
        for type_entry in &self.types {
            if type_entry.form == TypeForm::Domain {
                for variant in &self.variants {
                    if variant.type_id == type_entry.id {
                        if !variant.contains_type.is_empty() {
                            results.push(ContainedType { parent_type: type_entry.name.clone(), child_type: variant.contains_type.clone() });
                        }
                    }
                }
            }
        }
        self.contained_types = results;
    }

    fn derive_recursive_type_fixpoint(&mut self) {
        {
            let mut results = Vec::new();
            for contained_type in &self.contained_types {
                results.push(RecursiveType { parent_type: contained_type.parent_type.clone(), child_type: contained_type.child_type.clone() });
            }
            self.recursive_types = results;
        }
        loop {
            let mut new_items = Vec::new();
            for contained_type in &self.contained_types {
                for reach in &self.recursive_types {
                    if reach.parent_type == contained_type.child_type {
                        new_items.push(RecursiveType { parent_type: contained_type.parent_type.clone(), child_type: reach.child_type.clone() });
                    }
                }
            }
            new_items.retain(|item| !self.recursive_types.contains(item));
            if new_items.is_empty() { break; }
            self.recursive_types.extend(new_items);
        }
    }

}
