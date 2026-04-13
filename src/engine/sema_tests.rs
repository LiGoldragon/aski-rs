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

    fn make_world_from_source(source: &str) -> AskiWorld {
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
