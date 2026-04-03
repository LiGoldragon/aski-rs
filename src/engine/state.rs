//! Core parsing state: token stream and position tracking.

use crate::ast::Span;
use crate::lexer::Token;

/// A token with its span (byte range in source).
#[derive(Debug, Clone)]
pub(crate) struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// Parser state: a position in the token stream.
pub(crate) struct ParseState<'a> {
    pub tokens: &'a [SpannedToken],
    pub pos: usize,
}

impl<'a> ParseState<'a> {
    pub fn new(tokens: &'a [SpannedToken]) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub fn advance(&mut self) -> Option<&SpannedToken> {
        if self.pos < self.tokens.len() {
            let t = &self.tokens[self.pos];
            self.pos += 1;
            Some(t)
        } else {
            None
        }
    }

    pub fn expect(&mut self, expected: &Token) -> Result<Span, String> {
        match self.peek() {
            Some(t) if t == expected => {
                let span = self.tokens[self.pos].span.clone();
                self.pos += 1;
                Ok(span)
            }
            Some(t) => Err(format!(
                "expected {:?}, got {:?} at position {}",
                expected, t, self.pos
            )),
            None => Err(format!("expected {:?}, got end of input", expected)),
        }
    }

    pub fn skip_newlines(&mut self) {
        while let Some(Token::Newline) = self.peek() {
            self.pos += 1;
        }
    }

    pub fn save(&self) -> usize {
        self.pos
    }

    pub fn restore(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Span from `start` to current position.
    pub fn span_from(&self, start: usize) -> Span {
        let s = if start < self.tokens.len() {
            self.tokens[start].span.start
        } else if !self.tokens.is_empty() {
            self.tokens.last().unwrap().span.end
        } else {
            0
        };
        let e = if self.pos > 0 && self.pos <= self.tokens.len() {
            self.tokens[self.pos - 1].span.end
        } else {
            s
        };
        s..e
    }

    pub fn eat_pascal(&mut self) -> Option<(String, Span)> {
        if let Some(Token::PascalIdent(s)) = self.peek() {
            let s = s.clone();
            let span = self.tokens[self.pos].span.clone();
            self.pos += 1;
            Some((s, span))
        } else {
            None
        }
    }

    pub fn eat_camel(&mut self) -> Option<(String, Span)> {
        if let Some(Token::CamelIdent(s)) = self.peek() {
            let s = s.clone();
            let span = self.tokens[self.pos].span.clone();
            self.pos += 1;
            Some((s, span))
        } else {
            None
        }
    }

    pub fn eat(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}
