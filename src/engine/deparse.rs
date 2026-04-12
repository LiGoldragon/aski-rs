//! Deparse trait — AskiWorld → canonical aski text.
//! Walks parse nodes, emits text using the delimiter/key structure.

use super::aski_world::{AskiWorld, ParseNode};

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

impl AskiWorld {
    fn deparse_node(&self, out: &mut String, node_id: i64, indent: usize) {
        let node = match self.find_node(node_id) {
            Some(n) => n,
            None => return,
        };

        let children = self.children_of(node_id);
        let constructor = &node.constructor;

        match constructor.as_str() {
            // Delimiter nodes — emit open, key/, children, close
            "(" => {
                out.push('(');
                if !node.key.is_empty() {
                    out.push_str(&node.key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children_inline(out, &children, indent);
                out.push(')');
            }
            "[" => {
                out.push('[');
                if !node.key.is_empty() {
                    out.push_str(&node.key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children_inline(out, &children, indent);
                out.push(']');
            }
            "{" => {
                out.push('{');
                if !node.key.is_empty() {
                    out.push_str(&node.key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children_inline(out, &children, indent);
                out.push('}');
            }
            "(|" => {
                out.push_str("(|");
                if !node.key.is_empty() {
                    out.push_str(&node.key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children_inline(out, &children, indent);
                out.push_str("|)");
            }
            "[|" => {
                out.push_str("[|");
                if !node.key.is_empty() {
                    out.push_str(&node.key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children_inline(out, &children, indent);
                out.push_str("|]");
            }
            "{|" => {
                out.push_str("{|");
                if !node.key.is_empty() {
                    out.push_str(&node.key);
                    out.push('/');
                    if !children.is_empty() { out.push(' '); }
                }
                self.deparse_children_inline(out, &children, indent);
                out.push_str("|}");
            }

            // Leaf nodes — just emit the key
            _ => {
                out.push_str(&node.key);
            }
        }
    }

    fn deparse_children_inline(&self, out: &mut String, children: &[&ParseNode], indent: usize) {
        for (i, child) in children.iter().enumerate() {
            if i > 0 { out.push(' '); }
            self.deparse_node(out, child.id, indent);
        }
    }
}
