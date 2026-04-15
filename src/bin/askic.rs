//! askic — aski compiler (v0.16)
//!
//! Three-stage pipeline:
//!   Stage 1 (synth compiler): .synth + .aski headers → enums + scopes
//!   Stage 2 (aski compiler):  SynthOutput + .aski → typed data-tree
//!   Stage 3 (sema compiler):  DataTree → .sema + .aski-table.sema + codegen
//!
//! Commands:
//!   askic compile <file.aski>  → .sema + .aski-table.sema
//!   askic rust <file.sema>     → Rust source from sema binary
//!   askic deparse <file.sema>  → aski text from sema binary

fn main() {
    eprintln!("askic v0.16 — pipeline not yet implemented");
    eprintln!("See: ~/git/aski/spec/pipeline.md");
    std::process::exit(1);
}
