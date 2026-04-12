//! Register trait — name registration during parsing.
//! AskiWorld learns declared names so later code can query them.

use crate::synth::types::{Item, Delimiter};
use super::aski_world::{AskiWorld, TypeForm};

pub trait Register {
    fn register_from_key(&mut self, key_item: &Item, key_text: &str, delim: Delimiter);
}

impl Register for AskiWorld {
    fn register_from_key(&mut self, key_item: &Item, key_text: &str, delim: Delimiter) {
        match (key_item, delim) {
            (Item::Declare { kind, .. }, Delimiter::Paren) if kind == "Domain" => {
                self.register_domain(key_text);
            }
            (Item::Declare { kind, .. }, Delimiter::Brace) if kind == "Struct" => {
                self.register_struct(key_text);
            }
            (Item::Declare { kind, .. }, Delimiter::Paren) if kind == "trait" => {
                self.register_trait(key_text);
            }
            (Item::Declare { kind, .. }, _) if kind == "Variant" => {
                if let Some(parent) = self.known_types.iter().rev()
                    .find(|t| t.form == TypeForm::Domain)
                {
                    let parent_name = parent.name.clone();
                    self.register_variant(key_text, &parent_name);
                }
            }
            (Item::Declare { kind, .. }, _) if kind == "method" || kind == "foreignFunction" => {
                self.register_method(key_text);
            }
            _ => {}
        }
    }
}
