//! Sema tests — binary roundtrip and no-strings verification.

#[cfg(test)]
mod tests {
    use crate::synth::loader;
    use crate::engine::aski_world::AskiWorld;
    use crate::engine::parse::Parse;
    use crate::engine::lower::Lower;
    use crate::engine::sema::*;
    use crate::engine::codegen::{CodegenContext, Codegen};
    use std::collections::HashMap;

    fn make_world_from_source(_source: &str) -> AskiWorld {
        let synth_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../aski-core/source");
        let dialects = if synth_dir.exists() {
            loader::load_all(&synth_dir).unwrap_or_default()
        } else {
            // Inline minimal dialects for CI
            let mut d = HashMap::new();
            d.insert("aski".into(), loader::load_dialect("aski", r#"
                // !{@module/ <module>}
                // *(@Domain/ <domain>)
                // *(@trait/ <trait-decl>)
                // *[@trait/ <trait-impl>]
                // *{@Struct/ <struct>}
                // *{|@Const/ :Type @value|}
            "#).unwrap());
            d.insert("module".into(), loader::load_dialect("module", "+@export//@Export").unwrap());
            d.insert("domain".into(), loader::load_dialect("domain", "// *@Variant\n// *(@Variant/ :Type)").unwrap());
            d.insert("struct".into(), loader::load_dialect("struct", "+@Field :Type").unwrap());
            d.insert("trait-decl".into(), loader::load_dialect("trait-decl", "[+(@signature/)]").unwrap());
            d.insert("trait-impl".into(), loader::load_dialect("trait-impl", ":Type [<type-impl>]").unwrap());
            d.insert("type-impl".into(), loader::load_dialect("type-impl", "+(@method/)").unwrap());
            d
        };
        AskiWorld::new(dialects)
    }

    #[test]
    fn sema_binary_contains_no_strings() {
        let mut world = make_world_from_source("");
        world.parse_file("test.aski", concat!(
            "{test/ Element describe} ",
            "(Element/ Fire Earth Air Water) ",
            "(describe/ [(describe/ :@Self Quality)])",
        )).unwrap();

        let result = world.lower();
        let bytes = result.sema.to_sema_bytes();

        // Check that no type/variant/trait/method names appear in the binary
        let known_names = ["Element", "Fire", "Earth", "Air", "Water",
                          "describe", "Quality", "test", "Self"];
        for name in &known_names {
            let name_bytes = name.as_bytes();
            let found = bytes.windows(name_bytes.len()).any(|w| w == name_bytes);
            assert!(!found, "sema binary contains string '{}' — strings leaked into sema", name);
        }

        assert!(bytes.len() > 0, "sema binary should not be empty");
    }

    #[test]
    fn sema_binary_roundtrip() {
        let mut world = make_world_from_source("");
        world.parse_file("test.aski", concat!(
            "{test/ Element describe} ",
            "(Element/ Fire Earth Air Water) ",
        )).unwrap();

        let result = world.lower();

        // Serialize
        let bytes = result.sema.to_sema_bytes();

        // Deserialize
        let sema2 = Sema::from_sema_bytes(&bytes).expect("deserialization failed");

        // Verify structure matches
        assert_eq!(sema2.types.len(), result.sema.types.len());
        assert_eq!(sema2.variants.len(), result.sema.variants.len());
        for (a, b) in sema2.types.iter().zip(result.sema.types.iter()) {
            assert_eq!(a.name, b.name);
            assert_eq!(a.form, b.form);
        }
        for (a, b) in sema2.variants.iter().zip(result.sema.variants.iter()) {
            assert_eq!(a.type_id, b.type_id);
            assert_eq!(a.name, b.name);
            assert_eq!(a.ordinal, b.ordinal);
        }
    }

    #[test]
    fn aski_sema_aski_roundtrip() {
        let source = concat!(
            "{test/ Element describe} ",
            "(Element/ Fire Earth Air Water) ",
            "(describe/ [(describe/ :@Self Quality)]) ",
            "[describe/ Element [(describe/ :@Self Quality (| ",
            "(Fire) Passionate (Earth) Grounded (Air) Intellectual (Water) Intuitive ",
            "|))]]",
        );

        // Parse original → lower → sema
        let mut world1 = make_world_from_source("");
        world1.parse_file("test.aski", source).unwrap();
        let result1 = world1.lower();

        // Raise → deparse → aski text
        use crate::engine::raise::Raise;
        use crate::engine::deparse::Deparse;
        let raised = AskiWorld::raise(&result1.sema, &result1.names, &result1.exports, world1.dialects.clone());
        let roundtripped = raised.deparse();

        // Parse roundtripped text → lower → sema
        let mut world2 = make_world_from_source("");
        world2.parse_file("test.aski", &roundtripped).unwrap();
        let result2 = world2.lower();

        // Verify sema structure matches
        assert_eq!(result1.sema.types.len(), result2.sema.types.len(),
            "type count mismatch: {} vs {}", result1.sema.types.len(), result2.sema.types.len());
        assert_eq!(result1.sema.variants.len(), result2.sema.variants.len(),
            "variant count mismatch");
        assert_eq!(result1.sema.trait_decls.len(), result2.sema.trait_decls.len(),
            "trait_decl count mismatch");
        assert_eq!(result1.sema.trait_impls.len(), result2.sema.trait_impls.len(),
            "trait_impl count mismatch");

        // Verify names match
        assert_eq!(result1.names.type_names, result2.names.type_names, "type names differ");
        assert_eq!(result1.names.variant_names, result2.names.variant_names, "variant names differ");
    }

    #[test]
    fn sema_codegen_roundtrip() {
        let mut world = make_world_from_source("");
        world.parse_file("test.aski", concat!(
            "{test/ Element describe} ",
            "(Element/ Fire Earth Air Water) ",
        )).unwrap();

        let result = world.lower();

        // Serialize + deserialize
        let bytes = result.sema.to_sema_bytes();
        let sema2 = Sema::from_sema_bytes(&bytes).expect("deserialization failed");

        // Codegen from original
        let ctx1 = CodegenContext { sema: &result.sema, names: &result.names };
        let rust1 = ctx1.codegen();

        // Codegen from deserialized (same names — names are aski-side)
        let ctx2 = CodegenContext { sema: &sema2, names: &result.names };
        let rust2 = ctx2.codegen();

        assert_eq!(rust1, rust2, "codegen from original and deserialized sema should match");
    }
}
