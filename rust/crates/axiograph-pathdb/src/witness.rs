//! Runtime witness helpers (DB-scoped).
//!
//! These helpers construct witness/proof objects that are meant to be used
//! *against a specific in-memory PathDB instance*. To prevent accidental
//! cross-snapshot mixing, the returned witnesses are wrapped in `DbBranded<T>`.
//!
//! Note: the brand token is not serialized; persistence identity is handled by
//! `.axi` anchors in `CertificateV2`.

use crate::branding::DbBranded;
use crate::axi_meta::{ATTR_AXI_FACT_ID, META_ATTR_NAME};
use crate::certificate::{FixedPointProbability, ReachabilityProofV2, ReachabilityProofV3};
use crate::PathDB;
use anyhow::{anyhow, Result};

/// Build a `ReachabilityProofV2` from a chain of PathDB relation ids.
///
/// This is primarily used by certificate emission (`query_result_v1/v2`):
/// RPQ evaluation produces a sequence of relation ids; we reify that as a
/// reachability witness whose steps are anchored to those ids.
pub fn reachability_proof_v2_from_relation_ids(
    db: &PathDB,
    start: u32,
    relation_ids: &[u32],
) -> Result<DbBranded<ReachabilityProofV2>> {
    if relation_ids.is_empty() {
        return Ok(DbBranded::new(
            db.db_token(),
            ReachabilityProofV2::Reflexive { entity: start },
        ));
    }

    // Validate the chain (and compute the end entity).
    let mut current = start;
    for &rel_id in relation_ids {
        let rel = db
            .relations
            .get_relation(rel_id)
            .ok_or_else(|| anyhow!("missing relation {rel_id} in RelationStore"))?;
        if rel.source != current {
            return Err(anyhow!(
                "relation_id {rel_id} chain mismatch: expected source={current}, got {}",
                rel.source
            ));
        }
        current = rel.target;
    }
    let end = current;

    // Build a `ReachabilityProofV2` right-associated step chain.
    let mut rest = ReachabilityProofV2::Reflexive { entity: end };
    for &rel_id in relation_ids.iter().rev() {
        let rel = db
            .relations
            .get_relation(rel_id)
            .ok_or_else(|| anyhow!("missing relation {rel_id} in RelationStore"))?;
        let rel_confidence_fp = FixedPointProbability::from_f32(rel.confidence);
        rest = ReachabilityProofV2::Step {
            from: rel.source,
            rel_type: rel.rel_type.raw(),
            to: rel.target,
            rel_confidence_fp,
            relation_id: Some(rel_id),
            rest: Box::new(rest),
        };
    }

    Ok(DbBranded::new(db.db_token(), rest))
}

/// Resolve a stable `.axi`-anchored entity identifier for certificates.
///
/// Precedence:
/// - if the entity has an `axi_fact_id`, return that (tuple/fact nodes),
/// - otherwise, fall back to `name`.
///
/// This is used by name-based (`*_v3`) certificate formats.
pub fn stable_entity_id_v1(db: &PathDB, entity_id: u32) -> Result<String> {
    let fact_key = db.interner.intern(ATTR_AXI_FACT_ID);
    if let Some(fid) = db.entities.get_attr(entity_id, fact_key) {
        if let Some(s) = db.interner.lookup(fid) {
            return Ok(s);
        }
    }

    let name_key = db.interner.intern(META_ATTR_NAME);
    let name_val = db
        .entities
        .get_attr(entity_id, name_key)
        .ok_or_else(|| anyhow!("entity {entity_id} is missing a `{META_ATTR_NAME}` attribute"))?;
    db.interner
        .lookup(name_val)
        .ok_or_else(|| anyhow!("internal error: missing string interner entry {name_val:?}"))
}

fn relation_axi_fact_id_v1(db: &PathDB, rel_id: u32) -> Result<String> {
    let rel = db
        .relations
        .get_relation(rel_id)
        .ok_or_else(|| anyhow!("missing relation {rel_id} in RelationStore"))?;
    let key = db.interner.intern(ATTR_AXI_FACT_ID);
    for (k, v) in &rel.attrs {
        if *k == key {
            let Some(s) = db.interner.lookup(*v) else {
                return Err(anyhow!(
                    "internal error: missing string interner entry for relation attr value {v:?}"
                ));
            };
            return Ok(s);
        }
    }

    // Fallback: some tuple-field edges may not carry explicit attrs; in that
    // case the fact id is the source tuple's id.
    let src = rel.source;
    let src_fact = db.entities.get_attr(src, key).ok_or_else(|| {
        anyhow!(
            "cannot build `.axi`-anchored witness: relation {rel_id} is missing `{ATTR_AXI_FACT_ID}`"
        )
    })?;
    db.interner.lookup(src_fact).ok_or_else(|| {
        anyhow!(
            "internal error: missing string interner entry for entity attr value {src_fact:?}"
        )
    })
}

/// Build a `.axi`-anchored, name-based `ReachabilityProofV3` from a chain of
/// PathDB relation ids.
///
/// Unlike `reachability_proof_v2_from_relation_ids`, this format does not rely
/// on `PathDBExportV1` snapshot tables: it references canonical `.axi` tuple
/// facts via `axi_fact_id`.
pub fn reachability_proof_v3_from_relation_ids(
    db: &PathDB,
    start: u32,
    relation_ids: &[u32],
) -> Result<DbBranded<ReachabilityProofV3>> {
    if relation_ids.is_empty() {
        return Ok(DbBranded::new(
            db.db_token(),
            ReachabilityProofV3::Reflexive {
                entity: stable_entity_id_v1(db, start)?,
            },
        ));
    }

    // Validate the chain (and compute the end entity).
    let mut current = start;
    for &rel_id in relation_ids {
        let rel = db
            .relations
            .get_relation(rel_id)
            .ok_or_else(|| anyhow!("missing relation {rel_id} in RelationStore"))?;
        if rel.source != current {
            return Err(anyhow!(
                "relation_id {rel_id} chain mismatch: expected source={current}, got {}",
                rel.source
            ));
        }
        current = rel.target;
    }
    let end = current;

    // Build a `ReachabilityProofV3` right-associated step chain.
    let mut rest = ReachabilityProofV3::Reflexive {
        entity: stable_entity_id_v1(db, end)?,
    };
    for &rel_id in relation_ids.iter().rev() {
        let rel = db
            .relations
            .get_relation(rel_id)
            .ok_or_else(|| anyhow!("missing relation {rel_id} in RelationStore"))?;
        let rel_confidence_fp = FixedPointProbability::from_f32(rel.confidence);
        let rel_name = db
            .interner
            .lookup(rel.rel_type)
            .ok_or_else(|| anyhow!("internal error: missing rel_type name for {}", rel.rel_type.raw()))?;

        rest = ReachabilityProofV3::Step {
            from: stable_entity_id_v1(db, rel.source)?,
            rel: rel_name,
            to: stable_entity_id_v1(db, rel.target)?,
            rel_confidence_fp,
            axi_fact_id: relation_axi_fact_id_v1(db, rel_id)?,
            rest: Box::new(rest),
        };
    }

    Ok(DbBranded::new(db.db_token(), rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reachability_proof_v2_is_db_branded() {
        let mut db1 = PathDB::new();
        let a = db1.add_entity("Node", vec![("name", "a")]);
        let b = db1.add_entity("Node", vec![("name", "b")]);
        let rel_id = db1.add_relation("r", a, b, 0.9, Vec::new());

        let branded = reachability_proof_v2_from_relation_ids(&db1, a, &[rel_id]).expect("proof");
        assert!(branded.assert_in_db(&db1).is_ok());

        let db2 = PathDB::new();
        assert!(branded.assert_in_db(&db2).is_err());
    }
}
