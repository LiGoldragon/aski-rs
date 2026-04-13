//! Deparse trait — AskiWorld → canonical aski text.
//! Walks parse nodes, reconstructs the aski source with sigils,
//! delimiters, body blocks, match arms, and expressions.

use super::aski_world::AskiWorld;

pub trait Deparse {
    fn deparse(&self) -> String;
}

impl Deparse for AskiWorld {
    fn deparse(&self) -> String {
        let mut out = String::new();
        let root_children = self.children_of(self.root_id());
        for (i, node) in root_children.iter().enumerate() {
            if i > 0 { out.push('\n'); }
            self.deparse_node(&mut out, node.id, 0);
        }
        out.push('\n');
        out
    }
}

trait DeparseNode {
    fn deparse_node(&self, out: &mut String, node_id: i64, indent: usize);
    fn deparse_children(&self, out: &mut String, node_id: i64, indent: usize);
}

impl DeparseNode for AskiWorld {
    fn deparse_node(&self, out: &mut String, node_id: i64, indent: usize) {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return,
        };
        let children = self.children_of(node_id);
        let constructor = node.constructor.clone();
        let key = node.key.clone();

        match constructor.as_str() {
            // ── Delimiter nodes ──────────────────────────
            "(" | "[" | "{" | "(|" | "[|" | "{|" => {
                let (open, close) = match constructor.as_str() {
                    "(" => ("(", ")"),
                    "[" => ("[", "]"),
                    "{" => ("{", "}"),
                    "(|" => ("(|", "|)"),
                    "[|" => ("[|", "|]"),
                    "{|" => ("{|", "|}"),
                    _ => ("(", ")"),
                };
                out.push_str(open);
                if !key.is_empty() {
                    out.push_str(&key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children(out, node_id, indent);
                out.push_str(close);
            }

            // ── Param sigils ─────────────────────────────
            "BorrowParam" => {
                out.push_str(":@");
                out.push_str(&key);
            }
            "MutBorrowParam" => {
                out.push_str("~@");
                out.push_str(&key);
            }
            "OwnedParam" => {
                out.push_str("@");
                out.push_str(&key);
            }
            "NamedParam" => {
                out.push_str("@");
                out.push_str(&key);
                for child in &children {
                    if child.constructor == "TypeRef" {
                        out.push(' ');
                        out.push_str(&child.key);
                    }
                }
            }

            // ── Return type ──────────────────────────────
            "ReturnType" => {
                out.push_str(&key);
            }

            // ── Body blocks ──────────────────────────────
            "Block" => {
                out.push('[');
                let pad = "    ".repeat(indent + 1);
                for child in &children {
                    out.push('\n');
                    out.push_str(&pad);
                    self.deparse_node(out, child.id, indent + 1);
                }
                out.push('\n');
                out.push_str(&"    ".repeat(indent));
                out.push(']');
            }
            "TailBlock" => {
                out.push_str("[|");
                let pad = "    ".repeat(indent + 1);
                for child in &children {
                    out.push('\n');
                    out.push_str(&pad);
                    self.deparse_node(out, child.id, indent + 1);
                }
                out.push('\n');
                out.push_str(&"    ".repeat(indent));
                out.push_str("|]");
            }
            "MatchBody" => {
                out.push_str("(|");
                let pad = "    ".repeat(indent + 1);
                for child in &children {
                    if child.constructor == "MatchTarget" {
                        let target_children = self.children_of(child.id);
                        for tc in &target_children {
                            out.push('\n');
                            out.push_str(&pad);
                            self.deparse_node(out, tc.id, indent + 1);
                        }
                        out.push('/');
                    } else {
                        out.push('\n');
                        out.push_str(&pad);
                        self.deparse_node(out, child.id, indent + 1);
                    }
                }
                out.push('\n');
                out.push_str(&"    ".repeat(indent));
                out.push_str("|)");
            }
            "CommitArm" => {
                // Pattern then result
                for child in &children {
                    if child.constructor == "Pattern" {
                        out.push('(');
                        let pat_children = self.children_of(child.id);
                        for (i, p) in pat_children.iter().enumerate() {
                            if i > 0 { out.push_str(" | "); }
                            out.push_str(&p.key);
                        }
                        out.push_str(") ");
                    } else {
                        self.deparse_node(out, child.id, indent);
                    }
                }
            }

            // ── Expression nodes ─────────────────────────
            "Return" => {
                out.push('^');
                for child in &children {
                    self.deparse_node(out, child.id, indent);
                }
            }
            "InstanceRef" => {
                out.push_str("@");
                out.push_str(&key);
            }
            "BinOp" => {
                if children.len() >= 2 {
                    self.deparse_node(out, children[0].id, indent);
                    out.push_str(&format!(" {} ", key));
                    self.deparse_node(out, children[1].id, indent);
                }
            }
            "FieldAccess" => {
                if !children.is_empty() {
                    self.deparse_node(out, children[0].id, indent);
                    out.push('.');
                    out.push_str(&key);
                }
            }
            "MethodCall" => {
                if !children.is_empty() {
                    self.deparse_node(out, children[0].id, indent);
                    out.push('.');
                    out.push_str(&key);
                    out.push('(');
                    for (i, arg) in children.iter().skip(1).enumerate() {
                        if i > 0 { out.push_str(" "); }
                        self.deparse_node(out, arg.id, indent);
                    }
                    out.push(')');
                }
            }
            "Group" => {
                out.push('(');
                for child in &children {
                    self.deparse_node(out, child.id, indent);
                }
                out.push(')');
            }
            "QualifiedVariant" => {
                out.push_str(&key);
            }
            "TypePath" => {
                out.push_str(&key);
            }
            "InlineEval" => {
                out.push('[');
                for child in &children {
                    self.deparse_node(out, child.id, indent);
                    out.push(' ');
                }
                out.push(']');
            }

            // ── Statement nodes ──────────────────────────
            "Alloc" => {
                out.push_str("@");
                out.push_str(&key);
                for child in &children {
                    if child.constructor == "TypeRef" {
                        out.push_str(" :");
                        out.push_str(&child.key);
                    } else {
                        out.push(' ');
                        self.deparse_node(out, child.id, indent);
                    }
                }
            }
            "MutAlloc" => {
                out.push_str("~@");
                out.push_str(&key);
                for child in &children {
                    if child.constructor == "TypeRef" {
                        out.push_str(" :");
                        out.push_str(&child.key);
                    } else {
                        out.push(' ');
                        self.deparse_node(out, child.id, indent);
                    }
                }
            }
            "MutCall" => {
                out.push_str("~@");
                out.push_str(&key);
                for child in &children {
                    if child.constructor == "MethodName" {
                        out.push('.');
                        out.push_str(&child.key);
                    } else {
                        out.push(' ');
                        self.deparse_node(out, child.id, indent);
                    }
                }
            }
            "Iteration" => {
                out.push('#');
                for child in &children {
                    self.deparse_node(out, child.id, indent);
                    out.push(' ');
                }
            }
            "ProcessBody" => {
                for child in &children {
                    self.deparse_node(out, child.id, indent);
                    out.push('\n');
                }
            }

            // ── Leaf nodes ───────────────────────────────
            "Variant" | "Field" | "Export" | "Type" | "TypeRef"
            | "IntLit" | "FloatLit" | "StringLit" | "BareName"
            | "MethodName" | "MatchTarget" => {
                out.push_str(&key);
            }

            // Unknown — emit key
            _ => {
                out.push_str(&key);
            }
        }
    }

    fn deparse_children(&self, out: &mut String, node_id: i64, indent: usize) {
        let children = self.children_of(node_id);
        for (i, child) in children.iter().enumerate() {
            if i > 0 { out.push(' '); }
            self.deparse_node(out, child.id, indent);
        }
    }
}
