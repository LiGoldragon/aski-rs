//! Synth types — the structures that represent loaded dialect definitions.
//!
//! A Dialect is a list of Rules. Each Rule is either Sequential
//! (all items match in order) or OrderedChoice (first match wins).

/// A loaded dialect definition from a .synth file.
#[derive(Debug, Clone)]
pub struct Dialect {
    pub name: String,
    pub rules: Vec<Rule>,
}

/// A synth rule — either sequential or ordered choice.
#[derive(Debug, Clone)]
pub enum Rule {
    /// All items must match in order.
    Sequential(Vec<Item>),
    /// Lines starting with `//`. First alternative that matches wins.
    /// Each alternative has its own cardinality for the looping entry point.
    OrderedChoice(Vec<ChoiceAlternative>),
}

/// One alternative in an ordered choice, with a cardinality that the
/// looping engine tracks: `*` = zero or more, `?` = at most one, etc.
#[derive(Debug, Clone)]
pub struct ChoiceAlternative {
    pub items: Vec<SpacedItem>,
    pub cardinality: Card,
}

/// A single item in a synth rule.
#[derive(Debug, Clone)]
pub enum Item {
    /// `@Name` or `@name` — declare a name at this position.
    Declare { casing: Casing, kind: String },

    /// `:Name` or `:name` — reference an existing name.
    Reference { casing: Casing, kind: String },

    /// `@value` — a literal value (integer, float, string).
    Value,

    /// `<name>` — push inner dialect, parse content with it.
    DialectRef(String),

    /// A delimiter rule: `(@Domain/ <domain>)` `[@trait/ <trait-impl>]` etc.
    /// The wrapping delimiter IS the token. Key is before /, body is after.
    DelimiterRule {
        delimiter: Delimiter,
        key: Option<Box<Item>>,
        body: Vec<Item>,
    },

    /// Literal escape: `@_@name` — literal token, then synth placeholder.
    /// Left of `_` is the literal aski token. Right is the placeholder.
    LiteralEscape {
        literal: String,
        inner: Box<Item>,
    },

    /// Cardinality wrapper: `+item` `*item` `?item`
    Cardinality {
        kind: Card,
        inner: Box<Item>,
    },

    /// Inline or: `@export//@Export`
    Or(Vec<Item>),

    /// A sequence of items that parse together (for cardinality grouping).
    /// Uses SpacedItem to preserve adjacency within the group.
    Sequence(Vec<SpacedItem>),

    /// A literal token that must appear (e.g. `.set` `.new` `/new`).
    Literal(String),
}

/// An Item with adjacency info from the synth file.
/// `adjacent` = true means this item had no whitespace before it
/// in the synth source, so the corresponding aski tokens must be adjacent.
#[derive(Debug, Clone)]
pub struct SpacedItem {
    pub item: Item,
    pub adjacent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Casing {
    Pascal,
    Camel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    One,
    ZeroOrMore,
    OneOrMore,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delimiter {
    Paren,          // ( )
    Bracket,        // [ ]
    Brace,          // { }
    ParenPipe,      // (| |)
    BracketPipe,    // [| |]
    BracePipe,      // {| |}
}

impl Delimiter {
    pub fn open_str(&self) -> &'static str {
        match self {
            Delimiter::Paren => "(",
            Delimiter::Bracket => "[",
            Delimiter::Brace => "{",
            Delimiter::ParenPipe => "(|",
            Delimiter::BracketPipe => "[|",
            Delimiter::BracePipe => "{|",
        }
    }

    pub fn close_str(&self) -> &'static str {
        match self {
            Delimiter::Paren => ")",
            Delimiter::Bracket => "]",
            Delimiter::Brace => "}",
            Delimiter::ParenPipe => "|)",
            Delimiter::BracketPipe => "|]",
            Delimiter::BracePipe => "|}",
        }
    }
}
