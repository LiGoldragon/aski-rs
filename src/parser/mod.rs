#[cfg(test)]
mod tests;

use crate::ast::*;

/// Parse a full aski source file with optional module header.
pub fn parse_source_file(source: &str) -> Result<SourceFile, String> {
    crate::grammar_engine_full::parse_source_file(source)
}

/// Convenience: lex + parse in one step (no header).
pub fn parse_source(source: &str) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file(source)?;
    Ok(sf.items)
}

/// Parser backend selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserBackend {
    /// Data-driven grammar engine (default).
    GrammarEngine,
}

/// Parse a source file with the specified backend.
pub fn parse_source_file_with(
    source: &str,
    _backend: ParserBackend,
) -> Result<SourceFile, String> {
    parse_source_file(source)
}

/// Parse items with the specified backend.
pub fn parse_source_with(
    source: &str,
    _backend: ParserBackend,
) -> Result<Vec<Spanned<Item>>, String> {
    parse_source(source)
}
