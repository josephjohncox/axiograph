//! The single cryptographic identity family used by Axiograph.
//!
//! Every identity is a strict, kind-specific newtype over a full lowercase
//! SHA-256 wire value. The preimage is domain-separated and count/length
//! framed. This module is the only production implementation of that framing.

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};
use thiserror::Error;

pub const IDENTITY_MAGIC_V2: &[u8; 12] = b"AXIOGRAPH-ID";
pub const IDENTITY_VERSION_V2: u16 = 2;
const ALGORITHM: &str = "sha256";
pub const REVISION_ID_V2_PREFIX: &str = "axi:revision:v2:sha256:";
pub const FACT_ID_V2_PREFIX: &str = "axi:fact:v2:sha256:";

/// Closed registry of identity domains. Arbitrary string domains are not part
/// of the public construction API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdentityDomainV2 {
    Repository,
    Module,
    SemanticKey,
    Revision,
    Schema,
    Object,
    Relation,
    Role,
    Theory,
    Obligation,
    Instance,
    Constraint,
    Equation,
    Rewrite,
    Fact,
    Tree,
    Snapshot,
    Commit,
    Reconciliation,
    ObjectBlob,
    Materialization,
    Query,
    Answer,
    Certificate,
    Proposal,
    Run,
    Checker,
}

impl IdentityDomainV2 {
    pub const ALL: [Self; 27] = [
        Self::Repository,
        Self::Module,
        Self::SemanticKey,
        Self::Revision,
        Self::Schema,
        Self::Object,
        Self::Relation,
        Self::Role,
        Self::Theory,
        Self::Obligation,
        Self::Instance,
        Self::Constraint,
        Self::Equation,
        Self::Rewrite,
        Self::Fact,
        Self::Tree,
        Self::Snapshot,
        Self::Commit,
        Self::Reconciliation,
        Self::ObjectBlob,
        Self::Materialization,
        Self::Query,
        Self::Answer,
        Self::Certificate,
        Self::Proposal,
        Self::Run,
        Self::Checker,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Module => "module",
            Self::SemanticKey => "semantic-key",
            Self::Revision => "revision",
            Self::Schema => "schema",
            Self::Object => "object",
            Self::Relation => "relation",
            Self::Role => "role",
            Self::Theory => "theory",
            Self::Obligation => "obligation",
            Self::Instance => "instance",
            Self::Constraint => "constraint",
            Self::Equation => "equation",
            Self::Rewrite => "rewrite",
            Self::Fact => "fact",
            Self::Tree => "tree",
            Self::Snapshot => "snapshot",
            Self::Commit => "commit",
            Self::Reconciliation => "reconciliation",
            Self::ObjectBlob => "object-blob",
            Self::Materialization => "materialization",
            Self::Query => "query",
            Self::Answer => "answer",
            Self::Certificate => "certificate",
            Self::Proposal => "proposal",
            Self::Run => "run",
            Self::Checker => "checker",
        }
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("identity domain label is too large")]
    DomainTooLarge,
    #[error("identity preimage contains too many fields")]
    TooManyFields,
    #[error("identity preimage field is too large")]
    FieldTooLarge,
    #[error("accepted module bytes are not valid UTF-8")]
    InvalidAcceptedUtf8,
    #[error("malformed {expected_kind} identity; expected `axi:{expected_kind}:v2:sha256:<64-lowercase-hex>`")]
    Malformed { expected_kind: &'static str },
    #[error("evolution map must bind two different revision digests")]
    SameRevision,
    #[error("evolution entry has no source semantic keys")]
    MissingEvolutionSource,
    #[error("evolution entry has no target semantic keys")]
    MissingEvolutionTarget,
    #[error("semantic key `{0}` occurs more than once on the source side")]
    DuplicateEvolutionSource(String),
    #[error("semantic key `{0}` occurs more than once on the target side")]
    DuplicateEvolutionTarget(String),
    #[error("evolution operation `{operation}` has invalid source/target cardinality {sources}/{targets}")]
    InvalidEvolutionCardinality {
        operation: &'static str,
        sources: usize,
        targets: usize,
    },
}

/// Construct the canonical preimage for a registered domain.
pub fn canonical_identity_preimage_v2(
    domain: IdentityDomainV2,
    fields: &[&[u8]],
) -> Result<Vec<u8>, IdentityError> {
    let domain = domain.as_str().as_bytes();
    let domain_len = u16::try_from(domain.len()).map_err(|_| IdentityError::DomainTooLarge)?;
    let field_count = u32::try_from(fields.len()).map_err(|_| IdentityError::TooManyFields)?;

    let mut preimage = Vec::new();
    preimage.extend_from_slice(IDENTITY_MAGIC_V2);
    preimage.extend_from_slice(&IDENTITY_VERSION_V2.to_be_bytes());
    preimage.extend_from_slice(&domain_len.to_be_bytes());
    preimage.extend_from_slice(domain);
    preimage.extend_from_slice(&field_count.to_be_bytes());
    for field in fields {
        let field_len = u64::try_from(field.len()).map_err(|_| IdentityError::FieldTooLarge)?;
        preimage.extend_from_slice(&field_len.to_be_bytes());
        preimage.extend_from_slice(field);
    }
    Ok(preimage)
}

fn sha256_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn derive_hex(domain: IdentityDomainV2, fields: &[&[u8]]) -> Result<String, IdentityError> {
    Ok(sha256_hex(&canonical_identity_preimage_v2(domain, fields)?))
}

fn validate_wire(value: &str, domain: IdentityDomainV2) -> Result<(), IdentityError> {
    let kind = domain.as_str();
    let prefix = format!("axi:{kind}:v2:{ALGORITHM}:");
    let Some(hex) = value.strip_prefix(&prefix) else {
        return Err(IdentityError::Malformed {
            expected_kind: kind,
        });
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(IdentityError::Malformed {
            expected_kind: kind,
        });
    }
    Ok(())
}

macro_rules! identity_type {
    ($name:ident, $domain:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            pub const DOMAIN: IdentityDomainV2 = IdentityDomainV2::$domain;
            pub const KIND: &'static str = Self::DOMAIN.as_str();

            fn from_hex(hex: String) -> Self {
                Self(format!("axi:{}:v2:{}:{}", Self::KIND, ALGORITHM, hex))
            }

            fn derive_fields(fields: &[&[u8]]) -> Self {
                Self::from_hex(
                    derive_hex(Self::DOMAIN, fields).expect(
                        "registered identity domains and in-memory fields are representable",
                    ),
                )
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                validate_wire(value, Self::DOMAIN)?;
                Ok(Self(value.to_owned()))
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentityError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                validate_wire(&value, Self::DOMAIN)?;
                Ok(Self(value))
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdentityError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                value.parse()
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::try_from(value).map_err(D::Error::custom)
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(generator: &mut SchemaGenerator) -> Schema {
                let mut schema = generator.subschema_for::<String>();
                if let Some(object) = schema.as_object_mut() {
                    object.insert(
                        "pattern".to_string(),
                        serde_json::Value::String(format!(
                            "^axi:{}:v2:sha256:[0-9a-f]{{64}}$",
                            Self::KIND
                        )),
                    );
                }
                schema
            }
        }
    };
}

identity_type!(RepositoryIdV2, Repository);
identity_type!(ModuleIdV2, Module);
identity_type!(SemanticKeyV2, SemanticKey);
identity_type!(RevisionDigestV2, Revision);
identity_type!(SchemaIdV2, Schema);
identity_type!(ObjectTypeIdV2, Object);
identity_type!(RelationIdV2, Relation);
identity_type!(RoleIdV2, Role);
identity_type!(TheoryIdV2, Theory);
identity_type!(ObligationIdV2, Obligation);
identity_type!(InstanceIdV2, Instance);
identity_type!(ConstraintIdV2, Constraint);
identity_type!(EquationIdV2, Equation);
identity_type!(RewriteRuleIdV2, Rewrite);
identity_type!(FactIdV2, Fact);
identity_type!(TreeIdV2, Tree);
identity_type!(SnapshotIdV2, Snapshot);
identity_type!(CommitIdV2, Commit);
identity_type!(ReconciliationIdV2, Reconciliation);
identity_type!(ObjectBlobIdV2, ObjectBlob);
identity_type!(MaterializationIdV2, Materialization);
identity_type!(QueryIdV2, Query);
identity_type!(AnswerIdV2, Answer);
identity_type!(CertificateIdV2, Certificate);
identity_type!(ProposalIdV2, Proposal);
identity_type!(RunIdV2, Run);
identity_type!(CheckerIdV2, Checker);

impl RepositoryIdV2 {
    pub fn from_descriptor_bytes(bytes: &[u8]) -> Self {
        Self::derive_fields(&[bytes])
    }
}

impl ModuleIdV2 {
    pub fn derive(repository: &RepositoryIdV2, module_name: &str) -> Self {
        Self::derive_fields(&[repository.as_str().as_bytes(), module_name.as_bytes()])
    }
}

impl SemanticKeyV2 {
    pub fn derive(module: &ModuleIdV2, semantic_kind: &str, local_name: &str) -> Self {
        Self::derive_fields(&[
            module.as_str().as_bytes(),
            semantic_kind.as_bytes(),
            local_name.as_bytes(),
        ])
    }
}

impl RevisionDigestV2 {
    /// Hash exact accepted UTF-8 bytes without Unicode/newline normalization.
    pub fn from_accepted_bytes(bytes: &[u8]) -> Result<Self, IdentityError> {
        std::str::from_utf8(bytes).map_err(|_| IdentityError::InvalidAcceptedUtf8)?;
        Ok(Self::derive_fields(&[bytes]))
    }

    pub fn from_accepted_text(text: &str) -> Self {
        Self::derive_fields(&[text.as_bytes()])
    }
}

macro_rules! revision_local_id {
    ($name:ident, $parent:ty) => {
        impl $name {
            pub fn derive(
                revision: &RevisionDigestV2,
                parent: &$parent,
                semantic_key: &SemanticKeyV2,
            ) -> Self {
                Self::derive_fields(&[
                    revision.as_str().as_bytes(),
                    parent.as_str().as_bytes(),
                    semantic_key.as_str().as_bytes(),
                ])
            }
        }
    };
}

impl SchemaIdV2 {
    pub fn derive(revision: &RevisionDigestV2, semantic_key: &SemanticKeyV2) -> Self {
        Self::derive_fields(&[
            revision.as_str().as_bytes(),
            semantic_key.as_str().as_bytes(),
        ])
    }
}

revision_local_id!(ObjectTypeIdV2, SchemaIdV2);
revision_local_id!(RelationIdV2, SchemaIdV2);
revision_local_id!(TheoryIdV2, SchemaIdV2);
revision_local_id!(InstanceIdV2, SchemaIdV2);
revision_local_id!(ConstraintIdV2, TheoryIdV2);
revision_local_id!(EquationIdV2, TheoryIdV2);
revision_local_id!(RewriteRuleIdV2, TheoryIdV2);
revision_local_id!(ObligationIdV2, TheoryIdV2);

impl RoleIdV2 {
    pub fn derive(
        revision: &RevisionDigestV2,
        schema: &SchemaIdV2,
        relation: &RelationIdV2,
        semantic_key: &SemanticKeyV2,
        declared_order: u32,
    ) -> Self {
        Self::derive_fields(&[
            revision.as_str().as_bytes(),
            schema.as_str().as_bytes(),
            relation.as_str().as_bytes(),
            semantic_key.as_str().as_bytes(),
            &declared_order.to_be_bytes(),
        ])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FactRoleValueV2<'a> {
    pub role_id: &'a RoleIdV2,
    pub value: &'a [u8],
}

impl FactIdV2 {
    pub fn derive(
        revision: &RevisionDigestV2,
        schema: &SchemaIdV2,
        instance: &InstanceIdV2,
        relation: &RelationIdV2,
        ordered_roles: &[FactRoleValueV2<'_>],
    ) -> Self {
        let mut owned = Vec::with_capacity(4 + ordered_roles.len() * 2);
        owned.push(revision.as_str().as_bytes().to_vec());
        owned.push(schema.as_str().as_bytes().to_vec());
        owned.push(instance.as_str().as_bytes().to_vec());
        owned.push(relation.as_str().as_bytes().to_vec());
        for role in ordered_roles {
            owned.push(role.role_id.as_str().as_bytes().to_vec());
            owned.push(role.value.to_vec());
        }
        let fields = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Self::derive_fields(&fields)
    }

    /// Runtime locator for pre-elaboration code. Canonical accepted packages
    /// must use [`FactIdV2::derive`] with typed parent ids and role ids.
    pub fn from_runtime_locator(
        module_name: &str,
        schema_name: &str,
        instance_name: &str,
        relation_name: &str,
        ordered_fields: &[(&str, &str)],
    ) -> Self {
        let mut owned = vec![
            module_name.as_bytes().to_vec(),
            schema_name.as_bytes().to_vec(),
            instance_name.as_bytes().to_vec(),
            relation_name.as_bytes().to_vec(),
        ];
        for (field, value) in ordered_fields {
            owned.push(field.as_bytes().to_vec());
            owned.push(value.as_bytes().to_vec());
        }
        let fields = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Self::derive_fields(&fields)
    }
}

/// Exact accepted-text revision wire value for runtime surfaces that still
/// carry a string rather than the strict newtype.
pub fn revision_digest_v2(text: &str) -> String {
    RevisionDigestV2::from_accepted_text(text).into_inner()
}

/// Cryptographic runtime fact locator. This is not a substitute for the typed
/// accepted-package fact derivation.
pub fn object_blob_digest_v2(bytes: &[u8]) -> String {
    ObjectBlobIdV2::from_canonical_fields(&[bytes]).into_inner()
}

pub fn runtime_fact_id_v2(
    module_name: &str,
    schema_name: &str,
    instance_name: &str,
    relation_name: &str,
    ordered_fields: &[(&str, &str)],
) -> String {
    FactIdV2::from_runtime_locator(
        module_name,
        schema_name,
        instance_name,
        relation_name,
        ordered_fields,
    )
    .into_inner()
}

macro_rules! opaque_content_id {
    ($name:ident) => {
        impl $name {
            pub fn from_canonical_fields(fields: &[&[u8]]) -> Self {
                Self::derive_fields(fields)
            }
        }
    };
}

opaque_content_id!(TreeIdV2);
opaque_content_id!(SnapshotIdV2);
opaque_content_id!(CommitIdV2);
opaque_content_id!(ReconciliationIdV2);
opaque_content_id!(ObjectBlobIdV2);
opaque_content_id!(MaterializationIdV2);
opaque_content_id!(QueryIdV2);
opaque_content_id!(AnswerIdV2);
opaque_content_id!(CertificateIdV2);
opaque_content_id!(ProposalIdV2);
opaque_content_id!(RunIdV2);
opaque_content_id!(CheckerIdV2);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_closed_and_unique() {
        let domains = IdentityDomainV2::ALL
            .into_iter()
            .map(IdentityDomainV2::as_str)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(domains.len(), IdentityDomainV2::ALL.len());
    }

    #[test]
    fn rust_lean_parity_vectors_cover_every_registered_domain() {
        let expected = [
            "8a34106f978078d690364db0bad69d2e269fa7cbb35c325e99f6846ceb06d592",
            "2b409543af231f72fd8cc72ce4688ff02b1356343127c593ae6e9439c7b6aeee",
            "903c35101dfbfb73d69eb4545b9f33c5b858de32167739bbfde642a280a7cba8",
            "a7ce11aff75b4326bd06f0804c9a32ee057e2e17bb32f770e8f6ee7b2d7e622e",
            "c6f2bfba197a47f84ceea97390bf3d84f0211dbe0c172e7f95c82c55cd595cfd",
            "1c98766fbf321e12141f23196372273721bbb13008522c5dc90499e92501ee51",
            "abe43dddd80b68ef714b02344338133d736c60b2c7280e7b1b3918914ca1a826",
            "00722a929b1158131b850fadb64cff49b3bd31694cdbb188eb0d05812a2fe3bb",
            "4856a265bd932d2b53d1b800a8446a4ae5fdcbe6bfff245eb0e498763230e014",
            "6bff41d73b9cc9049ee089a35372daf25555067a8bcb46a44558b3ea58ebba5c",
            "880b54149cf0b65aafc272255819c1b19c7eeb2011c927707ff666bdd9339ddf",
            "1b1a8629443499055e827349508f540a936a28c6ce5b5f07bc0f0e2bb56b840c",
            "4aaf29b22021245b232c6b1440bae75d49e768db6386f5c9f6ac2876ceead7d3",
            "63c390da4e035602eb7a8912a71c2df0102bae1b2ce9e50b0ff2cce3d047c3f6",
            "78b3d493bd829a69e05be749c7ab7e9b036f2bd812082ac54f893f1c0cde9750",
            "15210fbebd387cb5c3e4d68e9a9fb03eab96c909eacc65bfb32f66df2e5571db",
            "7515f7ab415e1446322f478b5b7ab34328f69c00b9d76d7d81a0f0e74696ab26",
            "26143f9826097380221e2fc5d0a324d5db99a5792fe25df9aa61ae96e44cefa9",
            "0725fb86552e7173c31f49cf8b6df10b2373e7438226e8bb3703f294d5f6e6ca",
            "43ec352ed54b640cb2c253fc7667e6d776f2a77dfb4bea1271620d9209dd410c",
            "58c68b9b4e02a0ed6d32be863978729d9adfb568a58828ce66128b47ee4a5d91",
            "c9941b524ea638ab5926fc68666ffa567d0fe33333d8acb2942dd70bdbc0abc2",
            "81c80e5ed4f837d260a87d984ca7e0a47b3e714c5c13fd48175674c5cc468ec3",
            "264875b9d06b94a659bbf972a14a2743b9feeb40cfda5b3d9064287898f583c7",
            "488add43ef52e6b744aa8fc6d09b1f64dec0cdd84a7525cc0f340b2b878f6ec3",
            "b28ca681c858c2bcb1d28e68b8aaccde3b216df494d8f1c6b48d149bb8b5f6bc",
            "0e995b927377670dd7cde7dce9df7b8027d15ec5ed4affb6a56257e7d9a49367",
        ];
        for (domain, expected_hex) in IdentityDomainV2::ALL.into_iter().zip(expected) {
            let domain_bytes = domain.as_str().as_bytes();
            let fields = [domain_bytes, "λ|🧠".as_bytes(), &[0, 255]];
            assert_eq!(
                derive_hex(domain, &fields).expect("parity vector"),
                expected_hex
            );
        }
    }

    #[test]
    fn framing_distinguishes_boundaries_counts_and_order() {
        let a = canonical_identity_preimage_v2(IdentityDomainV2::Query, &[b"a", b"b|c"])
            .expect("frame a");
        let b = canonical_identity_preimage_v2(IdentityDomainV2::Query, &[b"a|b", b"c"])
            .expect("frame b");
        let c = canonical_identity_preimage_v2(IdentityDomainV2::Query, &[b"a", b"b", b"c"])
            .expect("frame c");
        let d = canonical_identity_preimage_v2(IdentityDomainV2::Query, &[b"b|c", b"a"])
            .expect("frame d");
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
    }

    #[test]
    fn exact_revision_bytes_are_not_normalized() {
        let cases = [
            b"".as_slice(),
            "module Ünicode\n".as_bytes(),
            b"module X\n".as_slice(),
            b"module X\r\n".as_slice(),
            b"module X\n\n".as_slice(),
        ];
        let ids = cases
            .iter()
            .map(|bytes| RevisionDigestV2::from_accepted_bytes(bytes).expect("valid UTF-8"))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), cases.len());
    }

    #[test]
    fn wrong_kind_uppercase_short_and_unknown_version_reject() {
        let revision = RevisionDigestV2::from_accepted_text("module X\n");
        assert!(serde_json::from_str::<FactIdV2>(&format!("\"{revision}\"")).is_err());
        assert!(
            "axi:revision:v2:sha256:ABCDEF0000000000000000000000000000000000000000000000000000"
                .parse::<RevisionDigestV2>()
                .is_err()
        );
        assert!("axi:revision:v2:sha256:1234"
            .parse::<RevisionDigestV2>()
            .is_err());
        assert!("axi:revision:v3:sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .parse::<RevisionDigestV2>()
            .is_err());
    }

    #[test]
    fn semantic_refs_are_scoped_by_module_and_revision() {
        let repository = RepositoryIdV2::from_descriptor_bytes(b"repo");
        let left_module = ModuleIdV2::derive(&repository, "Left");
        let right_module = ModuleIdV2::derive(&repository, "Right");
        let left_key = SemanticKeyV2::derive(&left_module, "schema", "S");
        let right_key = SemanticKeyV2::derive(&right_module, "schema", "S");
        assert_ne!(left_key, right_key);

        let r1 = RevisionDigestV2::from_accepted_text("module Left\n");
        let r2 = RevisionDigestV2::from_accepted_text("module Left\n\n");
        assert_ne!(
            SchemaIdV2::derive(&r1, &left_key),
            SchemaIdV2::derive(&r2, &left_key)
        );
    }

    #[test]
    fn changing_any_authenticated_field_changes_fact_id() {
        let repository = RepositoryIdV2::from_descriptor_bytes(b"repo");
        let module = ModuleIdV2::derive(&repository, "M");
        let revision = RevisionDigestV2::from_accepted_text("module M\n");
        let schema_key = SemanticKeyV2::derive(&module, "schema", "S");
        let schema = SchemaIdV2::derive(&revision, &schema_key);
        let instance_key = SemanticKeyV2::derive(&module, "instance", "I");
        let instance = InstanceIdV2::derive(&revision, &schema, &instance_key);
        let relation_key = SemanticKeyV2::derive(&module, "relation", "R");
        let relation = RelationIdV2::derive(&revision, &schema, &relation_key);
        let role_key = SemanticKeyV2::derive(&module, "role", "R.a");
        let role = RoleIdV2::derive(&revision, &schema, &relation, &role_key, 0);
        let first = FactIdV2::derive(
            &revision,
            &schema,
            &instance,
            &relation,
            &[FactRoleValueV2 {
                role_id: &role,
                value: b"x",
            }],
        );
        let changed = FactIdV2::derive(
            &revision,
            &schema,
            &instance,
            &relation,
            &[FactRoleValueV2 {
                role_id: &role,
                value: b"y",
            }],
        );
        assert_ne!(first, changed);
    }
}
