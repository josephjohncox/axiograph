use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

pub const RUNTIME_REFINEMENT_HANDLE_V1_VERSION: u32 = 1;

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRefinementDomainV1 {
    Query,
    OlogAuthoring,
    MigrationAuthoring,
    ReconciliationReview,
    CompetencyQuestionRepair,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRefinementCandidateKindV1 {
    AddTypeGuard,
    ExtendOutgoingPath,
    ExtendIncomingPath,
    BindFactRelation,
    BindRelationRole,
    RetargetRelationRole,
    AddressTransportObligation,
    ResolveConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OlogRefinementOpV1 {
    BindRelationRole {
        relation_box: String,
        role: String,
        target_box: String,
    },
    RetargetRelationRole {
        relation_box: String,
        role: String,
        target_box: String,
    },
}

impl OlogRefinementOpV1 {
    pub fn preview_fragment(&self) -> String {
        match self {
            Self::BindRelationRole {
                relation_box,
                role,
                target_box,
            } => format!("{relation_box}[{role} := {target_box}]"),
            Self::RetargetRelationRole {
                relation_box,
                role,
                target_box,
            } => format!("{relation_box}[{role} := {target_box}]"),
        }
    }

    fn stable_digest_input(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| self.preview_fragment())
    }
}

fn stable_payload_input<T>(value: &T, fallback: &str) -> String
where
    T: Serialize,
{
    serde_json::to_string(value).unwrap_or_else(|_| fallback.to_string())
}

fn theory_subject_kind_label(
    kind: axiograph_pathdb::kernel_ir::TheorySubjectKindIr,
) -> &'static str {
    match kind {
        axiograph_pathdb::kernel_ir::TheorySubjectKindIr::Theory => "theory",
        axiograph_pathdb::kernel_ir::TheorySubjectKindIr::Relation => "relation",
        axiograph_pathdb::kernel_ir::TheorySubjectKindIr::Role => "role",
    }
}

fn theory_obligation_kind_label(
    kind: axiograph_pathdb::kernel_ir::TheoryObligationKindIr,
) -> &'static str {
    match kind {
        axiograph_pathdb::kernel_ir::TheoryObligationKindIr::Constraint => "constraint",
        axiograph_pathdb::kernel_ir::TheoryObligationKindIr::PathEquation => "path_equation",
        axiograph_pathdb::kernel_ir::TheoryObligationKindIr::OpaqueEquation => "opaque_equation",
        axiograph_pathdb::kernel_ir::TheoryObligationKindIr::RewriteRule => "rewrite_rule",
    }
}

fn local_name(raw: &str) -> &str {
    raw.rsplit('.').next().unwrap_or(raw)
}

fn relation_subject_refs_for_candidate(
    compiled_schema: &axiograph_pathdb::kernel_ir::CompiledSchemaIr,
    relation_name: &str,
    role_name: Option<&str>,
) -> Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr> {
    let Some(relation) = compiled_schema
        .relations
        .get(relation_name)
        .or_else(|| compiled_schema.relations.get(local_name(relation_name)))
    else {
        return Vec::new();
    };

    let mut refs = vec![axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Relation {
        relation_id: relation.relation_id.clone(),
        relation_name: relation.name.clone(),
    }];
    if let Some(role_name) = role_name {
        refs.extend(
            relation
                .roles
                .iter()
                .filter(|role| {
                    role.name == role_name || local_name(&role.name) == local_name(role_name)
                })
                .map(
                    |role| axiograph_pathdb::kernel_ir::TheorySubjectRefIr::Role {
                        relation_id: relation.relation_id.clone(),
                        relation_name: relation.name.clone(),
                        role_id: role.role_id.clone(),
                        role_name: role.name.clone(),
                    },
                ),
        );
    }
    refs
}

fn theory_handles_for_candidate(
    compiled_schema: &axiograph_pathdb::kernel_ir::CompiledSchemaIr,
    theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
    relation_name: Option<&str>,
    role_name: Option<&str>,
) -> (
    Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
    Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
) {
    let Some(relation_name) = relation_name else {
        return (None, Vec::new());
    };

    let subjects = relation_subject_refs_for_candidate(compiled_schema, relation_name, role_name);
    let mut obligations: Vec<axiograph_pathdb::kernel_ir::TheoryObligationRefIr> = Vec::new();
    for subject in &subjects {
        for theory in theories {
            obligations.extend(theory.obligation_refs_for_subject(subject));
        }
    }
    obligations.sort();
    obligations.dedup();

    let obligation = if obligations.len() == 1 {
        obligations.into_iter().next()
    } else {
        None
    };
    (obligation, subjects)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MigrationRefinementOpV1 {
    AddressTransportObligation {
        obligation_id: String,
        obligation_kind: String,
        subject_ref: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    },
}

impl MigrationRefinementOpV1 {
    pub fn preview_fragment(&self) -> String {
        match self {
            Self::AddressTransportObligation {
                obligation_id,
                obligation_kind,
                subject_ref,
                theory_obligation_ref,
                theory_subject_refs,
            } => {
                let typed_subject = theory_obligation_ref
                    .as_ref()
                    .map(|obligation| {
                        format!(
                            " [{}:{}]",
                            theory_obligation_kind_label(obligation.obligation_kind()),
                            obligation.stable_id()
                        )
                    })
                    .unwrap_or_default();
                let typed_targets = if theory_subject_refs.is_empty() {
                    String::new()
                } else {
                    format!(
                        " -> {}",
                        theory_subject_refs
                            .iter()
                            .map(|subject| format!(
                                "{}:{}",
                                theory_subject_kind_label(subject.subject_kind()),
                                subject.stable_id()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    "address transport obligation {obligation_id} ({obligation_kind}) for {subject_ref}{typed_subject}{typed_targets}"
                )
            }
        }
    }

    fn stable_digest_input(&self) -> String {
        stable_payload_input(self, &self.preview_fragment())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReconciliationRefinementOpV1 {
    ResolveConflictByDecision {
        reconciliation_id: String,
        artifact_kind: String,
        artifact_id: String,
        resolution: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    },
}

impl ReconciliationRefinementOpV1 {
    pub fn preview_fragment(&self) -> String {
        match self {
            Self::ResolveConflictByDecision {
                artifact_kind,
                artifact_id,
                resolution,
                theory_obligation_ref,
                theory_subject_refs,
                ..
            } => {
                let typed_obligation = theory_obligation_ref
                    .as_ref()
                    .map(|obligation| {
                        format!(
                            " [{}:{}]",
                            theory_obligation_kind_label(obligation.obligation_kind()),
                            obligation.stable_id()
                        )
                    })
                    .unwrap_or_default();
                let typed_subjects = if theory_subject_refs.is_empty() {
                    String::new()
                } else {
                    format!(
                        " over {}",
                        theory_subject_refs
                            .iter()
                            .map(|subject| format!(
                                "{}:{}",
                                theory_subject_kind_label(subject.subject_kind()),
                                subject.stable_id()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                format!(
                    "resolve {artifact_kind} `{artifact_id}` via `{resolution}`{typed_obligation}{typed_subjects}"
                )
            }
        }
    }

    fn stable_digest_input(&self) -> String {
        stable_payload_input(self, &self.preview_fragment())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "domain", rename_all = "snake_case")]
pub enum RuntimeRefinementPayloadV1 {
    Query {
        handle: crate::axql::AxqlRefinementHandleV1,
    },
    OlogAuthoring {
        op: OlogRefinementOpV1,
    },
    MigrationAuthoring {
        op: MigrationRefinementOpV1,
    },
    ReconciliationReview {
        op: ReconciliationRefinementOpV1,
    },
    CompetencyQuestionRepair {
        question_name: String,
        handle: crate::axql::AxqlRefinementHandleV1,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRefinementHandleV1 {
    pub version: u32,
    pub id: String,
    pub payload: RuntimeRefinementPayloadV1,
}

impl RuntimeRefinementHandleV1 {
    pub fn from_query(handle: crate::axql::AxqlRefinementHandleV1) -> Self {
        Self {
            version: RUNTIME_REFINEMENT_HANDLE_V1_VERSION,
            id: handle.id.clone(),
            payload: RuntimeRefinementPayloadV1::Query { handle },
        }
    }

    pub fn new_olog(op: OlogRefinementOpV1) -> Self {
        let id = format!(
            "olog_refine_v1:{}",
            axiograph_dsl::digest::axi_digest_v1(&op.stable_digest_input())
        );
        Self {
            version: RUNTIME_REFINEMENT_HANDLE_V1_VERSION,
            id,
            payload: RuntimeRefinementPayloadV1::OlogAuthoring { op },
        }
    }

    pub fn new_migration(op: MigrationRefinementOpV1) -> Self {
        let id = format!(
            "migration_refine_v1:{}",
            axiograph_dsl::digest::axi_digest_v1(&op.stable_digest_input())
        );
        Self {
            version: RUNTIME_REFINEMENT_HANDLE_V1_VERSION,
            id,
            payload: RuntimeRefinementPayloadV1::MigrationAuthoring { op },
        }
    }

    pub fn new_reconciliation(op: ReconciliationRefinementOpV1) -> Self {
        let id = format!(
            "reconcile_refine_v1:{}",
            axiograph_dsl::digest::axi_digest_v1(&op.stable_digest_input())
        );
        Self {
            version: RUNTIME_REFINEMENT_HANDLE_V1_VERSION,
            id,
            payload: RuntimeRefinementPayloadV1::ReconciliationReview { op },
        }
    }

    pub fn new_competency_question_repair(
        question_name: impl Into<String>,
        handle: crate::axql::AxqlRefinementHandleV1,
    ) -> Self {
        let question_name = question_name.into();
        let payload = RuntimeRefinementPayloadV1::CompetencyQuestionRepair {
            question_name: question_name.clone(),
            handle,
        };
        let id = format!(
            "cq_refine_v1:{}",
            axiograph_dsl::digest::axi_digest_v1(&stable_payload_input(
                &payload,
                question_name.as_str()
            ))
        );
        Self {
            version: RUNTIME_REFINEMENT_HANDLE_V1_VERSION,
            id,
            payload,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn domain(&self) -> RuntimeRefinementDomainV1 {
        match &self.payload {
            RuntimeRefinementPayloadV1::Query { .. } => RuntimeRefinementDomainV1::Query,
            RuntimeRefinementPayloadV1::OlogAuthoring { .. } => {
                RuntimeRefinementDomainV1::OlogAuthoring
            }
            RuntimeRefinementPayloadV1::MigrationAuthoring { .. } => {
                RuntimeRefinementDomainV1::MigrationAuthoring
            }
            RuntimeRefinementPayloadV1::ReconciliationReview { .. } => {
                RuntimeRefinementDomainV1::ReconciliationReview
            }
            RuntimeRefinementPayloadV1::CompetencyQuestionRepair { .. } => {
                RuntimeRefinementDomainV1::CompetencyQuestionRepair
            }
        }
    }

    pub fn preview_fragment(&self) -> String {
        match &self.payload {
            RuntimeRefinementPayloadV1::Query { handle } => handle.preview_fragment(),
            RuntimeRefinementPayloadV1::OlogAuthoring { op } => op.preview_fragment(),
            RuntimeRefinementPayloadV1::MigrationAuthoring { op } => op.preview_fragment(),
            RuntimeRefinementPayloadV1::ReconciliationReview { op } => op.preview_fragment(),
            RuntimeRefinementPayloadV1::CompetencyQuestionRepair {
                question_name,
                handle,
            } => format!(
                "repair competency question `{question_name}` with {}",
                handle.preview_fragment()
            ),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != RUNTIME_REFINEMENT_HANDLE_V1_VERSION {
            return Err(anyhow!(
                "unsupported runtime refinement handle version {}; expected {}",
                self.version,
                RUNTIME_REFINEMENT_HANDLE_V1_VERSION
            ));
        }
        match &self.payload {
            RuntimeRefinementPayloadV1::Query { handle } => {
                handle.validate()?;
                if self.id != handle.id {
                    return Err(anyhow!(
                        "runtime query refinement handle id {} did not match embedded query handle id {}",
                        self.id,
                        handle.id
                    ));
                }
            }
            RuntimeRefinementPayloadV1::OlogAuthoring { op } => {
                let expected = Self::new_olog(op.clone());
                if self.id != expected.id {
                    return Err(anyhow!(
                        "invalid olog refinement handle id {}; expected {} for this payload",
                        self.id,
                        expected.id
                    ));
                }
            }
            RuntimeRefinementPayloadV1::MigrationAuthoring { op } => {
                let expected = Self::new_migration(op.clone());
                if self.id != expected.id {
                    return Err(anyhow!(
                        "invalid migration refinement handle id {}; expected {} for this payload",
                        self.id,
                        expected.id
                    ));
                }
            }
            RuntimeRefinementPayloadV1::ReconciliationReview { op } => {
                let expected = Self::new_reconciliation(op.clone());
                if self.id != expected.id {
                    return Err(anyhow!(
                        "invalid reconciliation refinement handle id {}; expected {} for this payload",
                        self.id,
                        expected.id
                    ));
                }
            }
            RuntimeRefinementPayloadV1::CompetencyQuestionRepair {
                question_name,
                handle,
            } => {
                handle.validate()?;
                let expected =
                    Self::new_competency_question_repair(question_name.clone(), handle.clone());
                if self.id != expected.id {
                    return Err(anyhow!(
                        "invalid competency-question refinement handle id {}; expected {} for this payload",
                        self.id,
                        expected.id
                    ));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeRefinementCandidateV1 {
    pub kind: RuntimeRefinementCandidateKindV1,
    pub summary: String,
    pub handle: RuntimeRefinementHandleV1,
    pub preview_fragment: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_box: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theory_subject_ref: Option<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
}

impl RuntimeRefinementCandidateV1 {
    pub fn from_axql(candidate: crate::axql::AxqlRefinementCandidateV1) -> Self {
        Self {
            kind: match candidate.kind {
                crate::axql::AxqlRefinementCandidateKindV1::AddTypeGuard => {
                    RuntimeRefinementCandidateKindV1::AddTypeGuard
                }
                crate::axql::AxqlRefinementCandidateKindV1::ExtendOutgoingPath => {
                    RuntimeRefinementCandidateKindV1::ExtendOutgoingPath
                }
                crate::axql::AxqlRefinementCandidateKindV1::ExtendIncomingPath => {
                    RuntimeRefinementCandidateKindV1::ExtendIncomingPath
                }
                crate::axql::AxqlRefinementCandidateKindV1::BindFactRelation => {
                    RuntimeRefinementCandidateKindV1::BindFactRelation
                }
            },
            summary: candidate.summary,
            preview_fragment: candidate.preview_fragment,
            handle: RuntimeRefinementHandleV1::from_query(candidate.handle),
            relation: candidate.relation,
            schema: candidate.schema,
            role: candidate.role,
            target_type: candidate.target_type,
            target_box: None,
            question_name: None,
            obligation_id: None,
            artifact_id: None,
            resolution: None,
            theory_obligation_ref: None,
            theory_subject_refs: Vec::new(),
            theory_subject_ref: None,
        }
    }

    pub fn from_axql_with_theory(
        candidate: crate::axql::AxqlRefinementCandidateV1,
        compiled_schema: &axiograph_pathdb::kernel_ir::CompiledSchemaIr,
        theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
    ) -> Self {
        let mut enriched = Self::from_axql(candidate);
        let (theory_obligation_ref, theory_subject_refs) = theory_handles_for_candidate(
            compiled_schema,
            theories,
            enriched.relation.as_deref(),
            enriched.role.as_deref(),
        );
        enriched.theory_obligation_ref = theory_obligation_ref;
        enriched.theory_subject_ref = theory_subject_refs.first().cloned();
        enriched.theory_subject_refs = theory_subject_refs;
        enriched
    }

    pub fn new_olog(
        kind: RuntimeRefinementCandidateKindV1,
        summary: String,
        op: OlogRefinementOpV1,
        relation: Option<String>,
        role: Option<String>,
        target_type: Option<String>,
        target_box: Option<String>,
    ) -> Self {
        let handle = RuntimeRefinementHandleV1::new_olog(op);
        let preview_fragment = handle.preview_fragment();
        Self {
            kind,
            summary,
            handle,
            preview_fragment,
            relation,
            schema: None,
            role,
            target_type,
            target_box,
            question_name: None,
            obligation_id: None,
            artifact_id: None,
            resolution: None,
            theory_obligation_ref: None,
            theory_subject_refs: Vec::new(),
            theory_subject_ref: None,
        }
    }

    pub fn new_migration(
        summary: String,
        op: MigrationRefinementOpV1,
        obligation_id: String,
        theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
        theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    ) -> Self {
        let handle = RuntimeRefinementHandleV1::new_migration(op);
        let preview_fragment = handle.preview_fragment();
        let theory_subject_ref = theory_subject_refs.first().cloned();
        Self {
            kind: RuntimeRefinementCandidateKindV1::AddressTransportObligation,
            summary,
            handle,
            preview_fragment,
            relation: None,
            schema: None,
            role: None,
            target_type: None,
            target_box: None,
            question_name: None,
            obligation_id: Some(obligation_id),
            artifact_id: None,
            resolution: None,
            theory_obligation_ref,
            theory_subject_refs,
            theory_subject_ref,
        }
    }

    pub fn new_reconciliation(
        summary: String,
        op: ReconciliationRefinementOpV1,
        artifact_id: String,
        resolution: String,
        theory_obligation_ref: Option<axiograph_pathdb::kernel_ir::TheoryObligationRefIr>,
        theory_subject_refs: Vec<axiograph_pathdb::kernel_ir::TheorySubjectRefIr>,
    ) -> Self {
        let handle = RuntimeRefinementHandleV1::new_reconciliation(op);
        let preview_fragment = handle.preview_fragment();
        let theory_subject_ref = theory_subject_refs.first().cloned();
        Self {
            kind: RuntimeRefinementCandidateKindV1::ResolveConflict,
            summary,
            handle,
            preview_fragment,
            relation: None,
            schema: None,
            role: None,
            target_type: None,
            target_box: None,
            question_name: None,
            obligation_id: None,
            artifact_id: Some(artifact_id),
            resolution: Some(resolution),
            theory_obligation_ref,
            theory_subject_refs,
            theory_subject_ref,
        }
    }

    pub fn wrap_axql_for_competency_question(
        question_name: impl Into<String>,
        candidate: crate::axql::AxqlRefinementCandidateV1,
    ) -> Self {
        let question_name = question_name.into();
        let handle = RuntimeRefinementHandleV1::new_competency_question_repair(
            question_name.clone(),
            candidate.handle,
        );
        Self {
            kind: match candidate.kind {
                crate::axql::AxqlRefinementCandidateKindV1::AddTypeGuard => {
                    RuntimeRefinementCandidateKindV1::AddTypeGuard
                }
                crate::axql::AxqlRefinementCandidateKindV1::ExtendOutgoingPath => {
                    RuntimeRefinementCandidateKindV1::ExtendOutgoingPath
                }
                crate::axql::AxqlRefinementCandidateKindV1::ExtendIncomingPath => {
                    RuntimeRefinementCandidateKindV1::ExtendIncomingPath
                }
                crate::axql::AxqlRefinementCandidateKindV1::BindFactRelation => {
                    RuntimeRefinementCandidateKindV1::BindFactRelation
                }
            },
            summary: format!(
                "repair competency question `{}`: {}",
                question_name, candidate.summary
            ),
            preview_fragment: handle.preview_fragment(),
            handle,
            relation: candidate.relation,
            schema: candidate.schema,
            role: candidate.role,
            target_type: candidate.target_type,
            target_box: None,
            question_name: Some(question_name),
            obligation_id: None,
            artifact_id: None,
            resolution: None,
            theory_obligation_ref: None,
            theory_subject_refs: Vec::new(),
            theory_subject_ref: None,
        }
    }

    pub fn wrap_axql_for_competency_question_with_theory(
        question_name: impl Into<String>,
        candidate: crate::axql::AxqlRefinementCandidateV1,
        compiled_schema: &axiograph_pathdb::kernel_ir::CompiledSchemaIr,
        theories: &[axiograph_pathdb::kernel_ir::TheoryIr],
    ) -> Self {
        let question_name = question_name.into();
        let mut enriched = Self::wrap_axql_for_competency_question(question_name, candidate);
        let (theory_obligation_ref, theory_subject_refs) = theory_handles_for_candidate(
            compiled_schema,
            theories,
            enriched.relation.as_deref(),
            enriched.role.as_deref(),
        );
        enriched.theory_obligation_ref = theory_obligation_ref;
        enriched.theory_subject_ref = theory_subject_refs.first().cloned();
        enriched.theory_subject_refs = theory_subject_refs;
        enriched
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_olog_refinement_handle(
    fragment: &crate::typed_authoring::OlogFragmentV1,
    handle: &RuntimeRefinementHandleV1,
) -> Result<crate::typed_authoring::OlogFragmentV1> {
    handle.validate()?;
    let RuntimeRefinementPayloadV1::OlogAuthoring { op } = &handle.payload else {
        return Err(anyhow!(
            "runtime refinement handle `{}` is not an olog authoring refinement",
            handle.id
        ));
    };
    let mut refined = fragment.clone();
    match op {
        OlogRefinementOpV1::BindRelationRole {
            relation_box,
            role,
            target_box,
        } => {
            let rel_box = refined
                .relation_boxes
                .iter_mut()
                .find(|candidate| candidate.box_id == *relation_box)
                .ok_or_else(|| anyhow!("unknown relation box `{relation_box}`"))?;
            if rel_box
                .role_bindings
                .iter()
                .any(|binding| binding.role == *role)
            {
                return Err(anyhow!(
                    "relation box `{relation_box}` already binds role `{role}`"
                ));
            }
            rel_box
                .role_bindings
                .push(crate::typed_authoring::OlogRoleBindingV1 {
                    role: role.clone(),
                    target_box: target_box.clone(),
                });
        }
        OlogRefinementOpV1::RetargetRelationRole {
            relation_box,
            role,
            target_box,
        } => {
            let rel_box = refined
                .relation_boxes
                .iter_mut()
                .find(|candidate| candidate.box_id == *relation_box)
                .ok_or_else(|| anyhow!("unknown relation box `{relation_box}`"))?;
            let binding = rel_box
                .role_bindings
                .iter_mut()
                .find(|binding| binding.role == *role)
                .ok_or_else(|| {
                    anyhow!("relation box `{relation_box}` does not bind role `{role}`")
                })?;
            binding.target_box = target_box.clone();
        }
    }
    Ok(refined)
}
