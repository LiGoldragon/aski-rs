//! Dialect loader — converts synth::loader output into a typed DialectTable.

use std::collections::HashMap;
use std::path::Path;

use crate::synth::loader;
use crate::synth::types::Dialect;
use super::types::{DialectKind, DialectTable};

/// Load all .synth files from a directory and build the DialectTable.
pub fn load_dialect_table(synth_dir: &Path) -> Result<DialectTable, String> {
    let raw = loader::load_all(synth_dir)?;
    build_table(raw)
}

/// Convert a HashMap<String, Dialect> into a DialectTable indexed by DialectKind.
fn build_table(raw: HashMap<String, Dialect>) -> Result<DialectTable, String> {
    let mut table = DialectTable::new();
    let mut loaded = 0;

    for (name, dialect) in raw {
        match DialectKind::from_filename(&name) {
            Some(kind) => {
                table.insert(kind, dialect);
                loaded += 1;
            }
            None => {
                return Err(format!("unknown synth dialect: {}", name));
            }
        }
    }

    // Validate we got all required dialects
    let required = [
        DialectKind::Aski,
        DialectKind::Module,
        DialectKind::Domain,
        DialectKind::Struct,
        DialectKind::TraitDecl,
        DialectKind::TraitImpl,
        DialectKind::TypeImpl,
        DialectKind::Method,
        DialectKind::Signature,
        DialectKind::Param,
        DialectKind::Body,
        DialectKind::Statement,
        DialectKind::Expr,
        DialectKind::ExprAtom,
    ];

    for kind in &required {
        if table.get(*kind).is_none() {
            return Err(format!("missing required dialect: {:?}", kind));
        }
    }

    eprintln!("synth: loaded {} dialects", loaded);
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn load_aski_core_dialects() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../aski-core/source");
        if !dir.exists() {
            eprintln!("skipping: aski-core not found at {:?}", dir);
            return;
        }
        let table = load_dialect_table(&dir).unwrap();
        assert!(table.get(DialectKind::Aski).is_some());
        assert!(table.get(DialectKind::Domain).is_some());
        assert!(table.get(DialectKind::Struct).is_some());
        assert!(table.get(DialectKind::Body).is_some());
        assert!(table.get(DialectKind::ExprAtom).is_some());
    }
}
