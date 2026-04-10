//! Kernel-specific parser — populates CodeWorld from lexer tokens.
//! Handles the five forms in kernel.aski:
//! 1. (Name Export1 ...) — module header, skipped
//! 2. Name (Variant1 Variant2 ...) — domain
//! 3. Name { Field1 Type1 ... } — struct
//! 4. derive ([...]) / derive [World [...]] — skipped
//! 5. ;; comment — skipped by lexer

use crate::lexer::{self, Token};
use crate::codegen_gen::{TypeForm, TypeEntry, VariantDef, FieldDef, CodeWorld};

pub fn parse(source: &str) -> Result<CodeWorld, String> {
    let tokens = lexer::lex(source).map_err(|errs| {
        errs.into_iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join(", ")
    })?;

    let mut world = CodeWorld {
        types: Vec::new(),
        variants: Vec::new(),
        fields: Vec::new(),
    };
    let mut pos = 0;
    let mut next_id: i64 = 1;

    pos = skip_ws(&tokens, pos);

    // Skip module header if present
    if peek(&tokens, pos) == Some(&Token::LParen) {
        pos = skip_balanced(&tokens, pos, &Token::LParen, &Token::RParen);
        pos = skip_ws(&tokens, pos);
    }

    // Parse items
    while pos < tokens.len() {
        pos = skip_ws(&tokens, pos);
        if pos >= tokens.len() { break; }

        match &tokens[pos].token {
            // camelCase = derive decl/impl — skip
            Token::CamelIdent(_) => {
                pos += 1;
                pos = skip_ws(&tokens, pos);
                match peek(&tokens, pos) {
                    Some(Token::LParen) => {
                        pos = skip_balanced(&tokens, pos, &Token::LParen, &Token::RParen);
                        pos = skip_ws(&tokens, pos);
                        if peek(&tokens, pos) == Some(&Token::LBracket) {
                            pos = skip_balanced(&tokens, pos, &Token::LBracket, &Token::RBracket);
                        }
                    }
                    Some(Token::LBracket) => {
                        pos = skip_balanced(&tokens, pos, &Token::LBracket, &Token::RBracket);
                    }
                    _ => {}
                }
            }
            // PascalCase = domain or struct
            Token::PascalIdent(name) => {
                let name = name.clone();
                pos += 1;
                pos = skip_ws(&tokens, pos);
                match peek(&tokens, pos) {
                    Some(Token::LParen) => {
                        pos += 1; // consume (
                        let type_id = next_id;
                        next_id += 1;
                        world.types.push(TypeEntry {
                            id: type_id,
                            name: name.clone(),
                            form: TypeForm::Domain,
                        });
                        let mut ordinal: i64 = 0;
                        loop {
                            pos = skip_ws(&tokens, pos);
                            match peek(&tokens, pos) {
                                Some(Token::RParen) => { pos += 1; break; }
                                Some(Token::PascalIdent(vname)) => {
                                    let vname = vname.clone();
                                    pos += 1;
                                    pos = skip_ws(&tokens, pos);
                                    // Check for data-carrying: Name (InnerType)
                                    if peek(&tokens, pos) == Some(&Token::LParen) {
                                        pos = skip_balanced(&tokens, pos, &Token::LParen, &Token::RParen);
                                    }
                                    world.variants.push(VariantDef {
                                        type_id,
                                        ordinal,
                                        name: vname,
                                        contains_type: String::new(),
                                    });
                                    ordinal += 1;
                                }
                                _ => { pos += 1; }
                            }
                        }
                    }
                    Some(Token::LBrace) => {
                        pos += 1; // consume {
                        let type_id = next_id;
                        next_id += 1;
                        world.types.push(TypeEntry {
                            id: type_id,
                            name: name.clone(),
                            form: TypeForm::Struct,
                        });
                        let mut ordinal: i64 = 0;
                        loop {
                            pos = skip_ws(&tokens, pos);
                            match peek(&tokens, pos) {
                                Some(Token::RBrace) => { pos += 1; break; }
                                Some(Token::PascalIdent(fname)) => {
                                    let fname = fname.clone();
                                    pos += 1;
                                    pos = skip_ws(&tokens, pos);
                                    // Parse type ref
                                    let ftype = parse_type_ref(&tokens, &mut pos);
                                    world.fields.push(FieldDef {
                                        type_id,
                                        ordinal,
                                        name: fname,
                                        field_type: ftype,
                                    });
                                    ordinal += 1;
                                }
                                _ => { pos += 1; }
                            }
                        }
                    }
                    // Type alias: Name TypeRef — skip
                    _ => {
                        if pos < tokens.len() {
                            pos += 1; // skip type ref
                        }
                    }
                }
            }
            // Semicolon-comments already filtered by lexer
            _ => { pos += 1; }
        }
    }

    Ok(world)
}

fn skip_ws(tokens: &[lexer::Spanned], mut pos: usize) -> usize {
    while pos < tokens.len() && matches!(tokens[pos].token, Token::Newline | Token::Comment) {
        pos += 1;
    }
    pos
}

fn peek<'a>(tokens: &'a [lexer::Spanned], pos: usize) -> Option<&'a Token> {
    tokens.get(pos).map(|t| &t.token)
}

fn skip_balanced(tokens: &[lexer::Spanned], mut pos: usize, open: &Token, close: &Token) -> usize {
    if pos >= tokens.len() { return pos; }
    pos += 1; // consume open
    let mut depth = 1;
    while pos < tokens.len() && depth > 0 {
        if std::mem::discriminant(&tokens[pos].token) == std::mem::discriminant(open) {
            depth += 1;
        } else if std::mem::discriminant(&tokens[pos].token) == std::mem::discriminant(close) {
            depth -= 1;
        }
        pos += 1;
    }
    pos
}

fn parse_type_ref(tokens: &[lexer::Spanned], pos: &mut usize) -> String {
    match peek(tokens, *pos) {
        Some(Token::PascalIdent(name)) => {
            let name = name.clone();
            *pos += 1;
            let p = skip_ws(tokens, *pos);
            *pos = p;
            if peek(tokens, *pos) == Some(&Token::LBrace) {
                *pos += 1; // consume {
                let inner = parse_type_ref(tokens, pos);
                let p = skip_ws(tokens, *pos);
                *pos = p;
                if peek(tokens, *pos) == Some(&Token::RBrace) {
                    *pos += 1;
                }
                format!("Vec{{{}}}", inner)
            } else {
                name
            }
        }
        _ => {
            *pos += 1;
            "Unknown".to_string()
        }
    }
}
