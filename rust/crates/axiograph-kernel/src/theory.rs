//! Executable finite category, dependent-witness, and explanation fragment.
//!
//! Rust remains an untrusted runtime. These finite checks produce replayable
//! evidence; the matching trusted checker is `Axiograph.Theory.Finite`.

use crate::{
    CompiledKernelSnapshot, ConstraintIdV2, EquationIdV2, FactIdV2, InstanceIdV2, RelationIdV2,
    RoleIdV2, RoleKindIr, SchemaGeneratorIr, SchemaGeneratorRefIr, SchemaIdV2, SchemaObjectRefIr,
    SchemaPathIr, SchemaPresentationIr, TheoryIdV2, TypedValueIr,
};
use axiograph_dsl::schema_v1::ConstraintV1;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

pub const CATEGORY_FORMATION_IR_VERSION: &str = "category_formation_ir_v3";
pub const FINITE_SATURATION_ALGORITHM: &str = "finite_floyd_warshall_v2";
pub const FINITE_THEORY_GATE_VERSION: &str = "finite_theory_gate_v3";
pub const FORMAL_GROUPOID_NORMALIZATION_NON_CLAIM: &str =
    "free-groupoid normalization is formal proof syntax; it does not make a non-reversible runtime projection executable";
pub const MAX_FINITE_CATEGORY_OBJECTS: usize = 64;
pub const MAX_FINITE_CATEGORY_ARROWS: usize = 4_096;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CheckedLifecycleStateIr {
    FormationChecked,
    ExplanationVerified,
    Residual,
    Rejected,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TheoryResidualKindIr {
    SaturationBound,
    TypedHole,
    UnsupportedTransport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct TheoryResidualObligationIr {
    pub obligation_id: String,
    pub kind: TheoryResidualKindIr,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SchemaObjectRefIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<SchemaObjectRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repair_handle_ids: Vec<String>,
}

impl TheoryResidualObligationIr {
    fn new(id: impl Into<String>, kind: TheoryResidualKindIr, message: impl Into<String>) -> Self {
        Self {
            obligation_id: id.into(),
            kind,
            message: message.into(),
            source: None,
            target: None,
            repair_handle_ids: Vec::new(),
        }
    }

    fn endpoints(mut self, source: SchemaObjectRefIr, target: SchemaObjectRefIr) -> Self {
        self.source = Some(source);
        self.target = Some(target);
        self
    }

    fn repair_handles(mut self, repair_handle_ids: Vec<String>) -> Self {
        self.repair_handle_ids = repair_handle_ids;
        self
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum FiniteTheoryError {
    #[error("unknown category object `{0:?}`")]
    UnknownObject(SchemaObjectRefIr),
    #[error("unknown schema generator `{0:?}`")]
    UnknownGenerator(SchemaGeneratorRefIr),
    #[error("category path endpoint mismatch")]
    EndpointMismatch,
    #[error("unknown schema equation `{0}`")]
    UnknownEquation(EquationIdV2),
    #[error("equation congruence step does not match the selected path segment")]
    CongruenceMismatch,
    #[error("finite saturation certificate is malformed: {0}")]
    MalformedCertificate(String),
    #[error("finite theory gate is blocked: {0}")]
    GateBlocked(String),
    #[error("instance `{0}` has role-indexed witness drift")]
    WitnessMismatch(String),
    #[error(
        "indexed finite fiber for relation `{relation_id}` role `{role_id}` is invalid: {detail}"
    )]
    IndexedFiberMismatch {
        relation_id: RelationIdV2,
        role_id: RoleIdV2,
        detail: String,
    },
    #[error("checked lifecycle state is inconsistent: {0}")]
    Lifecycle(String),
    #[error("scope transport is outside the implemented identity fragment: {0}")]
    UnsupportedTransport(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryIdentityIr {
    pub object: SchemaObjectRefIr,
    pub path: SchemaPathIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReachabilityExplanationIr {
    Identity {
        object: u32,
    },
    Generator {
        arrow: u32,
    },
    Trans {
        left: Box<ReachabilityExplanationIr>,
        right: Box<ReachabilityExplanationIr>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReachabilityEntryIr {
    pub source: u32,
    pub target: u32,
    pub explanation: ReachabilityExplanationIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FiniteSaturationCertificateIr {
    pub schema_id: SchemaIdV2,
    pub presentation_object_count: u32,
    pub presentation_arrow_count: u32,
    pub entries: Vec<ReachabilityEntryIr>,
    pub algorithm: String,
}

/// Replay evidence for the finite, explicitly presented category fragment.
///
/// `SchemaPresentationIr` is the sole category presentation. This structure
/// deliberately stores only derived formation evidence: identities, bounded
/// generator reachability, lifecycle, and residuals. Objects, relation
/// objects, ordered projections, generators, equations, and congruence
/// witnesses are not duplicated here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryFormationIr {
    pub version: String,
    pub schema_id: SchemaIdV2,
    pub identities: Vec<CategoryIdentityIr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation: Option<FiniteSaturationCertificateIr>,
    pub lifecycle: CheckedLifecycleStateIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<TheoryResidualObligationIr>,
    pub non_claims: Vec<String>,
}

fn category_object_refs(
    objects: &[crate::SchemaObjectIr],
    relations: &[crate::RelationObjectIr],
) -> Vec<SchemaObjectRefIr> {
    objects
        .iter()
        .map(|object| SchemaObjectRefIr::ObjectType {
            object_type_id: object.object_type_id.clone(),
        })
        .chain(
            relations
                .iter()
                .map(|relation| SchemaObjectRefIr::RelationObject {
                    relation_id: relation.relation_id.clone(),
                }),
        )
        .collect()
}

fn category_identities(objects: &[SchemaObjectRefIr]) -> Vec<CategoryIdentityIr> {
    objects
        .iter()
        .cloned()
        .map(|object| CategoryIdentityIr {
            path: SchemaPathIr {
                source: object.clone(),
                target: object.clone(),
                steps: Vec::new(),
            },
            object,
        })
        .collect()
}

fn verify_ordered_projection_parts(
    relations: &[crate::RelationObjectIr],
    generators: &[SchemaGeneratorIr],
) -> Result<(), FiniteTheoryError> {
    for relation in relations {
        let source = SchemaObjectRefIr::RelationObject {
            relation_id: relation.relation_id.clone(),
        };
        for (expected_order, role) in relation.roles.iter().enumerate() {
            if role.declared_order != expected_order as u32 {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "relation `{}` projection `{}` has order {}, expected {}",
                    relation.label, role.label, role.declared_order, expected_order
                )));
            }
            let projection_ref = SchemaGeneratorRefIr::RoleProjection {
                role_id: role.role_id.clone(),
            };
            let matches = generators
                .iter()
                .filter(|generator| generator.generator_ref == projection_ref)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "relation `{}` role `{}` must have exactly one projection generator",
                    relation.label, role.label
                )));
            }
            let projection = matches[0];
            if projection.source != source
                || projection.target != role.type_expr.carrier()
                || projection.kind != crate::SchemaGeneratorKindIr::RoleProjection
                || projection.label != format!("{}.{}", relation.label, role.label)
            {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "relation `{}` role `{}` projection generator drifted",
                    relation.label, role.label
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn compile_category_formation(
    schema_id: &SchemaIdV2,
    objects: &[crate::SchemaObjectIr],
    relations: &[crate::RelationObjectIr],
    generators: &[SchemaGeneratorIr],
) -> Result<CategoryFormationIr, FiniteTheoryError> {
    let object_refs = category_object_refs(objects, relations);
    verify_ordered_projection_parts(relations, generators)?;
    let identities = category_identities(&object_refs);
    let mut residual_obligations = Vec::new();
    let saturation = if object_refs.len() > MAX_FINITE_CATEGORY_OBJECTS {
        residual_obligations.push(TheoryResidualObligationIr::new(
            format!("schema:{schema_id}:finite-object-bound"),
            TheoryResidualKindIr::SaturationBound,
            format!(
                "category has {} objects, exceeding the explanation bound {}",
                object_refs.len(),
                MAX_FINITE_CATEGORY_OBJECTS
            ),
        ));
        None
    } else if generators.len() > MAX_FINITE_CATEGORY_ARROWS {
        residual_obligations.push(TheoryResidualObligationIr::new(
            format!("schema:{schema_id}:finite-arrow-bound"),
            TheoryResidualKindIr::SaturationBound,
            format!(
                "category has {} generators, exceeding the explanation bound {}",
                generators.len(),
                MAX_FINITE_CATEGORY_ARROWS
            ),
        ));
        None
    } else {
        Some(saturate_parts(schema_id, &object_refs, generators)?)
    };
    let lifecycle = if residual_obligations.is_empty() {
        CheckedLifecycleStateIr::ExplanationVerified
    } else {
        CheckedLifecycleStateIr::Residual
    };
    Ok(CategoryFormationIr {
        version: CATEGORY_FORMATION_IR_VERSION.to_string(),
        schema_id: schema_id.clone(),
        identities,
        saturation,
        lifecycle,
        residual_obligations,
        non_claims: vec![
            "finite saturation proves exact generator reachability only".to_string(),
            "presented equations identify parallel paths but do not create endpoints".to_string(),
            "formal inverses are proof syntax, not execution, unless a generator is explicitly reversible".to_string(),
            "no rewrite termination, confluence, open-world closure, univalence, higher inductive type, topos, or sheaf completeness is claimed".to_string(),
            "confidence arithmetic is outside category and groupoid equality laws".to_string(),
        ],
    })
}

fn saturate_parts(
    schema_id: &SchemaIdV2,
    objects: &[SchemaObjectRefIr],
    generators: &[SchemaGeneratorIr],
) -> Result<FiniteSaturationCertificateIr, FiniteTheoryError> {
    let index = object_index(objects);
    let mut closure = BTreeMap::<(u32, u32), ReachabilityExplanationIr>::new();
    for object in 0..objects.len() as u32 {
        closure.insert(
            (object, object),
            ReachabilityExplanationIr::Identity { object },
        );
    }
    for (arrow, generator) in generators.iter().enumerate() {
        let source = index
            .get(&generator.source)
            .copied()
            .ok_or_else(|| FiniteTheoryError::UnknownObject(generator.source.clone()))?;
        let target = index
            .get(&generator.target)
            .copied()
            .ok_or_else(|| FiniteTheoryError::UnknownObject(generator.target.clone()))?;
        closure
            .entry((source, target))
            .or_insert(ReachabilityExplanationIr::Generator {
                arrow: arrow as u32,
            });
    }
    let count = objects.len() as u32;
    for middle in 0..count {
        for source in 0..count {
            for target in 0..count {
                if closure.contains_key(&(source, target)) {
                    continue;
                }
                let Some(left) = closure.get(&(source, middle)).cloned() else {
                    continue;
                };
                let Some(right) = closure.get(&(middle, target)).cloned() else {
                    continue;
                };
                closure.insert(
                    (source, target),
                    ReachabilityExplanationIr::Trans {
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                );
            }
        }
    }
    let certificate = FiniteSaturationCertificateIr {
        schema_id: schema_id.clone(),
        presentation_object_count: objects.len() as u32,
        presentation_arrow_count: generators.len() as u32,
        entries: closure
            .into_iter()
            .map(|((source, target), explanation)| ReachabilityEntryIr {
                source,
                target,
                explanation,
            })
            .collect(),
        algorithm: FINITE_SATURATION_ALGORITHM.to_string(),
    };
    verify_saturation_parts(schema_id, objects, generators, &certificate)?;
    Ok(certificate)
}

fn object_index(objects: &[SchemaObjectRefIr]) -> BTreeMap<SchemaObjectRefIr, u32> {
    objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.clone(), index as u32))
        .collect()
}

fn compose_paths_unchecked(
    mut left: SchemaPathIr,
    right: SchemaPathIr,
) -> Result<SchemaPathIr, FiniteTheoryError> {
    if left.target != right.source {
        return Err(FiniteTheoryError::EndpointMismatch);
    }
    left.target = right.target;
    left.steps.extend(right.steps);
    Ok(left)
}

fn replay_explanation(
    objects: &[SchemaObjectRefIr],
    generators: &[SchemaGeneratorIr],
    explanation: &ReachabilityExplanationIr,
    depth: usize,
) -> Result<SchemaPathIr, FiniteTheoryError> {
    let depth_limit = objects
        .len()
        .saturating_mul(2)
        .saturating_add(generators.len())
        .saturating_add(1);
    if depth > depth_limit {
        return Err(FiniteTheoryError::MalformedCertificate(
            "explanation nesting exceeds the finite presentation bound".to_string(),
        ));
    }
    match explanation {
        ReachabilityExplanationIr::Identity { object } => {
            let object = objects.get(*object as usize).ok_or_else(|| {
                FiniteTheoryError::MalformedCertificate(format!(
                    "identity object index {object} is out of range"
                ))
            })?;
            Ok(SchemaPathIr {
                source: object.clone(),
                target: object.clone(),
                steps: Vec::new(),
            })
        }
        ReachabilityExplanationIr::Generator { arrow } => {
            let generator = generators.get(*arrow as usize).ok_or_else(|| {
                FiniteTheoryError::MalformedCertificate(format!(
                    "generator index {arrow} is out of range"
                ))
            })?;
            Ok(SchemaPathIr {
                source: generator.source.clone(),
                target: generator.target.clone(),
                steps: vec![generator.generator_ref.clone()],
            })
        }
        ReachabilityExplanationIr::Trans { left, right } => {
            let left = replay_explanation(objects, generators, left, depth + 1)?;
            let right = replay_explanation(objects, generators, right, depth + 1)?;
            compose_paths_unchecked(left, right)
        }
    }
}

fn saturation_keys(
    objects: &[SchemaObjectRefIr],
    generators: &[SchemaGeneratorIr],
) -> Result<BTreeSet<(u32, u32)>, FiniteTheoryError> {
    let index = object_index(objects);
    let mut keys = (0..objects.len() as u32)
        .map(|object| (object, object))
        .collect::<BTreeSet<_>>();
    for generator in generators {
        let source = index
            .get(&generator.source)
            .copied()
            .ok_or_else(|| FiniteTheoryError::UnknownObject(generator.source.clone()))?;
        let target = index
            .get(&generator.target)
            .copied()
            .ok_or_else(|| FiniteTheoryError::UnknownObject(generator.target.clone()))?;
        keys.insert((source, target));
    }
    for middle in 0..objects.len() as u32 {
        for source in 0..objects.len() as u32 {
            for target in 0..objects.len() as u32 {
                if keys.contains(&(source, middle)) && keys.contains(&(middle, target)) {
                    keys.insert((source, target));
                }
            }
        }
    }
    Ok(keys)
}

fn verify_saturation_parts(
    schema_id: &SchemaIdV2,
    objects: &[SchemaObjectRefIr],
    generators: &[SchemaGeneratorIr],
    certificate: &FiniteSaturationCertificateIr,
) -> Result<Vec<SchemaPathIr>, FiniteTheoryError> {
    if certificate.schema_id != *schema_id {
        return Err(FiniteTheoryError::MalformedCertificate(
            "schema identity does not match the checked presentation".to_string(),
        ));
    }
    if certificate.algorithm != FINITE_SATURATION_ALGORITHM {
        return Err(FiniteTheoryError::MalformedCertificate(format!(
            "unsupported saturation algorithm `{}`",
            certificate.algorithm
        )));
    }
    if certificate.presentation_object_count as usize != objects.len()
        || certificate.presentation_arrow_count as usize != generators.len()
    {
        return Err(FiniteTheoryError::MalformedCertificate(
            "presentation shape does not match the checked schema".to_string(),
        ));
    }
    if certificate.entries.len() > objects.len().saturating_mul(objects.len()) {
        return Err(FiniteTheoryError::MalformedCertificate(
            "certificate has more endpoint pairs than the finite presentation".to_string(),
        ));
    }
    let mut keys = BTreeSet::new();
    let mut paths = Vec::with_capacity(certificate.entries.len());
    for entry in &certificate.entries {
        if !keys.insert((entry.source, entry.target)) {
            return Err(FiniteTheoryError::MalformedCertificate(format!(
                "duplicate reachability entry {} -> {}",
                entry.source, entry.target
            )));
        }
        let path = replay_explanation(objects, generators, &entry.explanation, 0)?;
        let source = objects.get(entry.source as usize).ok_or_else(|| {
            FiniteTheoryError::MalformedCertificate(format!(
                "entry source index {} is out of range",
                entry.source
            ))
        })?;
        let target = objects.get(entry.target as usize).ok_or_else(|| {
            FiniteTheoryError::MalformedCertificate(format!(
                "entry target index {} is out of range",
                entry.target
            ))
        })?;
        if path.source != *source || path.target != *target {
            return Err(FiniteTheoryError::MalformedCertificate(format!(
                "explanation endpoints do not match entry {} -> {}",
                entry.source, entry.target
            )));
        }
        paths.push(path);
    }
    let expected = saturation_keys(objects, generators)?;
    if keys != expected {
        return Err(FiniteTheoryError::MalformedCertificate(
            "endpoint set is not the exact finite generator reachability closure".to_string(),
        ));
    }
    Ok(paths)
}

impl CategoryFormationIr {
    pub fn verify(&self, schema: &SchemaPresentationIr) -> Result<(), FiniteTheoryError> {
        if self.version != CATEGORY_FORMATION_IR_VERSION || self.schema_id != schema.schema_id {
            return Err(FiniteTheoryError::Lifecycle(
                "category formation version/schema identity drift".to_string(),
            ));
        }

        let objects = schema.object_refs();
        if objects.iter().collect::<BTreeSet<_>>().len() != objects.len() {
            return Err(FiniteTheoryError::Lifecycle(
                "category presentation contains duplicate object identities".to_string(),
            ));
        }
        let identities = category_identities(&objects);
        if self.identities != identities {
            return Err(FiniteTheoryError::Lifecycle(
                "category identity witnesses drifted".to_string(),
            ));
        }

        let object_set = objects.iter().collect::<BTreeSet<_>>();
        let mut generator_refs = BTreeSet::new();
        let mut generator_labels = BTreeSet::new();
        for generator in &schema.generators {
            if !generator_refs.insert(generator.generator_ref.clone()) {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "duplicate category generator reference `{:?}`",
                    generator.generator_ref
                )));
            }
            if !generator_labels.insert(generator.label.as_str()) {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "duplicate category generator label `{}`",
                    generator.label
                )));
            }
            if !object_set.contains(&generator.source) || !object_set.contains(&generator.target) {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "generator `{}` has an endpoint outside the category presentation",
                    generator.label
                )));
            }
        }
        verify_ordered_projection_parts(&schema.relations, &schema.generators)?;

        let mut equation_ids = BTreeSet::new();
        let mut category_equation_labels = BTreeSet::new();
        for equation in &schema.equations {
            if !equation_ids.insert(equation.equation_id.clone()) {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "duplicate presented equation identity `{}`",
                    equation.equation_id
                )));
            }
            if !category_equation_labels.insert(equation.label.as_str()) {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "duplicate presented category equation label `{}`",
                    equation.label
                )));
            }
            schema.verify_path(&equation.lhs)?;
            schema.verify_path(&equation.rhs)?;
            if equation.lhs.source != equation.rhs.source
                || equation.lhs.target != equation.rhs.target
            {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "presented equation `{}` is not a parallel path pair",
                    equation.label
                )));
            }
        }
        for equation in &schema.formal_groupoid_equations {
            if !equation_ids.insert(equation.equation_id.clone()) {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "duplicate presented equation identity `{}`",
                    equation.equation_id
                )));
            }
            schema.verify_formal_path(&equation.lhs)?;
            schema.verify_formal_path(&equation.rhs)?;
            if equation.lhs.source != equation.rhs.source
                || equation.lhs.target != equation.rhs.target
                || equation.lifecycle != CheckedLifecycleStateIr::FormationChecked
            {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "formal groupoid equation `{}` is not a checked parallel path pair",
                    equation.label
                )));
            }
        }

        if schema.congruence_witnesses.len() != schema.equations.len() {
            return Err(FiniteTheoryError::Lifecycle(
                "forward presented equations do not have exact congruence-witness coverage"
                    .to_string(),
            ));
        }
        for (equation, certificate) in schema
            .equations
            .iter()
            .zip(schema.congruence_witnesses.iter())
        {
            if certificate.steps.len() != 1
                || certificate.steps[0].equation_id != equation.equation_id
                || certificate.steps[0].direction != EquationDirectionIr::Forward
            {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "presented equation `{}` has a non-canonical congruence witness",
                    equation.label
                )));
            }
            certificate.replay(schema)?;
        }

        match (&self.saturation, self.lifecycle) {
            (Some(certificate), CheckedLifecycleStateIr::ExplanationVerified)
                if self.residual_obligations.is_empty() =>
            {
                verify_saturation_parts(
                    &schema.schema_id,
                    &objects,
                    &schema.generators,
                    certificate,
                )?;
            }
            (None, CheckedLifecycleStateIr::Residual) if !self.residual_obligations.is_empty() => {}
            _ => {
                return Err(FiniteTheoryError::Lifecycle(
                    "saturation evidence, residuals, and lifecycle disagree".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub fn replay(
        &self,
        schema: &SchemaPresentationIr,
        source: &SchemaObjectRefIr,
        target: &SchemaObjectRefIr,
    ) -> Result<Option<SchemaPathIr>, FiniteTheoryError> {
        self.verify(schema)?;
        let Some(certificate) = &self.saturation else {
            return Ok(None);
        };
        let objects = schema.object_refs();
        let index = object_index(&objects);
        let Some(source_index) = index.get(source).copied() else {
            return Err(FiniteTheoryError::UnknownObject(source.clone()));
        };
        let Some(target_index) = index.get(target).copied() else {
            return Err(FiniteTheoryError::UnknownObject(target.clone()));
        };
        let entry = certificate
            .entries
            .iter()
            .find(|entry| entry.source == source_index && entry.target == target_index);
        entry
            .map(|entry| replay_explanation(&objects, &schema.generators, &entry.explanation, 0))
            .transpose()
    }
}

impl SchemaPresentationIr {
    pub fn object_refs(&self) -> Vec<SchemaObjectRefIr> {
        category_object_refs(&self.objects, &self.relations)
    }

    fn generator(&self, generator_ref: &SchemaGeneratorRefIr) -> Option<&SchemaGeneratorIr> {
        self.generators
            .iter()
            .find(|generator| generator.generator_ref == *generator_ref)
    }

    pub fn identity_path(
        &self,
        object: SchemaObjectRefIr,
    ) -> Result<SchemaPathIr, FiniteTheoryError> {
        if !self.object_refs().contains(&object) {
            return Err(FiniteTheoryError::UnknownObject(object));
        }
        Ok(SchemaPathIr {
            source: object.clone(),
            target: object,
            steps: Vec::new(),
        })
    }

    pub fn generator_path(
        &self,
        generator_ref: &SchemaGeneratorRefIr,
    ) -> Result<SchemaPathIr, FiniteTheoryError> {
        let generator = self
            .generator(generator_ref)
            .ok_or_else(|| FiniteTheoryError::UnknownGenerator(generator_ref.clone()))?;
        Ok(SchemaPathIr {
            source: generator.source.clone(),
            target: generator.target.clone(),
            steps: vec![generator_ref.clone()],
        })
    }

    pub fn verify_path(&self, path: &SchemaPathIr) -> Result<(), FiniteTheoryError> {
        if !self.object_refs().contains(&path.source) || !self.object_refs().contains(&path.target)
        {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        let mut cursor = path.source.clone();
        for step in &path.steps {
            let generator = self
                .generator(step)
                .ok_or_else(|| FiniteTheoryError::UnknownGenerator(step.clone()))?;
            if generator.source != cursor {
                return Err(FiniteTheoryError::EndpointMismatch);
            }
            cursor = generator.target.clone();
        }
        if cursor != path.target {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        Ok(())
    }

    pub fn compose_paths(
        &self,
        left: &SchemaPathIr,
        right: &SchemaPathIr,
    ) -> Result<SchemaPathIr, FiniteTheoryError> {
        self.verify_path(left)?;
        self.verify_path(right)?;
        let result = compose_paths_unchecked(left.clone(), right.clone())?;
        self.verify_path(&result)?;
        Ok(result)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EquationDirectionIr {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EquationCongruenceStepIr {
    pub equation_id: EquationIdV2,
    pub direction: EquationDirectionIr,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PathCongruenceCertificateIr {
    pub schema_id: SchemaIdV2,
    pub input: SchemaPathIr,
    pub steps: Vec<EquationCongruenceStepIr>,
    pub output: SchemaPathIr,
    pub lifecycle: CheckedLifecycleStateIr,
}

fn equation<'a>(
    schema: &'a SchemaPresentationIr,
    equation_id: &EquationIdV2,
) -> Result<&'a crate::SchemaEquationIr, FiniteTheoryError> {
    schema
        .equations
        .iter()
        .find(|equation| equation.equation_id == *equation_id)
        .ok_or_else(|| FiniteTheoryError::UnknownEquation(equation_id.clone()))
}

fn path_object_at_offset(
    schema: &SchemaPresentationIr,
    path: &SchemaPathIr,
    offset: usize,
) -> Result<SchemaObjectRefIr, FiniteTheoryError> {
    if offset > path.steps.len() {
        return Err(FiniteTheoryError::CongruenceMismatch);
    }
    let mut cursor = path.source.clone();
    for generator_ref in path.steps.iter().take(offset) {
        let generator = schema
            .generator(generator_ref)
            .ok_or_else(|| FiniteTheoryError::UnknownGenerator(generator_ref.clone()))?;
        if generator.source != cursor {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        cursor = generator.target.clone();
    }
    Ok(cursor)
}

fn apply_equation_step(
    schema: &SchemaPresentationIr,
    path: &SchemaPathIr,
    step: &EquationCongruenceStepIr,
) -> Result<SchemaPathIr, FiniteTheoryError> {
    schema.verify_path(path)?;
    let equation = equation(schema, &step.equation_id)?;
    let (from, to) = match step.direction {
        EquationDirectionIr::Forward => (&equation.lhs, &equation.rhs),
        EquationDirectionIr::Reverse => (&equation.rhs, &equation.lhs),
    };
    let start = step.offset as usize;
    let end = start.saturating_add(from.steps.len());
    if end > path.steps.len()
        || path.steps[start..end] != from.steps
        || path_object_at_offset(schema, path, start)? != from.source
        || path_object_at_offset(schema, path, end)? != from.target
    {
        return Err(FiniteTheoryError::CongruenceMismatch);
    }
    let mut steps = path.steps[..start].to_vec();
    steps.extend(to.steps.clone());
    steps.extend(path.steps[end..].to_vec());
    let result = SchemaPathIr {
        source: path.source.clone(),
        target: path.target.clone(),
        steps,
    };
    schema.verify_path(&result)?;
    Ok(result)
}

impl PathCongruenceCertificateIr {
    pub fn replay(&self, schema: &SchemaPresentationIr) -> Result<SchemaPathIr, FiniteTheoryError> {
        if self.schema_id != schema.schema_id
            || self.lifecycle != CheckedLifecycleStateIr::ExplanationVerified
        {
            return Err(FiniteTheoryError::Lifecycle(
                "path congruence certificate is not explanation-verified for this schema"
                    .to_string(),
            ));
        }
        schema.verify_path(&self.input)?;
        schema.verify_path(&self.output)?;
        let mut path = self.input.clone();
        for step in &self.steps {
            path = apply_equation_step(schema, &path, step)?;
        }
        if path != self.output {
            return Err(FiniteTheoryError::CongruenceMismatch);
        }
        Ok(path)
    }
}

impl SchemaPresentationIr {
    pub fn equation_congruence_certificate(
        &self,
        input: SchemaPathIr,
        steps: Vec<EquationCongruenceStepIr>,
    ) -> Result<PathCongruenceCertificateIr, FiniteTheoryError> {
        self.verify_path(&input)?;
        let mut output = input.clone();
        for step in &steps {
            output = apply_equation_step(self, &output, step)?;
        }
        let certificate = PathCongruenceCertificateIr {
            schema_id: self.schema_id.clone(),
            input,
            steps,
            output,
            lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
        };
        certificate.replay(self)?;
        Ok(certificate)
    }

    /// Construct one deterministic contextual replay witness for every forward
    /// presented equation. When available, the first composable prefix and
    /// suffix generators are included so the witness exercises replacement
    /// under composition rather than merely restating the equation.
    pub(crate) fn compile_congruence_witnesses(
        &self,
    ) -> Result<Vec<PathCongruenceCertificateIr>, FiniteTheoryError> {
        let mut witnesses = Vec::with_capacity(self.equations.len());
        for equation in &self.equations {
            self.verify_path(&equation.lhs)?;
            self.verify_path(&equation.rhs)?;
            let mut input = equation.lhs.clone();
            let mut offset = 0_u32;
            if let Some(prefix) = self
                .generators
                .iter()
                .find(|generator| generator.target == equation.lhs.source)
            {
                input = self.compose_paths(&self.generator_path(&prefix.generator_ref)?, &input)?;
                offset = 1;
            }
            if let Some(suffix) = self
                .generators
                .iter()
                .find(|generator| generator.source == equation.lhs.target)
            {
                input = self.compose_paths(&input, &self.generator_path(&suffix.generator_ref)?)?;
            }
            witnesses.push(self.equation_congruence_certificate(
                input,
                vec![EquationCongruenceStepIr {
                    equation_id: equation.equation_id.clone(),
                    direction: EquationDirectionIr::Forward,
                    offset,
                }],
            )?);
        }
        Ok(witnesses)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FormalDirectionIr {
    Forward,
    Inverse,
}

impl FormalDirectionIr {
    fn opposite(self) -> Self {
        match self {
            Self::Forward => Self::Inverse,
            Self::Inverse => Self::Forward,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalGeneratorStepIr {
    pub generator: SchemaGeneratorRefIr,
    pub direction: FormalDirectionIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalGroupoidPathIr {
    pub schema_id: SchemaIdV2,
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub steps: Vec<FormalGeneratorStepIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalGroupoidEquationIr {
    pub equation_id: EquationIdV2,
    pub theory_id: crate::TheoryIdV2,
    pub label: String,
    pub lhs: FormalGroupoidPathIr,
    pub rhs: FormalGroupoidPathIr,
    pub lifecycle: CheckedLifecycleStateIr,
    pub non_claim: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalGroupoidRewriteStepIr {
    /// Zero-based offset in the current signed generator word.
    pub offset: u32,
    /// Generator expected at `offset` and `offset + 1`.
    pub generator: SchemaGeneratorRefIr,
    /// Direction of the first step; the second must have the opposite direction.
    pub first_direction: FormalDirectionIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormalNormalizationCertificateIr {
    pub input: FormalGroupoidPathIr,
    /// Deterministic leftmost cancellation trace. Every step removes one
    /// adjacent `g ; g⁻¹` or `g⁻¹ ; g` pair from the current word.
    pub rewrite_trace: Vec<FormalGroupoidRewriteStepIr>,
    pub normalized: FormalGroupoidPathIr,
    pub lifecycle: CheckedLifecycleStateIr,
    pub non_claim: String,
}

impl SchemaPresentationIr {
    fn formal_step_endpoints(
        &self,
        step: &FormalGeneratorStepIr,
    ) -> Result<(SchemaObjectRefIr, SchemaObjectRefIr), FiniteTheoryError> {
        let generator = self
            .generator(&step.generator)
            .ok_or_else(|| FiniteTheoryError::UnknownGenerator(step.generator.clone()))?;
        Ok(match step.direction {
            FormalDirectionIr::Forward => (generator.source.clone(), generator.target.clone()),
            FormalDirectionIr::Inverse => (generator.target.clone(), generator.source.clone()),
        })
    }

    pub fn verify_formal_path(&self, path: &FormalGroupoidPathIr) -> Result<(), FiniteTheoryError> {
        if path.schema_id != self.schema_id {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        let mut cursor = path.source.clone();
        for step in &path.steps {
            let (source, target) = self.formal_step_endpoints(step)?;
            if source != cursor {
                return Err(FiniteTheoryError::EndpointMismatch);
            }
            cursor = target;
        }
        if cursor != path.target {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        Ok(())
    }

    pub fn formal_identity(
        &self,
        object: SchemaObjectRefIr,
    ) -> Result<FormalGroupoidPathIr, FiniteTheoryError> {
        if !self.object_refs().contains(&object) {
            return Err(FiniteTheoryError::UnknownObject(object));
        }
        Ok(FormalGroupoidPathIr {
            schema_id: self.schema_id.clone(),
            source: object.clone(),
            target: object,
            steps: Vec::new(),
        })
    }

    pub fn formal_generator_path(
        &self,
        generator_ref: &SchemaGeneratorRefIr,
        direction: FormalDirectionIr,
    ) -> Result<FormalGroupoidPathIr, FiniteTheoryError> {
        let generator = self
            .generator(generator_ref)
            .ok_or_else(|| FiniteTheoryError::UnknownGenerator(generator_ref.clone()))?;
        let (source, target) = match direction {
            FormalDirectionIr::Forward => (generator.source.clone(), generator.target.clone()),
            FormalDirectionIr::Inverse => (generator.target.clone(), generator.source.clone()),
        };
        let path = FormalGroupoidPathIr {
            schema_id: self.schema_id.clone(),
            source,
            target,
            steps: vec![FormalGeneratorStepIr {
                generator: generator_ref.clone(),
                direction,
            }],
        };
        self.verify_formal_path(&path)?;
        Ok(path)
    }

    pub fn compose_formal_paths(
        &self,
        left: &FormalGroupoidPathIr,
        right: &FormalGroupoidPathIr,
    ) -> Result<FormalGroupoidPathIr, FiniteTheoryError> {
        self.verify_formal_path(left)?;
        self.verify_formal_path(right)?;
        if left.schema_id != right.schema_id || left.target != right.source {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        let mut steps = left.steps.clone();
        steps.extend(right.steps.clone());
        let composed = FormalGroupoidPathIr {
            schema_id: self.schema_id.clone(),
            source: left.source.clone(),
            target: right.target.clone(),
            steps,
        };
        self.verify_formal_path(&composed)?;
        Ok(composed)
    }

    pub fn formal_inverse(
        &self,
        path: &FormalGroupoidPathIr,
    ) -> Result<FormalGroupoidPathIr, FiniteTheoryError> {
        self.verify_formal_path(path)?;
        let inverse = FormalGroupoidPathIr {
            schema_id: self.schema_id.clone(),
            source: path.target.clone(),
            target: path.source.clone(),
            steps: path
                .steps
                .iter()
                .rev()
                .map(|step| FormalGeneratorStepIr {
                    generator: step.generator.clone(),
                    direction: step.direction.opposite(),
                })
                .collect(),
        };
        self.verify_formal_path(&inverse)?;
        Ok(inverse)
    }

    pub fn formal_path_runtime_executable(
        &self,
        path: &FormalGroupoidPathIr,
    ) -> Result<bool, FiniteTheoryError> {
        self.verify_formal_path(path)?;
        Ok(path.steps.iter().all(|step| {
            step.direction == FormalDirectionIr::Forward
                || self
                    .generator(&step.generator)
                    .is_some_and(|generator| generator.reversible)
        }))
    }

    fn apply_formal_rewrite_step(
        &self,
        path: &FormalGroupoidPathIr,
        step: &FormalGroupoidRewriteStepIr,
    ) -> Result<FormalGroupoidPathIr, FiniteTheoryError> {
        self.verify_formal_path(path)?;
        let offset = step.offset as usize;
        let Some(first) = path.steps.get(offset) else {
            return Err(FiniteTheoryError::MalformedCertificate(format!(
                "formal rewrite offset {} is out of range",
                step.offset
            )));
        };
        let Some(second) = path.steps.get(offset.saturating_add(1)) else {
            return Err(FiniteTheoryError::MalformedCertificate(format!(
                "formal rewrite offset {} has no adjacent step",
                step.offset
            )));
        };
        if first.generator != step.generator
            || first.direction != step.first_direction
            || second.generator != step.generator
            || second.direction != step.first_direction.opposite()
        {
            return Err(FiniteTheoryError::MalformedCertificate(
                "formal rewrite step does not cite an adjacent inverse pair".to_string(),
            ));
        }
        let mut steps = path.steps.clone();
        steps.drain(offset..offset + 2);
        let rewritten = FormalGroupoidPathIr {
            schema_id: path.schema_id.clone(),
            source: path.source.clone(),
            target: path.target.clone(),
            steps,
        };
        self.verify_formal_path(&rewritten)?;
        Ok(rewritten)
    }

    pub fn normalize_formal_path(
        &self,
        input: FormalGroupoidPathIr,
    ) -> Result<FormalNormalizationCertificateIr, FiniteTheoryError> {
        self.verify_formal_path(&input)?;
        let mut current = input.clone();
        let mut rewrite_trace = Vec::new();
        while let Some(offset) = current.steps.windows(2).position(|pair| {
            pair[0].generator == pair[1].generator
                && pair[0].direction == pair[1].direction.opposite()
        }) {
            let first = current.steps[offset].clone();
            let step = FormalGroupoidRewriteStepIr {
                offset: offset as u32,
                generator: first.generator,
                first_direction: first.direction,
            };
            current = self.apply_formal_rewrite_step(&current, &step)?;
            rewrite_trace.push(step);
        }
        Ok(FormalNormalizationCertificateIr {
            input,
            rewrite_trace,
            normalized: current,
            lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
            non_claim: FORMAL_GROUPOID_NORMALIZATION_NON_CLAIM.to_string(),
        })
    }
}

impl FormalNormalizationCertificateIr {
    pub fn replay(
        &self,
        schema: &SchemaPresentationIr,
    ) -> Result<FormalGroupoidPathIr, FiniteTheoryError> {
        if self.lifecycle != CheckedLifecycleStateIr::ExplanationVerified {
            return Err(FiniteTheoryError::Lifecycle(
                "formal normalization certificate is not explanation-verified".to_string(),
            ));
        }
        if self.non_claim != FORMAL_GROUPOID_NORMALIZATION_NON_CLAIM {
            return Err(FiniteTheoryError::Lifecycle(
                "formal normalization non-claim drifted".to_string(),
            ));
        }
        schema.verify_formal_path(&self.input)?;
        let mut current = self.input.clone();
        for step in &self.rewrite_trace {
            current = schema.apply_formal_rewrite_step(&current, step)?;
        }
        if current != self.normalized {
            return Err(FiniteTheoryError::MalformedCertificate(
                "formal rewrite trace does not produce the declared normal form".to_string(),
            ));
        }
        let expected = schema.normalize_formal_path(self.input.clone())?;
        if expected.rewrite_trace != self.rewrite_trace || expected.normalized != self.normalized {
            return Err(FiniteTheoryError::MalformedCertificate(
                "formal rewrite trace is not the deterministic leftmost normalization".to_string(),
            ));
        }
        Ok(self.normalized.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TypedPathRefinementHandleIr {
    pub handle_id: String,
    pub hole_id: String,
    pub schema_id: SchemaIdV2,
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub candidate_index: u32,
    pub candidate: SchemaPathIr,
    pub expected_lifecycle: CheckedLifecycleStateIr,
}

impl TypedPathRefinementHandleIr {
    fn new(
        hole_id: &str,
        schema_id: &SchemaIdV2,
        source: &SchemaObjectRefIr,
        target: &SchemaObjectRefIr,
        candidate_index: u32,
        candidate: SchemaPathIr,
    ) -> Self {
        let payload = serde_json::to_string(&(
            hole_id,
            schema_id,
            source,
            target,
            candidate_index,
            &candidate,
            CheckedLifecycleStateIr::Residual,
        ))
        .unwrap_or_else(|_| format!("{hole_id}:{candidate_index}"));
        Self {
            handle_id: format!(
                "typed-path-refinement:{}",
                crate::revision_digest_v2(&payload)
            ),
            hole_id: hole_id.to_string(),
            schema_id: schema_id.clone(),
            source: source.clone(),
            target: target.clone(),
            candidate_index,
            candidate,
            expected_lifecycle: CheckedLifecycleStateIr::Residual,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TypedPathHoleIr {
    pub hole_id: String,
    pub schema_id: SchemaIdV2,
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub candidate_handles: Vec<TypedPathRefinementHandleIr>,
    pub lifecycle: CheckedLifecycleStateIr,
    pub residual_obligations: Vec<TheoryResidualObligationIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CheckedPathSelectionIr {
    pub hole_id: String,
    pub refinement_handle_id: String,
    pub schema_id: SchemaIdV2,
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub selected: SchemaPathIr,
    pub lifecycle: CheckedLifecycleStateIr,
}

impl SchemaPresentationIr {
    pub fn typed_path_hole(
        &self,
        hole_id: impl Into<String>,
        source: SchemaObjectRefIr,
        target: SchemaObjectRefIr,
    ) -> Result<TypedPathHoleIr, FiniteTheoryError> {
        let hole_id = hole_id.into();
        if hole_id.trim().is_empty() {
            return Err(FiniteTheoryError::GateBlocked(
                "typed path hole id must be non-empty".to_string(),
            ));
        }
        let candidate_handles = self
            .category_formation
            .replay(self, &source, &target)?
            .into_iter()
            .enumerate()
            .map(|(index, candidate)| {
                TypedPathRefinementHandleIr::new(
                    &hole_id,
                    &self.schema_id,
                    &source,
                    &target,
                    index as u32,
                    candidate,
                )
            })
            .collect::<Vec<_>>();
        let repair_handle_ids = candidate_handles
            .iter()
            .map(|handle| handle.handle_id.clone())
            .collect();
        Ok(TypedPathHoleIr {
            residual_obligations: vec![TheoryResidualObligationIr::new(
                format!("typed-path-hole:{hole_id}"),
                TheoryResidualKindIr::TypedHole,
                if candidate_handles.is_empty() {
                    "no path is reachable in the exact finite generator closure"
                } else {
                    "a typed path candidate exists but has not been selected"
                },
            )
            .endpoints(source.clone(), target.clone())
            .repair_handles(repair_handle_ids)],
            hole_id,
            schema_id: self.schema_id.clone(),
            source,
            target,
            candidate_handles,
            lifecycle: CheckedLifecycleStateIr::Residual,
        })
    }
}

impl TypedPathHoleIr {
    pub fn select(
        &self,
        schema: &SchemaPresentationIr,
        handle: &TypedPathRefinementHandleIr,
    ) -> Result<CheckedPathSelectionIr, FiniteTheoryError> {
        let expected_residual_id = format!("typed-path-hole:{}", self.hole_id);
        let residual_matches = self.residual_obligations.iter().any(|residual| {
            residual.obligation_id == expected_residual_id
                && residual.kind == TheoryResidualKindIr::TypedHole
                && residual.source.as_ref() == Some(&self.source)
                && residual.target.as_ref() == Some(&self.target)
                && residual.repair_handle_ids.contains(&handle.handle_id)
        });
        if self.schema_id != schema.schema_id
            || self.lifecycle != CheckedLifecycleStateIr::Residual
            || !residual_matches
            || handle.expected_lifecycle != CheckedLifecycleStateIr::Residual
        {
            return Err(FiniteTheoryError::Lifecycle(format!(
                "typed path hole `{}` has inconsistent schema, residual, handle, or lifecycle evidence",
                self.hole_id
            )));
        }
        schema.category_formation.verify(schema)?;
        let current = schema.typed_path_hole(
            self.hole_id.clone(),
            self.source.clone(),
            self.target.clone(),
        )?;
        let expected_handle = current
            .candidate_handles
            .get(handle.candidate_index as usize)
            .ok_or_else(|| {
                FiniteTheoryError::GateBlocked(format!(
                    "typed path hole `{}` has no candidate {}",
                    self.hole_id, handle.candidate_index
                ))
            })?;
        if !self.candidate_handles.contains(handle) || expected_handle != handle {
            return Err(FiniteTheoryError::Lifecycle(format!(
                "typed path refinement handle `{}` does not match the current hole",
                handle.handle_id
            )));
        }
        schema.verify_path(&handle.candidate)?;
        if handle.candidate.source != self.source || handle.candidate.target != self.target {
            return Err(FiniteTheoryError::EndpointMismatch);
        }
        Ok(CheckedPathSelectionIr {
            hole_id: self.hole_id.clone(),
            refinement_handle_id: handle.handle_id.clone(),
            schema_id: self.schema_id.clone(),
            source: self.source.clone(),
            target: self.target.clone(),
            selected: handle.candidate.clone(),
            lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleIndexBindingIr {
    pub local_role_id: RoleIdV2,
    pub local_declared_order: u32,
    pub local_target: SchemaObjectRefIr,
    pub local_kind: RoleKindIr,
    pub value: TypedValueIr,
    /// For relation-valued families this is the corresponding projection on
    /// the referenced relation object. Object-valued families are constant
    /// over the local finite index and therefore leave it empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_role_id: Option<RoleIdV2>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FiniteRoleFiberIr {
    pub carrier: SchemaObjectRefIr,
    pub index_bindings: Vec<RoleIndexBindingIr>,
    /// Closed finite predicates whose membership was replayed against this
    /// exact fiber, not merely copied from the role declaration.
    pub refinements: Vec<crate::RefinementPredicateIr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleIndexedWitnessIr {
    pub instance_id: InstanceIdV2,
    pub fact_id: FactIdV2,
    pub relation_id: RelationIdV2,
    pub role_id: RoleIdV2,
    pub declared_order: u32,
    pub kind: RoleKindIr,
    pub fiber: FiniteRoleFiberIr,
    pub value: TypedValueIr,
}

/// Membership of one element in one carrier of the exact finite instance.
///
/// This is replayable Rust evidence tied to an `InstanceIdV2`; it is not a
/// proof term and is not covered by `category_kernel_v3`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectMembershipWitnessIr {
    pub instance_id: InstanceIdV2,
    pub object: SchemaObjectRefIr,
    pub member: TypedValueIr,
    pub lifecycle: CheckedLifecycleStateIr,
}

/// A successful decision-procedure result for one finite-model constraint.
/// Unsupported or review-only constraints never receive this witness.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TypedConstraintWitnessIr {
    pub instance_id: InstanceIdV2,
    pub theory_id: TheoryIdV2,
    pub constraint_id: ConstraintIdV2,
    pub label: String,
    pub relation_id: RelationIdV2,
    pub role_ids: Vec<RoleIdV2>,
    pub lifecycle: CheckedLifecycleStateIr,
    pub decision_procedure: String,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ScopeAxisIr {
    Context,
    World,
    Temporal,
}

impl RoleKindIr {
    pub fn scope_axis(self) -> Option<ScopeAxisIr> {
        match self {
            Self::Context => Some(ScopeAxisIr::Context),
            Self::World => Some(ScopeAxisIr::World),
            Self::Temporal => Some(ScopeAxisIr::Temporal),
            Self::Data | Self::Parameter | Self::Evidence => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeWitnessIr {
    pub axis: ScopeAxisIr,
    pub witness: RoleIndexedWitnessIr,
}

/// One finite context/world/temporal index together with the exact facts that
/// are visible through that index in the compiled instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DependentContextIr {
    pub context_id: String,
    pub instance_id: InstanceIdV2,
    pub axis: ScopeAxisIr,
    pub object: SchemaObjectRefIr,
    pub value: TypedValueIr,
    pub membership: ObjectMembershipWitnessIr,
    pub scope_witnesses: Vec<ScopeWitnessIr>,
    pub lifecycle: CheckedLifecycleStateIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<TheoryResidualObligationIr>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ScopeTransportWitnessIr {
    pub schema_id: SchemaIdV2,
    pub instance_id: InstanceIdV2,
    pub axis: ScopeAxisIr,
    pub source: RoleIndexedWitnessIr,
    pub target: RoleIndexedWitnessIr,
    pub lifecycle: CheckedLifecycleStateIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<TheoryResidualObligationIr>,
    pub basis: String,
}

fn fact_role_value<'a>(
    fact: &'a crate::RelationFactIr,
    role_id: &RoleIdV2,
) -> Option<&'a TypedValueIr> {
    fact.ordered_role_values
        .iter()
        .find(|value| &value.role_id == role_id)
        .map(|value| &value.value)
}

fn role_index_bindings_for_fact(
    schema: &SchemaPresentationIr,
    relation: &crate::RelationObjectIr,
    role: &crate::RoleProjectionIr,
    fact: &crate::RelationFactIr,
    facts: &[crate::RelationFactIr],
) -> Result<Vec<RoleIndexBindingIr>, FiniteTheoryError> {
    let fact_by_id = facts
        .iter()
        .map(|candidate| (candidate.fact_id.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let target_relation = match role.type_expr.carrier() {
        SchemaObjectRefIr::RelationObject { ref relation_id } => Some(
            schema
                .relations
                .iter()
                .find(|candidate| candidate.relation_id == *relation_id)
                .ok_or_else(|| FiniteTheoryError::IndexedFiberMismatch {
                    relation_id: relation.relation_id.clone(),
                    role_id: role.role_id.clone(),
                    detail: "target relation object is absent from the presentation".to_string(),
                })?,
        ),
        SchemaObjectRefIr::ObjectType { .. } => None,
    };
    let referenced_fact = if let Some(target_relation) = target_relation {
        let value = fact_role_value(fact, &role.role_id).ok_or_else(|| {
            FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: "indexed role value is absent from its fact".to_string(),
            }
        })?;
        let TypedValueIr::RelationFact { fact_id } = value else {
            return Err(FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: "relation-valued indexed role does not contain a typed fact id".to_string(),
            });
        };
        let target_fact = fact_by_id.get(fact_id).copied().ok_or_else(|| {
            FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: format!("referenced fact `{fact_id}` is absent from the finite model"),
            }
        })?;
        if target_fact.relation_id != target_relation.relation_id {
            return Err(FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: format!("referenced fact `{fact_id}` belongs to a different relation"),
            });
        }
        Some(target_fact)
    } else {
        None
    };

    let mut bindings = Vec::new();
    let mut seen = BTreeSet::new();
    for local_role_id in role.type_expr.index_roles() {
        if !seen.insert(local_role_id.clone()) {
            return Err(FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: format!("index role `{local_role_id}` is repeated"),
            });
        }
        let local_role = relation
            .roles
            .iter()
            .find(|candidate| candidate.role_id == local_role_id)
            .ok_or_else(|| FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: format!("index role `{local_role_id}` is absent"),
            })?;
        if local_role.declared_order >= role.declared_order {
            return Err(FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: format!("index role `{local_role_id}` is not earlier"),
            });
        }
        let value = fact_role_value(fact, &local_role.role_id)
            .cloned()
            .ok_or_else(|| FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: format!("index role `{local_role_id}` has no value"),
            })?;
        let target_role_id = if let (Some(target_relation), Some(target_fact)) =
            (target_relation, referenced_fact)
        {
            let target_role = target_relation
                .roles
                .iter()
                .find(|candidate| candidate.label == local_role.label)
                .ok_or_else(|| FiniteTheoryError::IndexedFiberMismatch {
                    relation_id: relation.relation_id.clone(),
                    role_id: role.role_id.clone(),
                    detail: format!(
                        "target relation `{}` has no `{}` projection",
                        target_relation.label, local_role.label
                    ),
                })?;
            let target_value =
                fact_role_value(target_fact, &target_role.role_id).ok_or_else(|| {
                    FiniteTheoryError::IndexedFiberMismatch {
                        relation_id: relation.relation_id.clone(),
                        role_id: role.role_id.clone(),
                        detail: format!(
                            "referenced fact lacks target projection `{}.{}`",
                            target_relation.label, target_role.label
                        ),
                    }
                })?;
            if target_value != &value {
                return Err(FiniteTheoryError::IndexedFiberMismatch {
                    relation_id: relation.relation_id.clone(),
                    role_id: role.role_id.clone(),
                    detail: format!(
                        "local index `{}={}` does not match referenced fact fiber value `{}`",
                        local_role.label,
                        value.wire_value(),
                        target_value.wire_value()
                    ),
                });
            }
            Some(target_role.role_id.clone())
        } else {
            None
        };
        bindings.push(RoleIndexBindingIr {
            local_role_id: local_role.role_id.clone(),
            local_declared_order: local_role.declared_order,
            local_target: local_role.type_expr.carrier(),
            local_kind: local_role.kind,
            value,
            target_role_id,
        });
    }
    Ok(bindings)
}

pub(crate) fn finite_role_fiber_values(
    schema: &SchemaPresentationIr,
    instance: &crate::InstanceModelIr,
    relation: &crate::RelationObjectIr,
    role: &crate::RoleProjectionIr,
    fact: &crate::RelationFactIr,
) -> Result<BTreeSet<String>, FiniteTheoryError> {
    let bindings = role_index_bindings_for_fact(schema, relation, role, fact, &instance.facts)?;
    match role.type_expr.carrier() {
        SchemaObjectRefIr::ObjectType { ref object_type_id } => instance
            .carriers
            .iter()
            .find(|carrier| {
                carrier.object
                    == SchemaObjectRefIr::ObjectType {
                        object_type_id: object_type_id.clone(),
                    }
            })
            .map(|carrier| carrier.elements.iter().cloned().collect())
            .ok_or_else(|| FiniteTheoryError::IndexedFiberMismatch {
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                detail: "object-valued fiber carrier is absent".to_string(),
            }),
        SchemaObjectRefIr::RelationObject { relation_id } => Ok(instance
            .facts
            .iter()
            .filter(|candidate| candidate.relation_id == relation_id)
            .filter(|candidate| {
                bindings.iter().all(|binding| {
                    binding
                        .target_role_id
                        .as_ref()
                        .is_some_and(|target_role_id| {
                            fact_role_value(candidate, target_role_id) == Some(&binding.value)
                        })
                })
            })
            .map(|candidate| candidate.fact_id.to_string())
            .collect()),
    }
}

pub(crate) fn build_dependent_witnesses(
    instance_id: &InstanceIdV2,
    schema: &SchemaPresentationIr,
    facts: &[crate::RelationFactIr],
) -> Result<(Vec<RoleIndexedWitnessIr>, Vec<ScopeWitnessIr>), FiniteTheoryError> {
    let relations = schema
        .relations
        .iter()
        .map(|relation| (relation.relation_id.clone(), relation))
        .collect::<BTreeMap<_, _>>();
    let mut role_witnesses = Vec::new();
    let mut scope_witnesses = Vec::new();
    for fact in facts {
        let relation = relations.get(&fact.relation_id).ok_or_else(|| {
            FiniteTheoryError::Lifecycle(format!(
                "fact `{}` references a relation outside its schema",
                fact.fact_id
            ))
        })?;
        if fact.ordered_role_values.len() != relation.roles.len() {
            return Err(FiniteTheoryError::WitnessMismatch(fact.fact_id.to_string()));
        }
        for (order, (role, value)) in relation
            .roles
            .iter()
            .zip(fact.ordered_role_values.iter())
            .enumerate()
        {
            if role.declared_order != order as u32 || role.role_id != value.role_id {
                return Err(FiniteTheoryError::WitnessMismatch(fact.fact_id.to_string()));
            }
            let witness = RoleIndexedWitnessIr {
                instance_id: instance_id.clone(),
                fact_id: fact.fact_id.clone(),
                relation_id: relation.relation_id.clone(),
                role_id: role.role_id.clone(),
                declared_order: role.declared_order,
                kind: role.kind,
                fiber: FiniteRoleFiberIr {
                    carrier: role.type_expr.carrier(),
                    index_bindings: role_index_bindings_for_fact(
                        schema, relation, role, fact, facts,
                    )?,
                    refinements: role.type_expr.refinements(),
                },
                value: value.value.clone(),
            };
            if let Some(axis) = role.kind.scope_axis() {
                scope_witnesses.push(ScopeWitnessIr {
                    axis,
                    witness: witness.clone(),
                });
            }
            role_witnesses.push(witness);
        }
    }
    Ok((role_witnesses, scope_witnesses))
}

pub(crate) fn build_object_membership_witnesses(
    instance_id: &InstanceIdV2,
    carriers: &[crate::CarrierInterpretationIr],
    facts: &[crate::RelationFactIr],
) -> Vec<ObjectMembershipWitnessIr> {
    let mut witnesses = Vec::new();
    for carrier in carriers {
        if matches!(carrier.object, SchemaObjectRefIr::RelationObject { .. }) {
            continue;
        }
        for member in &carrier.elements {
            witnesses.push(ObjectMembershipWitnessIr {
                instance_id: instance_id.clone(),
                object: carrier.object.clone(),
                member: TypedValueIr::ObjectElement {
                    value: member.clone(),
                },
                lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
            });
        }
    }
    for fact in facts {
        witnesses.push(ObjectMembershipWitnessIr {
            instance_id: instance_id.clone(),
            object: SchemaObjectRefIr::RelationObject {
                relation_id: fact.relation_id.clone(),
            },
            member: TypedValueIr::RelationFact {
                fact_id: fact.fact_id.clone(),
            },
            lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
        });
    }
    witnesses
}

fn constraint_relation_name(constraint: &ConstraintV1) -> Option<&str> {
    match constraint {
        ConstraintV1::Functional { relation, .. }
        | ConstraintV1::AtMost { relation, .. }
        | ConstraintV1::Typing { relation, .. }
        | ConstraintV1::SymmetricWhereIn { relation, .. }
        | ConstraintV1::Symmetric { relation, .. }
        | ConstraintV1::Transitive { relation, .. }
        | ConstraintV1::Key { relation, .. } => Some(relation),
        ConstraintV1::NamedBlock { .. } | ConstraintV1::Unknown { .. } => None,
    }
}

fn constraint_role_names(
    constraint: &ConstraintV1,
    relation: &crate::RelationObjectIr,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    match constraint {
        ConstraintV1::Functional {
            src_field,
            dst_field,
            ..
        } => {
            names.insert(src_field.clone());
            names.insert(dst_field.clone());
        }
        ConstraintV1::AtMost {
            src_field,
            dst_field,
            params,
            ..
        } => {
            names.insert(src_field.clone());
            names.insert(dst_field.clone());
            names.extend(params.iter().flatten().cloned());
        }
        ConstraintV1::Key { fields, .. } => names.extend(fields.iter().cloned()),
        ConstraintV1::Symmetric {
            carriers, params, ..
        }
        | ConstraintV1::Transitive {
            carriers, params, ..
        } => {
            if let Some(carriers) = carriers {
                names.insert(carriers.left_field.clone());
                names.insert(carriers.right_field.clone());
            } else {
                names.extend(relation.roles.iter().take(2).map(|role| role.label.clone()));
            }
            names.extend(params.iter().flatten().cloned());
        }
        ConstraintV1::SymmetricWhereIn {
            field,
            carriers,
            params,
            ..
        } => {
            names.insert(field.clone());
            if let Some(carriers) = carriers {
                names.insert(carriers.left_field.clone());
                names.insert(carriers.right_field.clone());
            } else {
                names.extend(relation.roles.iter().take(2).map(|role| role.label.clone()));
            }
            names.extend(params.iter().flatten().cloned());
        }
        ConstraintV1::Typing { .. }
        | ConstraintV1::NamedBlock { .. }
        | ConstraintV1::Unknown { .. } => {}
    }
    names
}

fn constraint_decision_procedure(constraint: &ConstraintV1) -> &'static str {
    match constraint {
        ConstraintV1::Functional { .. } => "finite_functional_v1",
        ConstraintV1::AtMost { .. } => "finite_at_most_v1",
        ConstraintV1::SymmetricWhereIn { .. } => "finite_symmetric_where_in_v1",
        ConstraintV1::Symmetric { .. } => "finite_symmetric_v1",
        ConstraintV1::Transitive { .. } => "finite_transitive_v1",
        ConstraintV1::Key { .. } => "finite_key_v1",
        ConstraintV1::Typing { .. }
        | ConstraintV1::NamedBlock { .. }
        | ConstraintV1::Unknown { .. } => "outside_finite_decision_fragment",
    }
}

pub(crate) fn build_typed_constraint_witnesses(
    instance_id: &InstanceIdV2,
    schema: &SchemaPresentationIr,
    theories: &[&crate::TypedTheoryIr],
) -> Result<Vec<TypedConstraintWitnessIr>, FiniteTheoryError> {
    let mut witnesses = Vec::new();
    for theory in theories {
        for constraint in &theory.constraints {
            if !constraint.finite_model_checked {
                continue;
            }
            let relation_name = constraint_relation_name(&constraint.source).ok_or_else(|| {
                FiniteTheoryError::Lifecycle(format!(
                    "finite constraint `{}` has no relation subject",
                    constraint.label
                ))
            })?;
            let relation = schema
                .relations
                .iter()
                .find(|relation| relation.label == relation_name)
                .ok_or_else(|| {
                    FiniteTheoryError::Lifecycle(format!(
                        "finite constraint `{}` references absent relation `{relation_name}`",
                        constraint.label
                    ))
                })?;
            let role_names = constraint_role_names(&constraint.source, relation);
            let role_ids = relation
                .roles
                .iter()
                .filter(|role| role_names.contains(&role.label))
                .map(|role| role.role_id.clone())
                .collect::<Vec<_>>();
            if role_ids.len() != role_names.len() {
                return Err(FiniteTheoryError::Lifecycle(format!(
                    "finite constraint `{}` has unresolved role subjects",
                    constraint.label
                )));
            }
            witnesses.push(TypedConstraintWitnessIr {
                instance_id: instance_id.clone(),
                theory_id: theory.theory_id.clone(),
                constraint_id: constraint.constraint_id.clone(),
                label: constraint.label.clone(),
                relation_id: relation.relation_id.clone(),
                role_ids,
                lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
                decision_procedure: constraint_decision_procedure(&constraint.source).to_string(),
                non_claims: vec![
                    "successful Rust finite-model checking is not Lean certification".to_string(),
                    "the witness covers this exact finite instance and constraint only".to_string(),
                ],
            });
        }
    }
    Ok(witnesses)
}

pub(crate) fn build_dependent_contexts(
    instance_id: &InstanceIdV2,
    memberships: &[ObjectMembershipWitnessIr],
    scopes: &[ScopeWitnessIr],
) -> Result<Vec<DependentContextIr>, FiniteTheoryError> {
    let mut grouped = BTreeMap::<
        (ScopeAxisIr, SchemaObjectRefIr, String),
        (ObjectMembershipWitnessIr, Vec<ScopeWitnessIr>),
    >::new();
    for scope in scopes {
        let object = scope.witness.fiber.carrier.clone();
        let membership = memberships
            .iter()
            .find(|membership| {
                membership.instance_id == *instance_id
                    && membership.object == object
                    && membership.member == scope.witness.value
                    && membership.lifecycle == CheckedLifecycleStateIr::ExplanationVerified
            })
            .cloned()
            .ok_or_else(|| {
                FiniteTheoryError::WitnessMismatch(format!(
                    "scope value `{}` has no exact object-membership witness",
                    scope.witness.value.wire_value()
                ))
            })?;
        let key = (scope.axis, object, scope.witness.value.wire_value());
        grouped
            .entry(key)
            .or_insert_with(|| (membership, Vec::new()))
            .1
            .push(scope.clone());
    }
    grouped
        .into_iter()
        .map(
            |((axis, object, wire_value), (membership, scope_witnesses))| {
                let digest_input = serde_json::to_string(&(
                    instance_id,
                    axis,
                    &object,
                    &wire_value,
                    &scope_witnesses,
                ))
                .map_err(|error| FiniteTheoryError::Lifecycle(error.to_string()))?;
                Ok(DependentContextIr {
                    context_id: format!(
                        "dependent-context:{}",
                        crate::revision_digest_v2(&digest_input)
                    ),
                    instance_id: instance_id.clone(),
                    axis,
                    object,
                    value: membership.member.clone(),
                    membership,
                    scope_witnesses,
                    lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
                    residual_obligations: Vec::new(),
                    non_claims: vec![
                    "context visibility is exact only for the compiled finite instance"
                        .to_string(),
                    "no sheaf condition, global context closure, or Lean transport proof is claimed"
                        .to_string(),
                ],
                })
            },
        )
        .collect()
}

impl ScopeWitnessIr {
    fn verify_in(
        &self,
        schema: &SchemaPresentationIr,
        theories: &[&crate::TypedTheoryIr],
        instance: &crate::InstanceModelIr,
    ) -> Result<(), FiniteTheoryError> {
        if instance.schema_id != schema.schema_id
            || self.witness.instance_id != instance.instance_id
            || self.axis
                != self
                    .witness
                    .kind
                    .scope_axis()
                    .ok_or_else(|| FiniteTheoryError::WitnessMismatch(instance.label.clone()))?
        {
            return Err(FiniteTheoryError::WitnessMismatch(instance.label.clone()));
        }
        crate::validate_instance_model_ir(schema, theories, instance)
            .map_err(|error| FiniteTheoryError::Lifecycle(error.to_string()))?;
        let (_, scopes) =
            build_dependent_witnesses(&instance.instance_id, schema, &instance.facts)?;
        if !scopes.contains(self) {
            return Err(FiniteTheoryError::WitnessMismatch(instance.label.clone()));
        }
        Ok(())
    }

    pub fn identity_transport(
        &self,
        schema: &SchemaPresentationIr,
        theories: &[&crate::TypedTheoryIr],
        instance: &crate::InstanceModelIr,
    ) -> Result<ScopeTransportWitnessIr, FiniteTheoryError> {
        self.verify_in(schema, theories, instance)?;
        Ok(ScopeTransportWitnessIr {
            schema_id: schema.schema_id.clone(),
            instance_id: instance.instance_id.clone(),
            axis: self.axis,
            source: self.witness.clone(),
            target: self.witness.clone(),
            lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
            residual_obligations: Vec::new(),
            basis: "identity_transport".to_string(),
        })
    }

    pub fn transport_obligation_to(
        &self,
        schema: &SchemaPresentationIr,
        theories: &[&crate::TypedTheoryIr],
        instance: &crate::InstanceModelIr,
        target: &ScopeWitnessIr,
    ) -> Result<ScopeTransportWitnessIr, FiniteTheoryError> {
        self.verify_in(schema, theories, instance)?;
        target.verify_in(schema, theories, instance)?;
        if self.axis != target.axis {
            return Err(FiniteTheoryError::UnsupportedTransport(format!(
                "scope axes differ: {:?} -> {:?}",
                self.axis, target.axis
            )));
        }
        if self == target {
            return self.identity_transport(schema, theories, instance);
        }
        let obligation_id = format!(
            "scope-transport:{:?}:{}:{}",
            self.axis, self.witness.fact_id, target.witness.fact_id
        );
        Ok(ScopeTransportWitnessIr {
            schema_id: schema.schema_id.clone(),
            instance_id: instance.instance_id.clone(),
            axis: self.axis,
            source: self.witness.clone(),
            target: target.witness.clone(),
            lifecycle: CheckedLifecycleStateIr::Residual,
            residual_obligations: vec![TheoryResidualObligationIr::new(
                obligation_id,
                TheoryResidualKindIr::UnsupportedTransport,
                "non-identity scope transport requires an explicit declared transport rule",
            )
            .endpoints(
                self.witness.fiber.carrier.clone(),
                target.witness.fiber.carrier.clone(),
            )],
            basis: "explicit_transport_rule_required".to_string(),
        })
    }

    pub fn transport_to(
        &self,
        schema: &SchemaPresentationIr,
        theories: &[&crate::TypedTheoryIr],
        instance: &crate::InstanceModelIr,
        target: &ScopeWitnessIr,
    ) -> Result<ScopeTransportWitnessIr, FiniteTheoryError> {
        let attempt = self.transport_obligation_to(schema, theories, instance, target)?;
        if attempt.lifecycle == CheckedLifecycleStateIr::ExplanationVerified
            && attempt.residual_obligations.is_empty()
        {
            return Ok(attempt);
        }
        Err(FiniteTheoryError::UnsupportedTransport(format!(
            "{:?} -> {:?} requires an explicit declared transport rule",
            self.witness.value, target.witness.value
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelArrowV3 {
    pub name: String,
    pub source: u32,
    pub target: u32,
    pub kind: crate::SchemaGeneratorKindIr,
    pub reversible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelRoleV3 {
    pub name: String,
    pub target: u32,
    pub projection: u32,
    pub declared_order: u32,
    pub kind: RoleKindIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelRelationV3 {
    pub name: String,
    pub object: u32,
    pub roles: Vec<CategoryKernelRoleV3>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelPathV3 {
    pub source: u32,
    pub target: u32,
    pub arrows: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelFormalStepV3 {
    pub arrow: u32,
    pub direction: FormalDirectionIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelFormalPathV3 {
    pub source: u32,
    pub target: u32,
    pub steps: Vec<CategoryKernelFormalStepV3>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelFormalRewriteStepV3 {
    pub offset: u32,
    pub arrow: u32,
    pub first_direction: FormalDirectionIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelFormalNormalizationV3 {
    pub input: CategoryKernelFormalPathV3,
    pub rewrite_trace: Vec<CategoryKernelFormalRewriteStepV3>,
    pub normalized: CategoryKernelFormalPathV3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelEquationV3 {
    pub name: String,
    pub lhs: CategoryKernelPathV3,
    pub rhs: CategoryKernelPathV3,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelPresentationV3 {
    pub object_names: Vec<String>,
    pub arrows: Vec<CategoryKernelArrowV3>,
    pub relations: Vec<CategoryKernelRelationV3>,
    pub identity_objects: Vec<u32>,
    pub equations: Vec<CategoryKernelEquationV3>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelCongruenceStepV3 {
    pub equation: u32,
    pub direction: EquationDirectionIr,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CategoryKernelCongruenceCertificateV3 {
    pub input: CategoryKernelPathV3,
    pub steps: Vec<CategoryKernelCongruenceStepV3>,
    pub output: CategoryKernelPathV3,
}

fn category_kernel_path_v3(
    schema: &SchemaPresentationIr,
    path: &SchemaPathIr,
    objects: &BTreeMap<SchemaObjectRefIr, u32>,
    arrows: &BTreeMap<SchemaGeneratorRefIr, u32>,
) -> Result<CategoryKernelPathV3, FiniteTheoryError> {
    schema.verify_path(path)?;
    let source = objects
        .get(&path.source)
        .copied()
        .ok_or_else(|| FiniteTheoryError::UnknownObject(path.source.clone()))?;
    let target = objects
        .get(&path.target)
        .copied()
        .ok_or_else(|| FiniteTheoryError::UnknownObject(path.target.clone()))?;
    let arrows = path
        .steps
        .iter()
        .map(|step| {
            arrows
                .get(step)
                .copied()
                .ok_or_else(|| FiniteTheoryError::UnknownGenerator(step.clone()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CategoryKernelPathV3 {
        source,
        target,
        arrows,
    })
}

fn category_kernel_formal_path_v3(
    schema: &SchemaPresentationIr,
    path: &FormalGroupoidPathIr,
    objects: &BTreeMap<SchemaObjectRefIr, u32>,
    arrows: &BTreeMap<SchemaGeneratorRefIr, u32>,
) -> Result<CategoryKernelFormalPathV3, FiniteTheoryError> {
    schema.verify_formal_path(path)?;
    Ok(CategoryKernelFormalPathV3 {
        source: objects
            .get(&path.source)
            .copied()
            .ok_or_else(|| FiniteTheoryError::UnknownObject(path.source.clone()))?,
        target: objects
            .get(&path.target)
            .copied()
            .ok_or_else(|| FiniteTheoryError::UnknownObject(path.target.clone()))?,
        steps: path
            .steps
            .iter()
            .map(|step| {
                Ok(CategoryKernelFormalStepV3 {
                    arrow: arrows.get(&step.generator).copied().ok_or_else(|| {
                        FiniteTheoryError::UnknownGenerator(step.generator.clone())
                    })?,
                    direction: step.direction,
                })
            })
            .collect::<Result<Vec<_>, FiniteTheoryError>>()?,
    })
}

fn category_kernel_formal_normalization_v3(
    schema: &SchemaPresentationIr,
    certificate: &FormalNormalizationCertificateIr,
    objects: &BTreeMap<SchemaObjectRefIr, u32>,
    arrows: &BTreeMap<SchemaGeneratorRefIr, u32>,
) -> Result<CategoryKernelFormalNormalizationV3, FiniteTheoryError> {
    certificate.replay(schema)?;
    Ok(CategoryKernelFormalNormalizationV3 {
        input: category_kernel_formal_path_v3(schema, &certificate.input, objects, arrows)?,
        rewrite_trace: certificate
            .rewrite_trace
            .iter()
            .map(|step| {
                Ok(CategoryKernelFormalRewriteStepV3 {
                    offset: step.offset,
                    arrow: arrows.get(&step.generator).copied().ok_or_else(|| {
                        FiniteTheoryError::UnknownGenerator(step.generator.clone())
                    })?,
                    first_direction: step.first_direction,
                })
            })
            .collect::<Result<Vec<_>, FiniteTheoryError>>()?,
        normalized: category_kernel_formal_path_v3(
            schema,
            &certificate.normalized,
            objects,
            arrows,
        )?,
    })
}

impl SchemaPresentationIr {
    /// Emit the name/index presentation that the exact-byte Lean checker
    /// independently reconstructs from canonical `.axi`. Revision-scoped ids
    /// remain in `KernelSnapshotIr`; this payload exists only to prove that
    /// Rust and Lean formed the same finite presentation.
    pub fn category_kernel_presentation_v3(
        &self,
    ) -> Result<CategoryKernelPresentationV3, FiniteTheoryError> {
        self.category_formation.verify(self)?;
        let object_refs = self.object_refs();
        let objects = object_index(&object_refs);
        let arrows = self
            .generators
            .iter()
            .enumerate()
            .map(|(index, generator)| (generator.generator_ref.clone(), index as u32))
            .collect::<BTreeMap<_, _>>();
        let object_names = self
            .objects
            .iter()
            .map(|object| object.label.clone())
            .chain(self.relations.iter().map(|relation| relation.label.clone()))
            .collect::<Vec<_>>();
        let arrow_manifest = self
            .generators
            .iter()
            .map(|generator| {
                Ok(CategoryKernelArrowV3 {
                    name: generator.label.clone(),
                    source: objects.get(&generator.source).copied().ok_or_else(|| {
                        FiniteTheoryError::UnknownObject(generator.source.clone())
                    })?,
                    target: objects.get(&generator.target).copied().ok_or_else(|| {
                        FiniteTheoryError::UnknownObject(generator.target.clone())
                    })?,
                    kind: generator.kind,
                    reversible: generator.reversible,
                })
            })
            .collect::<Result<Vec<_>, FiniteTheoryError>>()?;
        let relations = self
            .relations
            .iter()
            .map(|relation| {
                let object_ref = SchemaObjectRefIr::RelationObject {
                    relation_id: relation.relation_id.clone(),
                };
                let roles = relation
                    .roles
                    .iter()
                    .map(|role| {
                        let projection_ref = SchemaGeneratorRefIr::RoleProjection {
                            role_id: role.role_id.clone(),
                        };
                        Ok(CategoryKernelRoleV3 {
                            name: role.label.clone(),
                            target: objects.get(&role.type_expr.carrier()).copied().ok_or_else(
                                || FiniteTheoryError::UnknownObject(role.type_expr.carrier()),
                            )?,
                            projection: arrows.get(&projection_ref).copied().ok_or_else(|| {
                                FiniteTheoryError::UnknownGenerator(projection_ref.clone())
                            })?,
                            declared_order: role.declared_order,
                            kind: role.kind,
                        })
                    })
                    .collect::<Result<Vec<_>, FiniteTheoryError>>()?;
                Ok(CategoryKernelRelationV3 {
                    name: relation.label.clone(),
                    object: objects
                        .get(&object_ref)
                        .copied()
                        .ok_or_else(|| FiniteTheoryError::UnknownObject(object_ref.clone()))?,
                    roles,
                })
            })
            .collect::<Result<Vec<_>, FiniteTheoryError>>()?;
        let identity_objects = self
            .category_formation
            .identities
            .iter()
            .map(|identity| {
                self.verify_path(&identity.path)?;
                if identity.path.source != identity.object
                    || identity.path.target != identity.object
                    || !identity.path.steps.is_empty()
                {
                    return Err(FiniteTheoryError::Lifecycle(
                        "category identity payload is not an empty endomorphism".to_string(),
                    ));
                }
                objects
                    .get(&identity.object)
                    .copied()
                    .ok_or_else(|| FiniteTheoryError::UnknownObject(identity.object.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let equations = self
            .equations
            .iter()
            .map(|equation| {
                Ok(CategoryKernelEquationV3 {
                    name: equation.label.clone(),
                    lhs: category_kernel_path_v3(self, &equation.lhs, &objects, &arrows)?,
                    rhs: category_kernel_path_v3(self, &equation.rhs, &objects, &arrows)?,
                })
            })
            .collect::<Result<Vec<_>, FiniteTheoryError>>()?;
        Ok(CategoryKernelPresentationV3 {
            object_names,
            arrows: arrow_manifest,
            relations,
            identity_objects,
            equations,
        })
    }

    pub fn category_kernel_congruence_v3(
        &self,
    ) -> Result<Vec<CategoryKernelCongruenceCertificateV3>, FiniteTheoryError> {
        self.category_formation.verify(self)?;
        let objects = object_index(&self.object_refs());
        let arrows = self
            .generators
            .iter()
            .enumerate()
            .map(|(index, generator)| (generator.generator_ref.clone(), index as u32))
            .collect::<BTreeMap<_, _>>();
        let equations = self
            .equations
            .iter()
            .enumerate()
            .map(|(index, equation)| (equation.equation_id.clone(), index as u32))
            .collect::<BTreeMap<_, _>>();
        self.congruence_witnesses
            .iter()
            .map(|certificate| {
                certificate.replay(self)?;
                Ok(CategoryKernelCongruenceCertificateV3 {
                    input: category_kernel_path_v3(self, &certificate.input, &objects, &arrows)?,
                    steps: certificate
                        .steps
                        .iter()
                        .map(|step| {
                            Ok(CategoryKernelCongruenceStepV3 {
                                equation: equations.get(&step.equation_id).copied().ok_or_else(
                                    || FiniteTheoryError::UnknownEquation(step.equation_id.clone()),
                                )?,
                                direction: step.direction,
                                offset: step.offset,
                            })
                        })
                        .collect::<Result<Vec<_>, FiniteTheoryError>>()?,
                    output: category_kernel_path_v3(self, &certificate.output, &objects, &arrows)?,
                })
            })
            .collect()
    }

    /// Emit exact, index-based cancellation traces for both formal inverse
    /// words of every presented generator. These are untrusted wire-replay
    /// inputs; they do not assert executable inverse traversal or a Lean
    /// denotation theorem.
    pub fn category_kernel_groupoid_normalizations_v3(
        &self,
    ) -> Result<Vec<CategoryKernelFormalNormalizationV3>, FiniteTheoryError> {
        self.category_formation.verify(self)?;
        let objects = object_index(&self.object_refs());
        let arrows = self
            .generators
            .iter()
            .enumerate()
            .map(|(index, generator)| (generator.generator_ref.clone(), index as u32))
            .collect::<BTreeMap<_, _>>();
        let mut certificates = Vec::with_capacity(self.generators.len().saturating_mul(2));
        for generator in &self.generators {
            for first_direction in [FormalDirectionIr::Forward, FormalDirectionIr::Inverse] {
                let left = self.formal_generator_path(&generator.generator_ref, first_direction)?;
                let right = self
                    .formal_generator_path(&generator.generator_ref, first_direction.opposite())?;
                let input = self.compose_formal_paths(&left, &right)?;
                let certificate = self.normalize_formal_path(input)?;
                certificates.push(category_kernel_formal_normalization_v3(
                    self,
                    &certificate,
                    &objects,
                    &arrows,
                )?);
            }
        }
        Ok(certificates)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FiniteTheoryGateConsumerIr {
    Authoring,
    Query,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SchemaTheoryReceiptIr {
    pub schema_id: SchemaIdV2,
    pub lifecycle: CheckedLifecycleStateIr,
    pub object_count: u32,
    pub generator_count: u32,
    pub equation_count: u32,
    pub identity_path_count: u32,
    pub path_explanation_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation_algorithm: Option<String>,
}

/// Typed scope for the finite runtime replay. This is deliberately a scope,
/// not a synthetic category-closure or ontology-completeness claim.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FiniteTheoryScopeIr {
    pub fragment: String,
    pub category_object_bound: u32,
    pub category_generator_bound: u32,
    pub accepted_schema_count: u32,
    pub explicit_instance_count: u32,
}

/// Coverage actually replayed by one gate consumer. Counts are exact for the
/// compiled snapshot named by the enclosing receipt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FiniteTheoryCoverageIr {
    pub category_formations_replayed: u32,
    pub saturated_presentations: u32,
    pub identity_paths_replayed: u32,
    pub path_explanations_replayed: u32,
    pub object_memberships_replayed: u32,
    pub dependent_role_witnesses_replayed: u32,
    pub finite_refinement_predicates_replayed: u32,
    pub finite_constraint_witnesses_replayed: u32,
    pub dependent_contexts_replayed: u32,
    pub identity_scope_transports_replayed: u32,
    pub non_identity_scope_transports_certified: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FiniteTheoryGateReceiptIr {
    pub version: String,
    pub consumer: FiniteTheoryGateConsumerIr,
    pub accepted_snapshot_id: crate::SnapshotIdV2,
    pub kernel_ir_digest: crate::ObjectBlobIdV2,
    pub passed: bool,
    pub scope: FiniteTheoryScopeIr,
    pub coverage: FiniteTheoryCoverageIr,
    pub schema_receipts: Vec<SchemaTheoryReceiptIr>,
    pub instance_count: u32,
    pub object_membership_witness_count: u32,
    pub role_witness_count: u32,
    pub typed_constraint_witness_count: u32,
    pub dependent_context_count: u32,
    pub context_witness_count: u32,
    pub world_witness_count: u32,
    pub temporal_witness_count: u32,
    pub residual_obligations: Vec<TheoryResidualObligationIr>,
    pub non_claims: Vec<String>,
}

impl CompiledKernelSnapshot {
    pub fn finite_theory_gate_receipt(
        &self,
        consumer: FiniteTheoryGateConsumerIr,
    ) -> Result<FiniteTheoryGateReceiptIr, FiniteTheoryError> {
        let ir = self.ir();
        let mut schema_receipts = Vec::new();
        let mut residual_obligations = Vec::new();
        let mut saturated_presentations = 0_u32;
        let mut identity_paths_replayed = 0_u32;
        let mut path_explanations_replayed = 0_u32;
        for schema in ir.schemas() {
            schema.category_formation.verify(schema)?;
            residual_obligations.extend(schema.category_formation.residual_obligations.clone());
            let identity_path_count = schema.category_formation.identities.len() as u32;
            let path_explanation_count = schema
                .category_formation
                .saturation
                .as_ref()
                .map_or(0, |certificate| certificate.entries.len() as u32);
            if schema.category_formation.saturation.is_some() {
                saturated_presentations = saturated_presentations.saturating_add(1);
            }
            identity_paths_replayed = identity_paths_replayed.saturating_add(identity_path_count);
            path_explanations_replayed =
                path_explanations_replayed.saturating_add(path_explanation_count);
            schema_receipts.push(SchemaTheoryReceiptIr {
                schema_id: schema.schema_id.clone(),
                lifecycle: schema.category_formation.lifecycle,
                object_count: schema.object_refs().len() as u32,
                generator_count: schema.generators.len() as u32,
                equation_count: schema
                    .equations
                    .len()
                    .saturating_add(schema.formal_groupoid_equations.len())
                    as u32,
                identity_path_count,
                path_explanation_count,
                saturation_algorithm: schema
                    .category_formation
                    .saturation
                    .as_ref()
                    .map(|certificate| certificate.algorithm.clone()),
            });
        }
        let schemas = ir
            .schemas()
            .iter()
            .map(|schema| (schema.schema_id.clone(), schema))
            .collect::<BTreeMap<_, _>>();
        let mut object_membership_witness_count = 0_u32;
        let mut role_witness_count = 0_u32;
        let mut typed_constraint_witness_count = 0_u32;
        let mut dependent_context_count = 0_u32;
        let mut refinement_predicate_witness_count = 0_u32;
        let mut identity_scope_transport_count = 0_u32;
        let mut context_witness_count = 0_u32;
        let mut world_witness_count = 0_u32;
        let mut temporal_witness_count = 0_u32;
        for instance in ir.instances() {
            let schema = schemas.get(&instance.schema_id).ok_or_else(|| {
                FiniteTheoryError::Lifecycle(format!(
                    "instance `{}` references a missing schema",
                    instance.label
                ))
            })?;
            let theories = ir
                .theories()
                .iter()
                .filter(|theory| theory.schema_id == instance.schema_id)
                .collect::<Vec<_>>();
            crate::validate_instance_model_ir(schema, &theories, instance)
                .map_err(|error| FiniteTheoryError::Lifecycle(error.to_string()))?;
            let (roles, scopes) =
                build_dependent_witnesses(&instance.instance_id, schema, &instance.facts)?;
            let memberships = build_object_membership_witnesses(
                &instance.instance_id,
                &instance.carriers,
                &instance.facts,
            );
            let constraints =
                build_typed_constraint_witnesses(&instance.instance_id, schema, &theories)?;
            let contexts = build_dependent_contexts(&instance.instance_id, &memberships, &scopes)?;
            if instance.object_membership_witnesses != memberships
                || instance.role_witnesses != roles
                || instance.typed_constraint_witnesses != constraints
                || instance.scope_witnesses != scopes
                || instance.dependent_contexts != contexts
                || instance.lifecycle != CheckedLifecycleStateIr::ExplanationVerified
                || !instance.residual_obligations.is_empty()
            {
                return Err(FiniteTheoryError::WitnessMismatch(instance.label.clone()));
            }
            object_membership_witness_count =
                object_membership_witness_count.saturating_add(memberships.len() as u32);
            role_witness_count = role_witness_count.saturating_add(roles.len() as u32);
            refinement_predicate_witness_count = refinement_predicate_witness_count.saturating_add(
                roles
                    .iter()
                    .map(|witness| witness.fiber.refinements.len() as u32)
                    .sum::<u32>(),
            );
            typed_constraint_witness_count =
                typed_constraint_witness_count.saturating_add(constraints.len() as u32);
            dependent_context_count = dependent_context_count.saturating_add(contexts.len() as u32);
            for scope in scopes {
                let transport = scope.identity_transport(schema, &theories, instance)?;
                if transport.lifecycle != CheckedLifecycleStateIr::ExplanationVerified
                    || !transport.residual_obligations.is_empty()
                    || transport.basis != "identity_transport"
                {
                    return Err(FiniteTheoryError::Lifecycle(
                        "identity scope transport did not replay".to_string(),
                    ));
                }
                identity_scope_transport_count = identity_scope_transport_count.saturating_add(1);
                match scope.axis {
                    ScopeAxisIr::Context => {
                        context_witness_count = context_witness_count.saturating_add(1)
                    }
                    ScopeAxisIr::World => {
                        world_witness_count = world_witness_count.saturating_add(1)
                    }
                    ScopeAxisIr::Temporal => {
                        temporal_witness_count = temporal_witness_count.saturating_add(1)
                    }
                }
            }
        }
        Ok(FiniteTheoryGateReceiptIr {
            version: FINITE_THEORY_GATE_VERSION.to_string(),
            consumer,
            accepted_snapshot_id: ir.accepted_snapshot_id().clone(),
            kernel_ir_digest: ir.ir_digest().clone(),
            passed: residual_obligations.is_empty(),
            scope: FiniteTheoryScopeIr {
                fragment: "explicit_finite_category_instance_replay_v1".to_string(),
                category_object_bound: MAX_FINITE_CATEGORY_OBJECTS as u32,
                category_generator_bound: MAX_FINITE_CATEGORY_ARROWS as u32,
                accepted_schema_count: ir.schemas().len() as u32,
                explicit_instance_count: ir.instances().len() as u32,
            },
            coverage: FiniteTheoryCoverageIr {
                category_formations_replayed: ir.schemas().len() as u32,
                saturated_presentations,
                identity_paths_replayed,
                path_explanations_replayed,
                object_memberships_replayed: object_membership_witness_count,
                dependent_role_witnesses_replayed: role_witness_count,
                finite_refinement_predicates_replayed: refinement_predicate_witness_count,
                finite_constraint_witnesses_replayed: typed_constraint_witness_count,
                dependent_contexts_replayed: dependent_context_count,
                identity_scope_transports_replayed: identity_scope_transport_count,
                non_identity_scope_transports_certified: 0,
            },
            schema_receipts,
            instance_count: ir.instances().len() as u32,
            object_membership_witness_count,
            role_witness_count,
            typed_constraint_witness_count,
            dependent_context_count,
            context_witness_count,
            world_witness_count,
            temporal_witness_count,
            residual_obligations,
            non_claims: vec![
                "the Rust receipt is replay evidence, not a trusted Lean proof".to_string(),
                "the gate covers the explicit finite decidable fragment, not ontology closure"
                    .to_string(),
                "path explanation coverage is exact generator reachability, not fact or rewrite closure"
                    .to_string(),
                "only identity scope transports are replayed; non-identity transport remains uncertified"
                    .to_string(),
                "finite refinement counts cover closed predicates on this exact compiled instance only"
                    .to_string(),
            ],
        })
    }

    pub fn require_finite_theory_gate(
        &self,
        consumer: FiniteTheoryGateConsumerIr,
    ) -> Result<FiniteTheoryGateReceiptIr, FiniteTheoryError> {
        let receipt = self.finite_theory_gate_receipt(consumer)?;
        if !receipt.passed {
            return Err(FiniteTheoryError::GateBlocked(
                receipt
                    .residual_obligations
                    .iter()
                    .map(|residual| residual.message.as_str())
                    .collect::<Vec<_>>()
                    .join("; "),
            ));
        }
        Ok(receipt)
    }

    /// Emit the sole anchored category-kernel certificate accepted by the
    /// trusted `VerifyMain` import closure. Lean reconstructs the presentation
    /// from the exact `.axi` bytes before checking formation, congruence, and
    /// bounded generator reachability. The current anchor names one module, so
    /// export rejects forward equations contributed by importing modules.
    pub fn category_kernel_certificate_json(
        &self,
        schema_label: &str,
    ) -> Result<serde_json::Value, FiniteTheoryError> {
        let schema = self
            .ir()
            .schemas()
            .iter()
            .find(|schema| schema.label == schema_label)
            .ok_or_else(|| {
                FiniteTheoryError::GateBlocked(format!(
                    "unknown schema `{schema_label}` for category-kernel certificate"
                ))
            })?;
        for equation in &schema.equations {
            let theory = self
                .ir()
                .theories()
                .iter()
                .find(|theory| theory.theory_id == equation.theory_id)
                .ok_or_else(|| {
                    FiniteTheoryError::GateBlocked(format!(
                        "category equation `{}` has no compiled theory owner",
                        equation.label
                    ))
                })?;
            if theory.module_id != schema.module_id {
                return Err(FiniteTheoryError::GateBlocked(format!(
                    "category_kernel_v3 anchors one exact module; schema `{}` has forward equation `{}` from module `{}` in its import closure",
                    schema.label, equation.label, theory.module_name
                )));
            }
        }
        schema.category_formation.verify(schema)?;
        let certificate = schema
            .category_formation
            .saturation
            .as_ref()
            .ok_or_else(|| {
                FiniteTheoryError::GateBlocked(
                    "finite saturation is residual and cannot be exported".to_string(),
                )
            })?;
        Ok(json!({
            "version": 3,
            "kind": "category_kernel_v3",
            "anchor": {
                "revision_digest_v2": schema.revision,
            },
            "proof": {
                "schema_name": schema.label,
                "presentation": schema.category_kernel_presentation_v3()?,
                "congruence_certificates": schema.category_kernel_congruence_v3()?,
                "groupoid_normalizations": schema.category_kernel_groupoid_normalizations_v3()?,
                "certificate": {
                    "presentation_object_count": certificate.presentation_object_count,
                    "presentation_arrow_count": certificate.presentation_arrow_count,
                    "entries": certificate.entries,
                    "algorithm": certificate.algorithm,
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CanonicalCompiler, CanonicalModuleSource, KernelCompilationRequest, RepositoryIdV2,
        SnapshotIdV2,
    };

    const AXI: &str = r#"
module TheoryDemo
schema S:
  object A
  object B
  object Context
  relation R(left: A @data, right: B @data, ctx: Context @context)
  aspect back: B -> A @reversible

theory T on S:
  constraint functional R.left -> R.right
  equation back_after_right:
    R.right;back = R.left

instance I of S:
  A = {a}
  B = {b}
  Context = {reviewed}
  R = {r: (left=a, right=b, ctx=reviewed)}
  back = {(source=b, target=a)}
"#;

    fn compiled() -> CompiledKernelSnapshot {
        CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"finite-theory-test"),
            accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[AXI.as_bytes()]),
            root_module: "TheoryDemo".to_string(),
            modules: vec![CanonicalModuleSource::parse(AXI.as_bytes().to_vec()).unwrap()],
        })
        .unwrap()
    }

    #[test]
    fn category_formation_and_dependent_witness_gate_replay() {
        let compiled = compiled();
        for consumer in [
            FiniteTheoryGateConsumerIr::Authoring,
            FiniteTheoryGateConsumerIr::Query,
            FiniteTheoryGateConsumerIr::Merge,
        ] {
            let receipt = compiled.require_finite_theory_gate(consumer).unwrap();
            assert!(receipt.passed);
            assert_eq!(receipt.object_membership_witness_count, 4);
            assert_eq!(receipt.role_witness_count, 3);
            assert_eq!(receipt.typed_constraint_witness_count, 1);
            assert_eq!(receipt.dependent_context_count, 1);
            assert_eq!(receipt.context_witness_count, 1);
            assert_eq!(
                receipt.scope.fragment,
                "explicit_finite_category_instance_replay_v1"
            );
            assert_eq!(receipt.coverage.category_formations_replayed, 1);
            assert_eq!(receipt.coverage.saturated_presentations, 1);
            assert_eq!(receipt.coverage.identity_paths_replayed, 4);
            assert!(receipt.coverage.path_explanations_replayed >= 4);
            assert_eq!(receipt.coverage.identity_scope_transports_replayed, 1);
            assert_eq!(receipt.coverage.non_identity_scope_transports_certified, 0);
        }
        let schema = &compiled.ir().schemas()[0];
        assert_eq!(schema.category_formation.identities.len(), 4);
        assert_eq!(schema.relations[0].roles.len(), 3);
        assert_eq!(schema.relations[0].roles[1].declared_order, 1);
        let theories = compiled
            .ir()
            .theories()
            .iter()
            .filter(|theory| theory.schema_id == schema.schema_id)
            .collect::<Vec<_>>();
        let instance = &compiled.ir().instances()[0];
        assert_eq!(instance.object_membership_witnesses.len(), 4);
        assert_eq!(instance.typed_constraint_witnesses.len(), 1);
        assert_eq!(instance.dependent_contexts.len(), 1);
        assert_eq!(
            instance.typed_constraint_witnesses[0].lifecycle,
            CheckedLifecycleStateIr::ExplanationVerified
        );
        assert_eq!(
            instance.dependent_contexts[0].membership.member,
            instance.dependent_contexts[0].value
        );
        let scope = &instance.scope_witnesses[0];
        assert_eq!(
            scope
                .identity_transport(schema, &theories, instance)
                .unwrap()
                .lifecycle,
            CheckedLifecycleStateIr::ExplanationVerified
        );
        let mut undeclared_target = scope.clone();
        undeclared_target.witness.value = TypedValueIr::ObjectElement {
            value: "other-context".to_string(),
        };
        assert!(matches!(
            scope.transport_to(schema, &theories, instance, &undeclared_target),
            Err(FiniteTheoryError::WitnessMismatch(_))
        ));
    }

    #[test]
    fn endpoint_paths_congruence_and_formal_normalization_are_replayable() {
        let compiled = compiled();
        let schema = &compiled.ir().schemas()[0];
        let equation = &schema.equations[0];
        let certificate = schema
            .equation_congruence_certificate(
                equation.lhs.clone(),
                vec![EquationCongruenceStepIr {
                    equation_id: equation.equation_id.clone(),
                    direction: EquationDirectionIr::Forward,
                    offset: 0,
                }],
            )
            .unwrap();
        assert_eq!(certificate.replay(schema).unwrap(), equation.rhs);
        assert_eq!(schema.congruence_witnesses.len(), 1);
        schema.category_formation.verify(schema).unwrap();

        let mut invalid_path = equation.lhs.clone();
        invalid_path.target = invalid_path.source.clone();
        let zero_step = PathCongruenceCertificateIr {
            schema_id: schema.schema_id.clone(),
            input: invalid_path.clone(),
            steps: Vec::new(),
            output: invalid_path,
            lifecycle: CheckedLifecycleStateIr::ExplanationVerified,
        };
        assert!(matches!(
            zero_step.replay(schema),
            Err(FiniteTheoryError::EndpointMismatch)
        ));

        let generator = schema
            .generators
            .iter()
            .find(|generator| generator.label == "back")
            .unwrap();
        let path = FormalGroupoidPathIr {
            schema_id: schema.schema_id.clone(),
            source: generator.source.clone(),
            target: generator.source.clone(),
            steps: vec![
                FormalGeneratorStepIr {
                    generator: generator.generator_ref.clone(),
                    direction: FormalDirectionIr::Forward,
                },
                FormalGeneratorStepIr {
                    generator: generator.generator_ref.clone(),
                    direction: FormalDirectionIr::Inverse,
                },
            ],
        };
        let normalized = schema.normalize_formal_path(path).unwrap();
        assert!(normalized.replay(schema).unwrap().steps.is_empty());
        assert_eq!(
            normalized.rewrite_trace,
            vec![FormalGroupoidRewriteStepIr {
                offset: 0,
                generator: generator.generator_ref.clone(),
                first_direction: FormalDirectionIr::Forward,
            }]
        );

        let forward = schema
            .formal_generator_path(&generator.generator_ref, FormalDirectionIr::Forward)
            .unwrap();
        let inverse = schema.formal_inverse(&forward).unwrap();
        let source_identity = schema.formal_identity(forward.source.clone()).unwrap();
        let target_identity = schema.formal_identity(forward.target.clone()).unwrap();
        assert_eq!(
            schema
                .compose_formal_paths(&source_identity, &forward)
                .unwrap(),
            forward
        );
        assert_eq!(
            schema
                .compose_formal_paths(&forward, &target_identity)
                .unwrap(),
            forward
        );
        let first = schema
            .compose_formal_paths(&source_identity, &forward)
            .unwrap();
        let left_assoc = schema.compose_formal_paths(&first, &inverse).unwrap();
        let second = schema.compose_formal_paths(&forward, &inverse).unwrap();
        let right_assoc = schema
            .compose_formal_paths(&source_identity, &second)
            .unwrap();
        assert_eq!(left_assoc, right_assoc);
        assert!(schema
            .normalize_formal_path(left_assoc)
            .unwrap()
            .replay(schema)
            .unwrap()
            .steps
            .is_empty());

        let mut tampered_trace = normalized.clone();
        tampered_trace.rewrite_trace[0].offset = 1;
        assert!(matches!(
            tampered_trace.replay(schema),
            Err(FiniteTheoryError::MalformedCertificate(_))
        ));
        let mut tampered_output = normalized.clone();
        tampered_output
            .normalized
            .steps
            .push(FormalGeneratorStepIr {
                generator: generator.generator_ref.clone(),
                direction: FormalDirectionIr::Forward,
            });
        assert!(tampered_output.replay(schema).is_err());

        let projection = schema
            .generators
            .iter()
            .find(|generator| generator.kind == crate::SchemaGeneratorKindIr::RoleProjection)
            .unwrap();
        let formal_inverse = FormalGroupoidPathIr {
            schema_id: schema.schema_id.clone(),
            source: projection.target.clone(),
            target: projection.source.clone(),
            steps: vec![FormalGeneratorStepIr {
                generator: projection.generator_ref.clone(),
                direction: FormalDirectionIr::Inverse,
            }],
        };
        assert!(!schema
            .formal_path_runtime_executable(&formal_inverse)
            .unwrap());
    }

    #[test]
    fn holes_and_adversarial_certificates_remain_explicit() {
        let compiled = compiled();
        let schema = &compiled.ir().schemas()[0];
        let source = schema.generators[0].source.clone();
        let target = schema.generators[0].target.clone();
        let hole = schema.typed_path_hole("h", source, target).unwrap();
        assert_eq!(hole.lifecycle, CheckedLifecycleStateIr::Residual);
        assert!(!hole.candidate_handles.is_empty());
        let handle = &hole.candidate_handles[0];
        assert_eq!(
            hole.select(schema, handle).unwrap().lifecycle,
            CheckedLifecycleStateIr::ExplanationVerified
        );
        let mut forged_hole = hole.clone();
        forged_hole.lifecycle = CheckedLifecycleStateIr::ExplanationVerified;
        assert!(matches!(
            forged_hole.select(schema, handle),
            Err(FiniteTheoryError::Lifecycle(_))
        ));
        let mut tampered_handle = handle.clone();
        tampered_handle.candidate = schema.identity_path(hole.source.clone()).unwrap();
        assert!(matches!(
            hole.select(schema, &tampered_handle),
            Err(FiniteTheoryError::Lifecycle(_))
        ));
        let mut tampered_handle_id = handle.clone();
        tampered_handle_id.handle_id.push_str("-tampered");
        assert!(matches!(
            hole.select(schema, &tampered_handle_id),
            Err(FiniteTheoryError::Lifecycle(_))
        ));

        let mut tampered = schema.category_formation.clone();
        tampered.saturation.as_mut().unwrap().entries.remove(0);
        assert!(tampered.verify(schema).is_err());

        let mut identity_tampered = schema.category_formation.clone();
        identity_tampered.identities.remove(0);
        assert!(identity_tampered.verify(schema).is_err());

        let mut role_order_tampered = schema.clone();
        role_order_tampered.relations[0].roles[1].declared_order = 99;
        assert!(role_order_tampered
            .category_formation
            .verify(&role_order_tampered)
            .is_err());

        let mut generator_label_tampered = schema.clone();
        generator_label_tampered.generators[1].label =
            generator_label_tampered.generators[0].label.clone();
        assert!(generator_label_tampered
            .category_formation
            .verify(&generator_label_tampered)
            .is_err());

        let mut congruence_tampered = schema.clone();
        congruence_tampered.congruence_witnesses.clear();
        assert!(congruence_tampered
            .category_formation
            .verify(&congruence_tampered)
            .is_err());

        let mut wrong_witness = compiled.ir().instances()[0].role_witnesses[0].clone();
        wrong_witness.declared_order = 99;
        assert_ne!(
            wrong_witness,
            compiled.ir().instances()[0].role_witnesses[0]
        );

        let instance = &compiled.ir().instances()[0];
        let theories = compiled.ir().theories().iter().collect::<Vec<_>>();
        let mut membership_tampered = instance.clone();
        membership_tampered.object_membership_witnesses[0].lifecycle =
            CheckedLifecycleStateIr::Residual;
        assert!(
            crate::validate_instance_model_ir(schema, &theories, &membership_tampered).is_err()
        );

        let mut constraint_tampered = instance.clone();
        constraint_tampered.typed_constraint_witnesses[0].decision_procedure =
            "claimed_by_string".to_string();
        assert!(
            crate::validate_instance_model_ir(schema, &theories, &constraint_tampered).is_err()
        );

        let mut context_tampered = instance.clone();
        context_tampered.dependent_contexts[0]
            .scope_witnesses
            .clear();
        assert!(crate::validate_instance_model_ir(schema, &theories, &context_tampered).is_err());
    }

    #[test]
    fn explicit_but_unresolved_schema_equation_is_rejected() {
        let axi = r#"
module InvalidEquation
schema S:
  object A
  object B
  function f: A -> B

theory T on S:
  equation invalid:
    missing;f = missing;f
"#;
        let error = CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"invalid-equation"),
            accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[axi.as_bytes()]),
            root_module: "InvalidEquation".to_string(),
            modules: vec![CanonicalModuleSource::parse(axi.as_bytes().to_vec()).unwrap()],
        })
        .expect_err("explicit unknown schema paths must not become opaque equations");
        assert!(matches!(
            error,
            crate::KernelCompileError::InvalidSchemaEquation { .. }
        ));
    }

    #[test]
    fn identity_congruence_is_bound_to_the_equation_object() {
        let axi = r#"
module IdentityCongruence
schema S:
  object A
  object B

theory T on S:
  equation identity:
    id(A) = id(A)
"#;
        let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"identity-congruence"),
            accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[axi.as_bytes()]),
            root_module: "IdentityCongruence".to_string(),
            modules: vec![CanonicalModuleSource::parse(axi.as_bytes().to_vec()).unwrap()],
        })
        .expect("identity equations are valid forward equations");
        let schema = &compiled.ir().schemas()[0];
        assert_eq!(schema.equations.len(), 1);
        assert!(schema.formal_groupoid_equations.is_empty());
        let equation = &schema.equations[0];
        let object_b = schema
            .objects
            .iter()
            .find(|object| object.label == "B")
            .map(|object| SchemaObjectRefIr::ObjectType {
                object_type_id: object.object_type_id.clone(),
            })
            .unwrap();
        let wrong_identity = schema.identity_path(object_b).unwrap();
        let error = schema
            .equation_congruence_certificate(
                wrong_identity,
                vec![EquationCongruenceStepIr {
                    equation_id: equation.equation_id.clone(),
                    direction: EquationDirectionIr::Forward,
                    offset: 0,
                }],
            )
            .expect_err("id(A) must not rewrite an identity at B");
        assert_eq!(error, FiniteTheoryError::CongruenceMismatch);
    }

    #[test]
    fn category_certificate_export_rejects_imported_forward_theory_extensions() {
        let base = r#"module Base

schema Shared:
  object A
  function f: A -> A
"#;
        let extension = r#"module Extension
import Base

theory Extended on Shared:
  equation f_identity:
    f = id(A)
"#;
        let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"imported-theory-certificate"),
            accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[
                base.as_bytes(),
                extension.as_bytes(),
            ]),
            root_module: "Extension".to_string(),
            modules: vec![
                CanonicalModuleSource::parse(extension.as_bytes().to_vec()).unwrap(),
                CanonicalModuleSource::parse(base.as_bytes().to_vec()).unwrap(),
            ],
        })
        .expect("imported schema extensions are legal canonical packages");
        let error = compiled
            .category_kernel_certificate_json("Shared")
            .expect_err("one-module certificate anchors cannot cover imported equations");
        assert!(
            matches!(error, FiniteTheoryError::GateBlocked(message) if message.contains("import closure"))
        );
    }

    #[test]
    fn saturation_bound_becomes_a_blocking_residual_not_a_false_certificate() {
        let mut axi = "module Large\nschema Large:\n".to_string();
        for index in 0..=MAX_FINITE_CATEGORY_OBJECTS {
            axi.push_str(&format!("  object O{index}\n"));
        }
        let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"large-finite-theory"),
            accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[axi.as_bytes()]),
            root_module: "Large".to_string(),
            modules: vec![CanonicalModuleSource::parse(axi.into_bytes()).unwrap()],
        })
        .unwrap();
        let receipt = compiled
            .finite_theory_gate_receipt(FiniteTheoryGateConsumerIr::Authoring)
            .unwrap();
        assert!(!receipt.passed);
        assert_eq!(
            receipt.residual_obligations[0].kind,
            TheoryResidualKindIr::SaturationBound
        );
        assert!(compiled.category_kernel_certificate_json("Large").is_err());
    }

    #[test]
    fn regulated_shipment_category_and_groupoid_equations_are_checked() {
        let axi = include_str!("../../../../examples/regulated_shipment/RegulatedShipment.axi");
        let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"regulated-formal-theory"),
            accepted_snapshot_id: SnapshotIdV2::from_canonical_fields(&[axi.as_bytes()]),
            root_module: "RegulatedShipment".to_string(),
            modules: vec![CanonicalModuleSource::parse(axi.as_bytes().to_vec()).unwrap()],
        })
        .unwrap();
        let schema = &compiled.ir().schemas()[0];
        let presentation = schema.category_kernel_presentation_v3().unwrap();
        assert_eq!(presentation.object_names.len(), 23);
        assert_eq!(presentation.arrows.len(), 43);
        assert_eq!(presentation.identity_objects, (0..23).collect::<Vec<_>>());
        assert_eq!(presentation.equations.len(), 1);
        assert_eq!(
            presentation.equations[0].name,
            "lane_jurisdiction_factorization"
        );
        let congruence = schema.category_kernel_congruence_v3().unwrap();
        assert_eq!(congruence.len(), 1);
        assert_eq!(congruence[0].steps[0].offset, 1);
        let groupoid_normalizations = schema.category_kernel_groupoid_normalizations_v3().unwrap();
        assert_eq!(groupoid_normalizations.len(), presentation.arrows.len() * 2);
        assert!(groupoid_normalizations
            .iter()
            .all(|certificate| certificate.rewrite_trace.len() == 1
                && certificate.normalized.steps.is_empty()));
        let dispatch_manifest = presentation
            .relations
            .iter()
            .find(|relation| relation.name == "DispatchReview")
            .unwrap();
        let contained_batch_manifest = dispatch_manifest
            .roles
            .iter()
            .find(|role| role.name == "contained_batch")
            .unwrap();
        assert_eq!(
            presentation.object_names[contained_batch_manifest.target as usize],
            "ShipmentContainsBatch"
        );

        let equation = schema
            .formal_groupoid_equations
            .iter()
            .find(|equation| equation.label == "shipment_certificate_trace")
            .expect("regulated shipment equation must compile into formal groupoid syntax");
        assert_eq!(
            equation.lifecycle,
            CheckedLifecycleStateIr::FormationChecked
        );
        assert_eq!(equation.lhs.source, equation.rhs.source);
        assert_eq!(equation.lhs.target, equation.rhs.target);
        assert_eq!(equation.lhs.steps.len(), 4);
        assert_eq!(equation.rhs.steps.len(), 2);
        let dispatch = schema
            .relations
            .iter()
            .find(|relation| relation.label == "DispatchReview")
            .unwrap();
        let contained_batch = dispatch
            .roles
            .iter()
            .find(|role| role.label == "contained_batch")
            .unwrap();
        let reviewer = dispatch
            .roles
            .iter()
            .find(|role| role.label == "reviewer")
            .unwrap();
        let instance = &compiled.ir().instances()[0];
        assert!(instance.role_witnesses.iter().any(|witness| {
            witness.role_id == contained_batch.role_id
                && witness.fiber.index_bindings.len() == 1
                && witness.fiber.index_bindings[0].target_role_id.is_some()
        }));
        assert!(instance.role_witnesses.iter().any(|witness| {
            witness.role_id == reviewer.role_id && !witness.fiber.refinements.is_empty()
        }));
        let theories = compiled
            .ir()
            .theories()
            .iter()
            .filter(|theory| theory.schema_id == schema.schema_id)
            .collect::<Vec<_>>();
        let source_scope = &instance.scope_witnesses[0];
        let target_scope = instance
            .scope_witnesses
            .iter()
            .find(|scope| {
                scope.axis == source_scope.axis && scope.witness.value != source_scope.witness.value
            })
            .expect("regulated shipment has distinct valid values on one scope axis");
        let residual = source_scope
            .transport_obligation_to(schema, &theories, instance, target_scope)
            .expect("unsupported transport remains a typed residual");
        assert_eq!(residual.lifecycle, CheckedLifecycleStateIr::Residual);
        assert_eq!(residual.residual_obligations.len(), 1);
        assert_eq!(
            residual.residual_obligations[0].kind,
            TheoryResidualKindIr::UnsupportedTransport
        );
        assert!(matches!(
            source_scope.transport_to(schema, &theories, instance, target_scope),
            Err(FiniteTheoryError::UnsupportedTransport(_))
        ));
        assert!(
            compiled
                .require_finite_theory_gate(FiniteTheoryGateConsumerIr::Merge)
                .unwrap()
                .passed
        );
    }

    #[test]
    fn lean_category_kernel_envelope_is_anchored_and_current() {
        let envelope = compiled().category_kernel_certificate_json("S").unwrap();
        assert_eq!(envelope["version"], 3);
        assert_eq!(envelope["kind"], "category_kernel_v3");
        assert_eq!(
            envelope["proof"]["certificate"]["algorithm"],
            FINITE_SATURATION_ALGORITHM
        );
        assert_eq!(
            envelope["proof"]["presentation"]["equations"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            envelope["proof"]["congruence_certificates"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            envelope["proof"]["groupoid_normalizations"]
                .as_array()
                .unwrap()
                .len(),
            compiled().ir().schemas()[0].generators.len() * 2
        );
    }
}
