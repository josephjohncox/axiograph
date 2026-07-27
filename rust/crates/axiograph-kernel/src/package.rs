//! Canonical exact-byte compiler and immutable typed kernel IR.
//!
//! This is the only production facade allowed to turn accepted `.axi` bytes
//! into semantic IR. Parsing, formation checking, finite-model validation, and
//! deterministic import-closure ordering happen together. Rust remains an
//! untrusted runtime: a successfully compiled snapshot is not a Lean proof.

use crate::{
    ConstraintIdV2, EquationIdV2, FactIdV2, FactRoleValueV2, InstanceIdV2, KernelRefV2, ModuleIdV2,
    ObjectBlobIdV2, ObjectTypeIdV2, RelationIdV2, RepositoryIdV2, RevisionDigestV2,
    RewriteRuleIdV2, RoleIdV2, SchemaIdV2, SemanticKeyV2, SnapshotIdV2, TheoryIdV2,
};
use axiograph_dsl::{
    axi_v1::parse_axi_v1,
    schema_v1::{
        parse_path_expr_v3, CarrierFieldsV1, ConstraintV1, GeneratorKindV1, PathExprV3,
        RefinementPredicateV1, RewriteVarTypeV1, RoleKindV1, SchemaV1Instance, SchemaV1Module,
        SchemaV1Schema, SetItemV1, TypeExprV1,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use thiserror::Error;

pub const KERNEL_SNAPSHOT_IR_VERSION: &str = "kernel_snapshot_ir_v2";
pub const SCHEMA_PRESENTATION_IR_VERSION: &str = "schema_presentation_ir_v2";
pub const INSTANCE_MODEL_IR_VERSION: &str = "instance_model_ir_v3";

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum KernelCompileError {
    #[error("accepted module bytes are not valid UTF-8")]
    InvalidUtf8,
    #[error("cannot parse canonical .axi module: {0}")]
    Parse(String),
    #[error("duplicate accepted module `{0}` in compiler input")]
    DuplicateModule(String),
    #[error("root module `{0}` is absent from compiler input")]
    MissingRootModule(String),
    #[error("module `{module}` imports unknown module `{import}`")]
    UnknownImport { module: String, import: String },
    #[error("import cycle detected: {0}")]
    ImportCycle(String),
    #[error("unreachable compiler input module(s): {0}")]
    UnreachableModules(String),
    #[error("duplicate {kind} label `{label}` in `{scope}`")]
    DuplicateLabel {
        kind: &'static str,
        label: String,
        scope: String,
    },
    #[error("schema `{schema}` declares both an object and relation named `{label}`; relation objects are explicit and may not shadow object types")]
    ObjectRelationCollision { schema: String, label: String },
    #[error("{kind} `{label}` references unknown schema `{schema}`")]
    UnknownSchema {
        kind: &'static str,
        label: String,
        schema: String,
    },
    #[error("schema `{schema}` {site} references unknown object `{target}`")]
    UnknownObjectTarget {
        schema: String,
        site: String,
        target: String,
    },
    #[error("schema `{schema}` {site} references unknown relation object `{target}`")]
    UnknownRelationTarget {
        schema: String,
        site: String,
        target: String,
    },
    #[error("schema `{schema}` has a subtype cycle involving `{0}`", .cycle.join(" -> "))]
    SubtypeCycle { schema: String, cycle: Vec<String> },
    #[error("schema `{schema}` repeats subtype inclusion `{subtype} <: {supertype}`")]
    DuplicateSubtype {
        schema: String,
        subtype: String,
        supertype: String,
    },
    #[error("schema `{schema}` relation `{relation}` role `{role}` indexes unknown or non-earlier role `{index_role}`")]
    InvalidRoleIndex {
        schema: String,
        relation: String,
        role: String,
        index_role: String,
    },
    #[error("schema `{schema}` relation `{relation}` role `{role}` has an invalid finite indexed fiber: {detail}")]
    InvalidIndexedFiber {
        schema: String,
        relation: String,
        role: String,
        detail: String,
    },
    #[error(
        "schema `{schema}` relation `{relation}` role `{role}` has invalid refinement: {detail}"
    )]
    InvalidRefinement {
        schema: String,
        relation: String,
        role: String,
        detail: String,
    },
    #[error("schema `{schema}` generator `{generator}` has ambiguous or unknown {endpoint} object `{target}`")]
    UnknownGeneratorEndpoint {
        schema: String,
        generator: String,
        endpoint: &'static str,
        target: String,
    },
    #[error("theory `{theory}` equation `{equation}` is not a well-typed parallel schema path: {detail}")]
    InvalidSchemaEquation {
        theory: String,
        equation: String,
        detail: String,
    },
    #[error("theory `{theory}` rewrite `{rule}` is not a well-scoped endpoint-preserving typed path: {detail}")]
    InvalidRewriteRule {
        theory: String,
        rule: String,
        detail: String,
    },
    #[error("finite typed-theory compilation/checking failed: {0}")]
    FiniteTheory(String),
    #[error("instance `{instance}` identity or wire version does not match its compiled schema context: {detail}")]
    InstanceIdentityMismatch { instance: String, detail: String },
    #[error("instance `{instance}` repeats assignment `{assignment}`")]
    DuplicateAssignment {
        instance: String,
        assignment: String,
    },
    #[error("instance `{instance}` repeats carrier `{object:?}`")]
    DuplicateCarrier {
        instance: String,
        object: SchemaObjectRefIr,
    },
    #[error("instance `{instance}` carrier `{object:?}` repeats element `{element}`")]
    DuplicateCarrierElement {
        instance: String,
        object: SchemaObjectRefIr,
        element: String,
    },
    #[error("instance `{instance}` has no carrier for schema object `{object:?}`")]
    MissingCarrier {
        instance: String,
        object: SchemaObjectRefIr,
    },
    #[error("instance `{instance}` contains unexpected carrier `{object:?}`")]
    UnexpectedCarrier {
        instance: String,
        object: SchemaObjectRefIr,
    },
    #[error("instance `{instance}` repeats interpretation for schema generator `{generator:?}`")]
    DuplicateGeneratorInterpretation {
        instance: String,
        generator: SchemaGeneratorRefIr,
    },
    #[error("instance `{instance}` contains unexpected generator interpretation `{generator:?}`")]
    UnexpectedGeneratorInterpretation {
        instance: String,
        generator: SchemaGeneratorRefIr,
    },
    #[error("instance `{instance}` generator `{generator}` interpretation has endpoint drift")]
    GeneratorEndpointMismatch { instance: String, generator: String },
    #[error(
        "instance `{instance}` assignment `{assignment}` has the wrong literal shape: {detail}"
    )]
    InvalidAssignmentShape {
        instance: String,
        assignment: String,
        detail: String,
    },
    #[error("instance `{instance}` relation `{relation}` tuple is missing role `{role}`")]
    MissingRoleValue {
        instance: String,
        relation: String,
        role: String,
    },
    #[error("instance `{instance}` relation `{relation}` tuple repeats role `{role}`")]
    DuplicateRoleValue {
        instance: String,
        relation: String,
        role: String,
    },
    #[error("instance `{instance}` relation `{relation}` tuple supplies unknown role `{role}`")]
    UnknownRoleValue {
        instance: String,
        relation: String,
        role: String,
    },
    #[error("instance `{instance}` relation `{relation}` role `{role}` value `{value}` is outside target carrier `{target}`")]
    OutOfCodomain {
        instance: String,
        relation: String,
        role: String,
        value: String,
        target: String,
    },
    #[error("instance `{instance}` relation-valued role `{relation}.{role}` references unknown fact label `{label}` of relation `{target_relation}`")]
    UnknownRelationFact {
        instance: String,
        relation: String,
        role: String,
        label: String,
        target_relation: String,
    },
    #[error("instance `{instance}` repeats local fact label `{label}`")]
    DuplicateFactLabel { instance: String, label: String },
    #[error("instance `{instance}` fact `{fact_id}` references unknown relation `{relation_id}`")]
    UnknownFactRelation {
        instance: String,
        fact_id: FactIdV2,
        relation_id: RelationIdV2,
    },
    #[error(
        "instance `{instance}` relation carrier `{relation}` does not equal its exact fact-id set"
    )]
    RelationCarrierMismatch { instance: String, relation: String },
    #[error("instance `{instance}` relation `{relation}` role `{role}` stores the wrong typed value variant")]
    TypedValueKindMismatch {
        instance: String,
        relation: String,
        role: String,
    },
    #[error(
        "instance `{instance}` has cyclic relation-valued fact references involving: {labels}"
    )]
    CyclicFactReferences { instance: String, labels: String },
    #[error("instance `{instance}` derives duplicate stable fact id `{fact_id}`")]
    DuplicateFactId { instance: String, fact_id: FactIdV2 },
    #[error("instance `{instance}` fact id `{stored}` does not match canonical recomputation `{recomputed}`")]
    FactIdMismatch {
        instance: String,
        stored: FactIdV2,
        recomputed: FactIdV2,
    },
    #[error("instance `{instance}` fact `{fact_id}` projection `{role}` disagrees with its total function interpretation")]
    ProjectionMismatch {
        instance: String,
        fact_id: FactIdV2,
        role: String,
    },
    #[error("instance `{instance}` has no interpretation for schema generator `{generator}`")]
    MissingGeneratorInterpretation { instance: String, generator: String },
    #[error("instance `{instance}` generator `{generator}` is partial; missing source element(s): {missing}")]
    PartialGenerator {
        instance: String,
        generator: String,
        missing: String,
    },
    #[error("instance `{instance}` generator `{generator}` is not single-valued at source `{source_element}`")]
    NonFunctionalGenerator {
        instance: String,
        generator: String,
        source_element: String,
    },
    #[error("instance `{instance}` violates injectivity of subtype inclusion `{generator}` at target `{target}`")]
    NonInjectiveSubtype {
        instance: String,
        generator: String,
        target: String,
    },
    #[error("instance `{instance}` violates schema equation `{equation}` at source `{source_element}`: lhs={lhs:?}, rhs={rhs:?}")]
    ViolatedEquation {
        instance: String,
        equation: String,
        source_element: String,
        lhs: Option<String>,
        rhs: Option<String>,
    },
    #[error("instance `{instance}` violates constraint `{constraint}`: {detail}")]
    ViolatedConstraint {
        instance: String,
        constraint: String,
        detail: String,
    },
    #[error("instance `{instance}` finite-model validation claim does not replay: {detail}")]
    ValidationClaimMismatch { instance: String, detail: String },
    #[error("failed to serialize canonical kernel IR: {0}")]
    Serialization(String),
}

/// Exact accepted UTF-8 and its parsed AST. Private fields prevent pairing an
/// AST with different bytes.
#[derive(Debug, Clone)]
pub struct CanonicalModuleSource {
    exact_text: String,
    parsed: SchemaV1Module,
    revision: RevisionDigestV2,
}

impl CanonicalModuleSource {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, KernelCompileError> {
        let exact_text = String::from_utf8(bytes).map_err(|_| KernelCompileError::InvalidUtf8)?;
        let parsed = parse_axi_v1(&exact_text)
            .map_err(|error| KernelCompileError::Parse(error.to_string()))?;
        let revision = RevisionDigestV2::from_accepted_text(&exact_text);
        Ok(Self {
            exact_text,
            parsed,
            revision,
        })
    }

    pub fn exact_text(&self) -> &str {
        &self.exact_text
    }

    pub fn parsed(&self) -> &SchemaV1Module {
        &self.parsed
    }

    pub fn revision(&self) -> &RevisionDigestV2 {
        &self.revision
    }
}

/// The accepted binding is mandatory. Draft/review tooling may use a distinct
/// preview API, but it cannot manufacture a `KernelSnapshotIr`.
#[derive(Debug, Clone)]
pub struct KernelCompilationRequest {
    pub repository_id: RepositoryIdV2,
    pub accepted_snapshot_id: SnapshotIdV2,
    pub root_module: String,
    pub modules: Vec<CanonicalModuleSource>,
}

/// Immutable downstream handle. The IR is assembled only by
/// [`CanonicalCompiler`], then shared by `Arc` across runtime services.
#[derive(Debug, Clone)]
pub struct CompiledKernelSnapshot(Arc<KernelSnapshotIr>);

impl PartialEq for CompiledKernelSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for CompiledKernelSnapshot {}

impl CompiledKernelSnapshot {
    pub fn ir(&self) -> &KernelSnapshotIr {
        &self.0
    }

    pub fn shared_ir(&self) -> Arc<KernelSnapshotIr> {
        Arc::clone(&self.0)
    }

    /// Return one deterministic payload commitment for every addressable item
    /// in the compiled kernel IR. These commitments let reconciliation compare
    /// semantic payloads rather than treating stable-looking addresses as
    /// evidence that a role, refinement, equation, or fact stayed unchanged.
    pub fn payload_fingerprints(
        &self,
    ) -> Result<Vec<KernelPayloadFingerprintV2>, KernelCompileError> {
        self.0.payload_fingerprints()
    }
}

/// A typed, revision-scoped semantic address paired with a commitment to the
/// complete compiled payload at that address.
///
/// The digest is operational evidence used by the finite merge checker. It is
/// not a Lean proof, a claim that two arbitrary categorical presentations are
/// equivalent, or an open-world completeness result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct KernelPayloadFingerprintV2 {
    pub semantic_ref: KernelRefV2,
    pub payload_fingerprint: ObjectBlobIdV2,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct KernelSnapshotIr {
    version: String,
    repository_id: RepositoryIdV2,
    accepted_snapshot_id: SnapshotIdV2,
    root_module_id: ModuleIdV2,
    ordered_module_closure: Vec<CompiledModuleIr>,
    schemas: Vec<SchemaPresentationIr>,
    theories: Vec<TypedTheoryIr>,
    instances: Vec<InstanceModelIr>,
    refs: BTreeSet<KernelRefV2>,
    ir_digest: ObjectBlobIdV2,
    non_claims: Vec<String>,
}

impl KernelSnapshotIr {
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn repository_id(&self) -> &RepositoryIdV2 {
        &self.repository_id
    }
    pub fn accepted_snapshot_id(&self) -> &SnapshotIdV2 {
        &self.accepted_snapshot_id
    }
    pub fn root_module_id(&self) -> &ModuleIdV2 {
        &self.root_module_id
    }
    pub fn ordered_module_closure(&self) -> &[CompiledModuleIr] {
        &self.ordered_module_closure
    }
    pub fn schemas(&self) -> &[SchemaPresentationIr] {
        &self.schemas
    }
    pub fn theories(&self) -> &[TypedTheoryIr] {
        &self.theories
    }
    pub fn instances(&self) -> &[InstanceModelIr] {
        &self.instances
    }
    pub fn refs(&self) -> &BTreeSet<KernelRefV2> {
        &self.refs
    }
    pub fn ir_digest(&self) -> &ObjectBlobIdV2 {
        &self.ir_digest
    }
    pub fn non_claims(&self) -> &[String] {
        &self.non_claims
    }

    /// Return the finite payload-fingerprint index used by AxiStore review and
    /// reconciliation checks.
    ///
    /// This is a decidable structural comparison surface over the compiled
    /// snapshot. Equal fingerprints establish equality of the encoded finite IR
    /// payload only; they do not establish categorical equivalence, univalence,
    /// higher-path equality, ontology closure, or a Lean theorem.
    pub fn payload_fingerprints_v2(
        &self,
    ) -> Result<Vec<KernelPayloadFingerprintV2>, KernelCompileError> {
        self.payload_fingerprints()
    }

    pub fn schema(&self, module: &str, schema: &str) -> Option<&SchemaPresentationIr> {
        self.schemas
            .iter()
            .find(|candidate| candidate.module_name == module && candidate.label == schema)
    }

    pub fn instance(&self, module: &str, instance: &str) -> Option<&InstanceModelIr> {
        self.instances
            .iter()
            .find(|candidate| candidate.module_name == module && candidate.label == instance)
    }

    /// Human-readable label for a canonical typed reference. Labels are
    /// display metadata only; identity remains the complete `KernelRefV2`.
    pub fn label_for_ref(&self, reference: &KernelRefV2) -> Option<String> {
        match reference {
            KernelRefV2::Module { module_id, .. } => self
                .ordered_module_closure
                .iter()
                .find(|module| &module.module_id == module_id)
                .map(|module| module.module_name.clone()),
            KernelRefV2::Schema { schema_id, .. } => self
                .schemas
                .iter()
                .find(|schema| &schema.schema_id == schema_id)
                .map(|schema| schema.label.clone()),
            KernelRefV2::ObjectType { object_type_id, .. } => self
                .schemas
                .iter()
                .flat_map(|schema| schema.objects.iter())
                .find(|object| &object.object_type_id == object_type_id)
                .map(|object| object.label.clone()),
            KernelRefV2::Relation { relation_id, .. } => self
                .schemas
                .iter()
                .flat_map(|schema| schema.relations.iter())
                .find(|relation| &relation.relation_id == relation_id)
                .map(|relation| relation.label.clone()),
            KernelRefV2::Role { role_id, .. } => self
                .schemas
                .iter()
                .flat_map(|schema| schema.relations.iter())
                .find_map(|relation| {
                    relation
                        .roles
                        .iter()
                        .find(|role| &role.role_id == role_id)
                        .map(|role| format!("{}.{}", relation.label, role.label))
                }),
            KernelRefV2::Generator {
                schema_id,
                semantic_key,
                ..
            } => self
                .schemas
                .iter()
                .find(|schema| &schema.schema_id == schema_id)
                .and_then(|schema| {
                    schema
                        .generators
                        .iter()
                        .find(|generator| &generator.semantic_key == semantic_key)
                })
                .map(|generator| generator.label.clone()),
            KernelRefV2::Theory { theory_id, .. } => self
                .theories
                .iter()
                .find(|theory| &theory.theory_id == theory_id)
                .map(|theory| theory.label.clone()),
            KernelRefV2::Instance { instance_id, .. } => self
                .instances
                .iter()
                .find(|instance| &instance.instance_id == instance_id)
                .map(|instance| instance.label.clone()),
            KernelRefV2::Constraint { constraint_id, .. } => self
                .theories
                .iter()
                .flat_map(|theory| theory.constraints.iter())
                .find(|constraint| &constraint.constraint_id == constraint_id)
                .map(|constraint| constraint.label.clone()),
            KernelRefV2::Equation { equation_id, .. } => self
                .theories
                .iter()
                .flat_map(|theory| theory.equations.iter())
                .find(|equation| &equation.equation_id == equation_id)
                .map(|equation| equation.label.clone()),
            KernelRefV2::RewriteRule {
                rewrite_rule_id, ..
            } => self
                .theories
                .iter()
                .flat_map(|theory| theory.rewrite_rules.iter())
                .find(|rule| &rule.rewrite_rule_id == rewrite_rule_id)
                .map(|rule| rule.label.clone()),
            KernelRefV2::Fact {
                instance_id,
                fact_id,
                ..
            } => self
                .instances
                .iter()
                .find(|instance| &instance.instance_id == instance_id)
                .and_then(|instance| instance.facts.iter().find(|fact| &fact.fact_id == fact_id))
                .map(|fact| {
                    fact.local_label
                        .clone()
                        .unwrap_or_else(|| fact.fact_id.to_string())
                }),
        }
    }

    fn payload_fingerprints(&self) -> Result<Vec<KernelPayloadFingerprintV2>, KernelCompileError> {
        let mut payloads = Vec::with_capacity(self.refs.len());
        for module in &self.ordered_module_closure {
            push_payload(
                &mut payloads,
                KernelRefV2::Module {
                    module_id: module.module_id.clone(),
                    revision: module.revision.clone(),
                },
                "module",
                module,
            )?;
        }
        for schema in &self.schemas {
            push_payload(
                &mut payloads,
                KernelRefV2::Schema {
                    module_id: schema.module_id.clone(),
                    revision: schema.revision.clone(),
                    semantic_key: schema.semantic_key.clone(),
                    schema_id: schema.schema_id.clone(),
                },
                "schema",
                schema,
            )?;
            for object in &schema.objects {
                push_payload(
                    &mut payloads,
                    KernelRefV2::ObjectType {
                        module_id: schema.module_id.clone(),
                        revision: schema.revision.clone(),
                        schema_id: schema.schema_id.clone(),
                        semantic_key: object.semantic_key.clone(),
                        object_type_id: object.object_type_id.clone(),
                    },
                    "object_type",
                    object,
                )?;
            }
            for relation in &schema.relations {
                push_payload(
                    &mut payloads,
                    KernelRefV2::Relation {
                        module_id: schema.module_id.clone(),
                        revision: schema.revision.clone(),
                        schema_id: schema.schema_id.clone(),
                        semantic_key: relation.semantic_key.clone(),
                        relation_id: relation.relation_id.clone(),
                    },
                    "relation_object",
                    relation,
                )?;
                for role in &relation.roles {
                    push_payload(
                        &mut payloads,
                        KernelRefV2::Role {
                            module_id: schema.module_id.clone(),
                            revision: schema.revision.clone(),
                            schema_id: schema.schema_id.clone(),
                            relation_id: relation.relation_id.clone(),
                            semantic_key: role.semantic_key.clone(),
                            role_id: role.role_id.clone(),
                        },
                        "role_projection",
                        role,
                    )?;
                }
            }
            for generator in &schema.generators {
                push_payload(
                    &mut payloads,
                    KernelRefV2::Generator {
                        module_id: schema.module_id.clone(),
                        revision: schema.revision.clone(),
                        schema_id: schema.schema_id.clone(),
                        semantic_key: generator.semantic_key.clone(),
                    },
                    "schema_generator",
                    generator,
                )?;
            }
        }
        for theory in &self.theories {
            push_payload(
                &mut payloads,
                KernelRefV2::Theory {
                    module_id: theory.module_id.clone(),
                    revision: theory.revision.clone(),
                    schema_id: theory.schema_id.clone(),
                    semantic_key: theory.semantic_key.clone(),
                    theory_id: theory.theory_id.clone(),
                },
                "theory",
                theory,
            )?;
            for constraint in &theory.constraints {
                push_payload(
                    &mut payloads,
                    KernelRefV2::Constraint {
                        module_id: theory.module_id.clone(),
                        revision: theory.revision.clone(),
                        theory_id: theory.theory_id.clone(),
                        semantic_key: constraint.semantic_key.clone(),
                        constraint_id: constraint.constraint_id.clone(),
                    },
                    "constraint",
                    constraint,
                )?;
            }
            for equation in &theory.equations {
                push_payload(
                    &mut payloads,
                    KernelRefV2::Equation {
                        module_id: theory.module_id.clone(),
                        revision: theory.revision.clone(),
                        theory_id: theory.theory_id.clone(),
                        semantic_key: equation.semantic_key.clone(),
                        equation_id: equation.equation_id.clone(),
                    },
                    "equation",
                    equation,
                )?;
            }
            for rule in &theory.rewrite_rules {
                push_payload(
                    &mut payloads,
                    KernelRefV2::RewriteRule {
                        module_id: theory.module_id.clone(),
                        revision: theory.revision.clone(),
                        theory_id: theory.theory_id.clone(),
                        semantic_key: rule.semantic_key.clone(),
                        rewrite_rule_id: rule.rewrite_rule_id.clone(),
                    },
                    "rewrite_rule",
                    rule,
                )?;
            }
        }
        for instance in &self.instances {
            push_payload(
                &mut payloads,
                KernelRefV2::Instance {
                    module_id: instance.module_id.clone(),
                    revision: instance.revision.clone(),
                    schema_id: instance.schema_id.clone(),
                    semantic_key: instance.semantic_key.clone(),
                    instance_id: instance.instance_id.clone(),
                },
                "instance_model",
                instance,
            )?;
            for fact in &instance.facts {
                push_payload(
                    &mut payloads,
                    KernelRefV2::Fact {
                        module_id: instance.module_id.clone(),
                        revision: instance.revision.clone(),
                        schema_id: instance.schema_id.clone(),
                        instance_id: instance.instance_id.clone(),
                        relation_id: fact.relation_id.clone(),
                        fact_id: fact.fact_id.clone(),
                    },
                    "fact",
                    fact,
                )?;
            }
        }
        payloads.sort();
        if payloads.len() != self.refs.len()
            || payloads
                .iter()
                .map(|payload| &payload.semantic_ref)
                .collect::<BTreeSet<_>>()
                != self.refs.iter().collect::<BTreeSet<_>>()
        {
            return Err(KernelCompileError::Serialization(
                "payload fingerprint index does not cover the exact KernelRefV2 set".to_string(),
            ));
        }
        Ok(payloads)
    }
}

fn push_payload<T: Serialize>(
    payloads: &mut Vec<KernelPayloadFingerprintV2>,
    semantic_ref: KernelRefV2,
    kind: &'static str,
    payload: &T,
) -> Result<(), KernelCompileError> {
    let bytes = serde_json::to_vec(payload)
        .map_err(|error| KernelCompileError::Serialization(error.to_string()))?;
    payloads.push(KernelPayloadFingerprintV2 {
        semantic_ref,
        payload_fingerprint: ObjectBlobIdV2::from_canonical_fields(&[
            b"axiograph_kernel_payload_fingerprint_v2",
            kind.as_bytes(),
            &bytes,
        ]),
    });
    Ok(())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompiledModuleIr {
    pub module_id: ModuleIdV2,
    pub module_name: String,
    pub revision: RevisionDigestV2,
    pub ordered_imports: Vec<ModuleIdV2>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchemaPresentationIr {
    pub version: String,
    pub module_id: ModuleIdV2,
    pub module_name: String,
    pub revision: RevisionDigestV2,
    pub semantic_key: SemanticKeyV2,
    pub schema_id: SchemaIdV2,
    pub label: String,
    pub objects: Vec<SchemaObjectIr>,
    pub relations: Vec<RelationObjectIr>,
    pub generators: Vec<SchemaGeneratorIr>,
    pub subtype_coherence: Vec<SubtypeCoherenceIr>,
    pub equations: Vec<SchemaEquationIr>,
    /// One deterministic contextual replay witness for every forward equation.
    pub congruence_witnesses: Vec<crate::PathCongruenceCertificateIr>,
    /// Relation-span equations compiled into the free-groupoid presentation.
    pub formal_groupoid_equations: Vec<crate::FormalGroupoidEquationIr>,
    /// Derived identities, bounded reachability evidence, lifecycle, and
    /// residuals. The category presentation itself lives only in this
    /// `SchemaPresentationIr`; semantic objects and arrows are not duplicated.
    pub category_formation: crate::CategoryFormationIr,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaObjectRefIr {
    ObjectType { object_type_id: ObjectTypeIdV2 },
    RelationObject { relation_id: RelationIdV2 },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchemaObjectIr {
    pub semantic_key: SemanticKeyV2,
    pub object_type_id: ObjectTypeIdV2,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RelationObjectIr {
    pub semantic_key: SemanticKeyV2,
    pub relation_id: RelationIdV2,
    pub label: String,
    pub roles: Vec<RoleProjectionIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoleProjectionIr {
    pub semantic_key: SemanticKeyV2,
    pub role_id: RoleIdV2,
    pub label: String,
    pub declared_order: u32,
    pub kind: RoleKindIr,
    pub type_expr: TypeExprIr,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoleKindIr {
    Data,
    Context,
    World,
    Temporal,
    Parameter,
    Evidence,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeExprIr {
    Object {
        object_type_id: ObjectTypeIdV2,
    },
    RelationObject {
        relation_id: RelationIdV2,
    },
    Indexed {
        base: Box<TypeExprIr>,
        over_roles: Vec<RoleIdV2>,
    },
    Refined {
        base: Box<TypeExprIr>,
        predicates: Vec<RefinementPredicateIr>,
    },
}

impl TypeExprIr {
    pub fn carrier(&self) -> SchemaObjectRefIr {
        match self {
            Self::Object { object_type_id } => SchemaObjectRefIr::ObjectType {
                object_type_id: object_type_id.clone(),
            },
            Self::RelationObject { relation_id } => SchemaObjectRefIr::RelationObject {
                relation_id: relation_id.clone(),
            },
            Self::Indexed { base, .. } | Self::Refined { base, .. } => base.carrier(),
        }
    }

    pub fn index_roles(&self) -> Vec<RoleIdV2> {
        match self {
            Self::Indexed { base, over_roles } => {
                let mut roles = base.index_roles();
                roles.extend(over_roles.clone());
                roles
            }
            Self::Refined { base, .. } => base.index_roles(),
            Self::Object { .. } | Self::RelationObject { .. } => Vec::new(),
        }
    }

    pub fn refinements(&self) -> Vec<RefinementPredicateIr> {
        match self {
            Self::Refined { base, predicates } => {
                let mut refinements = base.refinements();
                refinements.extend(predicates.clone());
                refinements
            }
            Self::Indexed { base, .. } => base.refinements(),
            Self::Object { .. } | Self::RelationObject { .. } => Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefinementPredicateIr {
    Equals { value: String },
    MemberOf { values: Vec<String> },
    Cardinality { min: u32, max: u32 },
    Key { roles: Vec<RoleIdV2> },
    Enum { values: Vec<String> },
    Predicate { name: String, args: Vec<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SchemaGeneratorRefIr {
    RoleProjection { role_id: RoleIdV2 },
    SubtypeInclusion { semantic_key: SemanticKeyV2 },
    Explicit { semantic_key: SemanticKeyV2 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SchemaGeneratorKindIr {
    RoleProjection,
    SubtypeInclusion,
    Aspect,
    Function,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchemaGeneratorIr {
    pub generator_ref: SchemaGeneratorRefIr,
    pub semantic_key: SemanticKeyV2,
    pub label: String,
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub kind: SchemaGeneratorKindIr,
    pub reversible: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubtypeCoherenceIr {
    pub subtype: ObjectTypeIdV2,
    pub supertype: ObjectTypeIdV2,
    pub canonical_path: Vec<SchemaGeneratorRefIr>,
    pub equivalent_paths: Vec<Vec<SchemaGeneratorRefIr>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchemaPathIr {
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub steps: Vec<SchemaGeneratorRefIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchemaEquationIr {
    pub equation_id: EquationIdV2,
    pub theory_id: TheoryIdV2,
    pub label: String,
    pub lhs: SchemaPathIr,
    pub rhs: SchemaPathIr,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TypedTheoryIr {
    pub module_id: ModuleIdV2,
    pub module_name: String,
    pub revision: RevisionDigestV2,
    pub semantic_key: SemanticKeyV2,
    pub theory_id: TheoryIdV2,
    pub schema_id: SchemaIdV2,
    pub label: String,
    pub constraints: Vec<ConstraintObligationIr>,
    pub equations: Vec<TheoryEquationIr>,
    pub rewrite_rules: Vec<RewriteObligationIr>,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstraintObligationIr {
    pub semantic_key: SemanticKeyV2,
    pub constraint_id: ConstraintIdV2,
    pub label: String,
    pub source: ConstraintV1,
    pub finite_model_checked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TheoryEquationIr {
    pub semantic_key: SemanticKeyV2,
    pub equation_id: EquationIdV2,
    pub label: String,
    pub source_lhs: String,
    pub source_rhs: String,
    pub schema_equation: Option<SchemaEquationIr>,
    pub formal_groupoid_equation: Option<crate::FormalGroupoidEquationIr>,
    pub non_claim: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RewriteObligationIr {
    pub semantic_key: SemanticKeyV2,
    pub rewrite_rule_id: RewriteRuleIdV2,
    pub label: String,
    pub variables: Vec<String>,
    pub lhs: PathExprV3,
    pub rhs: PathExprV3,
    pub reversible: bool,
    pub non_claims: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstanceModelIr {
    pub version: String,
    pub module_id: ModuleIdV2,
    pub module_name: String,
    pub revision: RevisionDigestV2,
    pub semantic_key: SemanticKeyV2,
    pub instance_id: InstanceIdV2,
    pub schema_id: SchemaIdV2,
    pub label: String,
    pub carriers: Vec<CarrierInterpretationIr>,
    pub functions: Vec<FunctionInterpretationIr>,
    pub facts: Vec<RelationFactIr>,
    pub validation: FiniteModelValidationIr,
    /// Exact finite carrier membership for object elements and relation facts.
    pub object_membership_witnesses: Vec<crate::ObjectMembershipWitnessIr>,
    /// Every fact role reified as an ordered, role-indexed dependent witness.
    pub role_witnesses: Vec<crate::RoleIndexedWitnessIr>,
    /// Every executable finite constraint that succeeded for this exact instance.
    pub typed_constraint_witnesses: Vec<crate::TypedConstraintWitnessIr>,
    /// Context/world/temporal projections selected from `role_witnesses`.
    pub scope_witnesses: Vec<crate::ScopeWitnessIr>,
    /// Finite context-indexed families grouped by exact scope membership.
    pub dependent_contexts: Vec<crate::DependentContextIr>,
    pub lifecycle: crate::CheckedLifecycleStateIr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub residual_obligations: Vec<crate::TheoryResidualObligationIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CarrierInterpretationIr {
    pub object: SchemaObjectRefIr,
    pub elements: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FunctionInterpretationIr {
    pub generator: SchemaGeneratorRefIr,
    pub source: SchemaObjectRefIr,
    pub target: SchemaObjectRefIr,
    pub mappings: Vec<FunctionMappingIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FunctionMappingIr {
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RelationFactIr {
    pub fact_id: FactIdV2,
    pub relation_id: RelationIdV2,
    pub local_label: Option<String>,
    pub ordered_role_values: Vec<RoleValueIr>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoleValueIr {
    pub role_id: RoleIdV2,
    pub value: TypedValueIr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypedValueIr {
    ObjectElement { value: String },
    RelationFact { fact_id: FactIdV2 },
}

impl TypedValueIr {
    pub(crate) fn wire_value(&self) -> String {
        match self {
            Self::ObjectElement { value } => value.clone(),
            Self::RelationFact { fact_id } => fact_id.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FiniteModelValidationIr {
    pub role_projections_total_and_single_valued: bool,
    pub outputs_in_target_carriers: bool,
    pub subtype_maps_injective: bool,
    pub stable_fact_ids_recomputed: bool,
    pub supported_equations_hold_pointwise: bool,
    pub supported_constraints_hold: bool,
    pub non_claims: Vec<String>,
}

fn checked_finite_model_validation() -> FiniteModelValidationIr {
    FiniteModelValidationIr {
        role_projections_total_and_single_valued: true,
        outputs_in_target_carriers: true,
        subtype_maps_injective: true,
        stable_fact_ids_recomputed: true,
        supported_equations_hold_pointwise: true,
        supported_constraints_hold: true,
        non_claims: vec![
            "validation is finite-model satisfaction for this explicit instance, not ontology closure"
                .to_string(),
            "runtime validation is not a Lean proof or frontend-lowering receipt".to_string(),
        ],
    }
}

pub struct CanonicalCompiler;

impl CanonicalCompiler {
    pub fn compile(
        request: KernelCompilationRequest,
    ) -> Result<CompiledKernelSnapshot, KernelCompileError> {
        let KernelCompilationRequest {
            repository_id,
            accepted_snapshot_id,
            root_module,
            modules,
        } = request;
        let mut sources = BTreeMap::new();
        for source in modules {
            let name = source.parsed.module_name.clone();
            if sources.insert(name.clone(), source).is_some() {
                return Err(KernelCompileError::DuplicateModule(name));
            }
        }
        if !sources.contains_key(&root_module) {
            return Err(KernelCompileError::MissingRootModule(root_module));
        }

        let ordered_names = ordered_import_closure(&root_module, &sources)?;
        if ordered_names.len() != sources.len() {
            let reachable = ordered_names.iter().cloned().collect::<BTreeSet<_>>();
            let unreachable = sources
                .keys()
                .filter(|name| !reachable.contains(*name))
                .cloned()
                .collect::<Vec<_>>();
            return Err(KernelCompileError::UnreachableModules(
                unreachable.join(", "),
            ));
        }

        let module_ids = ordered_names
            .iter()
            .map(|name| (name.clone(), ModuleIdV2::derive(&repository_id, name)))
            .collect::<BTreeMap<_, _>>();
        let mut ordered_module_closure = Vec::new();
        for name in &ordered_names {
            let source = &sources[name];
            ordered_module_closure.push(CompiledModuleIr {
                module_id: module_ids[name].clone(),
                module_name: name.clone(),
                revision: source.revision.clone(),
                ordered_imports: source
                    .parsed
                    .imports
                    .iter()
                    .map(|import| module_ids[import].clone())
                    .collect(),
            });
        }

        let mut schemas = Vec::new();
        for module_name in &ordered_names {
            let source = &sources[module_name];
            let module_id = &module_ids[module_name];
            let mut seen_schema_labels = BTreeSet::new();
            for schema in &source.parsed.schemas {
                if !seen_schema_labels.insert(schema.name.clone()) {
                    return Err(KernelCompileError::DuplicateLabel {
                        kind: "schema",
                        label: schema.name.clone(),
                        scope: module_name.clone(),
                    });
                }
                schemas.push(compile_schema(
                    module_name,
                    module_id,
                    &source.revision,
                    schema,
                )?);
            }
        }

        let mut theories = Vec::new();
        for module_name in &ordered_names {
            let source = &sources[module_name];
            let module_id = &module_ids[module_name];
            let visible_schema_indexes = visible_schema_indexes(module_name, &sources, &schemas)?;
            let mut seen_theories = BTreeSet::new();
            for theory in &source.parsed.theories {
                if !seen_theories.insert((theory.schema.clone(), theory.name.clone())) {
                    return Err(KernelCompileError::DuplicateLabel {
                        kind: "theory",
                        label: theory.name.clone(),
                        scope: format!("{module_name}.{}", theory.schema),
                    });
                }
                let Some(schema_index) = visible_schema_indexes.get(&theory.schema).copied() else {
                    return Err(KernelCompileError::UnknownSchema {
                        kind: "theory",
                        label: theory.name.clone(),
                        schema: theory.schema.clone(),
                    });
                };
                let typed = compile_theory(
                    module_name,
                    module_id,
                    &source.revision,
                    &mut schemas[schema_index],
                    theory,
                )?;
                theories.push(typed);
            }
        }

        for schema in &mut schemas {
            schema.congruence_witnesses = schema
                .compile_congruence_witnesses()
                .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
            schema
                .category_formation
                .verify(schema)
                .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
        }

        let mut instances = Vec::new();
        for module_name in &ordered_names {
            let source = &sources[module_name];
            let module_id = &module_ids[module_name];
            let visible_schema_indexes = visible_schema_indexes(module_name, &sources, &schemas)?;
            let mut seen_instances = BTreeSet::new();
            for instance in &source.parsed.instances {
                if !seen_instances.insert((instance.schema.clone(), instance.name.clone())) {
                    return Err(KernelCompileError::DuplicateLabel {
                        kind: "instance",
                        label: instance.name.clone(),
                        scope: format!("{module_name}.{}", instance.schema),
                    });
                }
                let Some(schema_index) = visible_schema_indexes.get(&instance.schema).copied()
                else {
                    return Err(KernelCompileError::UnknownSchema {
                        kind: "instance",
                        label: instance.name.clone(),
                        schema: instance.schema.clone(),
                    });
                };
                let applicable_theories = theories
                    .iter()
                    .filter(|theory| theory.schema_id == schemas[schema_index].schema_id)
                    .collect::<Vec<_>>();
                instances.push(compile_instance_model(
                    module_name,
                    module_id,
                    &source.revision,
                    &schemas[schema_index],
                    &applicable_theories,
                    instance,
                )?);
            }
        }

        let refs = build_refs(&ordered_module_closure, &schemas, &theories, &instances);
        let root_module_id = module_ids[&root_module].clone();
        let non_claims = vec![
            "Rust formation and finite-model validation are not Lean proofs".to_string(),
            "the schema denotes a free category quotiented by declared equations; it is not claimed to be a finite category".to_string(),
            "indexed/refinement roles are a closed finite decision procedure, not general dependent type theory".to_string(),
            "no univalence, higher inductive types, completeness, open-world entailment, or ontology closure is claimed".to_string(),
        ];
        let digest_payload = serde_json::to_vec(&(
            KERNEL_SNAPSHOT_IR_VERSION,
            &repository_id,
            &accepted_snapshot_id,
            &root_module_id,
            &ordered_module_closure,
            &schemas,
            &theories,
            &instances,
            &refs,
            &non_claims,
        ))
        .map_err(|error| KernelCompileError::Serialization(error.to_string()))?;
        let ir_digest = ObjectBlobIdV2::from_canonical_fields(&[&digest_payload]);
        Ok(CompiledKernelSnapshot(Arc::new(KernelSnapshotIr {
            version: KERNEL_SNAPSHOT_IR_VERSION.to_string(),
            repository_id,
            accepted_snapshot_id,
            root_module_id,
            ordered_module_closure,
            schemas,
            theories,
            instances,
            refs,
            ir_digest,
            non_claims,
        })))
    }
}

fn visible_schema_indexes(
    module_name: &str,
    sources: &BTreeMap<String, CanonicalModuleSource>,
    schemas: &[SchemaPresentationIr],
) -> Result<BTreeMap<String, usize>, KernelCompileError> {
    let visible_modules = ordered_import_closure(module_name, sources)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let mut visible = BTreeMap::new();
    for (index, schema) in schemas.iter().enumerate() {
        if !visible_modules.contains(&schema.module_name) {
            continue;
        }
        if visible.insert(schema.label.clone(), index).is_some() {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "visible schema",
                label: schema.label.clone(),
                scope: format!("import closure of {module_name}"),
            });
        }
    }
    Ok(visible)
}

fn ordered_import_closure(
    root: &str,
    sources: &BTreeMap<String, CanonicalModuleSource>,
) -> Result<Vec<String>, KernelCompileError> {
    fn visit(
        module: &str,
        sources: &BTreeMap<String, CanonicalModuleSource>,
        temporary: &mut Vec<String>,
        permanent: &mut BTreeSet<String>,
        ordered: &mut Vec<String>,
    ) -> Result<(), KernelCompileError> {
        if permanent.contains(module) {
            return Ok(());
        }
        if let Some(position) = temporary.iter().position(|name| name == module) {
            let mut cycle = temporary[position..].to_vec();
            cycle.push(module.to_string());
            return Err(KernelCompileError::ImportCycle(cycle.join(" -> ")));
        }
        let source = sources
            .get(module)
            .ok_or_else(|| KernelCompileError::MissingRootModule(module.to_string()))?;
        temporary.push(module.to_string());
        for import in &source.parsed.imports {
            if !sources.contains_key(import) {
                return Err(KernelCompileError::UnknownImport {
                    module: module.to_string(),
                    import: import.clone(),
                });
            }
            visit(import, sources, temporary, permanent, ordered)?;
        }
        temporary.pop();
        permanent.insert(module.to_string());
        ordered.push(module.to_string());
        Ok(())
    }

    let mut temporary = Vec::new();
    let mut permanent = BTreeSet::new();
    let mut ordered = Vec::new();
    visit(root, sources, &mut temporary, &mut permanent, &mut ordered)?;
    Ok(ordered)
}

fn compile_schema(
    module_name: &str,
    module_id: &ModuleIdV2,
    revision: &RevisionDigestV2,
    schema: &SchemaV1Schema,
) -> Result<SchemaPresentationIr, KernelCompileError> {
    let semantic_key = SemanticKeyV2::derive(module_id, "schema", &schema.name);
    let schema_id = SchemaIdV2::derive(revision, &semantic_key);
    let scope = format!("{module_name}.{}", schema.name);

    let mut object_labels = BTreeSet::new();
    let mut objects = Vec::new();
    for label in &schema.objects {
        if !object_labels.insert(label.clone()) {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "object",
                label: label.clone(),
                scope: scope.clone(),
            });
        }
        let key = SemanticKeyV2::derive(module_id, "object", &format!("{}.{}", schema.name, label));
        objects.push(SchemaObjectIr {
            object_type_id: ObjectTypeIdV2::derive(revision, &schema_id, &key),
            semantic_key: key,
            label: label.clone(),
        });
    }

    let mut relation_labels = BTreeSet::new();
    for relation in &schema.relations {
        if !relation_labels.insert(relation.name.clone()) {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "relation",
                label: relation.name.clone(),
                scope: scope.clone(),
            });
        }
        if object_labels.contains(&relation.name) {
            return Err(KernelCompileError::ObjectRelationCollision {
                schema: scope.clone(),
                label: relation.name.clone(),
            });
        }
    }

    let object_ids = objects
        .iter()
        .map(|object| (object.label.clone(), object.object_type_id.clone()))
        .collect::<BTreeMap<_, _>>();
    let relation_identities = schema
        .relations
        .iter()
        .map(|relation| {
            let key = SemanticKeyV2::derive(
                module_id,
                "relation",
                &format!("{}.{}", schema.name, relation.name),
            );
            (
                relation.name.clone(),
                (
                    key.clone(),
                    RelationIdV2::derive(revision, &schema_id, &key),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut relations = Vec::new();
    for relation in &schema.relations {
        let (relation_key, relation_id) = &relation_identities[&relation.name];
        let mut role_labels = BTreeSet::new();
        let mut earlier_roles = BTreeMap::new();
        let mut roles = Vec::new();
        for (order, field) in relation.fields.iter().enumerate() {
            if !role_labels.insert(field.field.clone()) {
                return Err(KernelCompileError::DuplicateLabel {
                    kind: "role",
                    label: field.field.clone(),
                    scope: format!("{scope}.{}", relation.name),
                });
            }
            let key = SemanticKeyV2::derive(
                module_id,
                "role",
                &format!("{}.{}.{}", schema.name, relation.name, field.field),
            );
            let role_id = RoleIdV2::derive(revision, &schema_id, relation_id, &key, order as u32);
            let type_expr = compile_type_expr(
                &field.ty,
                &object_ids,
                &relation_identities,
                &earlier_roles,
                &scope,
                &relation.name,
                &field.field,
            )?;
            validate_refinement_formation(
                &type_expr,
                &earlier_roles,
                &scope,
                &relation.name,
                &field.field,
            )?;
            roles.push(RoleProjectionIr {
                semantic_key: key,
                role_id: role_id.clone(),
                label: field.field.clone(),
                declared_order: order as u32,
                kind: map_role_kind(field.kind),
                type_expr,
            });
            earlier_roles.insert(field.field.clone(), role_id);
        }
        relations.push(RelationObjectIr {
            semantic_key: relation_key.clone(),
            relation_id: relation_id.clone(),
            label: relation.name.clone(),
            roles,
        });
    }
    validate_indexed_fiber_formation(&scope, &relations)?;

    let mut generators = Vec::new();
    for relation in &relations {
        let source = SchemaObjectRefIr::RelationObject {
            relation_id: relation.relation_id.clone(),
        };
        for role in &relation.roles {
            generators.push(SchemaGeneratorIr {
                generator_ref: SchemaGeneratorRefIr::RoleProjection {
                    role_id: role.role_id.clone(),
                },
                semantic_key: role.semantic_key.clone(),
                label: format!("{}.{}", relation.label, role.label),
                source: source.clone(),
                target: role.type_expr.carrier(),
                kind: SchemaGeneratorKindIr::RoleProjection,
                reversible: false,
            });
        }
    }

    let mut direct_subtypes = BTreeMap::<String, Vec<String>>::new();
    let mut subtype_edges = BTreeSet::new();
    for subtype in &schema.subtypes {
        if !object_ids.contains_key(&subtype.sub) {
            return Err(KernelCompileError::UnknownObjectTarget {
                schema: scope.clone(),
                site: "subtype declaration".to_string(),
                target: subtype.sub.clone(),
            });
        }
        if !object_ids.contains_key(&subtype.sup) {
            return Err(KernelCompileError::UnknownObjectTarget {
                schema: scope.clone(),
                site: "subtype declaration".to_string(),
                target: subtype.sup.clone(),
            });
        }
        if !subtype_edges.insert((subtype.sub.clone(), subtype.sup.clone())) {
            return Err(KernelCompileError::DuplicateSubtype {
                schema: scope.clone(),
                subtype: subtype.sub.clone(),
                supertype: subtype.sup.clone(),
            });
        }
        direct_subtypes
            .entry(subtype.sub.clone())
            .or_default()
            .push(subtype.sup.clone());
        let label = subtype
            .inclusion
            .clone()
            .unwrap_or_else(|| format!("{}_to_{}", subtype.sub, subtype.sup));
        let key = SemanticKeyV2::derive(
            module_id,
            "subtype-inclusion",
            &format!("{}.{}.{}", schema.name, subtype.sub, subtype.sup),
        );
        generators.push(SchemaGeneratorIr {
            generator_ref: SchemaGeneratorRefIr::SubtypeInclusion {
                semantic_key: key.clone(),
            },
            semantic_key: key,
            label,
            source: SchemaObjectRefIr::ObjectType {
                object_type_id: object_ids[&subtype.sub].clone(),
            },
            target: SchemaObjectRefIr::ObjectType {
                object_type_id: object_ids[&subtype.sup].clone(),
            },
            kind: SchemaGeneratorKindIr::SubtypeInclusion,
            reversible: false,
        });
    }
    detect_subtype_cycles(&scope, &object_labels, &direct_subtypes)?;

    let mut generator_labels = BTreeSet::new();
    for generator in &generators {
        if !generator_labels.insert(generator.label.clone()) {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "category arrow",
                label: generator.label.clone(),
                scope: scope.clone(),
            });
        }
    }
    for generator in &schema.generators {
        if !generator_labels.insert(generator.name.clone()) {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "schema generator",
                label: generator.name.clone(),
                scope: scope.clone(),
            });
        }
        let source = resolve_generator_endpoint(
            &generator.source,
            &object_ids,
            &relation_identities,
            &scope,
            &generator.name,
            "source",
        )?;
        let target = resolve_generator_endpoint(
            &generator.target,
            &object_ids,
            &relation_identities,
            &scope,
            &generator.name,
            "target",
        )?;
        let key = SemanticKeyV2::derive(
            module_id,
            "generator",
            &format!("{}.{}", schema.name, generator.name),
        );
        generators.push(SchemaGeneratorIr {
            generator_ref: SchemaGeneratorRefIr::Explicit {
                semantic_key: key.clone(),
            },
            semantic_key: key,
            label: generator.name.clone(),
            source,
            target,
            kind: match generator.kind {
                GeneratorKindV1::Aspect => SchemaGeneratorKindIr::Aspect,
                GeneratorKindV1::Function => SchemaGeneratorKindIr::Function,
            },
            reversible: generator.reversible,
        });
    }

    let subtype_coherence = compile_subtype_coherence(&object_ids, &direct_subtypes, &generators);
    let category_formation =
        crate::compile_category_formation(&schema_id, &objects, &relations, &generators)
            .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
    Ok(SchemaPresentationIr {
        version: SCHEMA_PRESENTATION_IR_VERSION.to_string(),
        module_id: module_id.clone(),
        module_name: module_name.to_string(),
        revision: revision.clone(),
        semantic_key,
        schema_id,
        label: schema.name.clone(),
        objects,
        relations,
        generators,
        subtype_coherence,
        equations: Vec::new(),
        congruence_witnesses: Vec::new(),
        formal_groupoid_equations: Vec::new(),
        category_formation,
        non_claims: vec![
            "denotation is the free category on typed generators quotiented by declared equations, not a finite category enumeration".to_string(),
            "subtyping is a checked thin preorder fragment; nominal equality is not inferred".to_string(),
            "relation projections and subtype inclusions are not reversible unless a separate explicit reversible generator is declared".to_string(),
        ],
    })
}

fn map_role_kind(kind: RoleKindV1) -> RoleKindIr {
    match kind {
        RoleKindV1::Data => RoleKindIr::Data,
        RoleKindV1::Context => RoleKindIr::Context,
        RoleKindV1::World => RoleKindIr::World,
        RoleKindV1::Temporal => RoleKindIr::Temporal,
        RoleKindV1::Parameter => RoleKindIr::Parameter,
        RoleKindV1::Evidence => RoleKindIr::Evidence,
    }
}

fn compile_type_expr(
    expression: &TypeExprV1,
    object_ids: &BTreeMap<String, ObjectTypeIdV2>,
    relation_ids: &BTreeMap<String, (SemanticKeyV2, RelationIdV2)>,
    earlier_roles: &BTreeMap<String, RoleIdV2>,
    schema: &str,
    relation: &str,
    role: &str,
) -> Result<TypeExprIr, KernelCompileError> {
    match expression {
        TypeExprV1::Object { name } => object_ids
            .get(name)
            .cloned()
            .map(|object_type_id| TypeExprIr::Object { object_type_id })
            .ok_or_else(|| KernelCompileError::UnknownObjectTarget {
                schema: schema.to_string(),
                site: format!("relation `{relation}` role `{role}`"),
                target: name.clone(),
            }),
        TypeExprV1::RelationObject { relation: target } => relation_ids
            .get(target)
            .map(|(_, relation_id)| TypeExprIr::RelationObject {
                relation_id: relation_id.clone(),
            })
            .ok_or_else(|| KernelCompileError::UnknownRelationTarget {
                schema: schema.to_string(),
                site: format!("relation `{relation}` role `{role}`"),
                target: target.clone(),
            }),
        TypeExprV1::Indexed { base, over_roles } => {
            let base = compile_type_expr(
                base,
                object_ids,
                relation_ids,
                earlier_roles,
                schema,
                relation,
                role,
            )?;
            let mut compiled_roles = Vec::new();
            let mut seen_roles = BTreeSet::new();
            for index_role in over_roles {
                if !seen_roles.insert(index_role) {
                    return Err(KernelCompileError::InvalidIndexedFiber {
                        schema: schema.to_string(),
                        relation: relation.to_string(),
                        role: role.to_string(),
                        detail: format!("index role `{index_role}` is repeated"),
                    });
                }
                let Some(role_id) = earlier_roles.get(index_role) else {
                    return Err(KernelCompileError::InvalidRoleIndex {
                        schema: schema.to_string(),
                        relation: relation.to_string(),
                        role: role.to_string(),
                        index_role: index_role.clone(),
                    });
                };
                compiled_roles.push(role_id.clone());
            }
            Ok(TypeExprIr::Indexed {
                base: Box::new(base),
                over_roles: compiled_roles,
            })
        }
        TypeExprV1::Refined { base, predicates } => {
            let base = compile_type_expr(
                base,
                object_ids,
                relation_ids,
                earlier_roles,
                schema,
                relation,
                role,
            )?;
            let mut compiled = Vec::new();
            for predicate in predicates {
                compiled.push(match predicate {
                    RefinementPredicateV1::Equals { value } => RefinementPredicateIr::Equals {
                        value: value.clone(),
                    },
                    RefinementPredicateV1::MemberOf { values } => RefinementPredicateIr::MemberOf {
                        values: values.clone(),
                    },
                    RefinementPredicateV1::Cardinality { min, max } => {
                        RefinementPredicateIr::Cardinality {
                            min: *min,
                            max: *max,
                        }
                    }
                    RefinementPredicateV1::Key { .. } => {
                        return Err(KernelCompileError::InvalidRefinement {
                            schema: schema.to_string(),
                            relation: relation.to_string(),
                            role: role.to_string(),
                            detail: "key(...) role refinements have no implemented finite witness; declare a theory `constraint key Relation(...)` instead".to_string(),
                        });
                    }
                    RefinementPredicateV1::Enum { values } => RefinementPredicateIr::Enum {
                        values: values.clone(),
                    },
                    RefinementPredicateV1::Predicate { name, args } => {
                        if name != "non_empty" {
                            return Err(KernelCompileError::InvalidRefinement {
                                schema: schema.to_string(),
                                relation: relation.to_string(),
                                role: role.to_string(),
                                detail: format!("unsupported predicate `{name}`"),
                            });
                        }
                        RefinementPredicateIr::Predicate {
                            name: name.clone(),
                            args: args.clone(),
                        }
                    }
                });
            }
            Ok(TypeExprIr::Refined {
                base: Box::new(base),
                predicates: compiled,
            })
        }
    }
}

fn validate_refinement_formation(
    expression: &TypeExprIr,
    earlier_roles: &BTreeMap<String, RoleIdV2>,
    schema: &str,
    relation: &str,
    role: &str,
) -> Result<(), KernelCompileError> {
    match expression {
        TypeExprIr::Indexed { base, .. } => {
            validate_refinement_formation(base, earlier_roles, schema, relation, role)?;
        }
        TypeExprIr::Refined { base, predicates } => {
            validate_refinement_formation(base, earlier_roles, schema, relation, role)?;
            for predicate in predicates {
                match predicate {
                    RefinementPredicateIr::MemberOf { values }
                    | RefinementPredicateIr::Enum { values }
                        if values.is_empty() =>
                    {
                        return Err(KernelCompileError::InvalidRefinement {
                            schema: schema.to_string(),
                            relation: relation.to_string(),
                            role: role.to_string(),
                            detail: "membership/enum set is empty".to_string(),
                        });
                    }
                    RefinementPredicateIr::Cardinality { min, max } if min > max => {
                        return Err(KernelCompileError::InvalidRefinement {
                            schema: schema.to_string(),
                            relation: relation.to_string(),
                            role: role.to_string(),
                            detail: "cardinality minimum exceeds maximum".to_string(),
                        });
                    }
                    RefinementPredicateIr::Key { roles } if roles.is_empty() => {
                        return Err(KernelCompileError::InvalidRefinement {
                            schema: schema.to_string(),
                            relation: relation.to_string(),
                            role: role.to_string(),
                            detail: "key refinement has no earlier roles".to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }
        TypeExprIr::Object { .. } | TypeExprIr::RelationObject { .. } => {}
    }
    let _ = earlier_roles;
    Ok(())
}

fn validate_indexed_fiber_formation(
    schema: &str,
    relations: &[RelationObjectIr],
) -> Result<(), KernelCompileError> {
    let relations_by_id = relations
        .iter()
        .map(|relation| (relation.relation_id.clone(), relation))
        .collect::<BTreeMap<_, _>>();

    fn check_expression(
        schema: &str,
        relation: &RelationObjectIr,
        role: &RoleProjectionIr,
        expression: &TypeExprIr,
        relations_by_id: &BTreeMap<RelationIdV2, &RelationObjectIr>,
    ) -> Result<(), KernelCompileError> {
        match expression {
            TypeExprIr::Indexed { base, over_roles } => {
                check_expression(schema, relation, role, base, relations_by_id)?;
                let mut unique_roles = BTreeSet::new();
                for index_role_id in over_roles {
                    if !unique_roles.insert(index_role_id) {
                        return Err(KernelCompileError::InvalidIndexedFiber {
                            schema: schema.to_string(),
                            relation: relation.label.clone(),
                            role: role.label.clone(),
                            detail: format!("index role `{index_role_id}` is repeated"),
                        });
                    }
                }
                let SchemaObjectRefIr::RelationObject {
                    relation_id: target_relation_id,
                } = base.carrier()
                else {
                    // Indexed object carriers are constant finite families keyed by
                    // the exact local bindings. Relation-valued carriers additionally
                    // require a matching projection in the referenced relation.
                    return Ok(());
                };
                let Some(target_relation) = relations_by_id.get(&target_relation_id).copied()
                else {
                    return Err(KernelCompileError::InvalidIndexedFiber {
                        schema: schema.to_string(),
                        relation: relation.label.clone(),
                        role: role.label.clone(),
                        detail:
                            "referenced relation object is absent from the compiled presentation"
                                .to_string(),
                    });
                };
                for index_role_id in over_roles {
                    let local_role = relation
                        .roles
                        .iter()
                        .find(|candidate| &candidate.role_id == index_role_id)
                        .ok_or_else(|| KernelCompileError::InvalidIndexedFiber {
                            schema: schema.to_string(),
                            relation: relation.label.clone(),
                            role: role.label.clone(),
                            detail: format!(
                                "index role `{index_role_id}` is absent from the containing relation"
                            ),
                        })?;
                    let target_role = target_relation
                        .roles
                        .iter()
                        .find(|candidate| candidate.label == local_role.label)
                        .ok_or_else(|| KernelCompileError::InvalidIndexedFiber {
                            schema: schema.to_string(),
                            relation: relation.label.clone(),
                            role: role.label.clone(),
                            detail: format!(
                                "target relation `{}` has no role named `{}` for the indexed fiber",
                                target_relation.label, local_role.label
                            ),
                        })?;
                    if target_role.type_expr.carrier() != local_role.type_expr.carrier()
                        || target_role.kind != local_role.kind
                    {
                        return Err(KernelCompileError::InvalidIndexedFiber {
                            schema: schema.to_string(),
                            relation: relation.label.clone(),
                            role: role.label.clone(),
                            detail: format!(
                                "target relation `{}.{}` does not match local index role `{}` in carrier and role kind",
                                target_relation.label, target_role.label, local_role.label
                            ),
                        });
                    }
                }
                Ok(())
            }
            TypeExprIr::Refined { base, .. } => {
                check_expression(schema, relation, role, base, relations_by_id)
            }
            TypeExprIr::Object { .. } | TypeExprIr::RelationObject { .. } => Ok(()),
        }
    }

    for relation in relations {
        for role in &relation.roles {
            check_expression(schema, relation, role, &role.type_expr, &relations_by_id)?;
        }
    }
    Ok(())
}

fn resolve_generator_endpoint(
    label: &str,
    objects: &BTreeMap<String, ObjectTypeIdV2>,
    relations: &BTreeMap<String, (SemanticKeyV2, RelationIdV2)>,
    schema: &str,
    generator: &str,
    endpoint: &'static str,
) -> Result<SchemaObjectRefIr, KernelCompileError> {
    match (objects.get(label), relations.get(label)) {
        (Some(object_type_id), None) => Ok(SchemaObjectRefIr::ObjectType {
            object_type_id: object_type_id.clone(),
        }),
        (None, Some((_, relation_id))) => Ok(SchemaObjectRefIr::RelationObject {
            relation_id: relation_id.clone(),
        }),
        _ => Err(KernelCompileError::UnknownGeneratorEndpoint {
            schema: schema.to_string(),
            generator: generator.to_string(),
            endpoint,
            target: label.to_string(),
        }),
    }
}

fn detect_subtype_cycles(
    schema: &str,
    objects: &BTreeSet<String>,
    direct: &BTreeMap<String, Vec<String>>,
) -> Result<(), KernelCompileError> {
    fn visit(
        node: &str,
        direct: &BTreeMap<String, Vec<String>>,
        stack: &mut Vec<String>,
        complete: &mut BTreeSet<String>,
    ) -> Option<Vec<String>> {
        if complete.contains(node) {
            return None;
        }
        if let Some(index) = stack.iter().position(|candidate| candidate == node) {
            let mut cycle = stack[index..].to_vec();
            cycle.push(node.to_string());
            return Some(cycle);
        }
        stack.push(node.to_string());
        for target in direct.get(node).into_iter().flatten() {
            if let Some(cycle) = visit(target, direct, stack, complete) {
                return Some(cycle);
            }
        }
        stack.pop();
        complete.insert(node.to_string());
        None
    }

    let mut complete = BTreeSet::new();
    for object in objects {
        if let Some(cycle) = visit(object, direct, &mut Vec::new(), &mut complete) {
            return Err(KernelCompileError::SubtypeCycle {
                schema: schema.to_string(),
                cycle,
            });
        }
    }
    Ok(())
}

fn compile_subtype_coherence(
    object_ids: &BTreeMap<String, ObjectTypeIdV2>,
    direct: &BTreeMap<String, Vec<String>>,
    generators: &[SchemaGeneratorIr],
) -> Vec<SubtypeCoherenceIr> {
    fn paths(
        current: &str,
        target: &str,
        direct: &BTreeMap<String, Vec<String>>,
        stack: &mut Vec<String>,
        out: &mut Vec<Vec<(String, String)>>,
    ) {
        if out.len() >= 1024 {
            return;
        }
        if current == target {
            out.push(
                stack
                    .windows(2)
                    .map(|edge| (edge[0].clone(), edge[1].clone()))
                    .collect(),
            );
            return;
        }
        for next in direct.get(current).into_iter().flatten() {
            if stack.contains(next) {
                continue;
            }
            stack.push(next.clone());
            paths(next, target, direct, stack, out);
            stack.pop();
        }
    }

    let edge_refs = generators
        .iter()
        .filter_map(
            |generator| match (&generator.kind, &generator.source, &generator.target) {
                (
                    SchemaGeneratorKindIr::SubtypeInclusion,
                    SchemaObjectRefIr::ObjectType {
                        object_type_id: source,
                    },
                    SchemaObjectRefIr::ObjectType {
                        object_type_id: target,
                    },
                ) => Some((
                    (source.clone(), target.clone()),
                    generator.generator_ref.clone(),
                )),
                _ => None,
            },
        )
        .collect::<BTreeMap<_, _>>();
    let label_by_id = object_ids
        .iter()
        .map(|(label, id)| (id.clone(), label.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut coherence = Vec::new();
    for (sub_label, sub_id) in object_ids {
        for (sup_label, sup_id) in object_ids {
            if sub_label == sup_label {
                continue;
            }
            let mut edge_paths = Vec::new();
            paths(
                sub_label,
                sup_label,
                direct,
                &mut vec![sub_label.clone()],
                &mut edge_paths,
            );
            if edge_paths.is_empty() {
                continue;
            }
            let mut compiled_paths = edge_paths
                .into_iter()
                .map(|path| {
                    path.into_iter()
                        .map(|(source, target)| {
                            let source_id = &object_ids[&source];
                            let target_id = &object_ids[&target];
                            edge_refs[&(source_id.clone(), target_id.clone())].clone()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            compiled_paths.sort();
            let canonical_path = compiled_paths[0].clone();
            coherence.push(SubtypeCoherenceIr {
                subtype: sub_id.clone(),
                supertype: sup_id.clone(),
                canonical_path,
                equivalent_paths: compiled_paths,
            });
        }
    }
    coherence.sort_by(|left, right| {
        (
            label_by_id[&left.subtype].as_str(),
            label_by_id[&left.supertype].as_str(),
        )
            .cmp(&(
                label_by_id[&right.subtype].as_str(),
                label_by_id[&right.supertype].as_str(),
            ))
    });
    coherence
}

fn compile_theory(
    module_name: &str,
    module_id: &ModuleIdV2,
    revision: &RevisionDigestV2,
    schema: &mut SchemaPresentationIr,
    theory: &axiograph_dsl::schema_v1::SchemaV1Theory,
) -> Result<TypedTheoryIr, KernelCompileError> {
    let semantic_key = SemanticKeyV2::derive(
        module_id,
        "theory",
        &format!("{}.{}", theory.schema, theory.name),
    );
    let theory_id = TheoryIdV2::derive(revision, &schema.schema_id, &semantic_key);

    let mut constraints = Vec::new();
    for (order, constraint) in theory.constraints.iter().enumerate() {
        validate_constraint_formation(schema, theory, constraint)?;
        let label = match constraint {
            ConstraintV1::NamedBlock { name, .. } => name.clone(),
            _ => format!("constraint#{order}"),
        };
        let key = SemanticKeyV2::derive(
            module_id,
            "constraint",
            &format!("{}.{}.{}", theory.schema, theory.name, label),
        );
        let finite_model_checked = matches!(
            constraint,
            ConstraintV1::Functional { .. }
                | ConstraintV1::AtMost { .. }
                | ConstraintV1::SymmetricWhereIn { .. }
                | ConstraintV1::Symmetric { .. }
                | ConstraintV1::Transitive { .. }
                | ConstraintV1::Key { .. }
        );
        constraints.push(ConstraintObligationIr {
            constraint_id: ConstraintIdV2::derive(revision, &theory_id, &key),
            semantic_key: key,
            label,
            source: constraint.clone(),
            finite_model_checked,
        });
    }

    let mut equation_labels = BTreeSet::new();
    let mut equations = Vec::new();
    for equation in &theory.equations {
        if !equation_labels.insert(equation.name.clone()) {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "equation",
                label: equation.name.clone(),
                scope: format!("{module_name}.{}.{}", theory.schema, theory.name),
            });
        }
        let key = SemanticKeyV2::derive(
            module_id,
            "equation",
            &format!("{}.{}.{}", theory.schema, theory.name, equation.name),
        );
        let equation_id = EquationIdV2::derive(revision, &theory_id, &key);
        let schema_equation = match (
            compile_schema_path(schema, &equation.lhs),
            compile_schema_path(schema, &equation.rhs),
        ) {
            (Ok(lhs), Ok(rhs)) => {
                if lhs.source != rhs.source || lhs.target != rhs.target {
                    return Err(KernelCompileError::InvalidSchemaEquation {
                        theory: theory.name.clone(),
                        equation: equation.name.clone(),
                        detail: "lhs and rhs have different source/target objects".to_string(),
                    });
                }
                if schema
                    .equations
                    .iter()
                    .any(|existing| existing.label == equation.name)
                {
                    return Err(KernelCompileError::DuplicateLabel {
                        kind: "category equation",
                        label: equation.name.clone(),
                        scope: format!("{module_name}.{}", theory.schema),
                    });
                }
                let compiled = SchemaEquationIr {
                    equation_id: equation_id.clone(),
                    theory_id: theory_id.clone(),
                    label: equation.name.clone(),
                    lhs,
                    rhs,
                };
                schema.equations.push(compiled.clone());
                Some(compiled)
            }
            (Err(lhs_detail), Err(rhs_detail)) => {
                if looks_like_schema_path(&equation.lhs) || looks_like_schema_path(&equation.rhs) {
                    return Err(KernelCompileError::InvalidSchemaEquation {
                        theory: theory.name.clone(),
                        equation: equation.name.clone(),
                        detail: format!("lhs: {lhs_detail}; rhs: {rhs_detail}"),
                    });
                }
                None
            }
            (Err(detail), _) | (_, Err(detail)) => {
                if looks_like_schema_path(&equation.lhs) || looks_like_schema_path(&equation.rhs) {
                    return Err(KernelCompileError::InvalidSchemaEquation {
                        theory: theory.name.clone(),
                        equation: equation.name.clone(),
                        detail,
                    });
                }
                None
            }
        };
        let formal_groupoid_equation = if schema_equation.is_some() {
            None
        } else {
            match (
                compile_formal_groupoid_source_path(schema, &equation.lhs),
                compile_formal_groupoid_source_path(schema, &equation.rhs),
            ) {
                (Ok(lhs), Ok(rhs)) => {
                    if lhs.source != rhs.source || lhs.target != rhs.target {
                        return Err(KernelCompileError::InvalidSchemaEquation {
                            theory: theory.name.clone(),
                            equation: equation.name.clone(),
                            detail: "formal groupoid equation has different lhs/rhs endpoints"
                                .to_string(),
                        });
                    }
                    let compiled = crate::FormalGroupoidEquationIr {
                    equation_id: equation_id.clone(),
                    theory_id: theory_id.clone(),
                    label: equation.name.clone(),
                    lhs,
                    rhs,
                    lifecycle: crate::CheckedLifecycleStateIr::FormationChecked,
                    non_claim: "formation proves endpoint-indexed free-groupoid syntax, not rewrite termination or confluence".to_string(),
                };
                    schema.formal_groupoid_equations.push(compiled.clone());
                    Some(compiled)
                }
                _ => None,
            }
        };
        equations.push(TheoryEquationIr {
            semantic_key: key,
            equation_id,
            label: equation.name.clone(),
            source_lhs: equation.lhs.clone(),
            source_rhs: equation.rhs.clone(),
            non_claim: (schema_equation.is_none() && formal_groupoid_equation.is_none()).then(|| {
                "equation source is retained as an explicit residual obligation outside the closed finite schema/groupoid equation checker".to_string()
            }),
            schema_equation,
            formal_groupoid_equation,
        });
    }

    let mut rewrite_labels = BTreeSet::new();
    let mut rewrite_rules = Vec::new();
    for rule in &theory.rewrite_rules {
        if !rewrite_labels.insert(rule.name.clone()) {
            return Err(KernelCompileError::DuplicateLabel {
                kind: "rewrite rule",
                label: rule.name.clone(),
                scope: format!("{module_name}.{}.{}", theory.schema, theory.name),
            });
        }
        validate_rewrite_formation(schema, theory, rule)?;
        let key = SemanticKeyV2::derive(
            module_id,
            "rewrite",
            &format!("{}.{}.{}", theory.schema, theory.name, rule.name),
        );
        rewrite_rules.push(RewriteObligationIr {
            rewrite_rule_id: RewriteRuleIdV2::derive(revision, &theory_id, &key),
            semantic_key: key,
            label: rule.name.clone(),
            variables: rule.vars.iter().map(|variable| variable.name.clone()).collect(),
            lhs: rule.lhs.clone(),
            rhs: rule.rhs.clone(),
            reversible: matches!(
                rule.orientation,
                axiograph_dsl::schema_v1::RewriteOrientationV1::Bidirectional
            ),
            non_claims: vec![
                "runtime rewrite formation is not a Lean derivation certificate".to_string(),
                "inverse is available only for explicitly reversible generators or exact traversal witnesses".to_string(),
            ],
        });
    }

    Ok(TypedTheoryIr {
        module_id: module_id.clone(),
        module_name: module_name.to_string(),
        revision: revision.clone(),
        semantic_key,
        theory_id,
        schema_id: schema.schema_id.clone(),
        label: theory.name.clone(),
        constraints,
        equations,
        rewrite_rules,
        non_claims: vec![
            "typed runtime obligations do not imply completeness, saturation, a fixpoint, or ontology closure".to_string(),
        ],
    })
}

fn looks_like_schema_path(text: &str) -> bool {
    text.contains(';') || text.trim_start().starts_with("id(")
}

fn compile_schema_path(schema: &SchemaPresentationIr, text: &str) -> Result<SchemaPathIr, String> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix("id(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let object = resolve_schema_object_label(schema, inner.trim())
            .ok_or_else(|| format!("identity path references unknown object `{}`", inner.trim()))?;
        return Ok(SchemaPathIr {
            source: object.clone(),
            target: object,
            steps: Vec::new(),
        });
    }
    let labels = text
        .split(';')
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    if labels.is_empty()
        || (labels.len() == 1 && !schema.generators.iter().any(|g| g.label == labels[0]))
    {
        return Err("not a schema-generator path".to_string());
    }
    let mut selected = Vec::new();
    for label in labels {
        let matches = schema
            .generators
            .iter()
            .filter(|generator| generator.label == label)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(format!("path step `{label}` is unknown or ambiguous"));
        }
        selected.push(matches[0]);
    }
    for pair in selected.windows(2) {
        if pair[0].target != pair[1].source {
            return Err(format!(
                "path does not compose between `{}` and `{}`",
                pair[0].label, pair[1].label
            ));
        }
    }
    Ok(SchemaPathIr {
        source: selected[0].source.clone(),
        target: selected.last().expect("non-empty").target.clone(),
        steps: selected
            .iter()
            .map(|generator| generator.generator_ref.clone())
            .collect(),
    })
}

fn compile_formal_groupoid_source_path(
    schema: &SchemaPresentationIr,
    text: &str,
) -> Result<crate::FormalGroupoidPathIr, String> {
    fn compile(
        schema: &SchemaPresentationIr,
        expression: &PathExprV3,
    ) -> Result<crate::FormalGroupoidPathIr, String> {
        match expression {
            PathExprV3::Reflexive { entity } => {
                let object = resolve_schema_object_label(schema, entity).ok_or_else(|| {
                    format!("reflexive path references unknown schema object `{entity}`")
                })?;
                Ok(crate::FormalGroupoidPathIr {
                    schema_id: schema.schema_id.clone(),
                    source: object.clone(),
                    target: object,
                    steps: Vec::new(),
                })
            }
            PathExprV3::Step { from, rel, to } => {
                let relation = schema
                    .relations
                    .iter()
                    .find(|relation| relation.label == *rel)
                    .ok_or_else(|| format!("path step references unknown relation `{rel}`"))?;
                let from_role = relation
                    .roles
                    .iter()
                    .find(|role| role.label == *from)
                    .ok_or_else(|| {
                        format!("path step `{rel}` has no source role named `{from}`")
                    })?;
                let to_role = relation
                    .roles
                    .iter()
                    .find(|role| role.label == *to)
                    .ok_or_else(|| format!("path step `{rel}` has no target role named `{to}`"))?;
                Ok(crate::FormalGroupoidPathIr {
                    schema_id: schema.schema_id.clone(),
                    source: from_role.type_expr.carrier(),
                    target: to_role.type_expr.carrier(),
                    steps: vec![
                        crate::FormalGeneratorStepIr {
                            generator: SchemaGeneratorRefIr::RoleProjection {
                                role_id: from_role.role_id.clone(),
                            },
                            direction: crate::FormalDirectionIr::Inverse,
                        },
                        crate::FormalGeneratorStepIr {
                            generator: SchemaGeneratorRefIr::RoleProjection {
                                role_id: to_role.role_id.clone(),
                            },
                            direction: crate::FormalDirectionIr::Forward,
                        },
                    ],
                })
            }
            PathExprV3::Trans { left, right } => {
                let mut left = compile(schema, left)?;
                let right = compile(schema, right)?;
                if left.target != right.source {
                    return Err("formal groupoid path composition endpoints differ".to_string());
                }
                left.target = right.target;
                left.steps.extend(right.steps);
                Ok(left)
            }
            PathExprV3::Inv { path } => {
                let path = compile(schema, path)?;
                schema
                    .formal_inverse(&path)
                    .map_err(|error| error.to_string())
            }
            PathExprV3::Var { name } => Err(format!(
                "path metavariable `{name}` has no closed schema endpoint"
            )),
        }
    }

    let expression = parse_path_expr_v3(text)?;
    let path = compile(schema, &expression)?;
    schema
        .verify_formal_path(&path)
        .map_err(|error| error.to_string())?;
    Ok(path)
}

fn resolve_schema_object_label(
    schema: &SchemaPresentationIr,
    label: &str,
) -> Option<SchemaObjectRefIr> {
    schema
        .objects
        .iter()
        .find(|object| object.label == label)
        .map(|object| SchemaObjectRefIr::ObjectType {
            object_type_id: object.object_type_id.clone(),
        })
        .or_else(|| {
            schema
                .relations
                .iter()
                .find(|relation| relation.label == label)
                .map(|relation| SchemaObjectRefIr::RelationObject {
                    relation_id: relation.relation_id.clone(),
                })
        })
}

fn validate_constraint_formation(
    schema: &SchemaPresentationIr,
    theory: &axiograph_dsl::schema_v1::SchemaV1Theory,
    constraint: &ConstraintV1,
) -> Result<(), KernelCompileError> {
    let relation_and_roles: Option<(&str, Vec<&str>)> = match constraint {
        ConstraintV1::Functional {
            relation,
            src_field,
            dst_field,
        } => Some((relation, vec![src_field, dst_field])),
        ConstraintV1::AtMost {
            relation,
            src_field,
            dst_field,
            params,
            ..
        } => Some((
            relation,
            std::iter::once(src_field.as_str())
                .chain(std::iter::once(dst_field.as_str()))
                .chain(params.iter().flatten().map(String::as_str))
                .collect(),
        )),
        ConstraintV1::Typing { relation, .. } => Some((relation, Vec::new())),
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            carriers,
            params,
            ..
        } => Some((
            relation,
            std::iter::once(field.as_str())
                .chain(carrier_names(carriers.as_ref()))
                .chain(params.iter().flatten().map(String::as_str))
                .collect(),
        )),
        ConstraintV1::Symmetric {
            relation,
            carriers,
            params,
        }
        | ConstraintV1::Transitive {
            relation,
            carriers,
            params,
        } => Some((
            relation,
            carrier_names(carriers.as_ref())
                .chain(params.iter().flatten().map(String::as_str))
                .collect(),
        )),
        ConstraintV1::Key { relation, fields } => {
            Some((relation, fields.iter().map(String::as_str).collect()))
        }
        ConstraintV1::NamedBlock { name, body } => {
            if name.trim().is_empty() || body.iter().all(|line| line.trim().is_empty()) {
                return Err(KernelCompileError::InvalidSchemaEquation {
                    theory: theory.name.clone(),
                    equation: name.clone(),
                    detail: "named constraint block must have a non-empty name and body"
                        .to_string(),
                });
            }
            None
        }
        ConstraintV1::Unknown { text } => {
            if text.trim().is_empty() {
                return Err(KernelCompileError::InvalidSchemaEquation {
                    theory: theory.name.clone(),
                    equation: "unknown_constraint".to_string(),
                    detail: "unknown constraint text is empty".to_string(),
                });
            }
            None
        }
    };
    if let Some((relation_name, roles)) = relation_and_roles {
        let Some(relation) = schema.relations.iter().find(|r| r.label == relation_name) else {
            return Err(KernelCompileError::UnknownRelationTarget {
                schema: schema.label.clone(),
                site: format!("theory `{}` constraint", theory.name),
                target: relation_name.to_string(),
            });
        };
        for role in roles {
            if !relation
                .roles
                .iter()
                .any(|candidate| candidate.label == role)
            {
                return Err(KernelCompileError::InvalidRefinement {
                    schema: schema.label.clone(),
                    relation: relation_name.to_string(),
                    role: role.to_string(),
                    detail: format!(
                        "theory `{}` constraint references unknown role",
                        theory.name
                    ),
                });
            }
        }
    }
    Ok(())
}

fn carrier_names(carriers: Option<&CarrierFieldsV1>) -> impl Iterator<Item = &str> {
    carriers
        .into_iter()
        .flat_map(|carrier| [carrier.left_field.as_str(), carrier.right_field.as_str()])
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RewriteEndpoint {
    from_var: String,
    to_var: String,
    from_type: ObjectTypeIdV2,
    to_type: ObjectTypeIdV2,
}

#[derive(Debug, Default)]
struct RewriteTypingEnvironment {
    object_vars: BTreeMap<String, ObjectTypeIdV2>,
    path_vars: BTreeMap<String, (String, String)>,
}

fn invalid_rewrite(
    theory: &axiograph_dsl::schema_v1::SchemaV1Theory,
    rule: &axiograph_dsl::schema_v1::RewriteRuleV1,
    detail: impl Into<String>,
) -> KernelCompileError {
    KernelCompileError::InvalidRewriteRule {
        theory: theory.name.clone(),
        rule: rule.name.clone(),
        detail: detail.into(),
    }
}

fn rewrite_object_type(schema: &SchemaPresentationIr, label: &str) -> Option<ObjectTypeIdV2> {
    schema
        .objects
        .iter()
        .find(|object| object.label == label)
        .map(|object| object.object_type_id.clone())
}

fn rewrite_type_matches(
    schema: &SchemaPresentationIr,
    actual: &ObjectTypeIdV2,
    expected: &SchemaObjectRefIr,
) -> bool {
    let SchemaObjectRefIr::ObjectType {
        object_type_id: expected,
    } = expected
    else {
        return false;
    };
    actual == expected
        || schema
            .subtype_coherence
            .iter()
            .any(|coherence| coherence.subtype == *actual && coherence.supertype == *expected)
}

fn infer_rewrite_endpoint(
    schema: &SchemaPresentationIr,
    theory: &axiograph_dsl::schema_v1::SchemaV1Theory,
    rule: &axiograph_dsl::schema_v1::RewriteRuleV1,
    environment: &RewriteTypingEnvironment,
    expression: &PathExprV3,
) -> Result<RewriteEndpoint, KernelCompileError> {
    match expression {
        PathExprV3::Var { name } => {
            let (from_var, to_var) = environment.path_vars.get(name).ok_or_else(|| {
                invalid_rewrite(theory, rule, format!("unbound path variable `{name}`"))
            })?;
            Ok(RewriteEndpoint {
                from_var: from_var.clone(),
                to_var: to_var.clone(),
                from_type: environment.object_vars[from_var].clone(),
                to_type: environment.object_vars[to_var].clone(),
            })
        }
        PathExprV3::Reflexive { entity } => {
            let object_type = environment
                .object_vars
                .get(entity)
                .cloned()
                .ok_or_else(|| {
                    invalid_rewrite(theory, rule, format!("unbound object variable `{entity}`"))
                })?;
            Ok(RewriteEndpoint {
                from_var: entity.clone(),
                to_var: entity.clone(),
                from_type: object_type.clone(),
                to_type: object_type,
            })
        }
        PathExprV3::Step { from, rel, to } => {
            let from_type = environment.object_vars.get(from).cloned().ok_or_else(|| {
                invalid_rewrite(theory, rule, format!("unbound object variable `{from}`"))
            })?;
            let to_type = environment.object_vars.get(to).cloned().ok_or_else(|| {
                invalid_rewrite(theory, rule, format!("unbound object variable `{to}`"))
            })?;
            let relation = schema
                .relations
                .iter()
                .find(|relation| relation.label == *rel)
                .ok_or_else(|| {
                    invalid_rewrite(theory, rule, format!("unknown relation `{rel}`"))
                })?;
            let [source_role, target_role, ..] = relation.roles.as_slice() else {
                return Err(invalid_rewrite(
                    theory,
                    rule,
                    format!("relation `{rel}` has fewer than two traversal roles"),
                ));
            };
            if !rewrite_type_matches(schema, &from_type, &source_role.type_expr.carrier()) {
                return Err(invalid_rewrite(
                    theory,
                    rule,
                    format!(
                        "`{from}` has an incompatible type; expected subtype of `{:?}` for relation `{rel}` role `{}`",
                        source_role.type_expr.carrier(),
                        source_role.label
                    ),
                ));
            }
            if !rewrite_type_matches(schema, &to_type, &target_role.type_expr.carrier()) {
                return Err(invalid_rewrite(
                    theory,
                    rule,
                    format!(
                        "`{to}` has an incompatible type; expected subtype of `{:?}` for relation `{rel}` role `{}`",
                        target_role.type_expr.carrier(),
                        target_role.label
                    ),
                ));
            }
            Ok(RewriteEndpoint {
                from_var: from.clone(),
                to_var: to.clone(),
                from_type,
                to_type,
            })
        }
        PathExprV3::Trans { left, right } => {
            let left = infer_rewrite_endpoint(schema, theory, rule, environment, left)?;
            let right = infer_rewrite_endpoint(schema, theory, rule, environment, right)?;
            if left.to_var != right.from_var {
                return Err(invalid_rewrite(
                    theory,
                    rule,
                    format!(
                        "cannot compose paths because the left path ends at `{}` and the right path starts at `{}`",
                        left.to_var, right.from_var
                    ),
                ));
            }
            Ok(RewriteEndpoint {
                from_var: left.from_var,
                to_var: right.to_var,
                from_type: left.from_type,
                to_type: right.to_type,
            })
        }
        PathExprV3::Inv { path } => {
            let path = infer_rewrite_endpoint(schema, theory, rule, environment, path)?;
            Ok(RewriteEndpoint {
                from_var: path.to_var,
                to_var: path.from_var,
                from_type: path.to_type,
                to_type: path.from_type,
            })
        }
    }
}

fn validate_rewrite_formation(
    schema: &SchemaPresentationIr,
    theory: &axiograph_dsl::schema_v1::SchemaV1Theory,
    rule: &axiograph_dsl::schema_v1::RewriteRuleV1,
) -> Result<(), KernelCompileError> {
    let mut environment = RewriteTypingEnvironment::default();
    let mut pending_paths = Vec::new();
    for variable in &rule.vars {
        if environment.object_vars.contains_key(&variable.name)
            || environment.path_vars.contains_key(&variable.name)
            || pending_paths
                .iter()
                .any(|(name, _, _): &(String, String, String)| name == &variable.name)
        {
            return Err(invalid_rewrite(
                theory,
                rule,
                format!("duplicate variable `{}`", variable.name),
            ));
        }
        match &variable.ty {
            RewriteVarTypeV1::Object { ty } => {
                let object_type = rewrite_object_type(schema, ty).ok_or_else(|| {
                    invalid_rewrite(
                        theory,
                        rule,
                        format!(
                            "unknown object type `{ty}` for variable `{}`",
                            variable.name
                        ),
                    )
                })?;
                environment
                    .object_vars
                    .insert(variable.name.clone(), object_type);
            }
            RewriteVarTypeV1::Path { from, to } => {
                pending_paths.push((variable.name.clone(), from.clone(), to.clone()));
            }
        }
    }
    for (name, from, to) in pending_paths {
        if !environment.object_vars.contains_key(&from) {
            return Err(invalid_rewrite(
                theory,
                rule,
                format!("path variable `{name}` references unknown endpoint `{from}`"),
            ));
        }
        if !environment.object_vars.contains_key(&to) {
            return Err(invalid_rewrite(
                theory,
                rule,
                format!("path variable `{name}` references unknown endpoint `{to}`"),
            ));
        }
        environment.path_vars.insert(name, (from, to));
    }

    let lhs = infer_rewrite_endpoint(schema, theory, rule, &environment, &rule.lhs)?;
    let rhs = infer_rewrite_endpoint(schema, theory, rule, &environment, &rule.rhs)?;
    if lhs.from_var != rhs.from_var || lhs.to_var != rhs.to_var {
        return Err(invalid_rewrite(
            theory,
            rule,
            format!(
                "changes path endpoints (lhs=Path({},{}) rhs=Path({},{}))",
                lhs.from_var, lhs.to_var, rhs.from_var, rhs.to_var
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct PendingFact {
    relation_label: String,
    relation_id: RelationIdV2,
    local_label: Option<String>,
    fields: BTreeMap<String, String>,
}

fn compile_instance_model(
    module_name: &str,
    module_id: &ModuleIdV2,
    revision: &RevisionDigestV2,
    schema: &SchemaPresentationIr,
    theories: &[&TypedTheoryIr],
    instance: &SchemaV1Instance,
) -> Result<InstanceModelIr, KernelCompileError> {
    let semantic_key = SemanticKeyV2::derive(
        module_id,
        "instance",
        &format!("{}.{}", instance.schema, instance.name),
    );
    let instance_id = InstanceIdV2::derive(revision, &schema.schema_id, &semantic_key);
    let mut assignments = BTreeMap::new();
    for assignment in &instance.assignments {
        if assignments
            .insert(assignment.name.clone(), &assignment.value)
            .is_some()
        {
            return Err(KernelCompileError::DuplicateAssignment {
                instance: instance.name.clone(),
                assignment: assignment.name.clone(),
            });
        }
    }

    let object_by_label = schema
        .objects
        .iter()
        .map(|object| (object.label.clone(), object))
        .collect::<BTreeMap<_, _>>();
    let relation_by_label = schema
        .relations
        .iter()
        .map(|relation| (relation.label.clone(), relation))
        .collect::<BTreeMap<_, _>>();
    let explicit_generator_by_label = schema
        .generators
        .iter()
        .filter(|generator| {
            matches!(
                generator.kind,
                SchemaGeneratorKindIr::Aspect | SchemaGeneratorKindIr::Function
            )
        })
        .map(|generator| (generator.label.clone(), generator))
        .collect::<BTreeMap<_, _>>();

    for assignment in assignments.keys() {
        if !object_by_label.contains_key(assignment)
            && !relation_by_label.contains_key(assignment)
            && !explicit_generator_by_label.contains_key(assignment)
        {
            return Err(KernelCompileError::InvalidAssignmentShape {
                instance: instance.name.clone(),
                assignment: assignment.clone(),
                detail: "assignment target is not an object, relation, aspect, or function in the compiled schema".to_string(),
            });
        }
    }

    let mut carriers = BTreeMap::<SchemaObjectRefIr, BTreeSet<String>>::new();
    for object in &schema.objects {
        let object_ref = SchemaObjectRefIr::ObjectType {
            object_type_id: object.object_type_id.clone(),
        };
        let mut elements = BTreeSet::new();
        if let Some(set) = assignments.get(&object.label) {
            for item in &set.items {
                match item {
                    SetItemV1::Ident { name } => {
                        elements.insert(name.clone());
                    }
                    SetItemV1::Tuple { .. } => {
                        return Err(KernelCompileError::InvalidAssignmentShape {
                            instance: instance.name.clone(),
                            assignment: object.label.clone(),
                            detail: "object carrier must contain only identifiers".to_string(),
                        });
                    }
                }
            }
        }
        carriers.insert(object_ref, elements);
    }

    // Close finite carriers under the checked thin-preorder inclusions.
    for coherence in &schema.subtype_coherence {
        let source = SchemaObjectRefIr::ObjectType {
            object_type_id: coherence.subtype.clone(),
        };
        let target = SchemaObjectRefIr::ObjectType {
            object_type_id: coherence.supertype.clone(),
        };
        let source_elements = carriers.get(&source).cloned().unwrap_or_default();
        carriers.entry(target).or_default().extend(source_elements);
    }

    let mut pending = Vec::new();
    let mut fact_label_relation = BTreeMap::new();
    for relation in &schema.relations {
        let relation_ref = SchemaObjectRefIr::RelationObject {
            relation_id: relation.relation_id.clone(),
        };
        carriers.entry(relation_ref).or_default();
        let Some(set) = assignments.get(&relation.label) else {
            continue;
        };
        for item in &set.items {
            let SetItemV1::Tuple { label, fields } = item else {
                return Err(KernelCompileError::InvalidAssignmentShape {
                    instance: instance.name.clone(),
                    assignment: relation.label.clone(),
                    detail: "relation carrier must contain only tuples".to_string(),
                });
            };
            if let Some(label) = label {
                if fact_label_relation
                    .insert(label.clone(), relation.label.clone())
                    .is_some()
                {
                    return Err(KernelCompileError::DuplicateFactLabel {
                        instance: instance.name.clone(),
                        label: label.clone(),
                    });
                }
            }
            let mut values = BTreeMap::new();
            for (role, value) in fields {
                if values.insert(role.clone(), value.clone()).is_some() {
                    return Err(KernelCompileError::DuplicateRoleValue {
                        instance: instance.name.clone(),
                        relation: relation.label.clone(),
                        role: role.clone(),
                    });
                }
                if !relation
                    .roles
                    .iter()
                    .any(|candidate| candidate.label == *role)
                {
                    return Err(KernelCompileError::UnknownRoleValue {
                        instance: instance.name.clone(),
                        relation: relation.label.clone(),
                        role: role.clone(),
                    });
                }
            }
            for role in &relation.roles {
                if !values.contains_key(&role.label) {
                    return Err(KernelCompileError::MissingRoleValue {
                        instance: instance.name.clone(),
                        relation: relation.label.clone(),
                        role: role.label.clone(),
                    });
                }
            }
            pending.push(PendingFact {
                relation_label: relation.label.clone(),
                relation_id: relation.relation_id.clone(),
                local_label: label.clone(),
                fields: values,
            });
        }
    }

    let mut resolved_labels = BTreeMap::<String, FactIdV2>::new();
    let mut facts = Vec::new();
    let mut fact_ids = BTreeSet::new();
    let mut unresolved = pending;
    while !unresolved.is_empty() {
        let mut next = Vec::new();
        let mut progress = false;
        for pending_fact in unresolved {
            let relation = relation_by_label[&pending_fact.relation_label];
            let mut role_values = Vec::new();
            let mut blocked = false;
            for role in &relation.roles {
                let raw = &pending_fact.fields[&role.label];
                let value = match role.type_expr.carrier() {
                    SchemaObjectRefIr::ObjectType { ref object_type_id } => {
                        let object_ref = SchemaObjectRefIr::ObjectType {
                            object_type_id: object_type_id.clone(),
                        };
                        let carrier = carriers.get(&object_ref).cloned().unwrap_or_default();
                        if !carrier.contains(raw) {
                            return Err(KernelCompileError::OutOfCodomain {
                                instance: instance.name.clone(),
                                relation: relation.label.clone(),
                                role: role.label.clone(),
                                value: raw.clone(),
                                target: object_label(schema, &object_ref),
                            });
                        }
                        TypedValueIr::ObjectElement { value: raw.clone() }
                    }
                    SchemaObjectRefIr::RelationObject { ref relation_id } => {
                        let expected_relation = schema
                            .relations
                            .iter()
                            .find(|candidate| candidate.relation_id == *relation_id)
                            .expect("compiled role target relation exists");
                        match (fact_label_relation.get(raw), resolved_labels.get(raw)) {
                            (Some(actual), Some(fact_id)) if actual == &expected_relation.label => {
                                TypedValueIr::RelationFact {
                                    fact_id: fact_id.clone(),
                                }
                            }
                            (Some(actual), _) if actual != &expected_relation.label => {
                                return Err(KernelCompileError::UnknownRelationFact {
                                    instance: instance.name.clone(),
                                    relation: relation.label.clone(),
                                    role: role.label.clone(),
                                    label: raw.clone(),
                                    target_relation: expected_relation.label.clone(),
                                });
                            }
                            (Some(_), None) => {
                                blocked = true;
                                break;
                            }
                            (Some(_), Some(fact_id)) => TypedValueIr::RelationFact {
                                fact_id: fact_id.clone(),
                            },
                            (None, _) => {
                                return Err(KernelCompileError::UnknownRelationFact {
                                    instance: instance.name.clone(),
                                    relation: relation.label.clone(),
                                    role: role.label.clone(),
                                    label: raw.clone(),
                                    target_relation: expected_relation.label.clone(),
                                });
                            }
                        }
                    }
                };
                role_values.push(RoleValueIr {
                    role_id: role.role_id.clone(),
                    value,
                });
            }
            if blocked {
                next.push(pending_fact);
                continue;
            }
            let derivation_values = role_values
                .iter()
                .map(|value| (value.role_id.clone(), value.value.wire_value()))
                .collect::<Vec<_>>();
            let fact_fields = derivation_values
                .iter()
                .map(|(role_id, value)| FactRoleValueV2 {
                    role_id,
                    value: value.as_bytes(),
                })
                .collect::<Vec<_>>();
            let fact_id = FactIdV2::derive(
                revision,
                &schema.schema_id,
                &instance_id,
                &pending_fact.relation_id,
                &fact_fields,
            );
            if !fact_ids.insert(fact_id.clone()) {
                return Err(KernelCompileError::DuplicateFactId {
                    instance: instance.name.clone(),
                    fact_id,
                });
            }
            if let Some(label) = &pending_fact.local_label {
                resolved_labels.insert(label.clone(), fact_id.clone());
            }
            carriers
                .entry(SchemaObjectRefIr::RelationObject {
                    relation_id: pending_fact.relation_id.clone(),
                })
                .or_default()
                .insert(fact_id.to_string());
            facts.push(RelationFactIr {
                fact_id,
                relation_id: pending_fact.relation_id,
                local_label: pending_fact.local_label,
                ordered_role_values: role_values,
            });
            progress = true;
        }
        if !progress {
            let labels = next
                .iter()
                .map(|fact| {
                    fact.local_label
                        .clone()
                        .unwrap_or_else(|| format!("<unlabeled:{}>", fact.relation_label))
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(KernelCompileError::CyclicFactReferences {
                instance: instance.name.clone(),
                labels,
            });
        }
        unresolved = next;
    }
    facts.sort_by(|left, right| left.fact_id.cmp(&right.fact_id));

    let mut functions = Vec::new();
    for generator in &schema.generators {
        let source_elements = carriers.get(&generator.source).cloned().unwrap_or_default();
        let target_elements = carriers.get(&generator.target).cloned().unwrap_or_default();
        let mappings = match generator.kind {
            SchemaGeneratorKindIr::RoleProjection => {
                let SchemaGeneratorRefIr::RoleProjection { role_id } = &generator.generator_ref
                else {
                    unreachable!("role projection kind/ref agree")
                };
                let mut mappings = Vec::new();
                for fact in &facts {
                    if fact.relation_id
                        != match &generator.source {
                            SchemaObjectRefIr::RelationObject { relation_id } => {
                                relation_id.clone()
                            }
                            _ => unreachable!("role projection source is relation object"),
                        }
                    {
                        continue;
                    }
                    let value = fact
                        .ordered_role_values
                        .iter()
                        .find(|value| &value.role_id == role_id)
                        .expect("compiler required every role exactly once");
                    mappings.push(FunctionMappingIr {
                        source: fact.fact_id.to_string(),
                        target: value.value.wire_value(),
                    });
                }
                mappings
            }
            SchemaGeneratorKindIr::SubtypeInclusion => source_elements
                .iter()
                .map(|element| FunctionMappingIr {
                    source: element.clone(),
                    target: element.clone(),
                })
                .collect(),
            SchemaGeneratorKindIr::Aspect | SchemaGeneratorKindIr::Function => {
                compile_explicit_generator_mapping(
                    instance,
                    generator,
                    assignments.get(&generator.label).copied(),
                    &source_elements,
                    &target_elements,
                )?
            }
        };
        validate_function_total_single_codomain(
            instance,
            generator,
            &mappings,
            &source_elements,
            &target_elements,
        )?;
        if generator.kind == SchemaGeneratorKindIr::SubtypeInclusion {
            let targets = mappings
                .iter()
                .map(|mapping| mapping.target.clone())
                .collect::<BTreeSet<_>>();
            if targets.len() != mappings.len() {
                let duplicate = mappings
                    .iter()
                    .map(|mapping| mapping.target.clone())
                    .find(|target| mappings.iter().filter(|m| &m.target == target).count() > 1)
                    .unwrap_or_default();
                return Err(KernelCompileError::NonInjectiveSubtype {
                    instance: instance.name.clone(),
                    generator: generator.label.clone(),
                    target: duplicate,
                });
            }
        }
        functions.push(FunctionInterpretationIr {
            generator: generator.generator_ref.clone(),
            source: generator.source.clone(),
            target: generator.target.clone(),
            mappings,
        });
    }

    validate_equations(instance, schema, &carriers, &functions)?;
    validate_constraints(&instance.name, schema, theories, &facts)?;
    let carriers = carriers
        .into_iter()
        .map(|(object, elements)| CarrierInterpretationIr {
            object,
            elements: elements.into_iter().collect(),
        })
        .collect::<Vec<_>>();
    let object_membership_witnesses =
        crate::build_object_membership_witnesses(&instance_id, &carriers, &facts);
    let (role_witnesses, scope_witnesses) =
        crate::build_dependent_witnesses(&instance_id, schema, &facts)
            .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
    let typed_constraint_witnesses =
        crate::build_typed_constraint_witnesses(&instance_id, schema, theories)
            .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
    let dependent_contexts = crate::build_dependent_contexts(
        &instance_id,
        &object_membership_witnesses,
        &scope_witnesses,
    )
    .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;

    let model = InstanceModelIr {
        version: INSTANCE_MODEL_IR_VERSION.to_string(),
        module_id: module_id.clone(),
        module_name: module_name.to_string(),
        revision: revision.clone(),
        semantic_key,
        instance_id,
        schema_id: schema.schema_id.clone(),
        label: instance.name.clone(),
        carriers,
        functions,
        facts,
        validation: checked_finite_model_validation(),
        object_membership_witnesses,
        role_witnesses,
        typed_constraint_witnesses,
        scope_witnesses,
        dependent_contexts,
        lifecycle: crate::CheckedLifecycleStateIr::ExplanationVerified,
        residual_obligations: Vec::new(),
    };
    validate_instance_model_ir(schema, theories, &model)?;
    Ok(model)
}

/// Revalidate an externally transported or persisted finite interpretation
/// together with its exact compiled theory slice. This repeats structural,
/// dependent-fiber, refinement, equation, and every `finite_model_checked`
/// constraint check. It is a Rust decision procedure, not a Lean proof.
pub fn validate_instance_model_ir(
    schema: &SchemaPresentationIr,
    theories: &[&TypedTheoryIr],
    model: &InstanceModelIr,
) -> Result<(), KernelCompileError> {
    if model.version != INSTANCE_MODEL_IR_VERSION {
        return Err(KernelCompileError::InstanceIdentityMismatch {
            instance: model.label.clone(),
            detail: format!(
                "wire version `{}` is not `{INSTANCE_MODEL_IR_VERSION}`",
                model.version
            ),
        });
    }
    if model.schema_id != schema.schema_id {
        return Err(KernelCompileError::InstanceIdentityMismatch {
            instance: model.label.clone(),
            detail: "schema id differs from the validated presentation".to_string(),
        });
    }
    let expected_instance_id =
        InstanceIdV2::derive(&model.revision, &model.schema_id, &model.semantic_key);
    if model.instance_id != expected_instance_id {
        return Err(KernelCompileError::InstanceIdentityMismatch {
            instance: model.label.clone(),
            detail: "instance id does not match revision, schema, and semantic key".to_string(),
        });
    }
    if let Some(theory) = theories
        .iter()
        .find(|theory| theory.schema_id != schema.schema_id)
    {
        return Err(KernelCompileError::InstanceIdentityMismatch {
            instance: model.label.clone(),
            detail: format!("theory `{}` belongs to a different schema", theory.label),
        });
    }

    let expected_objects = schema.object_refs().into_iter().collect::<BTreeSet<_>>();
    let mut carriers = BTreeMap::<SchemaObjectRefIr, BTreeSet<String>>::new();
    for carrier in &model.carriers {
        if !expected_objects.contains(&carrier.object) {
            return Err(KernelCompileError::UnexpectedCarrier {
                instance: model.label.clone(),
                object: carrier.object.clone(),
            });
        }
        let mut elements = BTreeSet::new();
        for element in &carrier.elements {
            if !elements.insert(element.clone()) {
                return Err(KernelCompileError::DuplicateCarrierElement {
                    instance: model.label.clone(),
                    object: carrier.object.clone(),
                    element: element.clone(),
                });
            }
        }
        if carriers.insert(carrier.object.clone(), elements).is_some() {
            return Err(KernelCompileError::DuplicateCarrier {
                instance: model.label.clone(),
                object: carrier.object.clone(),
            });
        }
    }
    for object in &expected_objects {
        if !carriers.contains_key(object) {
            return Err(KernelCompileError::MissingCarrier {
                instance: model.label.clone(),
                object: object.clone(),
            });
        }
    }

    let expected_generators = schema
        .generators
        .iter()
        .map(|generator| generator.generator_ref.clone())
        .collect::<BTreeSet<_>>();
    let mut functions = BTreeMap::<SchemaGeneratorRefIr, &FunctionInterpretationIr>::new();
    for function in &model.functions {
        if !expected_generators.contains(&function.generator) {
            return Err(KernelCompileError::UnexpectedGeneratorInterpretation {
                instance: model.label.clone(),
                generator: function.generator.clone(),
            });
        }
        if functions
            .insert(function.generator.clone(), function)
            .is_some()
        {
            return Err(KernelCompileError::DuplicateGeneratorInterpretation {
                instance: model.label.clone(),
                generator: function.generator.clone(),
            });
        }
    }

    for generator in &schema.generators {
        let Some(function) = functions.get(&generator.generator_ref) else {
            return Err(KernelCompileError::MissingGeneratorInterpretation {
                instance: model.label.clone(),
                generator: generator.label.clone(),
            });
        };
        if function.source != generator.source || function.target != generator.target {
            return Err(KernelCompileError::GeneratorEndpointMismatch {
                instance: model.label.clone(),
                generator: generator.label.clone(),
            });
        }
        let source = &carriers[&generator.source];
        let target = &carriers[&generator.target];
        let mut seen_sources = BTreeMap::new();
        for mapping in &function.mappings {
            if !source.contains(&mapping.source) || !target.contains(&mapping.target) {
                return Err(KernelCompileError::OutOfCodomain {
                    instance: model.label.clone(),
                    relation: generator.label.clone(),
                    role: if source.contains(&mapping.source) {
                        "target".to_string()
                    } else {
                        "source".to_string()
                    },
                    value: if source.contains(&mapping.source) {
                        mapping.target.clone()
                    } else {
                        mapping.source.clone()
                    },
                    target: object_label(
                        schema,
                        if source.contains(&mapping.source) {
                            &generator.target
                        } else {
                            &generator.source
                        },
                    ),
                });
            }
            if seen_sources
                .insert(mapping.source.clone(), mapping.target.clone())
                .is_some()
            {
                return Err(KernelCompileError::NonFunctionalGenerator {
                    instance: model.label.clone(),
                    generator: generator.label.clone(),
                    source_element: mapping.source.clone(),
                });
            }
        }
        let missing = source
            .iter()
            .filter(|element| !seen_sources.contains_key(*element))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(KernelCompileError::PartialGenerator {
                instance: model.label.clone(),
                generator: generator.label.clone(),
                missing: missing.join(", "),
            });
        }
        if generator.kind == SchemaGeneratorKindIr::SubtypeInclusion {
            let image = function
                .mappings
                .iter()
                .map(|mapping| mapping.target.clone())
                .collect::<BTreeSet<_>>();
            if image.len() != function.mappings.len() {
                let duplicate = function
                    .mappings
                    .iter()
                    .map(|mapping| mapping.target.clone())
                    .find(|target| {
                        function
                            .mappings
                            .iter()
                            .filter(|mapping| &mapping.target == target)
                            .count()
                            > 1
                    })
                    .unwrap_or_default();
                return Err(KernelCompileError::NonInjectiveSubtype {
                    instance: model.label.clone(),
                    generator: generator.label.clone(),
                    target: duplicate,
                });
            }
        }
    }

    let relation_by_id = schema
        .relations
        .iter()
        .map(|relation| (relation.relation_id.clone(), relation))
        .collect::<BTreeMap<_, _>>();
    let mut fact_ids = BTreeSet::new();
    let mut fact_by_id = BTreeMap::new();
    let mut local_labels = BTreeSet::new();
    for fact in &model.facts {
        if !fact_ids.insert(fact.fact_id.clone()) {
            return Err(KernelCompileError::DuplicateFactId {
                instance: model.label.clone(),
                fact_id: fact.fact_id.clone(),
            });
        }
        if let Some(label) = &fact.local_label {
            if !local_labels.insert(label.clone()) {
                return Err(KernelCompileError::DuplicateFactLabel {
                    instance: model.label.clone(),
                    label: label.clone(),
                });
            }
        }
        if !relation_by_id.contains_key(&fact.relation_id) {
            return Err(KernelCompileError::UnknownFactRelation {
                instance: model.label.clone(),
                fact_id: fact.fact_id.clone(),
                relation_id: fact.relation_id.clone(),
            });
        }
        fact_by_id.insert(fact.fact_id.clone(), fact);
    }

    for relation in &schema.relations {
        let relation_ref = SchemaObjectRefIr::RelationObject {
            relation_id: relation.relation_id.clone(),
        };
        let expected = model
            .facts
            .iter()
            .filter(|fact| fact.relation_id == relation.relation_id)
            .map(|fact| fact.fact_id.to_string())
            .collect::<BTreeSet<_>>();
        if carriers[&relation_ref] != expected {
            return Err(KernelCompileError::RelationCarrierMismatch {
                instance: model.label.clone(),
                relation: relation.label.clone(),
            });
        }
    }

    for fact in &model.facts {
        let relation = relation_by_id[&fact.relation_id];
        if fact.ordered_role_values.len() != relation.roles.len() {
            return Err(KernelCompileError::MissingRoleValue {
                instance: model.label.clone(),
                relation: relation.label.clone(),
                role: "<projection-count>".to_string(),
            });
        }
        for (role, role_value) in relation.roles.iter().zip(&fact.ordered_role_values) {
            if role_value.role_id != role.role_id {
                return Err(KernelCompileError::MissingRoleValue {
                    instance: model.label.clone(),
                    relation: relation.label.clone(),
                    role: role.label.clone(),
                });
            }
            let target = role.type_expr.carrier();
            match (&target, &role_value.value) {
                (SchemaObjectRefIr::ObjectType { .. }, TypedValueIr::ObjectElement { .. }) => {}
                (
                    SchemaObjectRefIr::RelationObject { relation_id },
                    TypedValueIr::RelationFact { fact_id },
                ) => {
                    let Some(target_fact) = fact_by_id.get(fact_id) else {
                        return Err(KernelCompileError::OutOfCodomain {
                            instance: model.label.clone(),
                            relation: relation.label.clone(),
                            role: role.label.clone(),
                            value: fact_id.to_string(),
                            target: object_label(schema, &target),
                        });
                    };
                    if &target_fact.relation_id != relation_id {
                        return Err(KernelCompileError::TypedValueKindMismatch {
                            instance: model.label.clone(),
                            relation: relation.label.clone(),
                            role: role.label.clone(),
                        });
                    }
                }
                _ => {
                    return Err(KernelCompileError::TypedValueKindMismatch {
                        instance: model.label.clone(),
                        relation: relation.label.clone(),
                        role: role.label.clone(),
                    });
                }
            }
            let wire_value = role_value.value.wire_value();
            let target_carrier = &carriers[&target];
            if !target_carrier.contains(&wire_value) {
                return Err(KernelCompileError::OutOfCodomain {
                    instance: model.label.clone(),
                    relation: relation.label.clone(),
                    role: role.label.clone(),
                    value: wire_value,
                    target: object_label(schema, &target),
                });
            }
            let fiber_values = crate::finite_role_fiber_values(schema, model, relation, role, fact)
                .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
            if !fiber_values
                .iter()
                .any(|candidate| candidate == &wire_value)
            {
                return Err(KernelCompileError::OutOfCodomain {
                    instance: model.label.clone(),
                    relation: relation.label.clone(),
                    role: role.label.clone(),
                    value: wire_value,
                    target: format!("indexed fiber of {}", object_label(schema, &target)),
                });
            }
            validate_value_refinements(
                &model.label,
                relation,
                role,
                &wire_value,
                fiber_values.len(),
            )?;
            let projection_ref = SchemaGeneratorRefIr::RoleProjection {
                role_id: role.role_id.clone(),
            };
            let projection = functions.get(&projection_ref).ok_or_else(|| {
                KernelCompileError::MissingGeneratorInterpretation {
                    instance: model.label.clone(),
                    generator: format!("{}.{}", relation.label, role.label),
                }
            })?;
            let projection_values = projection
                .mappings
                .iter()
                .filter(|mapping| mapping.source == fact.fact_id.to_string())
                .map(|mapping| mapping.target.as_str())
                .collect::<Vec<_>>();
            if projection_values.as_slice() != [wire_value.as_str()] {
                return Err(KernelCompileError::ProjectionMismatch {
                    instance: model.label.clone(),
                    fact_id: fact.fact_id.clone(),
                    role: role.label.clone(),
                });
            }
        }
        let values = fact
            .ordered_role_values
            .iter()
            .map(|value| (value.role_id.clone(), value.value.wire_value()))
            .collect::<Vec<_>>();
        let framed = values
            .iter()
            .map(|(role_id, value)| FactRoleValueV2 {
                role_id,
                value: value.as_bytes(),
            })
            .collect::<Vec<_>>();
        let recomputed = FactIdV2::derive(
            &model.revision,
            &model.schema_id,
            &model.instance_id,
            &fact.relation_id,
            &framed,
        );
        if recomputed != fact.fact_id {
            return Err(KernelCompileError::FactIdMismatch {
                instance: model.label.clone(),
                stored: fact.fact_id.clone(),
                recomputed,
            });
        }
    }

    for equation in &schema.equations {
        let sources = &carriers[&equation.lhs.source];
        for source in sources {
            let lhs = evaluate_path(&equation.lhs, source, &functions);
            let rhs = evaluate_path(&equation.rhs, source, &functions);
            if lhs != rhs {
                return Err(KernelCompileError::ViolatedEquation {
                    instance: model.label.clone(),
                    equation: equation.label.clone(),
                    source_element: source.clone(),
                    lhs,
                    rhs,
                });
            }
        }
    }
    validate_constraints(&model.label, schema, theories, &model.facts)?;

    let expected_memberships =
        crate::build_object_membership_witnesses(&model.instance_id, &model.carriers, &model.facts);
    let (expected_roles, expected_scopes) =
        crate::build_dependent_witnesses(&model.instance_id, schema, &model.facts)
            .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
    let expected_constraints =
        crate::build_typed_constraint_witnesses(&model.instance_id, schema, theories)
            .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
    let expected_contexts = crate::build_dependent_contexts(
        &model.instance_id,
        &expected_memberships,
        &expected_scopes,
    )
    .map_err(|error| KernelCompileError::FiniteTheory(error.to_string()))?;
    if model.object_membership_witnesses != expected_memberships
        || model.role_witnesses != expected_roles
        || model.typed_constraint_witnesses != expected_constraints
        || model.scope_witnesses != expected_scopes
        || model.dependent_contexts != expected_contexts
    {
        return Err(KernelCompileError::FiniteTheory(format!(
            "instance `{}` dependent witness payload does not replay",
            model.label
        )));
    }
    if model.validation != checked_finite_model_validation() {
        return Err(KernelCompileError::ValidationClaimMismatch {
            instance: model.label.clone(),
            detail: "stored finite-model validation flags or non-claims differ from replay"
                .to_string(),
        });
    }
    if model.lifecycle != crate::CheckedLifecycleStateIr::ExplanationVerified
        || !model.residual_obligations.is_empty()
    {
        return Err(KernelCompileError::FiniteTheory(format!(
            "instance `{}` is not in a closed checked lifecycle state",
            model.label
        )));
    }
    Ok(())
}

fn object_label(schema: &SchemaPresentationIr, object: &SchemaObjectRefIr) -> String {
    match object {
        SchemaObjectRefIr::ObjectType { object_type_id } => schema
            .objects
            .iter()
            .find(|object| &object.object_type_id == object_type_id)
            .map(|object| object.label.clone())
            .unwrap_or_else(|| object_type_id.to_string()),
        SchemaObjectRefIr::RelationObject { relation_id } => schema
            .relations
            .iter()
            .find(|relation| &relation.relation_id == relation_id)
            .map(|relation| relation.label.clone())
            .unwrap_or_else(|| relation_id.to_string()),
    }
}

fn validate_value_refinements(
    instance_label: &str,
    relation: &RelationObjectIr,
    role: &RoleProjectionIr,
    value: &str,
    carrier_size: usize,
) -> Result<(), KernelCompileError> {
    fn walk(predicates: &TypeExprIr, value: &str, carrier_size: usize) -> Result<(), String> {
        match predicates {
            TypeExprIr::Refined { base, predicates } => {
                walk(base, value, carrier_size)?;
                for predicate in predicates {
                    match predicate {
                        RefinementPredicateIr::Equals { value: expected } if value != expected => {
                            return Err(format!("value `{value}` does not equal `{expected}`"));
                        }
                        RefinementPredicateIr::MemberOf { values }
                        | RefinementPredicateIr::Enum { values }
                            if !values.iter().any(|candidate| candidate == value) =>
                        {
                            return Err(format!(
                                "value `{value}` is outside finite set {values:?}"
                            ));
                        }
                        RefinementPredicateIr::Cardinality { min, max }
                            if carrier_size < *min as usize || carrier_size > *max as usize =>
                        {
                            return Err(format!(
                                "target carrier cardinality {carrier_size} is outside {min}..={max}"
                            ));
                        }
                        RefinementPredicateIr::Predicate { name, .. }
                            if name == "non_empty" && value.is_empty() =>
                        {
                            return Err("value is empty".to_string());
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            TypeExprIr::Indexed { base, .. } => walk(base, value, carrier_size),
            TypeExprIr::Object { .. } | TypeExprIr::RelationObject { .. } => Ok(()),
        }
    }
    walk(&role.type_expr, value, carrier_size).map_err(|detail| {
        KernelCompileError::ViolatedConstraint {
            instance: instance_label.to_string(),
            constraint: format!("refinement:{}.{}", relation.label, role.label),
            detail,
        }
    })
}

fn compile_explicit_generator_mapping(
    instance: &SchemaV1Instance,
    generator: &SchemaGeneratorIr,
    assignment: Option<&axiograph_dsl::schema_v1::SetLiteralV1>,
    source_elements: &BTreeSet<String>,
    target_elements: &BTreeSet<String>,
) -> Result<Vec<FunctionMappingIr>, KernelCompileError> {
    let mut mappings = Vec::new();
    if let Some(set) = assignment {
        for item in &set.items {
            let SetItemV1::Tuple {
                label: None,
                fields,
            } = item
            else {
                return Err(KernelCompileError::InvalidAssignmentShape {
                    instance: instance.name.clone(),
                    assignment: generator.label.clone(),
                    detail: "aspect/function assignments use unlabeled `(source=..., target=...)` tuples".to_string(),
                });
            };
            let mut values = BTreeMap::new();
            for (field, value) in fields {
                if values.insert(field.as_str(), value.as_str()).is_some() {
                    return Err(KernelCompileError::DuplicateRoleValue {
                        instance: instance.name.clone(),
                        relation: generator.label.clone(),
                        role: field.clone(),
                    });
                }
            }
            if values.len() != 2 || !values.contains_key("source") || !values.contains_key("target")
            {
                return Err(KernelCompileError::InvalidAssignmentShape {
                    instance: instance.name.clone(),
                    assignment: generator.label.clone(),
                    detail: "aspect/function tuple requires exactly `source` and `target`"
                        .to_string(),
                });
            }
            mappings.push(FunctionMappingIr {
                source: values["source"].to_string(),
                target: values["target"].to_string(),
            });
        }
    }
    let _ = (source_elements, target_elements);
    Ok(mappings)
}

fn validate_function_total_single_codomain(
    instance: &SchemaV1Instance,
    generator: &SchemaGeneratorIr,
    mappings: &[FunctionMappingIr],
    source_elements: &BTreeSet<String>,
    target_elements: &BTreeSet<String>,
) -> Result<(), KernelCompileError> {
    let mut by_source = BTreeMap::<String, String>::new();
    for mapping in mappings {
        if !source_elements.contains(&mapping.source) {
            return Err(KernelCompileError::OutOfCodomain {
                instance: instance.name.clone(),
                relation: generator.label.clone(),
                role: "source".to_string(),
                value: mapping.source.clone(),
                target: "generator source carrier".to_string(),
            });
        }
        if !target_elements.contains(&mapping.target) {
            return Err(KernelCompileError::OutOfCodomain {
                instance: instance.name.clone(),
                relation: generator.label.clone(),
                role: "target".to_string(),
                value: mapping.target.clone(),
                target: "generator target carrier".to_string(),
            });
        }
        if let Some(existing) = by_source.insert(mapping.source.clone(), mapping.target.clone()) {
            if existing != mapping.target {
                return Err(KernelCompileError::NonFunctionalGenerator {
                    instance: instance.name.clone(),
                    generator: generator.label.clone(),
                    source_element: mapping.source.clone(),
                });
            }
            return Err(KernelCompileError::NonFunctionalGenerator {
                instance: instance.name.clone(),
                generator: generator.label.clone(),
                source_element: mapping.source.clone(),
            });
        }
    }
    let missing = source_elements
        .iter()
        .filter(|source| !by_source.contains_key(*source))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(KernelCompileError::PartialGenerator {
            instance: instance.name.clone(),
            generator: generator.label.clone(),
            missing: missing.join(", "),
        });
    }
    Ok(())
}

fn validate_equations(
    instance: &SchemaV1Instance,
    schema: &SchemaPresentationIr,
    carriers: &BTreeMap<SchemaObjectRefIr, BTreeSet<String>>,
    functions: &[FunctionInterpretationIr],
) -> Result<(), KernelCompileError> {
    let function_map = functions
        .iter()
        .map(|function| (function.generator.clone(), function))
        .collect::<BTreeMap<_, _>>();
    for equation in &schema.equations {
        let sources = carriers
            .get(&equation.lhs.source)
            .cloned()
            .unwrap_or_default();
        for source in sources {
            let lhs = evaluate_path(&equation.lhs, &source, &function_map);
            let rhs = evaluate_path(&equation.rhs, &source, &function_map);
            if lhs != rhs {
                return Err(KernelCompileError::ViolatedEquation {
                    instance: instance.name.clone(),
                    equation: equation.label.clone(),
                    source_element: source,
                    lhs,
                    rhs,
                });
            }
        }
    }
    Ok(())
}

fn evaluate_path(
    path: &SchemaPathIr,
    source: &str,
    functions: &BTreeMap<SchemaGeneratorRefIr, &FunctionInterpretationIr>,
) -> Option<String> {
    let mut current = source.to_string();
    for step in &path.steps {
        let function = functions.get(step)?;
        current = function
            .mappings
            .iter()
            .find(|mapping| mapping.source == current)?
            .target
            .clone();
    }
    Some(current)
}

fn validate_constraints(
    instance_label: &str,
    schema: &SchemaPresentationIr,
    theories: &[&TypedTheoryIr],
    facts: &[RelationFactIr],
) -> Result<(), KernelCompileError> {
    let relation_by_name = schema
        .relations
        .iter()
        .map(|relation| (relation.label.as_str(), relation))
        .collect::<BTreeMap<_, _>>();
    for obligation in theories.iter().flat_map(|theory| &theory.constraints) {
        if !obligation.finite_model_checked {
            continue;
        }
        validate_constraint_runtime_shape(
            instance_label,
            &obligation.label,
            &obligation.source,
            &relation_by_name,
        )?;
        match &obligation.source {
            ConstraintV1::Functional {
                relation,
                src_field,
                dst_field,
            } => check_at_most(
                instance_label,
                &obligation.label,
                relation_by_name[relation.as_str()],
                facts,
                src_field,
                dst_field,
                &[],
                1,
            )?,
            ConstraintV1::AtMost {
                relation,
                src_field,
                dst_field,
                max,
                params,
            } => check_at_most(
                instance_label,
                &obligation.label,
                relation_by_name[relation.as_str()],
                facts,
                src_field,
                dst_field,
                params.as_deref().unwrap_or_default(),
                *max,
            )?,
            ConstraintV1::Key { relation, fields } => check_key(
                instance_label,
                &obligation.label,
                relation_by_name[relation.as_str()],
                facts,
                fields,
            )?,
            ConstraintV1::Symmetric {
                relation,
                carriers,
                params,
            } => check_symmetric(
                instance_label,
                &obligation.label,
                relation_by_name[relation.as_str()],
                facts,
                carriers.as_ref(),
                params.as_deref().unwrap_or_default(),
                None,
            )?,
            ConstraintV1::SymmetricWhereIn {
                relation,
                field,
                values,
                carriers,
                params,
            } => check_symmetric(
                instance_label,
                &obligation.label,
                relation_by_name[relation.as_str()],
                facts,
                carriers.as_ref(),
                params.as_deref().unwrap_or_default(),
                Some((field, values)),
            )?,
            ConstraintV1::Transitive {
                relation,
                carriers,
                params,
            } => check_transitive(
                instance_label,
                &obligation.label,
                relation_by_name[relation.as_str()],
                facts,
                carriers.as_ref(),
                params.as_deref().unwrap_or_default(),
            )?,
            ConstraintV1::Typing { .. }
            | ConstraintV1::NamedBlock { .. }
            | ConstraintV1::Unknown { .. } => {}
        }
    }
    Ok(())
}

fn validate_constraint_runtime_shape(
    instance_label: &str,
    constraint_label: &str,
    constraint: &ConstraintV1,
    relations: &BTreeMap<&str, &RelationObjectIr>,
) -> Result<(), KernelCompileError> {
    let (relation_name, fields, requires_default_pair): (&str, Vec<&str>, bool) = match constraint {
        ConstraintV1::Functional {
            relation,
            src_field,
            dst_field,
        } => (relation, vec![src_field, dst_field], false),
        ConstraintV1::AtMost {
            relation,
            src_field,
            dst_field,
            params,
            ..
        } => (
            relation,
            std::iter::once(src_field.as_str())
                .chain(std::iter::once(dst_field.as_str()))
                .chain(params.iter().flatten().map(String::as_str))
                .collect(),
            false,
        ),
        ConstraintV1::Key { relation, fields } => {
            (relation, fields.iter().map(String::as_str).collect(), false)
        }
        ConstraintV1::Symmetric {
            relation,
            carriers,
            params,
        }
        | ConstraintV1::Transitive {
            relation,
            carriers,
            params,
        } => (
            relation,
            carriers
                .iter()
                .flat_map(|pair| [pair.left_field.as_str(), pair.right_field.as_str()])
                .chain(params.iter().flatten().map(String::as_str))
                .collect(),
            carriers.is_none(),
        ),
        ConstraintV1::SymmetricWhereIn {
            relation,
            field,
            carriers,
            params,
            ..
        } => (
            relation,
            std::iter::once(field.as_str())
                .chain(
                    carriers
                        .iter()
                        .flat_map(|pair| [pair.left_field.as_str(), pair.right_field.as_str()]),
                )
                .chain(params.iter().flatten().map(String::as_str))
                .collect(),
            carriers.is_none(),
        ),
        ConstraintV1::Typing { .. }
        | ConstraintV1::NamedBlock { .. }
        | ConstraintV1::Unknown { .. } => return Ok(()),
    };
    let relation = relations.get(relation_name).copied().ok_or_else(|| {
        KernelCompileError::ViolatedConstraint {
            instance: instance_label.to_string(),
            constraint: constraint_label.to_string(),
            detail: format!("constraint references unknown relation `{relation_name}`"),
        }
    })?;
    if requires_default_pair && relation.roles.len() < 2 {
        return Err(KernelCompileError::ViolatedConstraint {
            instance: instance_label.to_string(),
            constraint: constraint_label.to_string(),
            detail: format!(
                "relation `{relation_name}` has fewer than two roles for the default carrier pair"
            ),
        });
    }
    for field in fields {
        if !relation.roles.iter().any(|role| role.label == field) {
            return Err(KernelCompileError::ViolatedConstraint {
                instance: instance_label.to_string(),
                constraint: constraint_label.to_string(),
                detail: format!("constraint references unknown role `{relation_name}.{field}`"),
            });
        }
    }
    Ok(())
}

fn fact_role_value(
    fact: &RelationFactIr,
    relation: &RelationObjectIr,
    role: &str,
) -> Option<String> {
    let role_id = &relation
        .roles
        .iter()
        .find(|candidate| candidate.label == role)?
        .role_id;
    fact.ordered_role_values
        .iter()
        .find(|value| &value.role_id == role_id)
        .map(|value| value.value.wire_value())
}

fn relation_facts<'a>(
    relation: &RelationObjectIr,
    facts: &'a [RelationFactIr],
) -> impl Iterator<Item = &'a RelationFactIr> + 'a {
    let relation_id = relation.relation_id.clone();
    facts
        .iter()
        .filter(move |fact| fact.relation_id == relation_id)
}

#[allow(clippy::too_many_arguments)]
fn check_at_most(
    instance_label: &str,
    constraint: &str,
    relation: &RelationObjectIr,
    facts: &[RelationFactIr],
    source: &str,
    target: &str,
    params: &[String],
    max: u32,
) -> Result<(), KernelCompileError> {
    let mut targets = BTreeMap::<Vec<String>, BTreeSet<String>>::new();
    for fact in relation_facts(relation, facts) {
        let mut key = params
            .iter()
            .map(|param| fact_role_value(fact, relation, param).expect("formed role"))
            .collect::<Vec<_>>();
        key.push(fact_role_value(fact, relation, source).expect("formed role"));
        targets
            .entry(key)
            .or_default()
            .insert(fact_role_value(fact, relation, target).expect("formed role"));
    }
    if let Some((key, values)) = targets
        .iter()
        .find(|(_, values)| values.len() > max as usize)
    {
        return Err(KernelCompileError::ViolatedConstraint {
            instance: instance_label.to_string(),
            constraint: constraint.to_string(),
            detail: format!(
                "key {key:?} maps to {} distinct targets, maximum is {max}",
                values.len()
            ),
        });
    }
    Ok(())
}

fn check_key(
    instance_label: &str,
    constraint: &str,
    relation: &RelationObjectIr,
    facts: &[RelationFactIr],
    fields: &[String],
) -> Result<(), KernelCompileError> {
    let mut keys = BTreeSet::new();
    for fact in relation_facts(relation, facts) {
        let key = fields
            .iter()
            .map(|field| fact_role_value(fact, relation, field).expect("formed role"))
            .collect::<Vec<_>>();
        if !keys.insert(key.clone()) {
            return Err(KernelCompileError::ViolatedConstraint {
                instance: instance_label.to_string(),
                constraint: constraint.to_string(),
                detail: format!("duplicate key {key:?}"),
            });
        }
    }
    Ok(())
}

fn carrier_pair<'a>(
    relation: &'a RelationObjectIr,
    carriers: Option<&'a CarrierFieldsV1>,
) -> (&'a str, &'a str) {
    carriers
        .map(|carriers| (carriers.left_field.as_str(), carriers.right_field.as_str()))
        .unwrap_or_else(|| {
            (
                relation.roles[0].label.as_str(),
                relation.roles[1].label.as_str(),
            )
        })
}

fn check_symmetric(
    instance_label: &str,
    constraint: &str,
    relation: &RelationObjectIr,
    facts: &[RelationFactIr],
    carriers: Option<&CarrierFieldsV1>,
    params: &[String],
    guard: Option<(&String, &Vec<String>)>,
) -> Result<(), KernelCompileError> {
    let (left, right) = carrier_pair(relation, carriers);
    let tuples = relation_facts(relation, facts)
        .map(|fact| {
            let params = params
                .iter()
                .map(|param| fact_role_value(fact, relation, param).expect("formed role"))
                .collect::<Vec<_>>();
            let guard_value = guard
                .map(|(field, _)| fact_role_value(fact, relation, field).expect("formed role"));
            (
                params,
                fact_role_value(fact, relation, left).expect("formed role"),
                fact_role_value(fact, relation, right).expect("formed role"),
                guard_value,
            )
        })
        .collect::<BTreeSet<_>>();
    for (fiber, from, to, guard_value) in &tuples {
        if let Some((_, allowed)) = guard {
            if !guard_value
                .as_ref()
                .is_some_and(|value| allowed.iter().any(|candidate| candidate == value))
            {
                continue;
            }
        }
        if !tuples
            .iter()
            .any(|(candidate_fiber, candidate_from, candidate_to, _)| {
                candidate_fiber == fiber && candidate_from == to && candidate_to == from
            })
        {
            return Err(KernelCompileError::ViolatedConstraint {
                instance: instance_label.to_string(),
                constraint: constraint.to_string(),
                detail: format!("missing symmetric tuple ({to}, {from}) in fiber {fiber:?}"),
            });
        }
    }
    Ok(())
}

fn check_transitive(
    instance_label: &str,
    constraint: &str,
    relation: &RelationObjectIr,
    facts: &[RelationFactIr],
    carriers: Option<&CarrierFieldsV1>,
    params: &[String],
) -> Result<(), KernelCompileError> {
    let (left, right) = carrier_pair(relation, carriers);
    let tuples = relation_facts(relation, facts)
        .map(|fact| {
            (
                params
                    .iter()
                    .map(|param| fact_role_value(fact, relation, param).expect("formed role"))
                    .collect::<Vec<_>>(),
                fact_role_value(fact, relation, left).expect("formed role"),
                fact_role_value(fact, relation, right).expect("formed role"),
            )
        })
        .collect::<BTreeSet<_>>();
    for (fiber, from, middle) in &tuples {
        for (_, candidate_middle, to) in
            tuples
                .iter()
                .filter(|(candidate_fiber, candidate_middle, _)| {
                    candidate_fiber == fiber && candidate_middle == middle
                })
        {
            if !tuples.contains(&(fiber.clone(), from.clone(), to.clone())) {
                return Err(KernelCompileError::ViolatedConstraint {
                    instance: instance_label.to_string(),
                    constraint: constraint.to_string(),
                    detail: format!(
                        "missing transitive tuple ({from}, {to}) via {candidate_middle}"
                    ),
                });
            }
        }
    }
    Ok(())
}

fn build_refs(
    modules: &[CompiledModuleIr],
    schemas: &[SchemaPresentationIr],
    theories: &[TypedTheoryIr],
    instances: &[InstanceModelIr],
) -> BTreeSet<KernelRefV2> {
    let mut refs = BTreeSet::new();
    for module in modules {
        refs.insert(KernelRefV2::Module {
            module_id: module.module_id.clone(),
            revision: module.revision.clone(),
        });
    }
    for schema in schemas {
        refs.insert(KernelRefV2::Schema {
            module_id: schema.module_id.clone(),
            revision: schema.revision.clone(),
            semantic_key: schema.semantic_key.clone(),
            schema_id: schema.schema_id.clone(),
        });
        for object in &schema.objects {
            refs.insert(KernelRefV2::ObjectType {
                module_id: schema.module_id.clone(),
                revision: schema.revision.clone(),
                schema_id: schema.schema_id.clone(),
                semantic_key: object.semantic_key.clone(),
                object_type_id: object.object_type_id.clone(),
            });
        }
        for relation in &schema.relations {
            refs.insert(KernelRefV2::Relation {
                module_id: schema.module_id.clone(),
                revision: schema.revision.clone(),
                schema_id: schema.schema_id.clone(),
                semantic_key: relation.semantic_key.clone(),
                relation_id: relation.relation_id.clone(),
            });
            for role in &relation.roles {
                refs.insert(KernelRefV2::Role {
                    module_id: schema.module_id.clone(),
                    revision: schema.revision.clone(),
                    schema_id: schema.schema_id.clone(),
                    relation_id: relation.relation_id.clone(),
                    semantic_key: role.semantic_key.clone(),
                    role_id: role.role_id.clone(),
                });
            }
        }
        for generator in &schema.generators {
            refs.insert(KernelRefV2::Generator {
                module_id: schema.module_id.clone(),
                revision: schema.revision.clone(),
                schema_id: schema.schema_id.clone(),
                semantic_key: generator.semantic_key.clone(),
            });
        }
    }
    for theory in theories {
        refs.insert(KernelRefV2::Theory {
            module_id: theory.module_id.clone(),
            revision: theory.revision.clone(),
            schema_id: theory.schema_id.clone(),
            semantic_key: theory.semantic_key.clone(),
            theory_id: theory.theory_id.clone(),
        });
        for constraint in &theory.constraints {
            refs.insert(KernelRefV2::Constraint {
                module_id: theory.module_id.clone(),
                revision: theory.revision.clone(),
                theory_id: theory.theory_id.clone(),
                semantic_key: constraint.semantic_key.clone(),
                constraint_id: constraint.constraint_id.clone(),
            });
        }
        for equation in &theory.equations {
            refs.insert(KernelRefV2::Equation {
                module_id: theory.module_id.clone(),
                revision: theory.revision.clone(),
                theory_id: theory.theory_id.clone(),
                semantic_key: equation.semantic_key.clone(),
                equation_id: equation.equation_id.clone(),
            });
        }
        for rule in &theory.rewrite_rules {
            refs.insert(KernelRefV2::RewriteRule {
                module_id: theory.module_id.clone(),
                revision: theory.revision.clone(),
                theory_id: theory.theory_id.clone(),
                semantic_key: rule.semantic_key.clone(),
                rewrite_rule_id: rule.rewrite_rule_id.clone(),
            });
        }
    }
    let schema_by_id = schemas
        .iter()
        .map(|schema| (schema.schema_id.clone(), schema))
        .collect::<BTreeMap<_, _>>();
    for instance in instances {
        let schema = schema_by_id[&instance.schema_id];
        refs.insert(KernelRefV2::Instance {
            module_id: instance.module_id.clone(),
            revision: instance.revision.clone(),
            schema_id: instance.schema_id.clone(),
            semantic_key: instance.semantic_key.clone(),
            instance_id: instance.instance_id.clone(),
        });
        for fact in &instance.facts {
            refs.insert(KernelRefV2::Fact {
                module_id: instance.module_id.clone(),
                revision: instance.revision.clone(),
                schema_id: instance.schema_id.clone(),
                instance_id: instance.instance_id.clone(),
                relation_id: fact.relation_id.clone(),
                fact_id: fact.fact_id.clone(),
            });
        }
        let _ = schema;
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_id(label: &str) -> SnapshotIdV2 {
        SnapshotIdV2::from_canonical_fields(&[label.as_bytes()])
    }

    fn compile_single(text: &str) -> Result<CompiledKernelSnapshot, KernelCompileError> {
        let source = CanonicalModuleSource::parse(text.as_bytes().to_vec())?;
        CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"test-repository"),
            accepted_snapshot_id: snapshot_id("accepted"),
            root_module: source.parsed().module_name.clone(),
            modules: vec![source],
        })
    }

    fn base_fixture() -> &'static str {
        r#"module Demo

schema S:
  object Person
  object Context
  object Rank
  subtype Rank < Person
  relation Parent(child: Person @data, parent: Person @data, ctx: Context @context)
  aspect manager: Person -> Person

theory T on S:
  constraint key Parent(child, parent, ctx)
  equation manager_identity:
    manager = id(Person)

instance I of S:
  Person = {Alice, Bob}
  Context = {Current}
  Rank = {}
  Parent = {
    parent_fact: (child=Alice, parent=Bob, ctx=Current)
  }
  manager = {
    (source=Alice, target=Alice),
    (source=Bob, target=Bob)
  }
"#
    }

    #[test]
    fn compiles_relation_objects_all_projections_and_finite_functions() {
        let snapshot = compile_single(base_fixture()).expect("canonical snapshot");
        let schema = &snapshot.ir().schemas()[0];
        assert_eq!(schema.relations.len(), 1);
        assert_eq!(schema.relations[0].roles.len(), 3);
        assert_eq!(
            schema
                .generators
                .iter()
                .filter(|generator| generator.kind == SchemaGeneratorKindIr::RoleProjection)
                .count(),
            3
        );
        assert!(
            snapshot.ir().instances()[0]
                .validation
                .role_projections_total_and_single_valued
        );
    }

    #[test]
    fn key_role_refinement_rejects_without_an_executable_witness() {
        let source = r#"module UnsupportedKeyRefinement
schema S:
  object A
  relation R(left: A, right: refined(A; key(left)))
instance I of S:
  A = {a}
  R = {(left=a, right=a)}
"#;
        assert!(matches!(
            compile_single(source),
            Err(KernelCompileError::InvalidRefinement { detail, .. })
                if detail.contains("no implemented finite witness")
        ));
    }

    #[test]
    fn canonical_rewrite_formation_rejects_unbound_typed_and_endpoint_drift() {
        for (label, body, expected_detail) in [
            (
                "unbound",
                "vars: x: A\n    lhs: step(x, R, y)\n    rhs: refl(x)",
                "unbound object variable `y`",
            ),
            (
                "wrong_type",
                "vars: x: B, y: B\n    lhs: step(x, R, y)\n    rhs: step(x, R, y)",
                "expected subtype",
            ),
            (
                "endpoint_drift",
                "vars: x: A, y: B\n    lhs: step(x, R, y)\n    rhs: refl(x)",
                "changes path endpoints",
            ),
        ] {
            let source = format!(
                "module Rewrite{label}\n\nschema S:\n  object A\n  object B\n  relation R(left: A, right: B)\n\ntheory T on S:\n  rewrite bad:\n    {body}\n"
            );
            let error = compile_single(&source).expect_err("ill-typed rewrite must reject");
            assert!(
                error.to_string().contains(expected_detail),
                "{label} produced the wrong diagnostic: {error}"
            );
        }
    }

    #[test]
    fn import_closure_is_depth_first_import_order_then_root() {
        let dep = CanonicalModuleSource::parse(b"module Dep\n\nschema D:\n  object X\n".to_vec())
            .expect("dep");
        let root = CanonicalModuleSource::parse(
            b"module Root\nimport Dep\n\nschema R:\n  object Y\n".to_vec(),
        )
        .expect("root");
        let compiled = CanonicalCompiler::compile(KernelCompilationRequest {
            repository_id: RepositoryIdV2::from_descriptor_bytes(b"repo"),
            accepted_snapshot_id: snapshot_id("s"),
            root_module: "Root".to_string(),
            modules: vec![root, dep],
        })
        .expect("closure");
        assert_eq!(
            compiled
                .ir()
                .ordered_module_closure()
                .iter()
                .map(|module| module.module_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Dep", "Root"]
        );
    }

    #[test]
    fn exact_bytes_change_revision_and_ir_digest() {
        let lf = compile_single(base_fixture()).expect("lf");
        let crlf_text = base_fixture().replace('\n', "\r\n");
        let crlf = compile_single(&crlf_text).expect("crlf");
        assert_ne!(
            lf.ir().ordered_module_closure()[0].revision,
            crlf.ir().ordered_module_closure()[0].revision
        );
        assert_ne!(lf.ir().ir_digest(), crlf.ir().ir_digest());
    }

    #[test]
    fn relation_valued_role_resolves_to_real_fact_id() {
        let text = r#"module RelationValue

schema S:
  object Node
  relation Flow(from: Node @data, to: Node @data)
  relation Depends(flow: relation(Flow) @data, node: Node @data)

instance I of S:
  Node = {A, B}
  Flow = { f1: (from=A, to=B) }
  Depends = { (flow=f1, node=A) }
"#;
        let compiled = compile_single(text).expect("relation-valued role");
        let model = &compiled.ir().instances()[0];
        let flow_fact = model
            .facts
            .iter()
            .find(|fact| fact.local_label.as_deref() == Some("f1"))
            .expect("flow fact");
        let depends = model
            .facts
            .iter()
            .find(|fact| fact.local_label.is_none())
            .expect("depends fact");
        assert!(depends.ordered_role_values.iter().any(|role| {
            matches!(&role.value, TypedValueIr::RelationFact { fact_id } if fact_id == &flow_fact.fact_id)
        }));
    }

    #[test]
    fn ghost_relation_string_is_rejected() {
        let text = r#"module Ghost
schema S:
  object Node
  relation Flow(from: Node, to: Node)
  relation Depends(flow: relation(Flow), node: Node)
instance I of S:
  Node = {A, B}
  Flow = { f1: (from=A, to=B) }
  Depends = { (flow=ghost, node=A) }
"#;
        assert!(matches!(
            compile_single(text),
            Err(KernelCompileError::UnknownRelationFact { .. })
        ));
    }

    #[test]
    fn subtype_cycles_are_rejected() {
        let text =
            "module Cycle\nschema S:\n  object A\n  object B\n  subtype A < B\n  subtype B < A\n";
        assert!(matches!(
            compile_single(text),
            Err(KernelCompileError::SubtypeCycle { .. })
        ));
    }

    #[test]
    fn missing_duplicate_and_out_of_codomain_roles_are_rejected() {
        let missing = "module M\nschema S:\n  object A\n  relation R(x: A, y: A)\ninstance I of S:\n  A = {a}\n  R = {(x=a)}\n";
        assert!(matches!(
            compile_single(missing),
            Err(KernelCompileError::MissingRoleValue { .. })
        ));
        let duplicate = "module M\nschema S:\n  object A\n  relation R(x: A, y: A)\ninstance I of S:\n  A = {a}\n  R = {(x=a, x=a, y=a)}\n";
        assert!(matches!(
            compile_single(duplicate),
            Err(KernelCompileError::DuplicateRoleValue { .. })
        ));
        let codomain = "module M\nschema S:\n  object A\n  relation R(x: A, y: A)\ninstance I of S:\n  A = {a}\n  R = {(x=a, y=ghost)}\n";
        assert!(matches!(
            compile_single(codomain),
            Err(KernelCompileError::OutOfCodomain { .. })
        ));
    }

    #[test]
    fn partial_explicit_function_and_violated_equation_are_rejected() {
        let partial = base_fixture().replace("    (source=Bob, target=Bob)\n", "");
        assert!(matches!(
            compile_single(&partial),
            Err(KernelCompileError::PartialGenerator { .. })
        ));
        let violated =
            base_fixture().replace("(source=Bob, target=Bob)", "(source=Bob, target=Alice)");
        assert!(matches!(
            compile_single(&violated),
            Err(KernelCompileError::ViolatedEquation { .. })
        ));
    }

    #[test]
    fn duplicate_facts_are_rejected_even_when_payloads_match() {
        let duplicate = base_fixture().replace(
            "parent_fact: (child=Alice, parent=Bob, ctx=Current)",
            "parent_fact: (child=Alice, parent=Bob, ctx=Current),\n    (child=Alice, parent=Bob, ctx=Current)",
        );
        assert!(matches!(
            compile_single(&duplicate),
            Err(KernelCompileError::DuplicateFactId { .. })
        ));
    }

    #[test]
    fn revalidation_rejects_mutated_structure_without_panicking() {
        let compiled = compile_single(base_fixture()).expect("canonical model");
        let schema = compiled.ir().schemas()[0].clone();
        let theories = compiled
            .ir()
            .theories()
            .iter()
            .filter(|theory| theory.schema_id == schema.schema_id)
            .collect::<Vec<_>>();
        let model = compiled.ir().instances()[0].clone();

        let mut duplicate_carrier = model.clone();
        duplicate_carrier
            .carriers
            .push(duplicate_carrier.carriers[0].clone());
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &duplicate_carrier),
            Err(KernelCompileError::DuplicateCarrier { .. })
        ));

        let mut duplicate_function = model.clone();
        duplicate_function
            .functions
            .push(duplicate_function.functions[0].clone());
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &duplicate_function),
            Err(KernelCompileError::DuplicateGeneratorInterpretation { .. })
        ));

        let mut endpoint_drift = model.clone();
        endpoint_drift.functions[0].target = endpoint_drift.functions[0].source.clone();
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &endpoint_drift),
            Err(KernelCompileError::GeneratorEndpointMismatch { .. })
        ));

        let mut relation_carrier_drift = model.clone();
        relation_carrier_drift
            .carriers
            .iter_mut()
            .find(|carrier| matches!(carrier.object, SchemaObjectRefIr::RelationObject { .. }))
            .expect("relation carrier")
            .elements
            .clear();
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &relation_carrier_drift),
            Err(KernelCompileError::OutOfCodomain { .. })
                | Err(KernelCompileError::RelationCarrierMismatch { .. })
        ));

        let mut unknown_relation = model.clone();
        let unknown_key = SemanticKeyV2::derive(&model.module_id, "relation", "S.UnknownRelation");
        unknown_relation.facts[0].relation_id =
            RelationIdV2::derive(&model.revision, &model.schema_id, &unknown_key);
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &unknown_relation),
            Err(KernelCompileError::UnknownFactRelation { .. })
        ));

        let mut missing_membership_witness = model.clone();
        missing_membership_witness.object_membership_witnesses.pop();
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &missing_membership_witness),
            Err(KernelCompileError::FiniteTheory(_))
        ));

        let mut forged_constraint_witness = model.clone();
        forged_constraint_witness.typed_constraint_witnesses[0]
            .decision_procedure
            .push_str("-forged");
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &forged_constraint_witness),
            Err(KernelCompileError::FiniteTheory(_))
        ));

        let mut missing_dependent_context = model.clone();
        missing_dependent_context.dependent_contexts.clear();
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &missing_dependent_context),
            Err(KernelCompileError::FiniteTheory(_))
        ));

        let mut false_validation_claim = model.clone();
        false_validation_claim.validation.supported_constraints_hold = false;
        assert!(matches!(
            validate_instance_model_ir(&schema, &theories, &false_validation_claim),
            Err(KernelCompileError::ValidationClaimMismatch { .. })
        ));
    }

    #[test]
    fn indexed_relation_roles_enforce_exact_fiber_bindings() {
        let mismatched = r#"module FiberMismatch
schema S:
  object Node
  object Context
  relation Base(node: Node @data, ctx: Context @context)
  relation Use(ctx: Context @context, base: indexed(relation(Base); ctx) @data)
instance I of S:
  Node = {N}
  Context = {C1, C2}
  Base = {b: (node=N, ctx=C1)}
  Use = {(ctx=C2, base=b)}
"#;
        assert!(matches!(
            compile_single(mismatched),
            Err(KernelCompileError::FiniteTheory(detail))
                if detail.contains("does not match referenced fact fiber")
        ));

        let valid = mismatched.replace("Use = {(ctx=C2, base=b)}", "Use = {(ctx=C1, base=b)}");
        let compiled = compile_single(&valid).expect("matching finite fiber");
        let schema = &compiled.ir().schemas()[0];
        let use_relation = schema
            .relations
            .iter()
            .find(|relation| relation.label == "Use")
            .expect("Use relation");
        let base_role = use_relation
            .roles
            .iter()
            .find(|role| role.label == "base")
            .expect("indexed base role");
        let witness = compiled.ir().instances()[0]
            .role_witnesses
            .iter()
            .find(|witness| witness.role_id == base_role.role_id)
            .expect("indexed witness");
        assert_eq!(witness.fiber.index_bindings.len(), 1);
        assert!(witness.fiber.index_bindings[0].target_role_id.is_some());
        assert_eq!(
            witness.fiber.index_bindings[0].value,
            TypedValueIr::ObjectElement {
                value: "C1".to_string()
            }
        );

        let theories = compiled
            .ir()
            .theories()
            .iter()
            .filter(|theory| theory.schema_id == schema.schema_id)
            .collect::<Vec<_>>();
        let mut forged_model = compiled.ir().instances()[0].clone();
        forged_model
            .role_witnesses
            .iter_mut()
            .find(|witness| witness.role_id == base_role.role_id)
            .expect("indexed witness")
            .fiber
            .index_bindings[0]
            .value = TypedValueIr::ObjectElement {
            value: "C2".to_string(),
        };
        assert!(matches!(
            validate_instance_model_ir(schema, &theories, &forged_model),
            Err(KernelCompileError::FiniteTheory(_))
        ));
    }

    #[test]
    fn indexed_relation_fiber_requires_matching_target_projection() {
        let source = r#"module MissingFiberProjection
schema S:
  object Node
  object Context
  relation Base(node: Node @data)
  relation Use(ctx: Context @context, base: indexed(relation(Base); ctx) @data)
"#;
        assert!(matches!(
            compile_single(source),
            Err(KernelCompileError::InvalidIndexedFiber { .. })
        ));
    }

    #[test]
    fn canonical_corpus_exact_sources_pass_the_single_source_gate() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .canonicalize()
            .expect("repository root");
        let corpus: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("fixtures/canonical/corpus.json"))
                .expect("canonical corpus"),
        )
        .expect("corpus json");
        for entry in corpus["modules"].as_array().expect("module list") {
            let path = entry["path"].as_str().expect("module path");
            let bytes = std::fs::read(root.join(path)).expect("canonical module bytes");
            let source = CanonicalModuleSource::parse(bytes).expect("canonical source");
            assert_eq!(
                source.revision(),
                &RevisionDigestV2::from_accepted_text(source.exact_text())
            );
        }
    }
}
