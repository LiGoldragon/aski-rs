/// Parser context — tracks what kind of syntactic scope we're inside.
/// Delimiters in aski are context-dependent: `()` means "domain variants"
/// at top level but "match arms" inside a function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    /// Top level — domain, struct, trait, impl, function, module declarations
    TopLevel,
    /// Inside `()` after `Name `: domain variants
    DomainBody,
    /// Inside `{}`: struct sub-type declarations
    StructBody,
    /// Inside `([])`: trait method signatures + associated types/consts
    TraitDecl,
    /// Inside `[]` at top level: trait impl
    TraitImpl,
    /// Inside `{}` after method/function signature: expressions
    MethodBody,
    /// Inside `{}` after function signature: expressions
    FunctionBody,
    /// Inside `[]`: closure inputs | body
    Closure,
    /// Inside `()` after @Instance: variant match arms (legacy)
    MatchArms,
    /// Inside `(| |)`: universal match/dispatch expression
    MatchDispatch,
    /// Inside `(| |)` at top level: composition factor headers + variant grid
    CompositionMatrix,
    /// Inside `{}` after Name (no space): generic type params
    GenericDecl,
    /// Second `{}` after generic decl: bounds (where clause)
    WhereClause,
    /// Inside `{}` of Module: declarations
    ModuleBody,
    /// Inside `()` of Module header: export list
    ModuleExports,
    /// Inside `[]`: loop iteration body
    LoopBody,
}

/// The context stack tracks nested scopes during parsing.
#[derive(Debug, Clone)]
pub struct ContextStack {
    stack: Vec<Context>,
}

impl ContextStack {
    pub fn new() -> Self {
        Self {
            stack: vec![Context::TopLevel],
        }
    }

    pub fn current(&self) -> Context {
        *self.stack.last().unwrap_or(&Context::TopLevel)
    }

    pub fn push(&mut self, ctx: Context) {
        self.stack.push(ctx);
    }

    pub fn pop(&mut self) -> Option<Context> {
        if self.stack.len() > 1 {
            self.stack.pop()
        } else {
            None
        }
    }

    /// Check if we're currently inside an expression context
    /// (function body, method body, closure, loop, or match arms).
    pub fn in_expr_context(&self) -> bool {
        matches!(
            self.current(),
            Context::FunctionBody
                | Context::MethodBody
                | Context::Closure
                | Context::MatchArms
                | Context::MatchDispatch
                | Context::LoopBody
        )
    }

    /// Check if we're at a declaration level (top-level or module body).
    pub fn in_decl_context(&self) -> bool {
        matches!(
            self.current(),
            Context::TopLevel | Context::ModuleBody | Context::StructBody
        )
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

impl Default for ContextStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_stack_basics() {
        let mut stack = ContextStack::new();
        assert_eq!(stack.current(), Context::TopLevel);
        assert!(stack.in_decl_context());
        assert!(!stack.in_expr_context());

        stack.push(Context::FunctionBody);
        assert_eq!(stack.current(), Context::FunctionBody);
        assert!(stack.in_expr_context());
        assert!(!stack.in_decl_context());

        stack.pop();
        assert_eq!(stack.current(), Context::TopLevel);
    }

    #[test]
    fn context_stack_no_underflow() {
        let mut stack = ContextStack::new();
        // Should not pop the last element
        assert!(stack.pop().is_none());
        assert_eq!(stack.current(), Context::TopLevel);
    }
}
