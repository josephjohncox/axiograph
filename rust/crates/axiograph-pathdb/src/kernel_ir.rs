//! Minimal compiled schema IR for semantic endpoint selection.
//!
//! This is intentionally narrow: it centralizes relation-role semantics so
//! endpoint choice becomes a compiled schema fact instead of being repeated as
//! local heuristics across import/check paths.

use std::collections::{HashMap, HashSet};

use axiograph_dsl::schema_v1::SchemaV1Schema;

use crate::SchemaId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleKind {
    Data,
    Context,
    Temporal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CarrierSource {
    HomotopyConvention,
    EndpointConvention,
    DeclaredOrder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WitnessViewIr {
    None,
    Morphism { from_role: u16, to_role: u16 },
    Homotopy { lhs_role: u16, rhs_role: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleIr {
    pub name: String,
    pub target_type: String,
    pub order: u16,
    pub kind: RoleKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarrierSpecIr {
    pub source_role: u16,
    pub target_role: u16,
    pub fiber_roles: Vec<u16>,
    pub source: CarrierSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationSemanticsIr {
    pub name: String,
    pub tuple_type_name: String,
    pub roles: Vec<RoleIr>,
    pub carrier: Option<CarrierSpecIr>,
    pub witness_view: WitnessViewIr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSchemaIr {
    pub schema_id: SchemaId,
    pub relations: HashMap<String, RelationSemanticsIr>,
}

impl RelationSemanticsIr {
    pub fn carrier_roles(&self) -> Option<(&RoleIr, &RoleIr)> {
        let carrier = self.carrier.as_ref()?;
        let src = self.roles.get(carrier.source_role as usize)?;
        let dst = self.roles.get(carrier.target_role as usize)?;
        Some((src, dst))
    }

    pub fn carrier_field_names(&self) -> Option<(&str, &str)> {
        let (src, dst) = self.carrier_roles()?;
        Some((src.name.as_str(), dst.name.as_str()))
    }

    pub fn morphism_field_names(&self) -> Option<(&str, &str)> {
        match self.witness_view {
            WitnessViewIr::Morphism { from_role, to_role } => {
                let from = self.roles.get(from_role as usize)?;
                let to = self.roles.get(to_role as usize)?;
                Some((from.name.as_str(), to.name.as_str()))
            }
            _ => None,
        }
    }

    pub fn homotopy_field_names(&self) -> Option<(&str, &str)> {
        match self.witness_view {
            WitnessViewIr::Homotopy { lhs_role, rhs_role } => {
                let lhs = self.roles.get(lhs_role as usize)?;
                let rhs = self.roles.get(rhs_role as usize)?;
                Some((lhs.name.as_str(), rhs.name.as_str()))
            }
            _ => None,
        }
    }
}

impl CompiledSchemaIr {
    pub fn relation(&self, name: &str) -> Option<&RelationSemanticsIr> {
        self.relations.get(name)
    }
}

pub fn compile_schema_ir(schema: &SchemaV1Schema) -> CompiledSchemaIr {
    let object_types = collect_object_types(schema);
    let supertypes_of = compute_supertypes_closure(schema, &object_types);
    let relations = schema
        .relations
        .iter()
        .map(|rel| {
            let tuple_type_name = if object_types.contains(&rel.name) {
                format!("{}Fact", rel.name)
            } else {
                rel.name.clone()
            };
            let roles = rel
                .fields
                .iter()
                .enumerate()
                .map(|(idx, field)| RoleIr {
                    name: field.field.clone(),
                    target_type: field.ty.clone(),
                    order: idx as u16,
                    kind: classify_role(
                        field.field.as_str(),
                        field.ty.as_str(),
                        is_context_axis_type(&supertypes_of, field.ty.as_str()),
                        is_temporal_axis_type(&supertypes_of, field.ty.as_str()),
                    ),
                })
                .collect::<Vec<_>>();
            (
                rel.name.clone(),
                compile_relation_semantics(&rel.name, tuple_type_name, roles),
            )
        })
        .collect();

    CompiledSchemaIr {
        schema_id: SchemaId::new(schema.name.clone()),
        relations,
    }
}

pub fn compile_relation_semantics(
    relation_name: &str,
    tuple_type_name: String,
    roles: Vec<RoleIr>,
) -> RelationSemanticsIr {
    let carrier = derive_carrier_spec(relation_name, &roles);
    let witness_view = match carrier.as_ref().map(|c| c.source) {
        Some(CarrierSource::HomotopyConvention) => carrier
            .as_ref()
            .map(|c| WitnessViewIr::Homotopy {
                lhs_role: c.source_role,
                rhs_role: c.target_role,
            })
            .unwrap_or(WitnessViewIr::None),
        Some(CarrierSource::EndpointConvention | CarrierSource::DeclaredOrder) => carrier
            .as_ref()
            .map(|c| WitnessViewIr::Morphism {
                from_role: c.source_role,
                to_role: c.target_role,
            })
            .unwrap_or(WitnessViewIr::None),
        None => WitnessViewIr::None,
    };

    RelationSemanticsIr {
        name: relation_name.to_string(),
        tuple_type_name,
        roles,
        carrier,
        witness_view,
    }
}

const HOMOTOPY_ROLE_PAIRS: &[(&str, &str)] = &[
    ("lhs", "rhs"),
    ("route1", "route2"),
    ("path1", "path2"),
    ("rel1", "rel2"),
    ("i1", "i2"),
    ("s1", "s2"),
    ("left", "right"),
];

const ENDPOINT_ROLE_PAIRS: &[(&str, &str)] =
    &[("from", "to"), ("source", "target"), ("src", "dst")];

pub fn classify_role(
    name: &str,
    target_type: &str,
    is_context_type: bool,
    is_temporal_type: bool,
) -> RoleKind {
    match name {
        "ctx" => RoleKind::Context,
        "time" => RoleKind::Temporal,
        _ if is_context_type || target_type == "Context" => RoleKind::Context,
        _ if is_temporal_type || target_type == "Time" => RoleKind::Temporal,
        _ => RoleKind::Data,
    }
}

fn derive_carrier_spec(relation_name: &str, roles: &[RoleIr]) -> Option<CarrierSpecIr> {
    let homotopy_pair = named_pair_index(roles, HOMOTOPY_ROLE_PAIRS);
    let endpoint_pair = named_pair_index(roles, ENDPOINT_ROLE_PAIRS);

    if let Some((source_role, target_role)) = homotopy_pair
        .filter(|_| endpoint_pair.is_some() || relation_name_hints_homotopy(relation_name))
    {
        return Some(build_carrier_spec(
            roles,
            source_role,
            target_role,
            CarrierSource::HomotopyConvention,
        ));
    }

    if let Some((source_role, target_role)) = endpoint_pair {
        return Some(build_carrier_spec(
            roles,
            source_role,
            target_role,
            CarrierSource::EndpointConvention,
        ));
    }

    let non_axis_roles = roles
        .iter()
        .filter(|role| role.kind == RoleKind::Data)
        .map(|role| role.order)
        .collect::<Vec<_>>();
    if non_axis_roles.len() == 2 {
        let source_role = non_axis_roles[0];
        let target_role = non_axis_roles[1];
        return Some(build_carrier_spec(
            roles,
            source_role,
            target_role,
            CarrierSource::DeclaredOrder,
        ));
    }

    None
}

fn build_carrier_spec(
    roles: &[RoleIr],
    source_role: u16,
    target_role: u16,
    source: CarrierSource,
) -> CarrierSpecIr {
    CarrierSpecIr {
        source_role,
        target_role,
        fiber_roles: axis_role_indexes(roles, source_role, target_role),
        source,
    }
}

fn named_pair_index(roles: &[RoleIr], pairs: &[(&str, &str)]) -> Option<(u16, u16)> {
    for (left, right) in pairs {
        let left_idx = roles
            .iter()
            .find(|role| role.name == *left)
            .map(|role| role.order);
        let right_idx = roles
            .iter()
            .find(|role| role.name == *right)
            .map(|role| role.order);
        if let (Some(left_idx), Some(right_idx)) = (left_idx, right_idx) {
            return Some((left_idx, right_idx));
        }
    }
    None
}

fn axis_role_indexes(roles: &[RoleIr], source_role: u16, target_role: u16) -> Vec<u16> {
    roles
        .iter()
        .filter(|role| role.order != source_role && role.order != target_role)
        .filter(|role| matches!(role.kind, RoleKind::Context | RoleKind::Temporal))
        .map(|role| role.order)
        .collect()
}

fn relation_name_hints_homotopy(relation_name: &str) -> bool {
    let lowered = relation_name.to_ascii_lowercase();
    lowered.contains("equiv") || lowered.contains("homotopy")
}

fn collect_object_types(schema: &SchemaV1Schema) -> HashSet<String> {
    let mut object_types: HashSet<String> = schema.objects.iter().cloned().collect();
    for subtype in &schema.subtypes {
        object_types.insert(subtype.sub.clone());
        object_types.insert(subtype.sup.clone());
    }
    for relation in &schema.relations {
        for field in &relation.fields {
            object_types.insert(field.ty.clone());
        }
    }
    object_types
}

fn compute_supertypes_closure(
    schema: &SchemaV1Schema,
    object_types: &HashSet<String>,
) -> HashMap<String, HashSet<String>> {
    let mut direct_supers: HashMap<String, Vec<String>> = HashMap::new();
    for ty in object_types {
        direct_supers.entry(ty.clone()).or_default();
    }
    for subtype in &schema.subtypes {
        direct_supers
            .entry(subtype.sub.clone())
            .or_default()
            .push(subtype.sup.clone());
        direct_supers.entry(subtype.sup.clone()).or_default();
    }

    let mut out = HashMap::new();
    for ty in object_types {
        let mut supers = HashSet::from([ty.clone()]);
        let mut stack = direct_supers.get(ty).cloned().unwrap_or_default();
        while let Some(next) = stack.pop() {
            if supers.insert(next.clone()) {
                if let Some(more) = direct_supers.get(&next) {
                    stack.extend(more.iter().cloned());
                }
            }
        }
        out.insert(ty.clone(), supers);
    }
    out
}

fn is_context_axis_type(supertypes_of: &HashMap<String, HashSet<String>>, ty: &str) -> bool {
    ty == "Context"
        || supertypes_of
            .get(ty)
            .is_some_and(|supers| supers.contains("Context"))
}

fn is_temporal_axis_type(supertypes_of: &HashMap<String, HashSet<String>>, ty: &str) -> bool {
    ty == "Time"
        || supertypes_of
            .get(ty)
            .is_some_and(|supers| supers.contains("Time"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiograph_dsl::schema_v1::{FieldDeclV1, RelationDeclV1, SchemaV1Schema};

    fn relation(fields: &[(&str, &str)]) -> RelationDeclV1 {
        RelationDeclV1 {
            name: "R".to_string(),
            fields: fields
                .iter()
                .map(|(field, ty)| FieldDeclV1 {
                    field: (*field).to_string(),
                    ty: (*ty).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn two_non_axis_roles_preserve_declared_order() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string(), "Context".to_string()],
            subtypes: Vec::new(),
            relations: vec![RelationDeclV1 {
                name: "Parent".to_string(),
                fields: vec![
                    FieldDeclV1 {
                        field: "parent".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "child".to_string(),
                        ty: "Person".to_string(),
                    },
                    FieldDeclV1 {
                        field: "ctx".to_string(),
                        ty: "Context".to_string(),
                    },
                ],
            }],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("Parent").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), Some(("parent", "child")));
        assert_eq!(rel.morphism_field_names(), Some(("parent", "child")));
    }

    #[test]
    fn equivalence_prefers_path_pair_and_marks_homotopy() {
        let roles = vec![
            RoleIr {
                name: "from".to_string(),
                target_type: "World".to_string(),
                order: 0,
                kind: RoleKind::Data,
            },
            RoleIr {
                name: "to".to_string(),
                target_type: "World".to_string(),
                order: 1,
                kind: RoleKind::Data,
            },
            RoleIr {
                name: "route1".to_string(),
                target_type: "Route".to_string(),
                order: 2,
                kind: RoleKind::Data,
            },
            RoleIr {
                name: "route2".to_string(),
                target_type: "Route".to_string(),
                order: 3,
                kind: RoleKind::Data,
            },
            RoleIr {
                name: "ctx".to_string(),
                target_type: "Context".to_string(),
                order: 4,
                kind: RoleKind::Context,
            },
        ];

        let rel =
            compile_relation_semantics("RouteEquivalence", "RouteEquivalence".to_string(), roles);
        assert_eq!(rel.carrier_field_names(), Some(("route1", "route2")));
        assert_eq!(rel.homotopy_field_names(), Some(("route1", "route2")));
        assert_eq!(rel.morphism_field_names(), None);
        assert_eq!(
            rel.carrier.as_ref().map(|c| c.fiber_roles.clone()),
            Some(vec![4])
        );
    }

    #[test]
    fn dual_pairs_choose_homotopy_without_name_hint() {
        let roles = vec![
            RoleIr {
                name: "from".to_string(),
                target_type: "World".to_string(),
                order: 0,
                kind: RoleKind::Context,
            },
            RoleIr {
                name: "to".to_string(),
                target_type: "World".to_string(),
                order: 1,
                kind: RoleKind::Context,
            },
            RoleIr {
                name: "route1".to_string(),
                target_type: "Route".to_string(),
                order: 2,
                kind: RoleKind::Data,
            },
            RoleIr {
                name: "route2".to_string(),
                target_type: "Route".to_string(),
                order: 3,
                kind: RoleKind::Data,
            },
        ];

        let rel = compile_relation_semantics("RouteWitness", "RouteWitness".to_string(), roles);
        assert_eq!(rel.carrier_field_names(), Some(("route1", "route2")));
        assert_eq!(rel.homotopy_field_names(), Some(("route1", "route2")));
        assert_eq!(
            rel.carrier.as_ref().map(|carrier| carrier.source),
            Some(CarrierSource::HomotopyConvention)
        );
    }

    #[test]
    fn exactly_two_non_axis_roles_drive_carrier_even_with_context_and_time() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec![
                "World".to_string(),
                "Context".to_string(),
                "Time".to_string(),
            ],
            subtypes: Vec::new(),
            relations: vec![relation(&[
                ("from", "World"),
                ("to", "World"),
                ("ctx", "Context"),
                ("time", "Time"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), Some(("from", "to")));
        assert_eq!(
            rel.carrier.as_ref().map(|c| c.fiber_roles.clone()),
            Some(vec![2, 3])
        );
    }

    #[test]
    fn extra_data_roles_without_explicit_convention_do_not_get_carrier() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec![
                "Person".to_string(),
                "World".to_string(),
                "Context".to_string(),
            ],
            subtypes: Vec::new(),
            relations: vec![relation(&[
                ("parent", "Person"),
                ("child", "Person"),
                ("scope", "World"),
                ("ctx", "Context"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), None);
        assert_eq!(rel.morphism_field_names(), None);
    }

    #[test]
    fn world_typed_scope_field_remains_data() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Person".to_string(), "World".to_string()],
            subtypes: Vec::new(),
            relations: vec![relation(&[
                ("parent", "Person"),
                ("child", "Person"),
                ("scope", "World"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), None);
        assert_eq!(rel.morphism_field_names(), None);
    }

    #[test]
    fn subtypes_of_context_and_time_are_treated_as_axes() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec![
                "Person".to_string(),
                "Context".to_string(),
                "ScopedContext".to_string(),
                "Time".to_string(),
                "EventTime".to_string(),
            ],
            subtypes: vec![
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "ScopedContext".to_string(),
                    sup: "Context".to_string(),
                    inclusion: None,
                },
                axiograph_dsl::schema_v1::SubtypeDeclV1 {
                    sub: "EventTime".to_string(),
                    sup: "Time".to_string(),
                    inclusion: None,
                },
            ],
            relations: vec![relation(&[
                ("actor", "Person"),
                ("target", "Person"),
                ("scope_ref", "ScopedContext"),
                ("recorded_at", "EventTime"),
            ])],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir.relation("R").expect("relation semantics");
        assert_eq!(rel.carrier_field_names(), Some(("actor", "target")));
        assert_eq!(rel.morphism_field_names(), Some(("actor", "target")));
        assert_eq!(
            rel.carrier.as_ref().map(|c| c.fiber_roles.clone()),
            Some(vec![2, 3])
        );
    }

    #[test]
    fn tuple_type_name_avoids_collision_with_implicit_field_types() {
        let schema = SchemaV1Schema {
            name: "S".to_string(),
            objects: vec!["Schema_".to_string(), "Migration".to_string()],
            subtypes: Vec::new(),
            relations: vec![
                RelationDeclV1 {
                    name: "SchemaEquiv".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "s1".to_string(),
                            ty: "Schema_".to_string(),
                        },
                        FieldDeclV1 {
                            field: "s2".to_string(),
                            ty: "Schema_".to_string(),
                        },
                        FieldDeclV1 {
                            field: "forward".to_string(),
                            ty: "Migration".to_string(),
                        },
                        FieldDeclV1 {
                            field: "backward".to_string(),
                            ty: "Migration".to_string(),
                        },
                    ],
                },
                RelationDeclV1 {
                    name: "EquivCompose".to_string(),
                    fields: vec![
                        FieldDeclV1 {
                            field: "lhs".to_string(),
                            ty: "SchemaEquiv".to_string(),
                        },
                        FieldDeclV1 {
                            field: "rhs".to_string(),
                            ty: "SchemaEquiv".to_string(),
                        },
                    ],
                },
            ],
        };

        let ir = compile_schema_ir(&schema);
        let rel = ir
            .relation("SchemaEquiv")
            .expect("schema equivalence relation semantics");
        assert_eq!(rel.tuple_type_name, "SchemaEquivFact");
    }
}
