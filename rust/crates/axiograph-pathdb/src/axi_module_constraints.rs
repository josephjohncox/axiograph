//! AST-level checking for a conservative subset of canonical `.axi` theory constraints.
//!
//! This is the Rust-side implementation for the `axi_constraints_ok_v1` certificate kind.
//!
//! Scope (initial release)
//! -----------------------
//! We intentionally start with constraints that are:
//! - easy to explain,
//! - common in schema-directed optimization (keys/functionals),
//! - and low ambiguity across dialects.
//!
//! Supported constraint kinds:
//! - `constraint key Rel(field, ...)`
//! - `constraint functional Rel.field -> Rel.field`
//!
//! Not yet certified:
//! - conditional constraints (`... where ...`),
//! - global entailment / inference,
//! - relational algebra beyond simple uniqueness checks.

use std::collections::HashMap;

use anyhow::{anyhow, Result};

use axiograph_dsl::schema_v1::{ConstraintV1, SchemaV1Instance, SchemaV1Module, SetItemV1};

use crate::certificate::AxiConstraintsOkProofV1;

#[derive(Debug, Clone)]
enum CoreConstraint<'a> {
    Key {
        schema: &'a str,
        relation: &'a str,
        fields: &'a [String],
    },
    Functional {
        schema: &'a str,
        relation: &'a str,
        src_field: &'a str,
        dst_field: &'a str,
    },
}

fn gather_core_constraints(module: &SchemaV1Module) -> Vec<CoreConstraint<'_>> {
    let mut out: Vec<CoreConstraint<'_>> = Vec::new();
    for th in &module.theories {
        for c in &th.constraints {
            match c {
                ConstraintV1::Key { relation, fields } => out.push(CoreConstraint::Key {
                    schema: &th.schema,
                    relation,
                    fields,
                }),
                ConstraintV1::Functional {
                    relation,
                    src_field,
                    dst_field,
                } => out.push(CoreConstraint::Functional {
                    schema: &th.schema,
                    relation,
                    src_field,
                    dst_field,
                }),
                _ => {}
            }
        }
    }
    out
}

fn relation_tuples<'a>(
    inst: &'a SchemaV1Instance,
    relation_name: &'a str,
) -> impl Iterator<Item = &'a Vec<(String, String)>> + 'a {
    inst.assignments
        .iter()
        .filter(move |a| a.name == relation_name)
        .flat_map(|a| a.value.items.iter())
        .filter_map(|it| match it {
            SetItemV1::Tuple { fields } => Some(fields),
            _ => None,
        })
}

/// Check that a canonical `.axi` module satisfies its core constraints.
///
/// Returns an `AxiConstraintsOkProofV1` summary suitable for certificate emission.
pub fn check_axi_constraints_ok_v1(module: &SchemaV1Module) -> Result<AxiConstraintsOkProofV1> {
    // Ensure schemas referenced by theories exist.
    let mut schema_names: HashMap<&str, ()> = HashMap::new();
    for s in &module.schemas {
        schema_names.insert(s.name.as_str(), ());
    }
    for th in &module.theories {
        if !schema_names.contains_key(th.schema.as_str()) {
            return Err(anyhow!(
                "theory `{}` references unknown schema `{}`",
                th.name,
                th.schema
            ));
        }
    }

    let constraints = gather_core_constraints(module);
    let mut check_count: u32 = 0;

    for inst in &module.instances {
        // Apply constraints only for the instance's schema.
        for c in constraints.iter().filter(|c| match c {
            CoreConstraint::Key { schema, .. } => *schema == inst.schema,
            CoreConstraint::Functional { schema, .. } => *schema == inst.schema,
        }) {
            check_count += 1;
            match c {
                CoreConstraint::Key {
                    relation, fields, ..
                } => {
                    let mut seen: HashMap<Vec<&str>, usize> = HashMap::new();
                    for (i, tuple) in relation_tuples(inst, relation).enumerate() {
                        let mut map: HashMap<&str, &str> = HashMap::new();
                        for (k, v) in tuple {
                            map.insert(k.as_str(), v.as_str());
                        }
                        let mut key: Vec<&str> = Vec::new();
                        for f in *fields {
                            let Some(v) = map.get(f.as_str()) else {
                                return Err(anyhow!(
                                    "instance `{}` relation `{}`: key field `{}` missing from tuple",
                                    inst.name,
                                    relation,
                                    f
                                ));
                            };
                            key.push(*v);
                        }
                        if let Some(prev) = seen.insert(key.clone(), i) {
                            return Err(anyhow!(
                                "key violation in instance `{}` on `{}`({}): duplicate key at tuples {prev} and {i}",
                                inst.name,
                                relation,
                                fields.join(", ")
                            ));
                        }
                    }
                }
                CoreConstraint::Functional {
                    relation,
                    src_field,
                    dst_field,
                    ..
                } => {
                    let mut map: HashMap<&str, &str> = HashMap::new();
                    for (i, tuple) in relation_tuples(inst, relation).enumerate() {
                        let mut tuple_map: HashMap<&str, &str> = HashMap::new();
                        for (k, v) in tuple {
                            tuple_map.insert(k.as_str(), v.as_str());
                        }
                        let Some(src) = tuple_map.get(src_field) else {
                            return Err(anyhow!(
                                "instance `{}` relation `{}`: functional src field `{}` missing from tuple",
                                inst.name,
                                relation,
                                src_field
                            ));
                        };
                        let Some(dst) = tuple_map.get(dst_field) else {
                            return Err(anyhow!(
                                "instance `{}` relation `{}`: functional dst field `{}` missing from tuple",
                                inst.name,
                                relation,
                                dst_field
                            ));
                        };
                        if let Some(prev) = map.get(src) {
                            if prev != dst {
                                return Err(anyhow!(
                                    "functional violation in instance `{}` on `{}`.{} -> {}.{}: src `{}` maps to both `{}` and `{}` (tuple {i})",
                                    inst.name,
                                    relation,
                                    src_field,
                                    relation,
                                    dst_field,
                                    src,
                                    prev,
                                    dst
                                ));
                            }
                        } else {
                            map.insert(src, dst);
                        }
                    }
                }
            }
        }
    }

    let constraint_count: u32 = constraints.len() as u32;
    let instance_count: u32 = module.instances.len() as u32;
    Ok(AxiConstraintsOkProofV1 {
        module_name: module.module_name.clone(),
        constraint_count,
        instance_count,
        check_count,
    })
}
