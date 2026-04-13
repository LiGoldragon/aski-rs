//! Compiler trait — multi-file orchestration.
//!
//! Parses multiple .aski files into a shared AskiWorld, resolving
//! imports across modules. Each file becomes one SemaModule in the
//! resulting SemaWorld.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::synth::types::Dialect;
use super::aski_world::AskiWorld;
use super::sema_world::SemaWorld;
use super::parse::Parse;
use super::lower::Lower;
use super::codegen::Codegen;

pub trait Compiler {
    fn compile_files(&mut self, files: &[String]) -> Result<SemaWorld, String>;
    fn compile_rust(&mut self, files: &[String]) -> Result<String, String>;
}

pub struct AskiCompiler {
    pub dialects: HashMap<String, Dialect>,
    pub synth_dir: String,
}

impl AskiCompiler {
    pub fn new(dialects: HashMap<String, Dialect>, synth_dir: &str) -> Self {
        AskiCompiler {
            dialects,
            synth_dir: synth_dir.to_string(),
        }
    }
}

impl Compiler for AskiCompiler {
    fn compile_files(&mut self, files: &[String]) -> Result<SemaWorld, String> {
        // Parse all files into a shared AskiWorld.
        // Files are parsed in order — earlier files' names are visible to later files.
        let mut world = AskiWorld::new(self.dialects.clone());

        for file in files {
            let source = fs::read_to_string(file)
                .map_err(|e| format!("{}: {}", file, e))?;

            if file.ends_with(".main") {
                world.parse_main(file, &source)?;
            } else {
                world.parse_file(file, &source)?;
            }
        }

        Ok(world.lower())
    }

    fn compile_rust(&mut self, files: &[String]) -> Result<String, String> {
        let sema = self.compile_files(files)?;
        Ok(sema.codegen())
    }
}

/// Resolve file paths from a root file. Reads the module header to
/// find imports, resolves them relative to the file's directory.
pub trait ResolveImports {
    fn resolve_imports(root: &str, synth_dir: &str, dialects: &HashMap<String, Dialect>) -> Result<Vec<String>, String>;
}

impl ResolveImports for AskiCompiler {
    fn resolve_imports(root: &str, _synth_dir: &str, dialects: &HashMap<String, Dialect>) -> Result<Vec<String>, String> {
        let root_path = Path::new(root);
        let dir = root_path.parent().unwrap_or(Path::new("."));

        let source = fs::read_to_string(root)
            .map_err(|e| format!("{}: {}", root, e))?;

        // Quick parse to extract imports
        let mut world = AskiWorld::new(dialects.clone());
        if root.ends_with(".main") {
            world.parse_main(root, &source)?;
        } else {
            world.parse_file(root, &source)?;
        }

        // Find import bracket nodes — children of root that are [ with module names
        let root_children = world.children_of(world.root_id());
        let mut files = Vec::new();

        for child in &root_children {
            if child.constructor == "[" && !child.key.is_empty() {
                // Import: [ModuleName/ items] — resolve ModuleName.aski
                let module_file = dir.join(format!("{}.aski", child.key.to_lowercase()));
                if module_file.exists() {
                    let path = module_file.to_string_lossy().to_string();
                    if !files.contains(&path) {
                        files.push(path);
                    }
                }
            }
        }

        // Add the root file last (imports must be parsed first)
        files.push(root.to_string());
        Ok(files)
    }
}
