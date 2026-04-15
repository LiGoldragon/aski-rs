//! Header scanner — reads .aski files to extract module declarations.
//!
//! Stage 1 only reads structural declarations (module block, domains,
//! structs, traits, consts, FFI). It does NOT parse method bodies.
//! Every name discovered is interned into the NameRegistry.
//! Every declaration is recorded in the ScopeGraph.

use std::fs;
use std::path::Path;

use crate::lexer::{self, Token, Spanned};
use crate::engine::tokens::TokenReader;
use crate::synth::types::Delimiter;
use super::types::*;

/// Scan all .aski and .main files in a directory.
pub fn scan_directory(
    dir: &Path,
    names: &mut NameRegistry,
    scope: &mut ScopeGraph,
) -> Result<(), String> {
    let mut paths: Vec<_> = Vec::new();

    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {}", dir.display(), e))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        match path.extension().and_then(|e| e.to_str()) {
            Some("aski") | Some("main") => paths.push(path),
            _ => {}
        }
    }

    paths.sort();

    for path in &paths {
        scan_file(path, names, scope)?;
    }

    validate_imports(names, scope)?;

    Ok(())
}

/// Scan a single .aski file for its header declarations.
fn scan_file(
    path: &Path,
    names: &mut NameRegistry,
    scope: &mut ScopeGraph,
) -> Result<(), String> {
    let source = fs::read_to_string(path)
        .map_err(|e| format!("{}: {}", path.display(), e))?;
    let tokens = lexer::lex(&source)
        .map_err(|errs| format!("{}: {}", path.display(), errs[0]))?;

    let is_main = path.extension().map(|e| e == "main").unwrap_or(false);
    let mut reader = TokenReader::new(&tokens);
    reader.skip_newlines();

    // First token group must be the module block: {name exports...}
    let module_scope = scan_module_header(&mut reader, path, names, is_main)?;
    let module_name = module_scope.name;

    scope.modules.push(module_scope);
    let module_idx = scope.modules.len() - 1;

    // Scan root-level declarations
    loop {
        reader.skip_newlines();
        if reader.at_end() { break; }

        match reader.peek() {
            // (Domain ...) or (trait ...) or (|Ffi ...|)
            Some(Token::LParen) => {
                scan_paren_decl(&mut reader, names, &mut scope.modules[module_idx], module_name)?;
            }
            // [trait ...] — trait impl
            Some(Token::LBracket) => {
                scan_trait_impl(&mut reader, names, &mut scope.modules[module_idx], module_name)?;
            }
            // {Struct ...} — struct
            Some(Token::LBrace) => {
                scan_struct_decl(&mut reader, names, &mut scope.modules[module_idx], module_name)?;
            }
            // {|Const ...|}
            Some(Token::LBracePipe) => {
                scan_const_decl(&mut reader, names, &mut scope.modules[module_idx], module_name)?;
            }
            // (|Ffi ...|)
            Some(Token::LParenPipe) => {
                scan_ffi_decl(&mut reader, names, &mut scope.modules[module_idx], module_name)?;
            }
            // [|process|]
            Some(Token::LBracketPipe) => {
                reader.skip_until_close(Delimiter::BracketPipe);
                reader.expect_close(Delimiter::BracketPipe)?;
            }
            _ => {
                return Err(format!(
                    "{}: unexpected token {:?} at root level",
                    path.display(),
                    reader.peek()
                ));
            }
        }
    }

    Ok(())
}

/// Parse the module header: {name export1 export2 ...}
fn scan_module_header(
    reader: &mut TokenReader,
    path: &Path,
    names: &mut NameRegistry,
    is_main: bool,
) -> Result<ModuleScope, String> {
    reader.expect_open(Delimiter::Brace)?;

    // Module name is camelCase
    let mod_name_str = reader.read_camel()
        .map_err(|e| format!("{}: module header: {}", path.display(), e))?;
    let mod_name = names.intern_module(&mod_name_str);

    // Remaining identifiers are exports
    let mut exports = Vec::new();
    loop {
        reader.skip_newlines();
        if reader.is_close(Delimiter::Brace) {
            reader.expect_close(Delimiter::Brace)?;
            break;
        }
        match reader.peek() {
            Some(Token::PascalIdent(_)) => {
                let name = reader.read_pascal()?;
                // Could be a Type or Trait — we'll classify later when we see declarations.
                // For now, record as a string to be resolved after all declarations.
                exports.push(name);
            }
            Some(Token::CamelIdent(_)) => {
                let name = reader.read_camel()?;
                exports.push(name);
            }
            _ => {
                return Err(format!(
                    "{}: unexpected token in module header: {:?}",
                    path.display(),
                    reader.peek()
                ));
            }
        }
    }

    // Parse imports: [:Module import1 import2]
    let mut imports = Vec::new();
    loop {
        reader.skip_newlines();
        // Import blocks start with [
        if reader.peek() != Some(&Token::LBracket) { break; }

        // Peek ahead: [:Module ...] — the colon distinguishes import from trait-impl
        let saved = reader.pos;
        reader.pos += 1; // skip [
        reader.skip_newlines();
        if reader.peek() != Some(&Token::Colon) {
            reader.pos = saved;
            break;
        }
        reader.pos += 1; // skip :

        let source_str = reader.read_pascal()
            .map_err(|e| format!("{}: import: {}", path.display(), e))?;
        let source = names.intern_module(&source_str);

        let mut imported = Vec::new();
        loop {
            reader.skip_newlines();
            if reader.is_close(Delimiter::Bracket) {
                reader.expect_close(Delimiter::Bracket)?;
                break;
            }
            match reader.peek() {
                Some(Token::PascalIdent(_)) => {
                    let name = reader.read_pascal()?;
                    imported.push(name);
                }
                Some(Token::CamelIdent(_)) => {
                    let name = reader.read_camel()?;
                    imported.push(name);
                }
                _ => {
                    return Err(format!(
                        "{}: unexpected token in import: {:?}",
                        path.display(),
                        reader.peek()
                    ));
                }
            }
        }

        // Store raw import names — resolve to NameRef after all modules scanned
        imports.push(RawImport { source, names: imported });
    }

    Ok(ModuleScope {
        name: mod_name,
        path: path.to_string_lossy().into_owned(),
        is_main,
        declared: Vec::new(),
        exports: exports.into_iter().map(|_| NameRef::None).collect(), // resolved later
        imports: imports.into_iter().map(|ri| ModuleImport {
            source: ri.source,
            names: ri.names.into_iter().map(|_| NameRef::None).collect(), // resolved later
        }).collect(),
    })
}

/// Temporary structure for imports before resolution.
struct RawImport {
    source: ModuleName,
    names: Vec<String>,
}

/// Scan a paren declaration: (Domain ...) or (trait ...)
fn scan_paren_decl(
    reader: &mut TokenReader,
    names: &mut NameRegistry,
    module: &mut ModuleScope,
    mod_name: ModuleName,
) -> Result<(), String> {
    reader.expect_open(Delimiter::Paren)?;

    match reader.peek() {
        // PascalCase = Domain declaration
        Some(Token::PascalIdent(_)) => {
            let domain_str = reader.read_pascal()?;
            let type_name = names.intern_type(&domain_str);
            module.declared.push(DeclaredName::Type {
                name: type_name,
                form: TypeForm::Domain,
            });

            // Scan variants
            let mut ordinal = 0u32;
            loop {
                reader.skip_newlines();
                if reader.is_close(Delimiter::Paren) {
                    reader.expect_close(Delimiter::Paren)?;
                    break;
                }

                match reader.peek() {
                    // Bare PascalCase = nullary variant
                    Some(Token::PascalIdent(_)) => {
                        let var_str = reader.read_pascal()?;

                        // Check if next is Pipe (or-pattern delimiter in domain context — skip)
                        // Actually in domain.synth, variants are just listed
                        let var_name = names.intern_variant(&var_str);
                        module.declared.push(DeclaredName::Variant {
                            name: var_name,
                            parent: type_name,
                            ordinal,
                            wraps: None,
                        });
                        ordinal += 1;
                    }
                    // (Variant :Type) — data variant
                    Some(Token::LParen) => {
                        reader.expect_open(Delimiter::Paren)?;
                        let var_str = reader.read_pascal()?;
                        let var_name = names.intern_variant(&var_str);

                        // Read wrapped type(s)
                        let mut wraps = None;
                        loop {
                            reader.skip_newlines();
                            if reader.is_close(Delimiter::Paren) {
                                reader.expect_close(Delimiter::Paren)?;
                                break;
                            }
                            match reader.peek() {
                                Some(Token::PascalIdent(_)) | Some(Token::Dollar) => {
                                    let type_str = reader.read_type()?;
                                    wraps = Some(names.intern_type(&type_str));
                                }
                                _ => {
                                    reader.pos += 1; // skip unknown
                                }
                            }
                        }

                        module.declared.push(DeclaredName::Variant {
                            name: var_name,
                            parent: type_name,
                            ordinal,
                            wraps,
                        });
                        ordinal += 1;
                    }
                    // {Variant <struct>} — struct variant
                    Some(Token::LBrace) => {
                        reader.expect_open(Delimiter::Brace)?;
                        let var_str = reader.read_pascal()?;
                        let var_name = names.intern_variant(&var_str);

                        // Struct variant: intern a synthetic type for the struct
                        let struct_type = names.intern_type(&var_str);
                        module.declared.push(DeclaredName::Type {
                            name: struct_type,
                            form: TypeForm::Struct,
                        });

                        // Scan fields inside the struct variant
                        scan_struct_fields(reader, names, module, struct_type, Delimiter::Brace)?;

                        module.declared.push(DeclaredName::Variant {
                            name: var_name,
                            parent: type_name,
                            ordinal,
                            wraps: Some(struct_type),
                        });
                        ordinal += 1;
                    }
                    _ => {
                        reader.pos += 1; // skip
                    }
                }
            }
        }
        // camelCase = trait declaration: (traitName [...])
        Some(Token::CamelIdent(_)) => {
            let trait_str = reader.read_camel()?;
            let trait_name = names.intern_trait(&trait_str);

            // Expect signature block: [(...) (...)]
            let mut methods = Vec::new();
            reader.skip_newlines();
            if reader.peek() == Some(&Token::LBracket) {
                reader.expect_open(Delimiter::Bracket)?;
                loop {
                    reader.skip_newlines();
                    if reader.is_close(Delimiter::Bracket) {
                        reader.expect_close(Delimiter::Bracket)?;
                        break;
                    }
                    // Each signature is (methodName params... ?:ReturnType)
                    if reader.peek() == Some(&Token::LParen) {
                        reader.expect_open(Delimiter::Paren)?;
                        let method_str = reader.read_camel()?;
                        let method_name = names.intern_method(&method_str);
                        methods.push(method_name);
                        module.declared.push(DeclaredName::Method { name: method_name });

                        // Skip the rest of the signature (params, return type)
                        reader.skip_until_close(Delimiter::Paren);
                        reader.expect_close(Delimiter::Paren)?;
                    } else {
                        reader.pos += 1;
                    }
                }
            }

            module.declared.push(DeclaredName::Trait {
                name: trait_name,
                methods,
            });

            reader.skip_newlines();
            reader.expect_close(Delimiter::Paren)?;
        }
        _ => {
            // Skip unknown paren content
            reader.skip_until_close(Delimiter::Paren);
            reader.expect_close(Delimiter::Paren)?;
        }
    }

    Ok(())
}

/// Scan struct fields and record them as DeclaredName::Field.
fn scan_struct_fields(
    reader: &mut TokenReader,
    names: &mut NameRegistry,
    module: &mut ModuleScope,
    owner: TypeName,
    close_delim: Delimiter,
) -> Result<(), String> {
    let mut ordinal = 0u32;
    loop {
        reader.skip_newlines();
        if reader.is_close(close_delim) {
            reader.expect_close(close_delim)?;
            break;
        }

        match reader.peek() {
            Some(Token::PascalIdent(_)) => {
                let field_str = reader.read_pascal()?;
                let field_name = names.intern_field(&field_str);

                // Check for : Type (typed field) vs bare (self-typed)
                reader.skip_newlines();
                let field_type = if reader.peek() == Some(&Token::Colon) {
                    reader.pos += 1; // consume :
                    let type_str = reader.read_type()?;
                    names.intern_type(&type_str)
                } else {
                    // Self-typed: field name IS the type name
                    names.intern_type(&field_str)
                };

                module.declared.push(DeclaredName::Field {
                    name: field_name,
                    owner,
                    field_type,
                    ordinal,
                });
                ordinal += 1;
            }
            _ => {
                reader.pos += 1; // skip unknown
            }
        }
    }
    Ok(())
}

/// Scan a struct declaration: {StructName fields...}
fn scan_struct_decl(
    reader: &mut TokenReader,
    names: &mut NameRegistry,
    module: &mut ModuleScope,
    _mod_name: ModuleName,
) -> Result<(), String> {
    reader.expect_open(Delimiter::Brace)?;
    let struct_str = reader.read_pascal()?;
    let type_name = names.intern_type(&struct_str);
    module.declared.push(DeclaredName::Type {
        name: type_name,
        form: TypeForm::Struct,
    });

    scan_struct_fields(reader, names, module, type_name, Delimiter::Brace)?;
    Ok(())
}

/// Scan a trait impl: [traitName Type [...] Type [...] ...]
fn scan_trait_impl(
    reader: &mut TokenReader,
    names: &mut NameRegistry,
    module: &mut ModuleScope,
    _mod_name: ModuleName,
) -> Result<(), String> {
    reader.expect_open(Delimiter::Bracket)?;

    // Trait name (camelCase)
    let trait_str = reader.read_camel()?;
    let _trait_name = names.intern_trait(&trait_str);

    // Type impls: TypeName [...methods...]
    loop {
        reader.skip_newlines();
        if reader.is_close(Delimiter::Bracket) {
            reader.expect_close(Delimiter::Bracket)?;
            break;
        }

        match reader.peek() {
            Some(Token::PascalIdent(_)) => {
                let _type_str = reader.read_pascal()?;
                // The type should already be declared — just skip the method block
                reader.skip_newlines();
                if reader.peek() == Some(&Token::LBracket) {
                    reader.expect_open(Delimiter::Bracket)?;
                    // Skip method bodies — Stage 1 doesn't parse these
                    skip_balanced(reader, Delimiter::Bracket);
                    reader.expect_close(Delimiter::Bracket)?;
                }
            }
            _ => {
                reader.pos += 1;
            }
        }
    }

    Ok(())
}

/// Scan a const: {|ConstName :Type value|}
fn scan_const_decl(
    reader: &mut TokenReader,
    names: &mut NameRegistry,
    module: &mut ModuleScope,
    _mod_name: ModuleName,
) -> Result<(), String> {
    reader.expect_open(Delimiter::BracePipe)?;

    let const_str = reader.read_pascal()?;
    let const_name = names.intern_type(&const_str);

    // Read type
    let mut typ = TypeName::NONE;
    reader.skip_newlines();
    match reader.peek() {
        Some(Token::PascalIdent(_)) | Some(Token::Dollar) => {
            let type_str = reader.read_type()?;
            typ = names.intern_type(&type_str);
        }
        _ => {}
    }

    module.declared.push(DeclaredName::Const {
        name: const_name,
        typ,
    });

    // Skip the value
    reader.skip_until_close(Delimiter::BracePipe);
    reader.expect_close(Delimiter::BracePipe)?;
    Ok(())
}

/// Scan an FFI declaration: (|FfiName (func1 ...) (func2 ...)|)
fn scan_ffi_decl(
    reader: &mut TokenReader,
    names: &mut NameRegistry,
    module: &mut ModuleScope,
    _mod_name: ModuleName,
) -> Result<(), String> {
    reader.expect_open(Delimiter::ParenPipe)?;

    let lib_str = reader.read_pascal()?;
    let library = names.intern_type(&lib_str);

    let mut functions = Vec::new();
    loop {
        reader.skip_newlines();
        if reader.is_close(Delimiter::ParenPipe) {
            reader.expect_close(Delimiter::ParenPipe)?;
            break;
        }

        if reader.peek() == Some(&Token::LParen) {
            reader.expect_open(Delimiter::Paren)?;
            let func_str = reader.read_camel()?;
            let func_name = names.intern_method(&func_str);
            functions.push(func_name);

            // Skip params
            reader.skip_until_close(Delimiter::Paren);
            reader.expect_close(Delimiter::Paren)?;
        } else {
            reader.pos += 1;
        }
    }

    module.declared.push(DeclaredName::Ffi { library, functions });
    Ok(())
}

/// Skip balanced delimiters (for skipping method bodies).
fn skip_balanced(reader: &mut TokenReader, delim: Delimiter) {
    let mut depth = 1i32;
    while !reader.at_end() && depth > 0 {
        if reader.is_close(delim) {
            depth -= 1;
            if depth == 0 { break; }
        }
        // Track nesting of the same delimiter type
        let opens = match delim {
            Delimiter::Paren => matches!(reader.peek(), Some(Token::LParen)),
            Delimiter::Bracket => matches!(reader.peek(), Some(Token::LBracket)),
            Delimiter::Brace => matches!(reader.peek(), Some(Token::LBrace)),
            Delimiter::ParenPipe => matches!(reader.peek(), Some(Token::LParenPipe)),
            Delimiter::BracketPipe => matches!(reader.peek(), Some(Token::LBracketPipe)),
            Delimiter::BracePipe => matches!(reader.peek(), Some(Token::LBracePipe)),
        };
        if opens { depth += 1; }
        reader.pos += 1;
    }
}

/// After all modules are scanned, resolve export/import names to NameRefs.
fn resolve_exports_and_imports(names: &NameRegistry, scope: &mut ScopeGraph) {
    for module in &mut scope.modules {
        // Re-resolve exports from declared names
        let mut resolved_exports = Vec::new();
        for decl in &module.declared {
            match decl {
                DeclaredName::Type { name, .. } => {
                    resolved_exports.push(NameRef::Type(*name));
                }
                DeclaredName::Trait { name, .. } => {
                    resolved_exports.push(NameRef::Trait(*name));
                }
                _ => {}
            }
        }
        module.exports = resolved_exports;
    }
}

/// Validate that all imports reference existing exports.
fn validate_imports(names: &NameRegistry, scope: &mut ScopeGraph) -> Result<(), String> {
    resolve_exports_and_imports(names, scope);

    // For now, just check that source modules exist
    let module_names: Vec<ModuleName> = scope.modules.iter().map(|m| m.name).collect();
    for module in &scope.modules {
        for import in &module.imports {
            if !module_names.contains(&import.source) {
                return Err(format!(
                    "module '{}' imports from unknown module '{}'",
                    names.resolve_module(module.name),
                    names.resolve_module(import.source),
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_elements_aski() {
        let source = r#"{elements Element Quality describe}

(Element Fire Earth Air Water)

(Quality Passionate Grounded Intellectual Intuitive)

(describe [(describe :@Self Quality)])

[describe Element [
  (describe :@Self Quality (|
    (Fire) Passionate
    (Earth) Grounded
    (Air) Intellectual
    (Water) Intuitive
  |))
]]
"#;

        let tokens = lexer::lex(source).unwrap();
        let mut names = NameRegistry::new();
        let mut scope = ScopeGraph::new();

        let mut reader = TokenReader::new(&tokens);
        reader.skip_newlines();

        let module_scope = scan_module_header(
            &mut reader,
            Path::new("elements.aski"),
            &mut names,
            false,
        ).unwrap();

        assert_eq!(names.resolve_module(module_scope.name), "elements");

        scope.modules.push(module_scope);
        let mod_name = scope.modules[0].name;

        // Scan declarations
        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            match reader.peek() {
                Some(Token::LParen) => {
                    scan_paren_decl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                Some(Token::LBracket) => {
                    scan_trait_impl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                _ => { reader.pos += 1; }
            }
        }

        // Check types
        assert!(names.type_names.contains(&"Element".to_string()));
        assert!(names.type_names.contains(&"Quality".to_string()));

        // Check variants
        assert!(names.variant_names.contains(&"Fire".to_string()));
        assert!(names.variant_names.contains(&"Earth".to_string()));
        assert!(names.variant_names.contains(&"Air".to_string()));
        assert!(names.variant_names.contains(&"Water".to_string()));
        assert!(names.variant_names.contains(&"Passionate".to_string()));

        // Check trait
        assert!(names.trait_names.contains(&"describe".to_string()));
        assert!(names.method_names.contains(&"describe".to_string()));

        // Check declared names in module
        let decls = &scope.modules[0].declared;
        let type_count = decls.iter().filter(|d| matches!(d, DeclaredName::Type { .. })).count();
        let variant_count = decls.iter().filter(|d| matches!(d, DeclaredName::Variant { .. })).count();
        assert_eq!(type_count, 2); // Element, Quality
        assert_eq!(variant_count, 9); // 4 + 5
    }

    #[test]
    fn scan_math_aski() {
        let source = r#"{math Addition compute}

{Addition Left: U32 Right: U32}

(compute [(add :@Self U32)])

[compute Addition [
  (add :@Self U32 [
    ^(@Self.Left + @Self.Right)
  ])
]]
"#;

        let tokens = lexer::lex(source).unwrap();
        let mut names = NameRegistry::new();
        let mut scope = ScopeGraph::new();
        let mut reader = TokenReader::new(&tokens);
        reader.skip_newlines();

        let module_scope = scan_module_header(
            &mut reader,
            Path::new("math.aski"),
            &mut names,
            false,
        ).unwrap();

        scope.modules.push(module_scope);
        let mod_name = scope.modules[0].name;

        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            match reader.peek() {
                Some(Token::LParen) => {
                    scan_paren_decl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                Some(Token::LBrace) => {
                    scan_struct_decl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                Some(Token::LBracket) => {
                    scan_trait_impl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                _ => { reader.pos += 1; }
            }
        }

        // Struct
        assert!(names.type_names.contains(&"Addition".to_string()));
        assert!(names.field_names.contains(&"Left".to_string()));
        assert!(names.field_names.contains(&"Right".to_string()));

        // Fields have correct types
        let fields: Vec<_> = scope.modules[0].declared.iter()
            .filter_map(|d| match d {
                DeclaredName::Field { name, field_type, .. } => {
                    Some((names.resolve_field(*name).to_string(), names.resolve_type(*field_type).to_string()))
                }
                _ => None,
            })
            .collect();
        assert!(fields.contains(&("Left".to_string(), "U32".to_string())));
        assert!(fields.contains(&("Right".to_string(), "U32".to_string())));
    }

    #[test]
    fn scan_generics_aski() {
        let source = r#"{generics Container Item process}

(Item (Text String) (Number I64) Empty)

{Container Items: Vec<Item> Count: U32}

(process [(transform :@Self @items Vec<Item> Vec<Item>)])

[process Container [
  (transform :@Self @items Vec<Item> Vec<Item> [
    ^@items
  ])
]]
"#;

        let tokens = lexer::lex(source).unwrap();
        let mut names = NameRegistry::new();
        let mut scope = ScopeGraph::new();
        let mut reader = TokenReader::new(&tokens);
        reader.skip_newlines();

        let module_scope = scan_module_header(
            &mut reader,
            Path::new("generics.aski"),
            &mut names,
            false,
        ).unwrap();

        scope.modules.push(module_scope);
        let mod_name = scope.modules[0].name;

        loop {
            reader.skip_newlines();
            if reader.at_end() { break; }
            match reader.peek() {
                Some(Token::LParen) => {
                    scan_paren_decl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                Some(Token::LBrace) => {
                    scan_struct_decl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                Some(Token::LBracket) => {
                    scan_trait_impl(&mut reader, &mut names, &mut scope.modules[0], mod_name).unwrap();
                }
                _ => { reader.pos += 1; }
            }
        }

        // Data variants
        assert!(names.variant_names.contains(&"Text".to_string()));
        assert!(names.variant_names.contains(&"Number".to_string()));
        assert!(names.variant_names.contains(&"Empty".to_string()));

        // Generic type
        assert!(names.type_names.contains(&"Vec<Item>".to_string()));

        // Struct fields with generic types
        let fields: Vec<_> = scope.modules[0].declared.iter()
            .filter_map(|d| match d {
                DeclaredName::Field { name, field_type, .. } => {
                    Some((names.resolve_field(*name).to_string(), names.resolve_type(*field_type).to_string()))
                }
                _ => None,
            })
            .collect();
        assert!(fields.contains(&("Items".to_string(), "Vec<Item>".to_string())));
        assert!(fields.contains(&("Count".to_string(), "U32".to_string())));
    }
}
