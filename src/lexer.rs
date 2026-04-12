use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t]+")]
pub enum Token {
    // === Comments ===
    #[regex(r";;[^\n]*")]
    Comment,

    // === Newlines (significant for statement separation) ===
    #[regex(r"\n+")]
    Newline,

    // === Multi-char operators (must come before single-char) ===
    #[token("(|")]
    LParenPipe,

    #[token("|)")]
    RPipeParen,

    #[token("{|")]
    LBracePipe,

    #[token("|}")]
    RPipeBrace,

    #[token("[|")]
    LBracketPipe,

    #[token("|]")]
    RPipeBracket,

    #[token("..=")]
    RangeInclusive,

    #[token("..")]
    RangeExclusive,

    #[token("___")]
    Stub,

    // === Delimiters ===
    #[token("(")]
    LParen,

    #[token(")")]
    RParen,

    #[token("[")]
    LBracket,

    #[token("]")]
    RBracket,

    #[token("{")]
    LBrace,

    #[token("}")]
    RBrace,

    // === Symbols ===
    #[token(".")]
    Dot,

    #[token("@")]
    At,

    #[token("$")]
    Dollar,

    #[token("^")]
    Caret,

    #[token(">")]
    GreaterThan,

    #[token("<")]
    LessThan,

    #[token("&")]
    Ampersand,

    #[token("~")]
    Tilde,

    #[token("?")]
    Question,

    #[token("!")]
    Bang,

    #[token("#")]
    Hash,

    #[token("|")]
    Pipe,

    #[token("'")]
    Tick,

    #[token(":")]
    Colon,

    // === Arithmetic / comparison operators ===
    #[token("+")]
    Plus,

    #[token("-")]
    Minus,

    #[token("*")]
    Star,

    #[token("/")]
    Slash,

    #[token("%")]
    Percent,

    #[token("==")]
    DoubleEquals,

    #[token("=")]
    Equals,

    #[token("!=")]
    NotEqual,

    #[token(">=")]
    GreaterThanOrEqual,

    #[token("<=")]
    LessThanOrEqual,

    #[token("&&")]
    LogicalAnd,

    #[token("||")]
    LogicalOr,

    #[token(",")]
    Comma,

    // === Bare underscore (wildcard in match patterns) ===
    #[token("_")]
    Underscore,

    // === Underscore-number suffix (for disambiguation) ===
    #[regex(r"_[0-9]+", |lex| lex.slice()[1..].parse::<u32>().ok(), priority = 3)]
    OrdinalSuffix(u32),

    // === PascalCase identifier (starts with uppercase) ===
    #[regex(r"[A-Z][a-zA-Z0-9]*", |lex| lex.slice().to_string())]
    PascalIdent(String),

    // === camelCase identifier (starts with lowercase, may contain underscores) ===
    #[regex(r"[a-z][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    CamelIdent(String),

    // === Numeric literals ===
    #[regex(r"[0-9]+\.[0-9]+", |lex| Some(lex.slice().to_string()))]
    Float(String),

    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok(), priority = 2)]
    Integer(i64),

    // === String literals (supports \" \n \\ escapes) ===
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        let inner = &s[1..s.len()-1];
        let mut out = String::new();
        let mut escape = false;
        for ch in inner.chars() {
            if escape {
                match ch {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    other => { out.push('\\'); out.push(other); }
                }
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else {
                out.push(ch);
            }
        }
        out
    })]
    StringLit(String),
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Comment => write!(f, ";;"),
            Token::Newline => write!(f, "\\n"),
            Token::LBracePipe => write!(f, "{{|"),
            Token::RPipeBrace => write!(f, "|}}"),
            Token::LParenPipe => write!(f, "(|"),
            Token::RPipeParen => write!(f, "|)"),
            Token::LBracketPipe => write!(f, "[|"),
            Token::RPipeBracket => write!(f, "|]"),
            Token::RangeInclusive => write!(f, "..="),
            Token::RangeExclusive => write!(f, ".."),
            Token::Stub => write!(f, "___"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::Dot => write!(f, "."),
            Token::At => write!(f, "@"),
            Token::Dollar => write!(f, "$"),
            Token::Caret => write!(f, "^"),
            Token::GreaterThan => write!(f, ">"),
            Token::LessThan => write!(f, "<"),
            Token::Ampersand => write!(f, "&"),
            Token::Tilde => write!(f, "~"),
            Token::Question => write!(f, "?"),
            Token::Bang => write!(f, "!"),
            Token::Hash => write!(f, "#"),
            Token::Pipe => write!(f, "|"),
            Token::Tick => write!(f, "'"),
            Token::Colon => write!(f, ":"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Percent => write!(f, "%"),
            Token::DoubleEquals => write!(f, "=="),
            Token::Equals => write!(f, "="),
            Token::NotEqual => write!(f, "!="),
            Token::GreaterThanOrEqual => write!(f, ">="),
            Token::LessThanOrEqual => write!(f, "<="),
            Token::LogicalAnd => write!(f, "&&"),
            Token::LogicalOr => write!(f, "||"),
            Token::Comma => write!(f, ","),
            Token::Underscore => write!(f, "_"),
            Token::OrdinalSuffix(n) => write!(f, "_{n}"),
            Token::PascalIdent(s) => write!(f, "{s}"),
            Token::CamelIdent(s) => write!(f, "{s}"),
            Token::Float(v) => write!(f, "{}", v),
            Token::Integer(v) => write!(f, "{v}"),
            Token::StringLit(s) => write!(f, "\"{s}\""),
        }
    }
}

/// A token with its span in the source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub token: Token,
    pub span: std::ops::Range<usize>,
}

/// Tokenize aski source into a list of spanned tokens, filtering comments.
pub fn lex(source: &str) -> Result<Vec<Spanned>, Vec<LexError>> {
    let mut lexer = Token::lexer(source);
    let mut tokens = Vec::new();
    let mut errors = Vec::new();

    while let Some(result) = lexer.next() {
        let span = lexer.span();
        match result {
            Ok(token) => {
                // Skip comments — they carry no semantic content
                if !matches!(token, Token::Comment) {
                    tokens.push(Spanned { token, span });
                }
            }
            Err(()) => {
                errors.push(LexError {
                    span: span.clone(),
                    text: source[span].to_string(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: std::ops::Range<usize>,
    pub text: String,
}

impl std::fmt::Display for LexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "unexpected character '{}' at {}..{}",
            self.text, self.span.start, self.span.end
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_domain_declaration() {
        let source = "Element (Fire Earth Air Water)";
        let tokens = lex(source).unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.token).collect();
        assert_eq!(
            kinds,
            vec![
                &Token::PascalIdent("Element".into()),
                &Token::LParen,
                &Token::PascalIdent("Fire".into()),
                &Token::PascalIdent("Earth".into()),
                &Token::PascalIdent("Air".into()),
                &Token::PascalIdent("Water".into()),
                &Token::RParen,
            ]
        );
    }

    #[test]
    fn lex_v06_function() {
        let source = "add(@Addition) U32 [ ^(@Addition.Left + @Addition.Right) ]";
        let tokens = lex(source).unwrap();
        assert!(tokens.len() > 10);
        assert!(tokens.iter().any(|t| t.token == Token::LBracket));
        assert!(tokens.iter().any(|t| t.token == Token::RBracket));
        assert!(tokens.iter().any(|t| t.token == Token::At));
    }

    #[test]
    fn lex_colon_borrow() {
        let source = ":@Self";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].token, Token::Colon);
        assert_eq!(tokens[1].token, Token::At);
        assert_eq!(tokens[2].token, Token::PascalIdent("Self".into()));
    }

    #[test]
    fn lex_struct_declaration() {
        let source = "Point { X F64 Y F64 }";
        let tokens = lex(source).unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.token).collect();
        assert_eq!(
            kinds,
            vec![
                &Token::PascalIdent("Point".into()),
                &Token::LBrace,
                &Token::PascalIdent("X".into()),
                &Token::PascalIdent("F64".into()),
                &Token::PascalIdent("Y".into()),
                &Token::PascalIdent("F64".into()),
                &Token::RBrace,
            ]
        );
    }

    #[test]
    fn lex_comment_stripped() {
        let source = ";; this is a comment\nElement (Fire)";
        let tokens = lex(source).unwrap();
        assert!(!tokens.iter().any(|t| matches!(t.token, Token::Comment)));
        assert!(tokens
            .iter()
            .any(|t| t.token == Token::PascalIdent("Element".into())));
    }

    #[test]
    fn lex_string_literal() {
        let source = r#""hello world""#;
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].token, Token::StringLit("hello world".into()));
    }

    #[test]
    fn lex_numeric_literals() {
        let source = "42 3.14";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].token, Token::Integer(42));
        assert_eq!(tokens[1].token, Token::Float("3.14".into()));
    }

    #[test]
    fn lex_const_binding() {
        let source = "!Pi F64 {3.14159265358979}";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].token, Token::Bang);
        assert_eq!(tokens[1].token, Token::PascalIdent("Pi".into()));
    }

    #[test]
    fn lex_trait_bound_delimiters() {
        let source = "{|display|}";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].token, Token::LBracePipe);
        assert_eq!(tokens[1].token, Token::CamelIdent("display".into()));
        assert_eq!(tokens[2].token, Token::RPipeBrace);
    }

    #[test]
    fn lex_trait_bound_compound() {
        let source = "{|sort&display|}";
        let tokens = lex(source).unwrap();
        assert_eq!(tokens[0].token, Token::LBracePipe);
        assert_eq!(tokens[1].token, Token::CamelIdent("sort".into()));
        assert_eq!(tokens[2].token, Token::Ampersand);
        assert_eq!(tokens[3].token, Token::CamelIdent("display".into()));
        assert_eq!(tokens[4].token, Token::RPipeBrace);
    }
}
