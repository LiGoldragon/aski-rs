//! Module header parser.

use crate::ast::*;
use crate::lexer::Token;
use super::state::ParseState;

pub(crate) fn parse_module_header(st: &mut ParseState) -> Result<ModuleHeader, String> {
    let start = st.save();

    // () Sol — identity + exports
    st.expect(&Token::LParen)?;
    let (name, _) = st.eat_pascal().ok_or("expected module name")?;
    let mut exports = Vec::new();
    loop {
        st.skip_newlines();
        if st.peek() == Some(&Token::RParen) {
            st.advance();
            break;
        }
        if let Some((e, _)) = st.eat_pascal() {
            exports.push(e);
        } else if let Some((e, _)) = st.eat_camel() {
            exports.push(e);
        } else {
            break;
        }
    }

    st.skip_newlines();

    // [] Luna — imports (optional)
    let imports = if st.peek() == Some(&Token::LBracket) {
        st.advance();
        let mut imps = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBracket) {
                st.advance();
                break;
            }
            let imp_start = st.save();
            let (module, _) = st.eat_pascal().ok_or("expected module name in import")?;
            st.skip_newlines();
            st.expect(&Token::LParen)?;
            let items = if st.eat(&Token::Underscore) {
                ImportItems::Wildcard
            } else {
                let mut named = Vec::new();
                loop {
                    st.skip_newlines();
                    if st.peek() == Some(&Token::RParen) {
                        break;
                    }
                    if let Some((n, _)) = st.eat_pascal() {
                        named.push(n);
                    } else if let Some((n, _)) = st.eat_camel() {
                        named.push(n);
                    } else {
                        break;
                    }
                    st.skip_newlines();
                }
                ImportItems::Named(named)
            };
            st.expect(&Token::RParen)?;
            let imp_span = st.span_from(imp_start);
            imps.push(ImportEntry {
                module,
                items,
                span: imp_span,
            });
            st.skip_newlines();
        }
        imps
    } else {
        Vec::new()
    };

    st.skip_newlines();

    // {} Saturn — constraints (optional)
    let constraints = if st.peek() == Some(&Token::LBrace) {
        st.advance();
        let mut cons = Vec::new();
        loop {
            st.skip_newlines();
            if st.peek() == Some(&Token::RBrace) {
                st.advance();
                break;
            }
            if let Some((c, _)) = st.eat_pascal() {
                cons.push(c);
            } else {
                break;
            }
        }
        cons
    } else {
        Vec::new()
    };

    let span = st.span_from(start);
    Ok(ModuleHeader {
        name,
        exports,
        imports,
        constraints,
        span,
    })
}
