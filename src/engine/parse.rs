//! Parse trait — entry points for parsing .aski and .main files.

use crate::lexer;
use super::aski_world::AskiWorld;
use super::tokens::TokenReader;
use super::parse_dialect::ParseDialect;

pub trait Parse {
    fn parse_file(&mut self, file_path: &str, source: &str) -> Result<(), String>;
    fn parse_main(&mut self, file_path: &str, source: &str) -> Result<(), String>;
}

impl Parse for AskiWorld {
    fn parse_file(&mut self, file_path: &str, source: &str) -> Result<(), String> {
        let tokens = lexer::lex(source).map_err(|errs| {
            errs.iter().map(|e| format!("{}: {}", e.span.start, e.text)).collect::<Vec<_>>().join(", ")
        })?;
        self.current_file = file_path.to_string();
        self.push_dialect("aski");
        let root = self.root_id();
        let mut reader = TokenReader::new(&tokens);
        self.parse_dialect_looping(&mut reader, root)?;
        self.pop_dialect();
        Ok(())
    }

    fn parse_main(&mut self, file_path: &str, source: &str) -> Result<(), String> {
        use super::parse_expr::ParseExpr;
        let tokens = lexer::lex(source).map_err(|errs| {
            errs.iter().map(|e| format!("{}: {}", e.span.start, e.text)).collect::<Vec<_>>().join(", ")
        })?;
        self.current_file = file_path.to_string();
        let root = self.root_id();
        let mut reader = TokenReader::new(&tokens);

        // Parse imports via main dialect
        self.push_dialect("main");
        self.parse_dialect_looping(&mut reader, root)?;
        self.pop_dialect();

        // Parse process body — remaining tokens are statements
        let body_id = self.make_node("ProcessBody", "", 0, 0);
        self.add_child(root, body_id);
        self.parse_body(&mut reader, body_id)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::loader;
    use std::collections::HashMap;

    fn make_world() -> AskiWorld {
        let mut dialects = HashMap::new();
        dialects.insert("aski".into(), loader::load_dialect("aski", r#"
            // !{@module/ <module>}
            // *(@Domain/ <domain>)
            // *(@trait/ <trait-decl>)
            // *[@trait/ <trait-impl>]
            // *{@Struct/ <struct>}
            // *{|@Const/ :Type @value|}
            // *(|@Ffi/ <ffi>|)
        "#).unwrap());
        dialects.insert("module".into(), loader::load_dialect("module", r#"
            +@export//@Export
        "#).unwrap());
        dialects.insert("domain".into(), loader::load_dialect("domain", r#"
            +@Variant
        "#).unwrap());
        dialects.insert("struct".into(), loader::load_dialect("struct", r#"
            +@Field :Type
        "#).unwrap());
        dialects.insert("trait-decl".into(), loader::load_dialect("trait-decl", r#"
            [+(@signature/)]
        "#).unwrap());
        dialects.insert("trait-impl".into(), loader::load_dialect("trait-impl", r#"
            :Type [<type-impl>]
        "#).unwrap());
        dialects.insert("type-impl".into(), loader::load_dialect("type-impl", r#"
            +(@method/)
        "#).unwrap());
        dialects.insert("ffi".into(), loader::load_dialect("ffi", r#"
            +(@foreignFunction/)
        "#).unwrap());
        AskiWorld::new(dialects)
    }

    #[test]
    fn parse_domain() {
        let mut world = make_world();
        world.parse_file("test.aski", "{test/ describe} (Element/ Fire Earth Air Water)").unwrap();
        assert!(world.is_domain("Element"));
        assert!(world.is_variant("Fire"));
        assert!(world.is_variant("Water"));
        let children = world.children_of(world.root_id());
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn parse_struct() {
        let mut world = make_world();
        world.parse_file("test.aski", "{test/ compute} {Point/ Horizontal F64 Vertical F64}").unwrap();
        assert!(world.is_struct("Point"));
        let children = world.children_of(world.root_id());
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn parse_multiple() {
        let mut world = make_world();
        world.parse_file("test.aski",
            "{test/ describe compute} (Element/ Fire Earth) {Point/ X F64 Y F64} (Quality/ High Low)"
        ).unwrap();
        assert!(world.is_domain("Element"));
        assert!(world.is_domain("Quality"));
        assert!(world.is_struct("Point"));
        assert!(world.is_variant("Fire"));
        assert!(world.is_variant("High"));
    }

    #[test]
    fn parse_trait_decl() {
        let mut world = make_world();
        world.parse_file("test.aski",
            "{test/ describe} (describe/ [(describe/ :@Self Quality)])"
        ).unwrap();
        assert!(world.is_trait("describe"));
        let children = world.children_of(world.root_id());
        assert_eq!(children.len(), 2); // module + trait-decl
    }

    #[test]
    fn parse_trait_impl() {
        let mut world = make_world();
        world.parse_file("test.aski", concat!(
            "{test/ Element describe} ",
            "(Element/ Fire Earth) ",
            "(describe/ [(describe/ :@Self Quality)]) ",
            "[describe/ Element [(describe/ :@Self Quality [^Fire])]]",
        )).unwrap();
        assert!(world.is_trait("describe"));
        assert!(world.is_domain("Element"));
        let children = world.children_of(world.root_id());
        assert_eq!(children.len(), 4); // module + domain + trait-decl + trait-impl
    }

    #[test]
    fn parse_const() {
        let mut world = make_world();
        world.parse_file("test.aski",
            "{test/ Pi} {|Pi/ F64 3.14|}"
        ).unwrap();
        let children = world.children_of(world.root_id());
        assert_eq!(children.len(), 2); // module + const
    }

    #[test]
    fn parse_ffi() {
        let mut world = make_world();
        world.parse_file("test.aski",
            "{test/ Cast} (|Cast/ (toF32/ @Self F32)|)"
        ).unwrap();
        let children = world.children_of(world.root_id());
        assert_eq!(children.len(), 2); // module + ffi
    }
}
