//! Raise trait — SemaWorld → AskiWorld.
//! Builds parse nodes from typed relations, reversing the lower step.

use std::collections::HashMap;
use crate::synth::types::Dialect;
use super::aski_world::AskiWorld;
use super::sema_world::*;

pub trait Raise {
    fn raise(sema: &SemaWorld, dialects: HashMap<String, Dialect>) -> AskiWorld;
}

impl Raise for AskiWorld {
    fn raise(sema: &SemaWorld, dialects: HashMap<String, Dialect>) -> AskiWorld {
        let mut world = AskiWorld::new(dialects);
        let root = world.root_id();

        // Rebuild domains
        for sema_type in &sema.types {
            let name = &sema.type_names[sema_type.name as usize];
            match sema_type.form {
                SemaTypeForm::Domain => {
                    let node_id = world.make_node("(", name, 0, 0);
                    world.add_child(root, node_id);
                    world.register_domain(name);

                    // Add variants
                    let variants: Vec<_> = sema.variants.iter()
                        .filter(|v| v.type_id == sema_type.name)
                        .collect();
                    for var in variants {
                        let var_name = &sema.variant_names[var.name as usize];
                        let var_id = world.make_node("Variant", var_name, 0, 0);
                        world.add_child(node_id, var_id);
                        world.register_variant(var_name, name);
                    }
                }
                SemaTypeForm::Struct => {
                    let node_id = world.make_node("{", name, 0, 0);
                    world.add_child(root, node_id);
                    world.register_struct(name);

                    // Add fields
                    let fields: Vec<_> = sema.fields.iter()
                        .filter(|f| f.type_id == sema_type.name)
                        .collect();
                    for field in fields {
                        let field_name = &sema.field_names[field.name as usize];
                        let field_id = world.make_node("Field", field_name, 0, 0);
                        world.add_child(node_id, field_id);
                        // TODO: add type ref as child
                    }
                }
                SemaTypeForm::Alias => {}
            }
        }

        world
    }
}
