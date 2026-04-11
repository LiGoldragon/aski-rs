use crate::helpers::{StringExt, VecExt, ToI64, WithPush};
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
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
            "struct" => Some(Self::Struct),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Domain => "domain",
            Self::Struct => "struct",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TypeEntry {
    pub id: i64,
    pub name: String,
    pub form: TypeForm,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct VariantDef {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub contains_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FieldDef {
    pub type_id: i64,
    pub ordinal: i64,
    pub name: String,
    pub field_type: String,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct FfiEntry {
    pub library: String,
    pub aski_name: String,
    pub rust_name: String,
    pub span: RustSpan,
    pub return_type: String,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ParamKind {
    #[default]
    BorrowSelf,
    MutBorrowSelf,
    OwnedSelf,
    Named,
}

impl std::fmt::Display for ParamKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ParamKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "borrow_self" => Some(Self::BorrowSelf),
            "BorrowSelf" => Some(Self::BorrowSelf),
            "mut_borrow_self" => Some(Self::MutBorrowSelf),
            "MutBorrowSelf" => Some(Self::MutBorrowSelf),
            "owned_self" => Some(Self::OwnedSelf),
            "OwnedSelf" => Some(Self::OwnedSelf),
            "named" => Some(Self::Named),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::BorrowSelf => "borrow_self",
            Self::MutBorrowSelf => "mut_borrow_self",
            Self::OwnedSelf => "owned_self",
            Self::Named => "named",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct Param {
    pub kind: ParamKind,
    pub name: String,
    pub param_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
}

impl Default for MethodSig { fn default() -> Self { Self { name: Default::default(), params: Default::default(), return_type: Default::default(), } } }

impl MethodSig {
    pub fn new() -> Self { Self::default() }

    pub fn param_by_kind(&self, val: ParamKind) -> Vec<&Param> {
        self.params.iter().filter(|r| r.kind == val).collect()
    }

    pub fn param_by_name(&self, val: &str) -> Vec<&Param> {
        self.params.iter().filter(|r| r.name == val).collect()
    }

    pub fn param_by_param_type(&self, val: &str) -> Vec<&Param> {
        self.params.iter().filter(|r| r.param_type == val).collect()
    }

}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum BodyKind {
    #[default]
    Block,
    TailBlock,
    MatchBody,
}

impl std::fmt::Display for BodyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl BodyKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "block" => Some(Self::Block),
            "tail_block" => Some(Self::TailBlock),
            "TailBlock" => Some(Self::TailBlock),
            "match_body" => Some(Self::MatchBody),
            "MatchBody" => Some(Self::MatchBody),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::TailBlock => "tail_block",
            Self::MatchBody => "match_body",
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ValueKind {
    #[default]
    Str,
    Int,
    List,
    None,
    TypeItem,
    VariantItem,
    FieldItem,
    FfiItem,
}

impl std::fmt::Display for ValueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl ValueKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "str" => Some(Self::Str),
            "int" => Some(Self::Int),
            "list" => Some(Self::List),
            "none" => Some(Self::None),
            "type_item" => Some(Self::TypeItem),
            "TypeItem" => Some(Self::TypeItem),
            "variant_item" => Some(Self::VariantItem),
            "VariantItem" => Some(Self::VariantItem),
            "field_item" => Some(Self::FieldItem),
            "FieldItem" => Some(Self::FieldItem),
            "ffi_item" => Some(Self::FfiItem),
            "FfiItem" => Some(Self::FfiItem),
            _ => None,
        }
    }

    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Str => "str",
            Self::Int => "int",
            Self::List => "list",
            Self::None => "none",
            Self::TypeItem => "type_item",
            Self::VariantItem => "variant_item",
            Self::FieldItem => "field_item",
            Self::FfiItem => "ffi_item",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value {
    pub kind: ValueKind,
    pub text: String,
    pub int_val: i64,
    pub children: Vec<Value>,
}

impl Default for Value { fn default() -> Self { Self { kind: Default::default(), text: Default::default(), int_val: Default::default(), children: Default::default(), } } }

impl Value {
    pub fn new() -> Self { Self::default() }

    pub fn value_by_kind(&self, val: ValueKind) -> Vec<&Value> {
        self.children.iter().filter(|r| r.kind == val).collect()
    }

    pub fn value_by_text(&self, val: &str) -> Vec<&Value> {
        self.children.iter().filter(|r| r.text == val).collect()
    }

    pub fn value_by_int_val(&self, val: i64) -> Vec<&Value> {
        self.children.iter().filter(|r| r.int_val == val).collect()
    }

    pub fn value_by_children(&self, val: Vec<Value>) -> Vec<&Value> {
        self.children.iter().filter(|r| r.children == val).collect()
    }

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: String,
    pub body_kind: BodyKind,
    pub body: Vec<Value>,
}

impl Default for MethodDef { fn default() -> Self { Self { name: Default::default(), params: Default::default(), return_type: Default::default(), body_kind: Default::default(), body: Default::default(), } } }

impl MethodDef {
    pub fn new() -> Self { Self::default() }

    pub fn param_by_kind(&self, val: ParamKind) -> Vec<&Param> {
        self.params.iter().filter(|r| r.kind == val).collect()
    }

    pub fn param_by_name(&self, val: &str) -> Vec<&Param> {
        self.params.iter().filter(|r| r.name == val).collect()
    }

    pub fn param_by_param_type(&self, val: &str) -> Vec<&Param> {
        self.params.iter().filter(|r| r.param_type == val).collect()
    }

    pub fn value_by_kind(&self, val: ValueKind) -> Vec<&Value> {
        self.body.iter().filter(|r| r.kind == val).collect()
    }

    pub fn value_by_text(&self, val: &str) -> Vec<&Value> {
        self.body.iter().filter(|r| r.text == val).collect()
    }

    pub fn value_by_int_val(&self, val: i64) -> Vec<&Value> {
        self.body.iter().filter(|r| r.int_val == val).collect()
    }

    pub fn value_by_children(&self, val: Vec<Value>) -> Vec<&Value> {
        self.body.iter().filter(|r| r.children == val).collect()
    }

}

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct TraitDecl {
    pub name: String,
    pub methods: Vec<MethodSig>,
}

impl Default for TraitDecl { fn default() -> Self { Self { name: Default::default(), methods: Default::default(), } } }

impl TraitDecl {
    pub fn new() -> Self { Self::default() }

    pub fn method_sig_by_name(&self, val: &str) -> Vec<&MethodSig> {
        self.methods.iter().filter(|r| r.name == val).collect()
    }

    pub fn method_sig_by_params(&self, val: Vec<Param>) -> Vec<&MethodSig> {
        self.methods.iter().filter(|r| r.params == val).collect()
    }

    pub fn method_sig_by_return_type(&self, val: &str) -> Vec<&MethodSig> {
        self.methods.iter().filter(|r| r.return_type == val).collect()
    }

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraitImpl {
    pub trait_name: String,
    pub target_type: String,
    pub methods: Vec<MethodDef>,
}

impl Default for TraitImpl { fn default() -> Self { Self { trait_name: Default::default(), target_type: Default::default(), methods: Default::default(), } } }

impl TraitImpl {
    pub fn new() -> Self { Self::default() }

    pub fn method_def_by_name(&self, val: &str) -> Vec<&MethodDef> {
        self.methods.iter().filter(|r| r.name == val).collect()
    }

    pub fn method_def_by_params(&self, val: Vec<Param>) -> Vec<&MethodDef> {
        self.methods.iter().filter(|r| r.params == val).collect()
    }

    pub fn method_def_by_return_type(&self, val: &str) -> Vec<&MethodDef> {
        self.methods.iter().filter(|r| r.return_type == val).collect()
    }

    pub fn method_def_by_body_kind(&self, val: BodyKind) -> Vec<&MethodDef> {
        self.methods.iter().filter(|r| r.body_kind == val).collect()
    }

    pub fn method_def_by_body(&self, val: Vec<Value>) -> Vec<&MethodDef> {
        self.methods.iter().filter(|r| r.body == val).collect()
    }

}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeWorld {
    pub types: Vec<TypeEntry>,
    pub variants: Vec<VariantDef>,
    pub fields: Vec<FieldDef>,
    pub ffi_entries: Vec<FfiEntry>,
    pub trait_decls: Vec<TraitDecl>,
    pub trait_impls: Vec<TraitImpl>,
}

impl Default for CodeWorld { fn default() -> Self { Self { types: Default::default(), variants: Default::default(), fields: Default::default(), ffi_entries: Default::default(), trait_decls: Default::default(), trait_impls: Default::default(), } } }

impl CodeWorld {
    pub fn new() -> Self { Self::default() }

    pub fn type_entry_by_id(&self, val: i64) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.id == val).collect()
    }

    pub fn type_entry_by_name(&self, val: &str) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.name == val).collect()
    }

    pub fn type_entry_by_form(&self, val: TypeForm) -> Vec<&TypeEntry> {
        self.types.iter().filter(|r| r.form == val).collect()
    }

    pub fn variant_def_by_type_id(&self, val: i64) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn variant_def_by_ordinal(&self, val: i64) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn variant_def_by_name(&self, val: &str) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.name == val).collect()
    }

    pub fn variant_def_by_contains_type(&self, val: &str) -> Vec<&VariantDef> {
        self.variants.iter().filter(|r| r.contains_type == val).collect()
    }

    pub fn field_def_by_type_id(&self, val: i64) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.type_id == val).collect()
    }

    pub fn field_def_by_ordinal(&self, val: i64) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.ordinal == val).collect()
    }

    pub fn field_def_by_name(&self, val: &str) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.name == val).collect()
    }

    pub fn field_def_by_field_type(&self, val: &str) -> Vec<&FieldDef> {
        self.fields.iter().filter(|r| r.field_type == val).collect()
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

    pub fn trait_decl_by_name(&self, val: &str) -> Vec<&TraitDecl> {
        self.trait_decls.iter().filter(|r| r.name == val).collect()
    }

    pub fn trait_decl_by_methods(&self, val: Vec<MethodSig>) -> Vec<&TraitDecl> {
        self.trait_decls.iter().filter(|r| r.methods == val).collect()
    }

    pub fn trait_impl_by_trait_name(&self, val: &str) -> Vec<&TraitImpl> {
        self.trait_impls.iter().filter(|r| r.trait_name == val).collect()
    }

    pub fn trait_impl_by_target_type(&self, val: &str) -> Vec<&TraitImpl> {
        self.trait_impls.iter().filter(|r| r.target_type == val).collect()
    }

    pub fn trait_impl_by_methods(&self, val: Vec<MethodDef>) -> Vec<&TraitImpl> {
        self.trait_impls.iter().filter(|r| r.methods == val).collect()
    }

}

pub trait Generate {
    fn generate(&self) -> String;
    fn emit_domains(&self) -> String;
    fn emit_structs(&self) -> String;
    fn emit_world_struct(&self) -> String;
    fn emit_trait_decls(&self) -> String;
    fn emit_trait_impls(&self) -> String;
    fn emit_derive(&self) -> String;
    fn emit_params(&self, params: Vec<Param>) -> String;
    fn emit_param(&self, p: Param, first: i64) -> String;
    fn emit_method_body(&self, body: Vec<Value>, kind: BodyKind, indent: String) -> String;
    fn emit_stmts(&self, stmts: Vec<Value>, indent: String, idx: i64) -> String;
    fn emit_expr(&self, e: Value) -> String;
    fn emit_expr_lit(&self, e: Value) -> String;
    fn emit_expr_ref(&self, e: Value) -> String;
    fn emit_expr_more(&self, e: Value) -> String;
    fn emit_expr_rest(&self, e: Value) -> String;
    fn emit_mutable_new(&self, e: Value) -> String;
    fn emit_mutable_set(&self, e: Value) -> String;
    fn emit_set_chain(&self, target: String, chain: Value) -> String;
    fn emit_set_val(&self, target: String, chain: Value) -> String;
    fn emit_access(&self, e: Value) -> String;
    fn is_builtin_method(&self, name: String) -> i64;
    fn emit_method_call(&self, e: Value) -> String;
    fn emit_bin_op(&self, e: Value) -> String;
    fn emit_inline_eval(&self, e: Value) -> String;
    fn emit_match(&self, e: Value) -> String;
    fn emit_match_arms(&self, arms: Vec<Value>, idx: i64) -> String;
    fn emit_yield(&self, e: Value) -> String;
    fn emit_std_out(&self, e: Value) -> String;
    fn emit_children(&self, children: Vec<Value>, sep: String, idx: i64) -> String;
}

pub trait EmitCodeExt {
    fn emit_expr_op(&self, e: Value) -> String;
    fn emit_expr_stmt(&self, e: Value) -> String;
    fn emit_expr_block(&self, e: Value) -> String;
    fn emit_expr_construct(&self, e: Value) -> String;
}

impl Generate for CodeWorld {
    fn generate(&self) -> String {
        let mut out: String = String::new();
        out = (out + &self.emit_domains());
        out = (out + &self.emit_structs());
        out = (out + &self.emit_world_struct());
        out = (out + &self.emit_trait_decls());
        out = (out + &self.emit_trait_impls());
        out = (out + &self.emit_derive());
        out
    }
    fn emit_domains(&self) -> String {
        let mut out: String = String::new();
        for type_entry in self.type_entry_by_form(TypeForm::Domain).iter() {
            out = (((out + "#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum ") + &type_entry.name) + " {
");
            for variant_def in self.variant_def_by_type_id(type_entry.id).iter() {
                out = (((out + "    ") + &variant_def.name) + ",
");
            }
            out = (out + "}

");
            out = (((out + "impl std::fmt::Display for ") + &type_entry.name) + " {
");
            out = (out + "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
");
            out = (out + "        write!(f, \"{:?}\", self)
    }
}

");
            out = (((out + "impl ") + &type_entry.name) + " {
");
            out = (out + "    pub fn from_str(s: &str) -> Option<Self> {
");
            out = (out + "        match s {
");
            for variant_def in self.variant_def_by_type_id(type_entry.id).iter() {
                out = (((((out + "            \"") + &variant_def.name.to_snake()) + "\" => Some(Self::") + &variant_def.name) + "),
");
                out = (out + &variant_def.name.needs_pascal_alias());
            }
            out = (out + "            _ => None,
        }
    }

");
            out = (out + "    pub fn to_str(&self) -> &'static str {
        match self {
");
            for variant_def in self.variant_def_by_type_id(type_entry.id).iter() {
                out = (((((out + "            Self::") + &variant_def.name) + " => \"") + &variant_def.name.to_snake()) + "\",
");
            }
            out = (out + "        }
    }
}

");
        }
        out
    }
    fn emit_structs(&self) -> String {
        let mut out: String = String::new();
        for type_entry in self.type_entry_by_form(TypeForm::Struct).iter() {
            let mut field_types: String = String::new();
            for field_def in self.field_def_by_type_id(type_entry.id).iter() {
                field_types = ((field_types + &field_def.field_type) + ",");
            }
            out = (((((out + "#[derive(Debug, ") + &field_types.all_fields_copy()) + "Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ") + &type_entry.name) + " {
");
            for field_def in self.field_def_by_type_id(type_entry.id).iter() {
                out = (((((out + "    pub ") + &field_def.name.to_snake()) + ": ") + &field_def.field_type.to_rust_type()) + ",
");
            }
            out = (out + "}

");
        }
        out
    }
    fn emit_world_struct(&self) -> String {
        let mut out: String = String::new();
        for type_entry in self.type_entry_by_name("World").iter() {
            out = (out + "impl Default for World { fn default() -> Self { Self {");
            for field_def in self.field_def_by_type_id(type_entry.id).iter() {
                out = (((out + " ") + &field_def.name.to_snake()) + ": Default::default(),");
            }
            out = (out + " } } }

");
            out = (out + "impl World {
");
            out = (out + "    pub fn new() -> Self { Self::default() }

");
            for field_def in self.field_def_by_type_id(type_entry.id).iter() {
                let mut elem_type: String = field_def.field_type.strip_vec();
                for elem_type_entry in self.type_entry_by_name(&elem_type).iter() {
                    for elem_field_def in self.field_def_by_type_id(elem_type_entry.id).iter() {
                        out = ((((out + "    pub fn ") + &elem_type.to_snake()) + "_by_") + &elem_field_def.name.to_snake());
                        out = (((((out + "(&self, val: ") + &elem_field_def.field_type.to_param_type()) + ") -> Vec<&") + &elem_type) + "> {
");
                        out = (((((out + "        self.") + &field_def.name.to_snake()) + ".iter().filter(|r| r.") + &elem_field_def.name.to_snake()) + " == val).collect()
");
                        out = (out + "    }

");
                    }
                }
            }
            out = (out + "}

");
        }
        out
    }
    fn emit_trait_decls(&self) -> String {
        let mut out: String = String::new();
        for trait_decl in self.trait_decls.iter() {
            out = (((out + "pub trait ") + &trait_decl.name.to_snake()) + " {
");
            for method_sig in trait_decl.methods.iter() {
                out = (((((out + "    fn ") + &method_sig.name.to_snake()) + "(&self) -> ") + &method_sig.return_type.to_rust_type()) + ";
");
            }
            out = (out + "}

");
        }
        out
    }
    fn emit_trait_impls(&self) -> String {
        let mut out: String = String::new();
        for trait_impl in self.trait_impls.iter() {
            out = (((((out + "impl ") + &trait_impl.trait_name.to_snake()) + " for ") + &trait_impl.target_type) + " {
");
            for method_def in trait_impl.methods.iter() {
                out = ((out + "    fn ") + &method_def.name.to_snake());
                out = (((out + "(") + &self.emit_params(method_def.params.clone())) + ")");
                out = (((out + " -> ") + &method_def.return_type.to_rust_type()) + " {
");
                out = (out + &self.emit_method_body(method_def.body.clone(), method_def.body_kind.clone(), "        ".to_string()));
                out = (out + "    }
");
            }
            out = (out + "}

");
        }
        out
    }
    fn emit_params(&self, params: Vec<Param>) -> String {
        let mut out: String = String::new();
        let mut first: i64 = 1;
        for param in params.iter() {
            out = (out + &self.emit_param(param.clone(), first));
            first = 0;
        }
        out
    }
    fn emit_param(&self, p: Param, first: i64) -> String {
        match p.kind { ParamKind::BorrowSelf => "&self".to_string(), ParamKind::MutBorrowSelf => "&mut self".to_string(), ParamKind::OwnedSelf => "self".to_string(), ParamKind::Named => (((match ((first == 1)) { true => String::new(), false => ", ".to_string() } + &p.name.to_snake()) + ": ") + &p.param_type.to_rust_type()) }
    }
    fn emit_method_body(&self, body: Vec<Value>, kind: BodyKind, indent: String) -> String {
        let mut last_idx: i64 = ((body.len() as u32).to_i64() - 1);
        self.emit_stmts(body, indent, last_idx)
    }
    fn emit_stmts(&self, stmts: Vec<Value>, indent: String, idx: i64) -> String {
        match ((idx < 0)) { true => String::new(), false => (((indent.clone() + &self.emit_expr(stmts.from_ordinal(idx).clone())) + ";
") + &self.emit_stmts(stmts, indent, (idx - 1))) }
    }
    fn emit_expr(&self, e: Value) -> String {
        match ((e.int_val == 100)) { true => self.emit_expr(e.children.from_ordinal(0).clone()), false => match ((e.int_val == 102)) { true => self.emit_expr_ref(e), false => match ((e.int_val == 103)) { true => e.text, false => match ((e.int_val == 107)) { true => self.emit_expr_lit(e), false => match ((e.int_val == 108)) { true => e.text, false => self.emit_expr_more(e) } } } } }
    }
    fn emit_expr_more(&self, e: Value) -> String {
        match ((e.int_val == 110)) { true => self.emit_mutable_new(e), false => match ((e.int_val == 111)) { true => self.emit_mutable_set(e), false => match ((e.int_val == 112)) { true => self.emit_access(e), false => match ((e.int_val == 113)) { true => self.emit_method_call(e), false => self.emit_expr_rest(e) } } } }
    }
    fn emit_expr_rest(&self, e: Value) -> String {
        match ((e.int_val == 114)) { true => self.emit_bin_op(e), false => match ((e.int_val == 104)) { true => self.emit_inline_eval(e), false => match ((e.int_val == 101)) { true => self.emit_yield(e), false => match ((e.int_val == 106)) { true => self.emit_std_out(e), false => match ((e.int_val == 117)) { true => self.emit_match(e), false => match ((e.int_val == 105)) { true => (("(".to_string() + &self.emit_expr(e.children.from_ordinal(0).clone())) + ")"), false => "todo!()".to_string() } } } } } }
    }
    fn emit_expr_lit(&self, e: Value) -> String {
        match (e.text.is_empty()) { true => "String::new()".to_string(), false => (("\"".to_string() + &e.text) + "\"") }
    }
    fn emit_expr_ref(&self, e: Value) -> String {
        match ((e.text == "Self")) { true => "self".to_string(), false => e.text.to_snake() }
    }
    fn emit_mutable_new(&self, e: Value) -> String {
        ((("let mut ".to_string() + &e.text.to_snake()) + " = ") + &self.emit_expr(e.children.from_ordinal(0).clone()))
    }
    fn emit_mutable_set(&self, e: Value) -> String {
        let mut chain: Value = e.children.from_ordinal(0).clone();
        self.emit_set_chain(e.text.to_snake(), chain)
    }
    fn emit_set_chain(&self, target: String, chain: Value) -> String {
        match ((chain.int_val == 113)) { true => (((((target + ".") + &chain.text.to_snake()) + "(") + &self.emit_children(chain.children, ", ".to_string(), 0)) + ")"), false => match ((chain.int_val == 112)) { true => self.emit_set_chain(((target + ".") + &chain.text.to_snake()), chain.children.from_ordinal(0).clone()), false => self.emit_set_val(target, chain) } }
    }
    fn emit_set_val(&self, target: String, chain: Value) -> String {
        let mut inner: Value = chain.children.from_ordinal(0).clone();
        match ((chain.text == "extend")) { true => (((target + ".extend(") + &self.emit_expr(inner)) + ")"), false => match ((inner.int_val == 104)) { true => match (((inner.children.len() as u32).to_i64() == 1)) { true => ((target + " = ") + &self.emit_expr(inner.children.from_ordinal(0).clone())), false => ((target + " = ") + &self.emit_expr(inner)) }, false => ((target + " = ") + &self.emit_expr(inner)) } }
    }
    fn emit_access(&self, e: Value) -> String {
        let mut is_method: i64 = self.is_builtin_method(e.text.clone());
        match ((is_method == 1)) { true => (((self.emit_expr(e.children.from_ordinal(0).clone()) + ".") + &e.text.to_snake()) + "()"), false => ((self.emit_expr(e.children.from_ordinal(0).clone()) + ".") + &e.text.to_snake()) }
    }
    fn is_builtin_method(&self, name: String) -> i64 {
        match name.as_str() { "clone" => 1, "toString" => 1, "toSnake" => 1, "toRustType" => 1, "toParamType" => 1, "stripVec" => 1, "allFieldsCopy" => 1, "needsPascalAlias" => 1, "len" => 1, "isEmpty" => 1, "toI64" => 1, "toF32" => 1, "toU32" => 1, "unwrap" => 1, "toHex" => 1, "toBytes" => 1, "asSlice" => 1, "toU8" => 1, _ => 0 }
    }
    fn emit_method_call(&self, e: Value) -> String {
        (((((self.emit_expr(e.children.from_ordinal(0).clone()) + ".") + &e.text.to_snake()) + "(") + &self.emit_children(e.children, ", ".to_string(), 1)) + ")")
    }
    fn emit_bin_op(&self, e: Value) -> String {
        (((((("(".to_string() + &self.emit_expr(e.children.from_ordinal(0).clone())) + " ") + &e.text) + " &") + &self.emit_expr(e.children.from_ordinal(1).clone())) + ")")
    }
    fn emit_inline_eval(&self, e: Value) -> String {
        let mut len: i64 = (e.children.len() as u32).to_i64();
        match ((len == 1)) { true => self.emit_expr(e.children.from_ordinal(0).clone()), false => (("{ ".to_string() + &self.emit_stmts(e.children, "    ".to_string(), (len - 1))) + " }") }
    }
    fn emit_match(&self, e: Value) -> String {
        let mut cond: String = self.emit_expr(e.children.from_ordinal(0).clone());
        let mut arms_val: Value = e.children.from_ordinal(1).clone();
        let mut last_arm: i64 = ((arms_val.children.len() as u32).to_i64() - 1);
        (((("match (".to_string() + &cond) + ") { ") + &self.emit_match_arms(arms_val.children, last_arm)) + " }")
    }
    fn emit_match_arms(&self, arms: Vec<Value>, idx: i64) -> String {
        match ((idx < 0)) { true => String::new(), false => { let mut arm: Value = arms.from_ordinal(idx).clone(); let mut pat: String = arm.text.clone(); let mut body: String = self.emit_expr(arm.children.from_ordinal(0).clone()); let mut rest: String = self.emit_match_arms(arms, (idx - 1)); match (rest.is_empty()) { true => ((pat + " => ") + &body), false => ((((rest + ", ") + &pat) + " => ") + &body) } } }
    }
    fn emit_yield(&self, e: Value) -> String {
        let mut inner: Value = e.children.from_ordinal(0).clone();
        let mut var_name: String = inner.text.to_snake();
        let mut collection: String = self.emit_expr(inner.children.from_ordinal(0).clone());
        let mut has_body: i64 = match (((inner.children.len() as u32).to_i64() > 1)) { true => 1, false => 0 };
        match ((has_body == 1)) { true => { let mut body_val: Value = inner.children.from_ordinal(1).clone(); let mut body_len: i64 = (body_val.children.len() as u32).to_i64(); (((((("for ".to_string() + &var_name) + " in ") + &collection) + ".iter() {
") + &self.emit_stmts(body_val.children, "        ".to_string(), (body_len - 1))) + "        }
") }, false => (((collection + ".") + &var_name) + "()") }
    }
    fn emit_std_out(&self, e: Value) -> String {
        (("println!(\"{}\", ".to_string() + &self.emit_expr(e.children.from_ordinal(0).clone())) + ")")
    }
    fn emit_derive(&self) -> String {
        let mut out: String = String::new();
        out = (out + "pub trait Derive {
");
        out = (out + "    fn derive(&mut self);
");
        out = (out + "    fn derive_variant_of(&mut self);
");
        out = (out + "    fn derive_type_kind(&mut self);
");
        out = (out + "    fn derive_contained_type(&mut self);
");
        out = (out + "    fn derive_recursive_type(&mut self);
");
        out = (out + "}

");
        out = (out + "impl World {
");
        out = (out + "    pub fn derive(&mut self) {
");
        out = (out + "        self.derive_variant_of();
");
        out = (out + "        self.derive_type_kind();
");
        out = (out + "        self.derive_contained_type();
");
        out = (out + "        self.derive_recursive_type_fixpoint();
");
        out = (out + "    }

");
        out = (out + "    fn derive_variant_of(&mut self) {
");
        out = (out + "        let mut results = Vec::new();
");
        out = (out + "        for type_entry in &self.types {
");
        out = (out + "            if type_entry.form == TypeForm::Domain {
");
        out = (out + "                for variant in &self.variants {
");
        out = (out + "                    if variant.type_id == type_entry.id {
");
        out = (out + "                        results.push(VariantOf { variant_name: variant.name.clone(), type_name: type_entry.name.clone(), type_id: type_entry.id });
");
        out = (out + "                    }
");
        out = (out + "                }
");
        out = (out + "            }
");
        out = (out + "        }
");
        out = (out + "        self.variant_ofs = results;
");
        out = (out + "    }

");
        out = (out + "    fn derive_type_kind(&mut self) {
");
        out = (out + "        let mut results = Vec::new();
");
        out = (out + "        for type_entry in &self.types {
");
        out = (out + "            results.push(TypeKind { type_name: type_entry.name.clone(), category: type_entry.form });
");
        out = (out + "        }
");
        out = (out + "        self.type_kinds = results;
");
        out = (out + "    }

");
        out = (out + "    fn derive_contained_type(&mut self) {
");
        out = (out + "        let mut results = Vec::new();
");
        out = (out + "        for type_entry in &self.types {
");
        out = (out + "            if type_entry.form == TypeForm::Struct {
");
        out = (out + "                for field in &self.fields {
");
        out = (out + "                    if field.type_id == type_entry.id {
");
        out = (out + "                        results.push(ContainedType { parent_type: type_entry.name.clone(), child_type: field.field_type.clone() });
");
        out = (out + "                    }
");
        out = (out + "                }
");
        out = (out + "            }
");
        out = (out + "        }
");
        out = (out + "        for type_entry in &self.types {
");
        out = (out + "            if type_entry.form == TypeForm::Domain {
");
        out = (out + "                for variant in &self.variants {
");
        out = (out + "                    if variant.type_id == type_entry.id {
");
        out = (out + "                        if !variant.contains_type.is_empty() {
");
        out = (out + "                            results.push(ContainedType { parent_type: type_entry.name.clone(), child_type: variant.contains_type.clone() });
");
        out = (out + "                        }
");
        out = (out + "                    }
");
        out = (out + "                }
");
        out = (out + "            }
");
        out = (out + "        }
");
        out = (out + "        self.contained_types = results;
");
        out = (out + "    }

");
        out = (out + "    fn derive_recursive_type_fixpoint(&mut self) {
");
        out = (out + "        {
");
        out = (out + "            let mut results = Vec::new();
");
        out = (out + "            for contained_type in &self.contained_types {
");
        out = (out + "                results.push(RecursiveType { parent_type: contained_type.parent_type.clone(), child_type: contained_type.child_type.clone() });
");
        out = (out + "            }
");
        out = (out + "            self.recursive_types = results;
");
        out = (out + "        }
");
        out = (out + "        loop {
");
        out = (out + "            let mut new_items = Vec::new();
");
        out = (out + "            for contained_type in &self.contained_types {
");
        out = (out + "                for reach in &self.recursive_types {
");
        out = (out + "                    if reach.parent_type == contained_type.child_type {
");
        out = (out + "                        new_items.push(RecursiveType { parent_type: contained_type.parent_type.clone(), child_type: reach.child_type.clone() });
");
        out = (out + "                    }
");
        out = (out + "                }
");
        out = (out + "            }
");
        out = (out + "            new_items.retain(|item| !self.recursive_types.contains(item));
");
        out = (out + "            if new_items.is_empty() { break; }
");
        out = (out + "            self.recursive_types.extend(new_items);
");
        out = (out + "        }
");
        out = (out + "    }

");
        out = (out + "}
");
        out
    }
    fn emit_children(&self, children: Vec<Value>, sep: String, idx: i64) -> String {
        match ((idx >= (children.len() as u32).to_i64())) { true => String::new(), false => { let mut child: String = self.emit_expr(children.from_ordinal(idx).clone()); let mut rest: String = self.emit_children(children, sep.clone(), (idx + 1)); match (rest.is_empty()) { true => child, false => ((child + &sep) + &rest) } } }
    }
}

