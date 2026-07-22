//! Verified hydration of derived PathDB query state from authenticated `.axpd`.
//!
//! Hydration happens only after `axiograph-store` has checked the exact image,
//! schema, limits, logical digest, semantic anchors, and materialization id.
//! The resulting `PathDB` remains an untrusted execution index; it does not gain
//! accepted-plane or Lean authority.

use crate::axi_meta::{ATTR_AXI_FACT_ID, ATTR_AXI_RELATION, REL_AXI_FACT_IN_CONTEXT};
use crate::PathDB;
use anyhow::{anyhow, Context, Result};
use axiograph_kernel::{KernelSnapshotIr, MaterializationIdV2};
use axiograph_store::{
    AcceptedBuildManifest, AxiStore, AxpdBuildSpec, AxpdConfiguration, AxpdLimits,
    AxpdLogicalImage, AxpdOverlayInput, AxpdReceipt, VerifiedAxpd,
};
use std::collections::BTreeMap;
use std::path::Path;

pub struct MaterializedPathDb {
    db: PathDB,
    receipt: AxpdReceipt,
}

impl MaterializedPathDb {
    pub fn db(&self) -> &PathDB {
        &self.db
    }

    pub fn db_mut(&mut self) -> &mut PathDB {
        &mut self.db
    }

    pub fn into_db(self) -> PathDB {
        self.db
    }

    pub fn receipt(&self) -> &AxpdReceipt {
        &self.receipt
    }
}

pub fn publish_kernel_pathdb(
    store_root: impl AsRef<Path>,
    manifest: &AcceptedBuildManifest,
    kernel: &KernelSnapshotIr,
    overlays: &[AxpdOverlayInput],
    configuration: AxpdConfiguration,
    limits: &AxpdLimits,
) -> Result<AxpdReceipt> {
    let spec = AxpdBuildSpec::from_accepted_kernel(manifest, kernel, overlays, configuration)
        .context("build authenticated PathDB materialization specification")?;
    let store = AxiStore::open(store_root.as_ref()).context("open AxiStore")?;
    store
        .publish_axpd(spec, limits)
        .context("publish authenticated PathDB materialization in AxiStore family")
}

pub fn load_verified_pathdb(
    store_root: impl AsRef<Path>,
    materialization_id: &MaterializationIdV2,
    limits: &AxpdLimits,
) -> Result<MaterializedPathDb> {
    let store = AxiStore::open(store_root.as_ref()).context("open AxiStore")?;
    let verified = store
        .open_axpd(materialization_id, limits)
        .context("verify authenticated PathDB materialization from AxiStore family")?;
    hydrate_verified(verified)
}

fn hydrate_verified(verified: VerifiedAxpd) -> Result<MaterializedPathDb> {
    let mut db = hydrate_image(verified.image())?;
    db.build_indexes();
    Ok(MaterializedPathDb {
        db,
        receipt: verified.receipt().clone(),
    })
}

fn hydrate_image(image: &AxpdLogicalImage) -> Result<PathDB> {
    let mut db = PathDB::new();
    let mut entity_ids = BTreeMap::new();
    for row in &image.entities {
        let id = db.add_entity(
            &row.object_ref_json,
            vec![
                ("axiograph.entity_key", row.entity_key.as_str()),
                ("axiograph.instance_id", row.instance_id.as_str()),
                ("axiograph.value", &row.value),
            ],
        );
        if entity_ids.insert(row.entity_key.clone(), id).is_some() {
            return Err(anyhow!(
                "duplicate materialized entity key {}",
                row.entity_key
            ));
        }
    }

    let mut fact_ids = BTreeMap::new();
    for row in &image.relation_facts {
        let entity_type = format!("axiograph.relation_fact:{}", row.relation_id);
        let mut attrs = vec![
            (ATTR_AXI_FACT_ID, row.fact_id.as_str()),
            (ATTR_AXI_RELATION, row.relation_id.as_str()),
            ("axiograph.instance_id", row.instance_id.as_str()),
            ("axiograph.relation_id", row.relation_id.as_str()),
        ];
        if let Some(label) = row.local_label.as_deref() {
            attrs.push(("axiograph.local_label", label));
        }
        let id = db.add_entity(&entity_type, attrs);
        if fact_ids.insert(row.fact_id.clone(), id).is_some() {
            return Err(anyhow!("duplicate materialized fact id {}", row.fact_id));
        }
    }

    let context_axes = image
        .contexts
        .iter()
        .map(|row| ((row.fact_id.clone(), row.ordinal, row.role_id.clone()), row))
        .collect::<BTreeMap<_, _>>();

    for row in &image.projections {
        let source = fact_ids
            .get(&row.fact_id)
            .copied()
            .ok_or_else(|| anyhow!("projection source fact {} is absent", row.fact_id))?;
        let target = match (&row.target_entity_key, &row.target_fact_id) {
            (Some(entity), None) => entity_ids
                .get(entity)
                .copied()
                .ok_or_else(|| anyhow!("projection target entity {entity} is absent"))?,
            (None, Some(fact)) => fact_ids
                .get(fact)
                .copied()
                .ok_or_else(|| anyhow!("projection target fact {fact} is absent"))?,
            _ => return Err(anyhow!("projection target kind is inconsistent")),
        };
        let ordinal = row.ordinal.to_string();
        let context = context_axes.get(&(row.fact_id.clone(), row.ordinal, row.role_id.clone()));
        let mut attrs = vec![
            ("axiograph.ordinal", ordinal.as_str()),
            ("axiograph.value_kind", row.value_kind.as_str()),
            ("axiograph.value", row.value.as_str()),
        ];
        if let Some(context) = context {
            attrs.push(("axiograph.axis_kind", context.axis_kind.as_str()));
        }
        db.add_relation(row.role_id.as_str(), source, target, 1.0, attrs);
        if let Some(context) = context {
            db.add_relation(
                REL_AXI_FACT_IN_CONTEXT,
                source,
                target,
                1.0,
                vec![
                    ("axiograph.axis_kind", context.axis_kind.as_str()),
                    ("axiograph.role_id", row.role_id.as_str()),
                ],
            );
        }
    }

    for row in &image.equivalences {
        let source = entity_ids
            .get(&row.source_entity_key)
            .copied()
            .ok_or_else(|| anyhow!("equivalence source is absent"))?;
        let target = entity_ids
            .get(&row.target_entity_key)
            .copied()
            .ok_or_else(|| anyhow!("equivalence target is absent"))?;
        db.add_equivalence(source, target, &row.generator_ref_json);
    }
    Ok(db)
}
