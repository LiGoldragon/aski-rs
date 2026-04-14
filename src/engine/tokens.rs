//! TokenReader — cursor over a token stream.
//! All token-level operations are methods on this type.

use crate::lexer::{Token, Spanned};
use crate::synth::types::Delimiter;

pub struct TokenReader<'a> {
    pub tokens: &'a [Spanned],
    pub pos: usize,
}

impl<'a> TokenReader<'a> {
    pub fn new(tokens: &'a [Spanned]) -> Self {
        TokenReader { tokens, pos: 0 }
    }

    pub fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    /// Check if current token is adjacent to the previous one (no whitespace gap).
    pub fn is_adjacent(&self) -> bool {
        if self.pos == 0 || self.pos >= self.tokens.len() { return false; }
        let prev_end = self.tokens[self.pos - 1].span.end;
        let curr_start = self.tokens[self.pos].span.start;
        curr_start == prev_end
    }

    pub fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|t| &t.token)
    }

    pub fn span_start(&self) -> i64 {
        self.tokens.get(self.pos).map(|t| t.span.start as i64).unwrap_or(0)
    }

    pub fn span_end(&self) -> i64 {
        self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span.end as i64).unwrap_or(0)
    }

    pub fn skip_newlines(&mut self) {
        while self.pos < self.tokens.len() && matches!(self.tokens[self.pos].token, Token::Newline) {
            self.pos += 1;
        }
    }

    pub fn expect_open(&mut self, delim: Delimiter) -> Result<(), String> {
        self.skip_newlines();
        let tok = self.tokens.get(self.pos).ok_or("expected delimiter, got EOF")?;
        let ok = match (delim, &tok.token) {
            (Delimiter::Paren, Token::LParen) |
            (Delimiter::Bracket, Token::LBracket) |
            (Delimiter::Brace, Token::LBrace) |
            (Delimiter::ParenPipe, Token::LParenPipe) |
            (Delimiter::BracketPipe, Token::LBracketPipe) |
            (Delimiter::BracePipe, Token::LBracePipe) => true,
            _ => false,
        };
        if ok { self.pos += 1; Ok(()) }
        else { Err(format!("expected {}, got {:?}", delim.open_str(), tok.token)) }
    }

    pub fn expect_close(&mut self, delim: Delimiter) -> Result<(), String> {
        self.skip_newlines();
        let tok = self.tokens.get(self.pos).ok_or("expected close, got EOF")?;
        let ok = match (delim, &tok.token) {
            (Delimiter::Paren, Token::RParen) |
            (Delimiter::Bracket, Token::RBracket) |
            (Delimiter::Brace, Token::RBrace) |
            (Delimiter::ParenPipe, Token::RPipeParen) |
            (Delimiter::BracketPipe, Token::RPipeBracket) |
            (Delimiter::BracePipe, Token::RPipeBrace) => true,
            _ => false,
        };
        if ok { self.pos += 1; Ok(()) }
        else { Err(format!("expected {}, got {:?}", delim.close_str(), tok.token)) }
    }

    pub fn is_close(&self, delim: Delimiter) -> bool {
        self.tokens.get(self.pos).map(|t| match (delim, &t.token) {
            (Delimiter::Paren, Token::RParen) |
            (Delimiter::Bracket, Token::RBracket) |
            (Delimiter::Brace, Token::RBrace) |
            (Delimiter::ParenPipe, Token::RPipeParen) |
            (Delimiter::BracketPipe, Token::RPipeBracket) |
            (Delimiter::BracePipe, Token::RPipeBrace) => true,
            _ => false,
        }).unwrap_or(false)
    }

    pub fn expect_literal(&mut self, expected: &str) -> Result<(), String> {
        self.skip_newlines();
        let tok = self.tokens.get(self.pos)
            .ok_or_else(|| format!("expected '{}', got EOF", expected))?;
        let ok = match &tok.token {
            Token::PascalIdent(s) | Token::CamelIdent(s) if s == expected => true,
            Token::At if expected == "@" => true,
            Token::Colon if expected == ":" => true,
            Token::Tilde if expected == "~" => true,
            Token::Caret if expected == "^" => true,
            Token::Hash if expected == "#" => true,
            Token::Bang if expected == "!" => true,
            Token::Dot if expected == "." => true,
            Token::Slash if expected == "/" => true,
            _ => false,
        };
        if ok { self.pos += 1; Ok(()) }
        else { Err(format!("expected '{}', got {:?}", expected, tok.token)) }
    }

    pub fn read_pascal(&mut self) -> Result<String, String> {
        self.skip_newlines();
        match self.tokens.get(self.pos).map(|t| &t.token) {
            Some(Token::PascalIdent(s)) => { let s = s.clone(); self.pos += 1; Ok(s) }
            other => Err(format!("expected PascalCase, got {:?}", other)),
        }
    }

    pub fn read_camel(&mut self) -> Result<String, String> {
        self.skip_newlines();
        match self.tokens.get(self.pos).map(|t| &t.token) {
            Some(Token::CamelIdent(s)) => { let s = s.clone(); self.pos += 1; Ok(s) }
            other => Err(format!("expected camelCase, got {:?}", other)),
        }
    }

    pub fn read_name(&mut self) -> Result<String, String> {
        self.skip_newlines();
        match self.tokens.get(self.pos).map(|t| &t.token) {
            Some(Token::PascalIdent(s)) | Some(Token::CamelIdent(s)) => {
                let s = s.clone(); self.pos += 1; Ok(s)
            }
            other => Err(format!("expected name, got {:?}", other)),
        }
    }

    /// Read a type reference: `Name`, `Vec{Item}`, `$Trait`, `$Trait&Trait`
    pub fn read_type(&mut self) -> Result<String, String> {
        self.skip_newlines();

        // $Trait or $Trait&Trait — generic type parameter
        if self.peek() == Some(&Token::Dollar) {
            self.pos += 1;
            let mut bounds = self.read_pascal()?;
            while self.peek() == Some(&Token::Ampersand) {
                self.pos += 1;
                let next = self.read_pascal()?;
                bounds.push('&');
                bounds.push_str(&next);
            }
            return Ok(format!("${}", bounds));
        }

        // PascalIdent — possibly followed by <args> for generics
        let name = self.read_pascal()?;
        if self.peek() == Some(&Token::LessThan) {
            self.pos += 1;
            let mut result = format!("{}<", name);
            let mut first = true;
            loop {
                self.skip_newlines();
                if self.peek() == Some(&Token::GreaterThan) {
                    self.pos += 1;
                    break;
                }
                if self.at_end() { break; }
                if !first { result.push(' '); }
                first = false;
                let arg = self.read_type()?;
                result.push_str(&arg);
            }
            result.push('>');
            Ok(result)
        } else {
            Ok(name)
        }
    }

    pub fn skip_until_close(&mut self, delim: Delimiter) {
        let mut depth = 1;
        while self.pos < self.tokens.len() && depth > 0 {
            if self.is_close(delim) {
                depth -= 1;
                if depth == 0 { break; }
            }
            match (&self.tokens[self.pos].token, delim) {
                (Token::LParen, Delimiter::Paren) |
                (Token::LBracket, Delimiter::Bracket) |
                (Token::LBrace, Delimiter::Brace) |
                (Token::LParenPipe, Delimiter::ParenPipe) |
                (Token::LBracketPipe, Delimiter::BracketPipe) |
                (Token::LBracePipe, Delimiter::BracePipe) => depth += 1,
                _ => {}
            }
            self.pos += 1;
        }
    }
}
