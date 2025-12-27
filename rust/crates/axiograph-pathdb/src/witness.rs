//! Runtime witness helpers (DB-scoped).
//!
//! These helpers construct witness/proof objects that are meant to be used
//! *against a specific in-memory PathDB instance*. To prevent accidental
//! cross-snapshot mixing, the returned witnesses are wrapped in `DbBranded<T>`.
//!
//! Note: the brand token is not serialized; persistence identity is handled by
//! `.axi` anchors in `CertificateV2`.

use crate::branding::DbBranded;
use crate::certificate::{FixedPointProbability, ReachabilityProofV2};
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
