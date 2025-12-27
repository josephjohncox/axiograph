//! Learning-oriented “extension structures” extracted from a PathDB knowledge graph.
//!
//! The canonical `.axi` language lets domains model learning constructs directly
//! (Concepts, safety guidelines, examples, prerequisites, etc.). In PathDB those
//! become ordinary entities/relations — but we still want *typed*, higher-level
//! tooling for:
//!
//! - conceptual discovery (what are the concepts? how do they depend on each other?),
//! - educational explanations (concept → guideline/example links),
//! - and future certificate-backed learning queries.
//!
//! This module provides a first step: build a schema-scoped, witness-carrying
//! `LearningGraph` from imported `.axi` instance data.

use crate::axi_typed::{AxiSchemaContext, AxiTypedEntity, AxiTypingContext};
use crate::PathDB;
use anyhow::{anyhow, Result};
use roaring::RoaringBitmap;
use std::collections::HashMap;

// -----------------------------------------------------------------------------
// Canonical learning vocabulary (v1)
// -----------------------------------------------------------------------------
//
// Today this matches the canonical example `examples/learning/MachinistLearning.axi`.
// Longer term, we likely want:
// - an explicit `.axi` extension block that declares which relations play these roles, or
// - per-schema annotations in the meta-plane.

pub const TYPE_CONCEPT: &str = "Concept";
pub const TYPE_SAFETY_GUIDELINE: &str = "SafetyGuideline";
pub const TYPE_EXAMPLE: &str = "Example";
pub const TYPE_TEXT: &str = "Text";

pub const REL_REQUIRES: &str = "requires";
pub const REL_EXPLAINS: &str = "explains";
pub const REL_DEMONSTRATES: &str = "demonstrates";
pub const REL_CONCEPT_DESCRIPTION: &str = "conceptDescription";

// -----------------------------------------------------------------------------
// Extracted “learning graph”
// -----------------------------------------------------------------------------

/// A typed binary edge extracted from PathDB.
#[derive(Debug, Clone, PartialEq)]
pub struct LearningEdge {
    pub rel_type: String,
    pub from: AxiTypedEntity,
    pub to: AxiTypedEntity,
    /// PathDB relation id (useful for anchored certificates via `PathDBExportV1`).
    pub relation_id: Option<u32>,
    pub confidence: f32,
}

/// Schema-scoped view of learning structures.
#[derive(Debug, Clone)]
pub struct LearningGraph {
    pub schema: String,

    /// Concepts declared in this schema (including subtypes of `Concept`).
    pub concepts: Vec<AxiTypedEntity>,

    /// Concept prerequisites: `requires(concept, prereq)`.
    pub requires: Vec<LearningEdge>,

    /// Concept → guideline links: `explains(concept, guideline)`.
    pub explains: Vec<LearningEdge>,

    /// Example → concept links: `demonstrates(example, concept)`.
    pub demonstrates: Vec<LearningEdge>,

    /// Concept descriptions: `conceptDescription(concept, text)`.
    pub concept_descriptions: Vec<LearningEdge>,
}

pub fn extract_learning_graph(db: &PathDB, schema_name: &str) -> Result<LearningGraph> {
    let typing = AxiTypingContext::from_db(db)?;
    let schema = typing.schema(schema_name).map_err(|e| anyhow!("{e}"))?;

    let concept_map = build_typed_entity_map(&schema, db, TYPE_CONCEPT)?;
    let guideline_map = build_typed_entity_map(&schema, db, TYPE_SAFETY_GUIDELINE)?;
    let example_map = build_typed_entity_map(&schema, db, TYPE_EXAMPLE)?;
    let text_map = build_typed_entity_map(&schema, db, TYPE_TEXT)?;

    let concept_set = ids_to_bitmap(concept_map.keys().copied());
    let guideline_set = ids_to_bitmap(guideline_map.keys().copied());
    let example_set = ids_to_bitmap(example_map.keys().copied());
    let text_set = ids_to_bitmap(text_map.keys().copied());

    let requires = extract_edges(
        db,
        REL_REQUIRES,
        &concept_set,
        &concept_map,
        &concept_set,
        &concept_map,
    )?;

    let explains = extract_edges(
        db,
        REL_EXPLAINS,
        &concept_set,
        &concept_map,
        &guideline_set,
        &guideline_map,
    )?;

    let demonstrates = extract_edges(
        db,
        REL_DEMONSTRATES,
        &example_set,
        &example_map,
        &concept_set,
        &concept_map,
    )?;

    let concept_descriptions = extract_edges(
        db,
        REL_CONCEPT_DESCRIPTION,
        &concept_set,
        &concept_map,
        &text_set,
        &text_map,
    )?;

    let mut concepts: Vec<AxiTypedEntity> = concept_map.into_values().collect();
    concepts.sort_by_key(|c| c.raw_entity_id());

    Ok(LearningGraph {
        schema: schema_name.to_string(),
        concepts,
        requires,
        explains,
        demonstrates,
        concept_descriptions,
    })
}

fn build_typed_entity_map(
    schema: &AxiSchemaContext,
    db: &PathDB,
    type_name: &str,
) -> Result<HashMap<u32, AxiTypedEntity>> {
    let ids = schema.find_by_axi_type(db, type_name);
    let mut out = HashMap::new();
    for id in ids.iter() {
        let typed = schema
            .typed_entity(db, id, type_name)
            .map_err(|e| anyhow!("{e}"))?;
        out.insert(id, typed);
    }
    Ok(out)
}

fn ids_to_bitmap(ids: impl Iterator<Item = u32>) -> RoaringBitmap {
    let mut out = RoaringBitmap::new();
    out.extend(ids);
    out
}

fn extract_edges(
    db: &PathDB,
    rel_type: &str,
    from_set: &RoaringBitmap,
    from_map: &HashMap<u32, AxiTypedEntity>,
    to_set: &RoaringBitmap,
    to_map: &HashMap<u32, AxiTypedEntity>,
) -> Result<Vec<LearningEdge>> {
    let Some(rel_type_id) = db.interner.id_of(rel_type) else {
        // Relation label not present in the DB.
        return Ok(Vec::new());
    };

    let mut out: Vec<LearningEdge> = Vec::new();
    for from_id in from_set.iter() {
        let targets = db.relations.targets(from_id, rel_type_id);
        for to_id in targets.iter() {
            if !to_set.contains(to_id) {
                continue;
            }

            let Some(from) = from_map.get(&from_id).cloned() else {
                continue;
            };
            let Some(to) = to_map.get(&to_id).cloned() else {
                continue;
            };

            let relation_id = db.relations.edge_relation_id(from_id, rel_type_id, to_id);
            let confidence = relation_id
                .and_then(|id| db.relations.get_relation(id).map(|r| r.confidence))
                .unwrap_or(1.0);

            out.push(LearningEdge {
                rel_type: rel_type.to_string(),
                from,
                to,
                relation_id,
                confidence,
            });
        }
    }

    Ok(out)
}
