//! Backward-compatibility shim — re-exports from `engine`.
//!
//! All parser logic now lives in `crate::engine`. This module exists only
//! so that `crate::grammar_engine_full::parse_source_file` and
//! `parse_source` continue to resolve for any external callers.

pub use crate::engine::{parse_source, parse_source_file};
