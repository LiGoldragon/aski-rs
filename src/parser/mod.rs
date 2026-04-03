pub mod tokens;
pub mod patterns;
pub mod expressions;
pub mod statements;
pub mod items;
#[cfg(test)]
mod tests;

use chumsky::prelude::*;

use crate::ast::*;
use crate::lexer::Token;
use tokens::{skip_newlines, pascal, camel};
use items::item;

macro_rules! make_stream {
    ($tokens:expr) => {{
        let len = $tokens.len();
        chumsky::Stream::from_iter(
            len..len + 1,
            $tokens.into_iter().enumerate().map(|(i, t)| (t, i..i + 1)),
        )
    }};
}

/// Parse a module header: `(Name export1 export2)` `[Mod (items)]` `{constraints}`
fn module_header() -> impl Parser<Token, ModuleHeader, Error = Simple<Token>> + Clone {
    // () Sol — module identity + exports
    // First item is PascalCase (module name), rest are PascalCase or camelCase (exports)
    let identity = pascal()
        .then(
            skip_newlines()
                .ignore_then(choice((pascal(), camel())))
                .repeated()
        )
        .delimited_by(
            tok(Token::LParen),
            tok(Token::RParen),
        );

    // Import entry: ModuleName (Item1 Item2) or ModuleName (_)
    let import_items = choice((
        tok(Token::Underscore).map(|_| ImportItems::Wildcard),
        choice((pascal(), camel()))
            .separated_by(skip_newlines())
            .at_least(1)
            .map(ImportItems::Named),
    ))
    .delimited_by(tok(Token::LParen), tok(Token::RParen));

    let import_entry = pascal()
        .then(skip_newlines().ignore_then(import_items))
        .map_with_span(|(module, items), span| ImportEntry { module, items, span });

    // [] Luna — imports
    let imports = skip_newlines()
        .ignore_then(import_entry)
        .separated_by(skip_newlines())
        .allow_trailing()
        .delimited_by(tok(Token::LBracket), tok(Token::RBracket));

    // {} Saturn — constraints
    let constraints = pascal()
        .separated_by(skip_newlines())
        .allow_trailing()
        .delimited_by(tok(Token::LBrace), tok(Token::RBrace));

    identity
        .then(skip_newlines().ignore_then(imports).or_not())
        .then(skip_newlines().ignore_then(constraints).or_not())
        .map_with_span(|(((name, exports), imports), constraints), span| {
            ModuleHeader {
                name,
                exports,
                imports: imports.unwrap_or_default(),
                constraints: constraints.unwrap_or_default(),
                span,
            }
        })
}

fn tok(expected: Token) -> impl Parser<Token, Token, Error = Simple<Token>> + Clone {
    filter(move |t: &Token| *t == expected).labelled("token")
}

/// Parse a full aski source file (legacy — no header).
pub fn parse_file(
    tokens: Vec<Token>,
) -> (Vec<Spanned<Item>>, Vec<Simple<Token>>) {
    let parser = skip_newlines()
        .ignore_then(
            item()
                .separated_by(skip_newlines().then(skip_newlines()))
                .allow_trailing(),
        )
        .then_ignore(skip_newlines())
        .then_ignore(end());

    let (result, errors) = parser.parse_recovery(make_stream!(tokens));
    (result.unwrap_or_default(), errors)
}

/// Parse a source file with optional module header.
pub fn parse_source_file(source: &str) -> Result<SourceFile, String> {
    let spanned_tokens = crate::lexer::lex(source).map_err(|errs| {
        errs.into_iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    })?;

    let tokens: Vec<Token> = spanned_tokens.into_iter().map(|s| s.token).collect();

    // Try parsing with header first
    let with_header = skip_newlines()
        .ignore_then(module_header())
        .then(
            skip_newlines()
                .ignore_then(item())
                .separated_by(skip_newlines().then(skip_newlines()))
                .allow_trailing()
        )
        .then_ignore(skip_newlines())
        .then_ignore(end())
        .map(|(header, items)| SourceFile {
            header: Some(header),
            items,
        });

    let without_header = skip_newlines()
        .ignore_then(
            item()
                .separated_by(skip_newlines().then(skip_newlines()))
                .allow_trailing(),
        )
        .then_ignore(skip_newlines())
        .then_ignore(end())
        .map(|items| SourceFile {
            header: None,
            items,
        });

    let parser = with_header.or(without_header);

    let (result, errors) = parser.parse_recovery(make_stream!(tokens));

    match result {
        Some(sf) => Ok(sf),
        None => Err(errors
            .into_iter()
            .map(|e| format!("{:?}", e))
            .collect::<Vec<_>>()
            .join("\n")),
    }
}

/// Convenience: lex + parse in one step (legacy — no header).
pub fn parse_source(source: &str) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file(source)?;
    Ok(sf.items)
}

/// Parser backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserBackend {
    /// Chumsky parser (original, default).
    Chumsky,
    /// Data-driven grammar engine (new, full v0.10 coverage).
    GrammarEngine,
}

/// Parse a source file with the specified backend.
pub fn parse_source_file_with(
    source: &str,
    backend: ParserBackend,
) -> Result<SourceFile, String> {
    match backend {
        ParserBackend::Chumsky => parse_source_file(source),
        ParserBackend::GrammarEngine => {
            crate::grammar_engine_full::parse_source_file(source)
        }
    }
}

/// Parse items with the specified backend.
pub fn parse_source_with(
    source: &str,
    backend: ParserBackend,
) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file_with(source, backend)?;
    Ok(sf.items)
}
