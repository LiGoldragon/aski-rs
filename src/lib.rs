pub mod lexer;
pub mod ast;
pub mod context;
pub mod parser;
pub mod engine;
pub mod grammar_engine_full;
pub mod ir;
pub mod codegen;
pub mod codec;
pub mod compiler;

/// Deprecated re-export — use `ir` instead.
pub mod db {
    pub use crate::ir::*;
    /// Alias: create_db() → create_world() + returns World
    pub type DbInstance = crate::ir::World;
    pub fn create_db() -> Result<crate::ir::World, String> {
        let w = crate::ir::create_world();
        // No schema init needed — Ascent handles it
        Ok(w)
    }
}
