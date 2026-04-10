use crate::helpers::{StringExt, VecExt, ToI64, WithPush};
#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum TypeForm {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub enum RustSpan {
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

#[derive(Debug, Clone, PartialEq, Eq, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct CodeWorld {
    pub types: Vec<TypeEntry>,
    pub variants: Vec<VariantDef>,
    pub fields: Vec<FieldDef>,
    pub ffi_entries: Vec<FfiEntry>,
}

impl Default for CodeWorld { fn default() -> Self { Self { types: Default::default(), variants: Default::default(), fields: Default::default(), ffi_entries: Default::default(), } } }

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

}

pub trait Generate {
    fn generate(&self) -> String;
    fn emit_domains(&self) -> String;
    fn emit_structs(&self) -> String;
    fn emit_world_struct(&self) -> String;
    fn emit_derive(&self) -> String;
}

impl Generate for CodeWorld {
    fn generate(&self) -> String {
        let mut out: String = String::new();
        out = (out + &self.emit_domains());
        out = (out + &self.emit_structs());
        out = (out + &self.emit_world_struct());
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
}

