#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeForm {
    #[default]
    Domain,
    Struct,
}

impl std::fmt::Display for TypeForm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl TypeForm {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "domain" => Some(Self::Domain),
            "r#struct" => Some(Self::Struct),
            "Struct" => Some(Self::Struct),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Struct => "r#struct",
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatElemKind {
    #[default]
    Terminal,
    NonTerminal,
    Binding,
    BindType,
    BindLit,
    Rest,
}

impl std::fmt::Display for PatElemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl PatElemKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "terminal" => Some(Self::Terminal),
            "non_terminal" => Some(Self::NonTerminal),
            "NonTerminal" => Some(Self::NonTerminal),
            "binding" => Some(Self::Binding),
            "bind_type" => Some(Self::BindType),
            "BindType" => Some(Self::BindType),
            "bind_lit" => Some(Self::BindLit),
            "BindLit" => Some(Self::BindLit),
            "rest" => Some(Self::Rest),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::NonTerminal => "non_terminal",
            Self::Binding => "binding",
            Self::BindType => "bind_type",
            Self::BindLit => "bind_lit",
            Self::Rest => "rest",
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultElemKind {
    #[default]
    Create,
    Bind,
    Recurse,
    Literal,
}

impl std::fmt::Display for ResultElemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ResultElemKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "create" => Some(Self::Create),
            "bind" => Some(Self::Bind),
            "recurse" => Some(Self::Recurse),
            "literal" => Some(Self::Literal),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Bind => "bind",
            Self::Recurse => "recurse",
            Self::Literal => "literal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Type {
    pub id: i64,
    pub name: String,
    pub form: TypeForm,
    pub parent: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub contains_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub field_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: i64,
    pub name: String,
    pub dialect: String,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Arm {
    pub rule_id: i64,
    pub ordinal: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatElem {
    pub rule_id: i64,
    pub arm_ordinal: i64,
    pub elem_ordinal: i64,
    pub kind: PatElemKind,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultElem {
    pub rule_id: i64,
    pub arm_ordinal: i64,
    pub elem_ordinal: i64,
    pub kind: ResultElemKind,
    pub type_name: String,
    pub field_name: String,
    pub binding_name: String,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustSpan {
    #[default]
    Cast,
    MethodCall,
    FreeCall,
    BlockExpr,
    IndexAccess,
}

impl std::fmt::Display for RustSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl RustSpan {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "cast" => Some(Self::Cast),
            "method_call" => Some(Self::MethodCall),
            "MethodCall" => Some(Self::MethodCall),
            "free_call" => Some(Self::FreeCall),
            "FreeCall" => Some(Self::FreeCall),
            "block_expr" => Some(Self::BlockExpr),
            "BlockExpr" => Some(Self::BlockExpr),
            "index_access" => Some(Self::IndexAccess),
            "IndexAccess" => Some(Self::IndexAccess),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Cast => "cast",
            Self::MethodCall => "method_call",
            Self::FreeCall => "free_call",
            Self::BlockExpr => "block_expr",
            Self::IndexAccess => "index_access",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiEntry {
    pub library: String,
    pub aski_name: String,
    pub rust_name: String,
    pub span: RustSpan,
    pub return_type: String,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtxKind {
    #[default]
    Root,
    Item,
    Expr,
    Stmt,
    Pattern,
    TypeRef,
    Body,
    Param,
    Ffi,
    Module,
}

impl std::fmt::Display for CtxKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl CtxKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "root" => Some(Self::Root),
            "item" => Some(Self::Item),
            "expr" => Some(Self::Expr),
            "stmt" => Some(Self::Stmt),
            "pattern" => Some(Self::Pattern),
            "type_ref" => Some(Self::TypeRef),
            "TypeRef" => Some(Self::TypeRef),
            "body" => Some(Self::Body),
            "param" => Some(Self::Param),
            "ffi" => Some(Self::Ffi),
            "module" => Some(Self::Module),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Item => "item",
            Self::Expr => "expr",
            Self::Stmt => "stmt",
            Self::Pattern => "pattern",
            Self::TypeRef => "type_ref",
            Self::Body => "body",
            Self::Param => "param",
            Self::Ffi => "ffi",
            Self::Module => "module",
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseStatus {
    #[default]
    Staged,
    Committed,
}

impl std::fmt::Display for ParseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ParseStatus {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "staged" => Some(Self::Staged),
            "committed" => Some(Self::Committed),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Committed => "committed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseNode {
    pub id: i64,
    pub constructor: String,
    pub ctx: CtxKind,
    pub parent_id: i64,
    pub status: ParseStatus,
    pub text: String,
    pub token_start: i64,
    pub token_end: i64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ParseChild {
    pub parent_id: i64,
    pub ordinal: i64,
    pub child_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub module_name: String,
    pub export_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantOf {
    pub variant_name: String,
    pub type_name: String,
    pub type_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeKind {
    pub type_name: String,
    pub category: TypeForm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainedType {
    pub parent_type: String,
    pub child_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecursiveType {
    pub parent_type: String,
    pub child_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    pub types: Vec<Type>,
    pub variants: Vec<Variant>,
    pub fields: Vec<Field>,
    pub rules: Vec<Rule>,
    pub arms: Vec<Arm>,
    pub pat_elems: Vec<PatElem>,
    pub result_elems: Vec<ResultElem>,
    pub ffi_entries: Vec<FfiEntry>,
    pub parse_nodes: Vec<ParseNode>,
    pub parse_children: Vec<ParseChild>,
    pub modules: Vec<Module>,
    pub exports: Vec<Export>,
    pub variant_ofs: Vec<VariantOf>,
    pub type_kinds: Vec<TypeKind>,
    pub contained_types: Vec<ContainedType>,
    pub recursive_types: Vec<RecursiveType>,
}

impl Default for World { fn default() -> Self { Self { types: Default::default(), variants: Default::default(), fields: Default::default(), rules: Default::default(), arms: Default::default(), pat_elems: Default::default(), result_elems: Default::default(), ffi_entries: Default::default(), parse_nodes: Default::default(), parse_children: Default::default(), modules: Default::default(), exports: Default::default(), variant_ofs: Default::default(), type_kinds: Default::default(), contained_types: Default::default(), recursive_types: Default::default(), } } }

impl World {
    pub fn new() -> Self { Self::default() }

    pub fn r#type_by_id(&self, val: i64) -> Vec<&Type> {
        self.types.iter().filter(|r| r.id == val).collect()
    }

    pub fn r#type_by_name(&self, val: &str) -> Vec<&Type> {
        self.types.iter().filter(|r| r.name == val).collect()
    }

    pub fn r#type_by_form(&self, val: TypeForm) -> Vec<&Type> {
        self.types.iter().filter(|r| r.form == val).collect()
    }

    pub fn r#type_by_parent(&self, val: i64) -> Vec<&Type> {
        self.types.iter().filter(|r| r.parent == val).collect()
    }

    pub fn variant_by_type_id(&self, val: i64) -> Vec<&Variant> {
        self.variants.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn variant_by_ordinal(&self, val: i64) -> Vec<&Variant> {
        self.variants.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn variant_by_name(&self, val: &str) -> Vec<&Variant> {
        self.variants.iter().filter(|r| r.name == val).collect()
    }

    pub fn variant_by_contains_type(&self, val: &str) -> Vec<&Variant> {
        self.variants.iter().filter(|r| r.contains_type == val).collect()
    }

    pub fn field_by_type_id(&self, val: i64) -> Vec<&Field> {
        self.fields.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn field_by_ordinal(&self, val: i64) -> Vec<&Field> {
        self.fields.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn field_by_name(&self, val: &str) -> Vec<&Field> {
        self.fields.iter().filter(|r| r.name == val).collect()
    }

    pub fn field_by_field_type(&self, val: &str) -> Vec<&Field> {
        self.fields.iter().filter(|r| r.field_type == val).collect()
    }

    pub fn rule_by_id(&self, val: i64) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.id == val).collect()
    }

    pub fn rule_by_name(&self, val: &str) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.name == val).collect()
    }

    pub fn rule_by_dialect(&self, val: &str) -> Vec<&Rule> {
        self.rules.iter().filter(|r| r.dialect == val).collect()
    }

    pub fn arm_by_rule_id(&self, val: i64) -> Vec<&Arm> {
        self.arms.iter().filter(|r| r.rule_id == val).collect()
    }

    pub fn arm_by_ordinal(&self, val: i64) -> Vec<&Arm> {
        self.arms.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn pat_elem_by_rule_id(&self, val: i64) -> Vec<&PatElem> {
        self.pat_elems.iter().filter(|r| r.rule_id == val).collect()
    }

    pub fn pat_elem_by_arm_ordinal(&self, val: i64) -> Vec<&PatElem> {
        self.pat_elems.iter().filter(|r| r.arm_ordinal == val).collect()
    }

    pub fn pat_elem_by_elem_ordinal(&self, val: i64) -> Vec<&PatElem> {
        self.pat_elems.iter().filter(|r| r.elem_ordinal == val).collect()
    }

    pub fn pat_elem_by_kind(&self, val: PatElemKind) -> Vec<&PatElem> {
        self.pat_elems.iter().filter(|r| r.kind == val).collect()
    }

    pub fn pat_elem_by_name(&self, val: &str) -> Vec<&PatElem> {
        self.pat_elems.iter().filter(|r| r.name == val).collect()
    }

    pub fn result_elem_by_rule_id(&self, val: i64) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.rule_id == val).collect()
    }

    pub fn result_elem_by_arm_ordinal(&self, val: i64) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.arm_ordinal == val).collect()
    }

    pub fn result_elem_by_elem_ordinal(&self, val: i64) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.elem_ordinal == val).collect()
    }

    pub fn result_elem_by_kind(&self, val: ResultElemKind) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.kind == val).collect()
    }

    pub fn result_elem_by_type_name(&self, val: &str) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.type_name == val).collect()
    }

    pub fn result_elem_by_field_name(&self, val: &str) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.field_name == val).collect()
    }

    pub fn result_elem_by_binding_name(&self, val: &str) -> Vec<&ResultElem> {
        self.result_elems.iter().filter(|r| r.binding_name == val).collect()
    }

    pub fn ffi_entry_by_library(&self, val: &str) -> Vec<&FfiEntry> {
        self.ffi_entries.iter().filter(|r| r.library == val).collect()
    }

    pub fn ffi_entry_by_aski_name(&self, val: &str) -> Vec<&FfiEntry> {
        self.ffi_entries.iter().filter(|r| r.aski_name == val).collect()
    }

    pub fn ffi_entry_by_rust_name(&self, val: &str) -> Vec<&FfiEntry> {
        self.ffi_entries.iter().filter(|r| r.rust_name == val).collect()
    }

    pub fn ffi_entry_by_span(&self, val: RustSpan) -> Vec<&FfiEntry> {
        self.ffi_entries.iter().filter(|r| r.span == val).collect()
    }

    pub fn ffi_entry_by_return_type(&self, val: &str) -> Vec<&FfiEntry> {
        self.ffi_entries.iter().filter(|r| r.return_type == val).collect()
    }

    pub fn parse_node_by_id(&self, val: i64) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.id == val).collect()
    }

    pub fn parse_node_by_constructor(&self, val: &str) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.constructor == val).collect()
    }

    pub fn parse_node_by_ctx(&self, val: CtxKind) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.ctx == val).collect()
    }

    pub fn parse_node_by_parent_id(&self, val: i64) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.parent_id == val).collect()
    }

    pub fn parse_node_by_status(&self, val: ParseStatus) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.status == val).collect()
    }

    pub fn parse_node_by_text(&self, val: &str) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.text == val).collect()
    }

    pub fn parse_node_by_token_start(&self, val: i64) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.token_start == val).collect()
    }

    pub fn parse_node_by_token_end(&self, val: i64) -> Vec<&ParseNode> {
        self.parse_nodes.iter().filter(|r| r.token_end == val).collect()
    }

    pub fn parse_child_by_parent_id(&self, val: i64) -> Vec<&ParseChild> {
        self.parse_children.iter().filter(|r| r.parent_id == val).collect()
    }

    pub fn parse_child_by_ordinal(&self, val: i64) -> Vec<&ParseChild> {
        self.parse_children.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn parse_child_by_child_id(&self, val: i64) -> Vec<&ParseChild> {
        self.parse_children.iter().filter(|r| r.child_id == val).collect()
    }

    pub fn module_by_name(&self, val: &str) -> Vec<&Module> {
        self.modules.iter().filter(|r| r.name == val).collect()
    }

    pub fn module_by_path(&self, val: &str) -> Vec<&Module> {
        self.modules.iter().filter(|r| r.path == val).collect()
    }

    pub fn export_by_module_name(&self, val: &str) -> Vec<&Export> {
        self.exports.iter().filter(|r| r.module_name == val).collect()
    }

    pub fn export_by_export_name(&self, val: &str) -> Vec<&Export> {
        self.exports.iter().filter(|r| r.export_name == val).collect()
    }

    pub fn variant_of_by_variant_name(&self, val: &str) -> Vec<&VariantOf> {
        self.variant_ofs.iter().filter(|r| r.variant_name == val).collect()
    }

    pub fn variant_of_by_type_name(&self, val: &str) -> Vec<&VariantOf> {
        self.variant_ofs.iter().filter(|r| r.type_name == val).collect()
    }

    pub fn variant_of_by_type_id(&self, val: i64) -> Vec<&VariantOf> {
        self.variant_ofs.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn type_kind_by_type_name(&self, val: &str) -> Vec<&TypeKind> {
        self.type_kinds.iter().filter(|r| r.type_name == val).collect()
    }

    pub fn type_kind_by_category(&self, val: TypeForm) -> Vec<&TypeKind> {
        self.type_kinds.iter().filter(|r| r.category == val).collect()
    }

    pub fn contained_type_by_parent_type(&self, val: &str) -> Vec<&ContainedType> {
        self.contained_types.iter().filter(|r| r.parent_type == val).collect()
    }

    pub fn contained_type_by_child_type(&self, val: &str) -> Vec<&ContainedType> {
        self.contained_types.iter().filter(|r| r.child_type == val).collect()
    }

    pub fn recursive_type_by_parent_type(&self, val: &str) -> Vec<&RecursiveType> {
        self.recursive_types.iter().filter(|r| r.parent_type == val).collect()
    }

    pub fn recursive_type_by_child_type(&self, val: &str) -> Vec<&RecursiveType> {
        self.recursive_types.iter().filter(|r| r.child_type == val).collect()
    }

}

pub trait Derive {
    fn derive(&mut self);
    fn derive_variant_of(&mut self);
    fn derive_type_kind(&mut self);
    fn derive_contained_type(&mut self);
    fn derive_recursive_type(&mut self);
}

impl World {
    pub fn derive(&mut self) {
        self.derive_variant_of();
        self.derive_type_kind();
        self.derive_contained_type();
        self.derive_recursive_type_fixpoint();
    }

    fn derive_variant_of(&mut self) {
        let mut results = Vec::new();
        for r#type in &self.types {
            if r#type.form == TypeForm::Domain {
                for variant in &self.variants {
                    if variant.type_id == r#type.id {
                        results.push(VariantOf { variant_name: variant.name.clone(), type_name: r#type.name.clone(), type_id: r#type.id });
                    }
                }
            }
        }
        self.variant_ofs = results;
    }

    fn derive_type_kind(&mut self) {
        let mut results = Vec::new();
        for r#type in &self.types {
            results.push(TypeKind { type_name: r#type.name.clone(), category: r#type.form });
        }
        self.type_kinds = results;
    }

    fn derive_contained_type(&mut self) {
        let mut results = Vec::new();
        for r#type in &self.types {
            if r#type.form == TypeForm::Struct {
                for field in &self.fields {
                    if field.type_id == r#type.id {
                        results.push(ContainedType { parent_type: r#type.name.clone(), child_type: field.field_type.clone() });
                    }
                }
            }
        }
        for r#type in &self.types {
            if r#type.form == TypeForm::Domain {
                for variant in &self.variants {
                    if variant.type_id == r#type.id {
                        if !variant.contains_type.is_empty() {
                            results.push(ContainedType { parent_type: r#type.name.clone(), child_type: variant.contains_type.clone() });
                        }
                    }
                }
            }
        }
        self.contained_types = results;
    }

    fn derive_recursive_type_fixpoint(&mut self) {
        {
            let mut results = Vec::new();
            for contained_type in &self.contained_types {
                results.push(RecursiveType { parent_type: contained_type.parent_type.clone(), child_type: contained_type.child_type.clone() });
            }
            self.recursive_types = results;
        }
        loop {
            let mut new_items = Vec::new();
            for contained_type in &self.contained_types {
                for reach in &self.recursive_types {
                    if reach.parent_type == contained_type.child_type {
                        new_items.push(RecursiveType { parent_type: contained_type.parent_type.clone(), child_type: reach.child_type.clone() });
                    }
                }
            }
            new_items.retain(|item| !self.recursive_types.contains(item));
            if new_items.is_empty() { break; }
            self.recursive_types.extend(new_items);
        }
    }

}

