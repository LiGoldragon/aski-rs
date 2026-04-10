use crate::helpers::{StringExt, VecExt, ToI64};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeEntry {
    pub id: i64,
    pub name: String,
    pub form: TypeForm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub contains_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub field_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeWorld {
    pub types: Vec<TypeEntry>,
    pub variants: Vec<VariantDef>,
    pub fields: Vec<FieldDef>,
}

impl Default for CodeWorld { fn default() -> Self { Self { types: Default::default(), variants: Default::default(), fields: Default::default(), } } }

impl CodeWorld {
    pub fn new() -> Self { Self::default() }

    pub fn type_entry_by_id(&self, val: i64) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.id == val).collect()
    }

    pub fn type_entry_by_name(&self, val: &str) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.name == val).collect()
    }

    pub fn type_entry_by_form(&self, val: TypeForm) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.form == val).collect()
    }

    pub fn variant_def_by_type_id(&self, val: i64) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn variant_def_by_ordinal(&self, val: i64) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn variant_def_by_name(&self, val: &str) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.name == val).collect()
    }

    pub fn variant_def_by_contains_type(&self, val: &str) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.contains_type == val).collect()
    }

    pub fn field_def_by_type_id(&self, val: i64) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn field_def_by_ordinal(&self, val: i64) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn field_def_by_name(&self, val: &str) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.name == val).collect()
    }

    pub fn field_def_by_field_type(&self, val: &str) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.field_type == val).collect()
    }

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseState {
    pub tokens: Vec<Token>,
    pub pos: i64,
    pub next_id: i64,
    pub types: Vec<TypeEntry>,
    pub variants: Vec<VariantDef>,
    pub fields: Vec<FieldDef>,
}

impl Default for ParseState { fn default() -> Self { Self { tokens: Default::default(), pos: Default::default(), next_id: Default::default(), types: Default::default(), variants: Default::default(), fields: Default::default(), } } }

impl ParseState {
    pub fn new() -> Self { Self::default() }

    pub fn token_by_kind(&self, val: TokenKind) -> Vec<&Token> {
        self.tokens.iter().filter(|r| r.kind == val).collect()
    }

    pub fn token_by_text(&self, val: &str) -> Vec<&Token> {
        self.tokens.iter().filter(|r| r.text == val).collect()
    }

    pub fn type_entry_by_id(&self, val: i64) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.id == val).collect()
    }

    pub fn type_entry_by_name(&self, val: &str) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.name == val).collect()
    }

    pub fn type_entry_by_form(&self, val: TypeForm) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.form == val).collect()
    }

    pub fn variant_def_by_type_id(&self, val: i64) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn variant_def_by_ordinal(&self, val: i64) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn variant_def_by_name(&self, val: &str) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.name == val).collect()
    }

    pub fn variant_def_by_contains_type(&self, val: &str) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.contains_type == val).collect()
    }

    pub fn field_def_by_type_id(&self, val: i64) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn field_def_by_ordinal(&self, val: i64) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn field_def_by_name(&self, val: &str) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.name == val).collect()
    }

    pub fn field_def_by_field_type(&self, val: &str) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.field_type == val).collect()
    }

}

pub trait Parse {
    fn parse_all(self) -> ParseState;
    fn skip_ws(self) -> ParseState;
    fn parse_item(self) -> ParseState;
    fn parse_domain(self, name: &String) -> ParseState;
    fn parse_variants(self, type_id: &i64, ordinal: &i64) -> ParseState;
    fn parse_struct(self, name: &String) -> ParseState;
    fn parse_fields(self, type_id: &i64, ordinal: &i64) -> ParseState;
    fn skip_balanced_parens(self, depth: &i64) -> ParseState;
    fn skip_balanced_brackets(self, depth: &i64) -> ParseState;
    fn add_type(self, name: &String, form: &TypeForm) -> ParseState;
    fn add_variant(self, type_id: &i64, ordinal: &i64, v_name: &String) -> ParseState;
    fn add_field(self, type_id: &i64, ordinal: &i64, f_name: &String, f_type: &String) -> ParseState;
    fn peek(&self) -> TokenKind;
    fn peek_text(&self) -> String;
    fn advance(self) -> ParseState;
    fn bump_id(self) -> ParseState;
    fn to_world(&self) -> CodeWorld;
}

impl Parse for ParseState {
    fn parse_all(self) -> ParseState {
        let mut s: ParseState = self.skip_ws();
        match s.peek() { TokenKind::LParen => s.skip_balanced_parens(&1).parse_all(), TokenKind::EOF => s, _ => s.parse_item().parse_all() }
    }
    fn skip_ws(self) -> ParseState {
        match self.peek() { TokenKind::Newline => self.advance().skip_ws(), TokenKind::Comment => self.advance().skip_ws(), _ => self }
    }
    fn parse_item(self) -> ParseState {
        let mut s: ParseState = self.skip_ws();
        match s.peek() { TokenKind::CamelIdent => { let mut s2: ParseState = s.advance().skip_ws(); match s2.peek() { TokenKind::LParen => s2.skip_balanced_parens(&1).skip_ws().skip_balanced_brackets(&1), TokenKind::LBracket => s2.skip_balanced_brackets(&1), _ => s2 } }, TokenKind::PascalIdent => { let mut name: String = s.peek_text(); let mut s2: ParseState = s.advance().skip_ws(); match s2.peek() { TokenKind::LParen => s2.parse_domain(&name), TokenKind::LBrace => s2.parse_struct(&name), _ => s2.advance().skip_ws() } }, TokenKind::Bang => s.advance().skip_ws().advance().skip_ws().advance().skip_ws().skip_balanced_parens(&1), _ => s.advance() }
    }
    fn parse_domain(self, name: &String) -> ParseState {
        let mut s: ParseState = self.advance();
        let mut type_id: i64 = s.next_id;
        s.bump_id().add_type(&name, &TypeForm::Domain).parse_variants(&type_id, &0).advance()
    }
    fn parse_variants(self, type_id: &i64, ordinal: &i64) -> ParseState {
        let mut s: ParseState = self.skip_ws();
        match s.peek() { TokenKind::RParen => s, TokenKind::PascalIdent => { let mut v_name: String = s.peek_text(); let mut s2: ParseState = s.advance().skip_ws(); match s2.peek() { TokenKind::LParen => s2.skip_balanced_parens(&1).add_variant(&type_id, &ordinal, &v_name).parse_variants(&type_id, &(ordinal + 1)), _ => s2.add_variant(&type_id, &ordinal, &v_name).parse_variants(&type_id, &(ordinal + 1)) } }, _ => s }
    }
    fn parse_struct(self, name: &String) -> ParseState {
        let mut s: ParseState = self.advance();
        let mut type_id: i64 = s.next_id;
        s.bump_id().add_type(&name, &TypeForm::Struct).parse_fields(&type_id, &0).advance()
    }
    fn parse_fields(self, type_id: &i64, ordinal: &i64) -> ParseState {
        let mut s: ParseState = self.skip_ws();
        match s.peek() { TokenKind::RBrace => s, TokenKind::PascalIdent => { let mut f_name: String = s.peek_text(); let mut s2: ParseState = s.advance().skip_ws(); let mut f_type: String = s2.peek_text(); let mut s3: ParseState = s2.advance().skip_ws(); match s3.peek() { TokenKind::LBrace => { let mut s4: ParseState = s3.advance().skip_ws(); let mut inner_type: String = s4.peek_text(); let mut s5: ParseState = s4.advance().skip_ws().advance(); let mut full_type: String = (((f_type + "{") + &inner_type) + "}"); s5.add_field(&type_id, &ordinal, &f_name, &full_type).parse_fields(&type_id, &(ordinal + 1)) }, _ => s3.add_field(&type_id, &ordinal, &f_name, &f_type).parse_fields(&type_id, &(ordinal + 1)) } }, TokenKind::Comment => s.advance().parse_fields(&type_id, &ordinal), _ => s }
    }
    fn skip_balanced_parens(self, depth: &i64) -> ParseState {
        let mut s: ParseState = self.advance();
        match s.peek() { TokenKind::LParen => s.skip_balanced_parens(&(depth + 1)), TokenKind::RParen => match ((*depth == 1)) { true => s.advance(), false => s.skip_balanced_parens(&(*depth - 1)) }, TokenKind::EOF => s, _ => s.skip_balanced_parens(&depth) }
    }
    fn skip_balanced_brackets(self, depth: &i64) -> ParseState {
        let mut s: ParseState = self.advance();
        match s.peek() { TokenKind::LBracket => s.skip_balanced_brackets(&(depth + 1)), TokenKind::RBracket => match ((*depth == 1)) { true => s.advance(), false => s.skip_balanced_brackets(&(*depth - 1)) }, TokenKind::EOF => s, _ => s.skip_balanced_brackets(&depth) }
    }
    fn add_type(self, name: &String, form: &TypeForm) -> ParseState {
        ParseState { tokens: self.tokens, pos: self.pos, next_id: self.next_id, types: { let mut v = self.types; v.push(TypeEntry { id: (self.next_id - 1), name: name.clone(), form: *form }); v }, variants: self.variants, fields: self.fields }
    }
    fn add_variant(self, type_id: &i64, ordinal: &i64, v_name: &String) -> ParseState {
        ParseState { tokens: self.tokens, pos: self.pos, next_id: self.next_id, types: self.types, variants: { let mut v = self.variants; v.push(VariantDef { type_id: *type_id, ordinal: *ordinal, name: v_name.clone(), contains_type: String::new() }); v }, fields: self.fields }
    }
    fn add_field(self, type_id: &i64, ordinal: &i64, f_name: &String, f_type: &String) -> ParseState {
        ParseState { tokens: self.tokens, pos: self.pos, next_id: self.next_id, types: self.types, variants: self.variants, fields: { let mut v = self.fields; v.push(FieldDef { type_id: *type_id, ordinal: *ordinal, name: f_name.clone(), field_type: f_type.clone() }); v } }
    }
    fn bump_id(self) -> ParseState {
        ParseState { tokens: self.tokens, pos: self.pos, next_id: (self.next_id + 1), types: self.types, variants: self.variants, fields: self.fields }
    }
    fn peek(&self) -> TokenKind {
        match (((self.pos as usize) >= ((self.tokens.len())))) { true => TokenKind::EOF, false => self.tokens.from_ordinal(&self.pos).kind }
    }
    fn peek_text(&self) -> String {
        match (((self.pos as usize) >= ((self.tokens.len())))) { true => String::new(), false => self.tokens.from_ordinal(&self.pos).text.clone() }
    }
    fn advance(self) -> ParseState {
        ParseState { tokens: self.tokens, pos: (self.pos + 1), next_id: self.next_id, types: self.types, variants: self.variants, fields: self.fields }
    }
    fn to_world(&self) -> CodeWorld {
        CodeWorld { types: self.types.clone(), variants: self.variants.clone(), fields: self.fields.clone() }
    }
}

