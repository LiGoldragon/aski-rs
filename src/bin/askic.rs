use std::env;
use std::fs;
use std::process;

use aski_rs::lexer;
use aski_rs::parser_gen::{TokenKind, Token, ParseState, Parse};
use aski_rs::codegen_gen::Generate;

fn convert_tokens(lexer_tokens: &[lexer::Spanned]) -> Vec<Token> {
    lexer_tokens.iter().filter_map(|st| {
        let kind = match &st.token {
            lexer::Token::PascalIdent(_) => TokenKind::PascalIdent,
            lexer::Token::CamelIdent(_) => TokenKind::CamelIdent,
            lexer::Token::Integer(_) => TokenKind::Integer,
            lexer::Token::Float(_) => TokenKind::Float,
            lexer::Token::StringLit(_) => TokenKind::StringLit,
            lexer::Token::LParen => TokenKind::LParen,
            lexer::Token::RParen => TokenKind::RParen,
            lexer::Token::LBracket => TokenKind::LBracket,
            lexer::Token::RBracket => TokenKind::RBracket,
            lexer::Token::LBrace => TokenKind::LBrace,
            lexer::Token::RBrace => TokenKind::RBrace,
            lexer::Token::Dot => TokenKind::Dot,
            lexer::Token::At => TokenKind::At,
            lexer::Token::Caret => TokenKind::Caret,
            lexer::Token::Ampersand => TokenKind::Ampersand,
            lexer::Token::Tilde => TokenKind::Tilde,
            lexer::Token::Bang => TokenKind::Bang,
            lexer::Token::Hash => TokenKind::Hash,
            lexer::Token::Pipe => TokenKind::Pipe,
            lexer::Token::Tick => TokenKind::Tick,
            lexer::Token::Colon => TokenKind::Colon,
            lexer::Token::Comma => TokenKind::Comma,
            lexer::Token::Underscore => TokenKind::Underscore,
            lexer::Token::Equals => TokenKind::Equals,
            lexer::Token::CompositionOpen => TokenKind::CompositionOpen,
            lexer::Token::CompositionClose => TokenKind::CompositionClose,
            lexer::Token::TraitBoundOpen => TokenKind::TraitBoundOpen,
            lexer::Token::TraitBoundClose => TokenKind::TraitBoundClose,
            lexer::Token::IterOpen => TokenKind::IterOpen,
            lexer::Token::IterClose => TokenKind::IterClose,
            lexer::Token::Stub => TokenKind::Stub,
            lexer::Token::Newline => TokenKind::Newline,
            lexer::Token::Comment => TokenKind::Comment,
            _ => return None,
        };
        let text = match &st.token {
            lexer::Token::PascalIdent(s) | lexer::Token::CamelIdent(s) => s.clone(),
            lexer::Token::Integer(n) => n.to_string(),
            lexer::Token::Float(s) | lexer::Token::StringLit(s) => s.clone(),
            _ => String::new(),
        };
        Some(Token { kind, text })
    }).collect()
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() { eprintln!("usage: askic <file.aski>"); process::exit(1); }
    let path = &args[args.len() - 1];
    let source = fs::read_to_string(path).unwrap_or_else(|e| { eprintln!("askic: {path}: {e}"); process::exit(1); });
    let lexer_tokens = lexer::lex(&source).unwrap_or_else(|errs| { for e in &errs { eprintln!("askic: {e}"); } process::exit(1); });
    let tokens = convert_tokens(&lexer_tokens);
    let state = ParseState { tokens, pos: 0, next_id: 1, types: Vec::new(), variants: Vec::new(), fields: Vec::new(), ffi_entries: Vec::new() };
    let result = state.parse_all();
    let pw = result.to_world();
    // Convert parser_gen::CodeWorld to codegen_gen::CodeWorld
    let world = aski_rs::codegen_gen::CodeWorld {
        types: pw.types.into_iter().map(|t| aski_rs::codegen_gen::TypeEntry {
            id: t.id, name: t.name,
            form: if t.form == aski_rs::parser_gen::TypeForm::Domain { aski_rs::codegen_gen::TypeForm::Domain } else { aski_rs::codegen_gen::TypeForm::Struct },
        }).collect(),
        variants: pw.variants.into_iter().map(|v| aski_rs::codegen_gen::VariantDef {
            type_id: v.type_id, ordinal: v.ordinal, name: v.name, contains_type: v.contains_type,
        }).collect(),
        fields: pw.fields.into_iter().map(|f| aski_rs::codegen_gen::FieldDef {
            type_id: f.type_id, ordinal: f.ordinal, name: f.name, field_type: f.field_type,
        }).collect(),
        ffi_entries: pw.ffi_entries.into_iter().map(|e| aski_rs::codegen_gen::FfiEntry {
            library: e.library, aski_name: e.aski_name, rust_name: e.rust_name,
            span: match e.span {
                aski_rs::parser_gen::RustSpan::Cast => aski_rs::codegen_gen::RustSpan::Cast,
                aski_rs::parser_gen::RustSpan::MethodCall => aski_rs::codegen_gen::RustSpan::MethodCall,
                aski_rs::parser_gen::RustSpan::FreeCall => aski_rs::codegen_gen::RustSpan::FreeCall,
                aski_rs::parser_gen::RustSpan::BlockExpr => aski_rs::codegen_gen::RustSpan::BlockExpr,
                aski_rs::parser_gen::RustSpan::IndexAccess => aski_rs::codegen_gen::RustSpan::IndexAccess,
            },
            return_type: e.return_type,
        }).collect(),
    };
    print!("{}", world.generate());
}
