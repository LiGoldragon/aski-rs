#[cfg(test)]
mod tests;

use crate::ast::*;

/// Parse a full aski source file with optional module header.
/// Uses the grammar engine — grammar rules from .aski files drive parsing.
pub fn parse_source_file(source: &str) -> Result<SourceFile, String> {
    crate::grammar::parse_source_file(source)
}

/// Convenience: lex + parse in one step (no header).
pub fn parse_source(source: &str) -> Result<Vec<Spanned<Item>>, String> {
    let sf = parse_source_file(source)?;
    Ok(sf.items)
}
