//! Modal Logic Support for PathDB
//!
//! This module provides modal logic operations that can be stored and queried
//! in PathDB, supporting:
//! - Kripke frames and models
//! - Epistemic logic (knowledge/belief)
//! - Deontic logic (obligation/permission)
//! - Temporal logic
//!
//! All structures are designed to be:
//! - Serializable to the shared binary format
//! - Compatible with the PathDB `.axpd` schema and certificate checker model
//! - Efficiently queryable via PathDB indexes

#![allow(unused_imports, unused_mut)]

use crate::verified::{
    EncodedAccessibility, EncodedModalFrame, EncodedWorld, ModalFrameType, VerifiedProb,
};
use crate::{EntityStore, PathDB, PathSig, RelationStore, StrId, StringInterner};
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Modal World Representation
// ============================================================================

/// A world in a modal frame, stored as an entity with special type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModalWorld {
    /// Entity ID in PathDB
    pub entity_id: u32,
    /// World-specific identifier
    pub world_id: u32,
    /// True propositions at this world (entity IDs)
    pub true_props: RoaringBitmap,
    /// Metadata (e.g., world description)
    pub metadata: HashMap<StrId, StrId>,
}

/// Accessibility relation between worlds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessibilityRelation {
    /// Relation type (e.g., "accessible", "knows_agent_alice")
    pub rel_type: StrId,
    /// From world -> to worlds
    pub edges: HashMap<u32, RoaringBitmap>,
}

impl AccessibilityRelation {
    pub fn new(rel_type: StrId) -> Self {
        Self {
            rel_type,
            edges: HashMap::new(),
        }
    }

    /// Add an accessibility edge
    pub fn add_edge(&mut self, from: u32, to: u32) {
        self.edges
            .entry(from)
            .or_insert_with(RoaringBitmap::new)
            .insert(to);
    }

    /// Get accessible worlds from a given world
    pub fn accessible(&self, from: u32) -> Option<&RoaringBitmap> {
        self.edges.get(&from)
    }

    /// Check if world is accessible
    pub fn is_accessible(&self, from: u32, to: u32) -> bool {
        self.edges.get(&from).map_or(false, |ws| ws.contains(to))
    }
}

// ============================================================================
// Modal Frame
// ============================================================================

/// A modal frame stored in PathDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModalFrame {
    /// Frame identifier
    pub frame_id: u32,
    /// Frame type
    pub frame_type: ModalFrameTypeTag,
    /// All worlds in the frame (world_id -> ModalWorld)
    pub worlds: HashMap<u32, ModalWorld>,
    /// Accessibility relations (indexed by relation type)
    pub accessibility: HashMap<StrId, AccessibilityRelation>,
    /// Optional agent association (for epistemic logic)
    pub agents: Vec<StrId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModalFrameTypeTag {
    Kripke,
    Epistemic,
    Deontic,
    Temporal,
}

impl ModalFrame {
    /// Create a new Kripke frame
    pub fn new_kripke(frame_id: u32) -> Self {
        Self {
            frame_id,
            frame_type: ModalFrameTypeTag::Kripke,
            worlds: HashMap::new(),
            accessibility: HashMap::new(),
            agents: Vec::new(),
        }
    }

    /// Create an epistemic frame with agents
    pub fn new_epistemic(frame_id: u32, agents: Vec<StrId>) -> Self {
        Self {
            frame_id,
            frame_type: ModalFrameTypeTag::Epistemic,
            worlds: HashMap::new(),
            accessibility: HashMap::new(),
            agents,
        }
    }

    /// Create a deontic frame
    pub fn new_deontic(frame_id: u32) -> Self {
        Self {
            frame_id,
            frame_type: ModalFrameTypeTag::Deontic,
            worlds: HashMap::new(),
            accessibility: HashMap::new(),
            agents: Vec::new(),
        }
    }

    /// Add a world to the frame
    pub fn add_world(&mut self, world: ModalWorld) {
        self.worlds.insert(world.world_id, world);
    }

    /// Add accessibility relation
    pub fn add_accessibility(&mut self, rel_type: StrId, from: u32, to: u32) {
        self.accessibility
            .entry(rel_type)
            .or_insert_with(|| AccessibilityRelation::new(rel_type))
            .add_edge(from, to);
    }

    /// Get world by ID
    pub fn get_world(&self, world_id: u32) -> Option<&ModalWorld> {
        self.worlds.get(&world_id)
    }

    /// Evaluate Box (necessity): true at w iff phi true at all accessible worlds
    pub fn eval_box(&self, w: u32, rel_type: StrId, phi_worlds: &RoaringBitmap) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            if let Some(accessible) = acc.accessible(w) {
                // All accessible worlds must be in phi_worlds
                accessible.is_subset(phi_worlds)
            } else {
                // No accessible worlds = vacuously true
                true
            }
        } else {
            true
        }
    }

    /// Evaluate Diamond (possibility): true at w iff phi true at some accessible world
    pub fn eval_diamond(&self, w: u32, rel_type: StrId, phi_worlds: &RoaringBitmap) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            if let Some(accessible) = acc.accessible(w) {
                // Some accessible world must be in phi_worlds
                !(&*accessible & phi_worlds).is_empty()
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Find all worlds where Box(phi) holds
    pub fn box_worlds(&self, rel_type: StrId, phi_worlds: &RoaringBitmap) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        for &w in self.worlds.keys() {
            if self.eval_box(w, rel_type, phi_worlds) {
                result.insert(w);
            }
        }
        result
    }

    /// Find all worlds where Diamond(phi) holds
    pub fn diamond_worlds(&self, rel_type: StrId, phi_worlds: &RoaringBitmap) -> RoaringBitmap {
        let mut result = RoaringBitmap::new();
        for &w in self.worlds.keys() {
            if self.eval_diamond(w, rel_type, phi_worlds) {
                result.insert(w);
            }
        }
        result
    }

    // ========================================================================
    // Frame Properties (for specific modal logics)
    // ========================================================================

    /// Check if frame is reflexive (T axiom: □p → p)
    pub fn is_reflexive(&self, rel_type: StrId) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            self.worlds.keys().all(|&w| acc.is_accessible(w, w))
        } else {
            true
        }
    }

    /// Check if frame is symmetric (B axiom: p → □◇p)
    pub fn is_symmetric(&self, rel_type: StrId) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            self.worlds.keys().all(|&w1| {
                self.worlds
                    .keys()
                    .all(|&w2| !acc.is_accessible(w1, w2) || acc.is_accessible(w2, w1))
            })
        } else {
            true
        }
    }

    /// Check if frame is transitive (4 axiom: □p → □□p)
    pub fn is_transitive(&self, rel_type: StrId) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            self.worlds.keys().all(|&w1| {
                self.worlds.keys().all(|&w2| {
                    self.worlds.keys().all(|&w3| {
                        !(acc.is_accessible(w1, w2) && acc.is_accessible(w2, w3))
                            || acc.is_accessible(w1, w3)
                    })
                })
            })
        } else {
            true
        }
    }

    /// Check if frame is serial (D axiom: □p → ◇p)
    pub fn is_serial(&self, rel_type: StrId) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            self.worlds
                .keys()
                .all(|&w| acc.accessible(w).map_or(false, |ws| !ws.is_empty()))
        } else {
            false
        }
    }

    /// Check if frame is Euclidean (5 axiom: ◇p → □◇p)
    pub fn is_euclidean(&self, rel_type: StrId) -> bool {
        if let Some(acc) = self.accessibility.get(&rel_type) {
            self.worlds.keys().all(|&w| {
                self.worlds.keys().all(|&w1| {
                    self.worlds.keys().all(|&w2| {
                        !(acc.is_accessible(w, w1) && acc.is_accessible(w, w2))
                            || acc.is_accessible(w1, w2)
                    })
                })
            })
        } else {
            true
        }
    }

    // ========================================================================
    // Serialization
    // ========================================================================

    /// Convert to binary-compatible encoding
    pub fn to_encoded(&self) -> EncodedModalFrame {
        let frame_type = match self.frame_type {
            ModalFrameTypeTag::Kripke => ModalFrameType::Kripke,
            ModalFrameTypeTag::Epistemic => ModalFrameType::Epistemic,
            ModalFrameTypeTag::Deontic => ModalFrameType::Deontic,
            ModalFrameTypeTag::Temporal => ModalFrameType::Temporal,
        };

        let worlds: Vec<EncodedWorld> = self
            .worlds
            .values()
            .map(|w| EncodedWorld {
                world_id: w.world_id,
                true_prop_ids: w.true_props.iter().collect(),
            })
            .collect();

        let accessibility: Vec<EncodedAccessibility> = self
            .accessibility
            .values()
            .map(|acc| {
                let edges: Vec<(u32, u32)> = acc
                    .edges
                    .iter()
                    .flat_map(|(&from, tos)| tos.iter().map(move |to| (from, to)))
                    .collect();
                EncodedAccessibility {
                    relation_name: acc.rel_type.0,
                    edges,
                }
            })
            .collect();

        EncodedModalFrame {
            frame_id: self.frame_id,
            frame_type,
            worlds,
            accessibility,
        }
    }
}

// ============================================================================
// Epistemic Logic Extensions
// ============================================================================

/// Epistemic frame with per-agent accessibility
pub struct EpistemicFrame {
    pub base: ModalFrame,
    /// Agent -> accessibility relation name
    pub agent_relations: HashMap<StrId, StrId>,
}

impl EpistemicFrame {
    /// Create new epistemic frame
    pub fn new(frame_id: u32, agents: Vec<StrId>, interner: &StringInterner) -> Self {
        let mut base = ModalFrame::new_epistemic(frame_id, agents.clone());
        let mut agent_relations = HashMap::new();

        // Create accessibility relation for each agent
        for agent in &agents {
            let agent_name = interner.lookup(*agent).unwrap_or_default();
            let rel_name = interner.intern(&format!("knows_{}", agent_name));
            agent_relations.insert(*agent, rel_name);
        }

        Self {
            base,
            agent_relations,
        }
    }

    /// Agent knows phi at world w
    pub fn knows(&self, agent: StrId, w: u32, phi_worlds: &RoaringBitmap) -> bool {
        if let Some(&rel_type) = self.agent_relations.get(&agent) {
            self.base.eval_box(w, rel_type, phi_worlds)
        } else {
            false
        }
    }

    /// Find worlds where agent knows phi
    pub fn knows_worlds(&self, agent: StrId, phi_worlds: &RoaringBitmap) -> RoaringBitmap {
        if let Some(&rel_type) = self.agent_relations.get(&agent) {
            self.base.box_worlds(rel_type, phi_worlds)
        } else {
            RoaringBitmap::new()
        }
    }

    /// Common knowledge among agents (bounded depth approximation)
    pub fn common_knowledge(
        &self,
        agents: &[StrId],
        phi_worlds: &RoaringBitmap,
        max_depth: usize,
    ) -> RoaringBitmap {
        let mut current = phi_worlds.clone();

        for _ in 0..max_depth {
            let mut everyone_knows = current.clone();
            for agent in agents {
                let agent_knows = self.knows_worlds(*agent, &current);
                everyone_knows &= &agent_knows;
            }

            // Fixed point check
            if everyone_knows == current {
                return current;
            }
            current = everyone_knows;
        }

        current
    }
}

// ============================================================================
// Deontic Logic Extensions
// ============================================================================

/// Deontic frame with ideal/acceptable worlds
pub struct DeonticFrame {
    pub base: ModalFrame,
    /// Ideal worlds relation
    pub ideal_rel: StrId,
}

impl DeonticFrame {
    /// Create new deontic frame
    pub fn new(frame_id: u32, ideal_rel: StrId) -> Self {
        Self {
            base: ModalFrame::new_deontic(frame_id),
            ideal_rel,
        }
    }

    /// Obligatory(phi) at world w: phi true at all ideal worlds
    pub fn obligatory(&self, w: u32, phi_worlds: &RoaringBitmap) -> bool {
        self.base.eval_box(w, self.ideal_rel, phi_worlds)
    }

    /// Permitted(phi) at world w: phi true at some ideal world
    pub fn permitted(&self, w: u32, phi_worlds: &RoaringBitmap) -> bool {
        self.base.eval_diamond(w, self.ideal_rel, phi_worlds)
    }

    /// Forbidden(phi) at world w: Obligatory(not-phi)
    pub fn forbidden(
        &self,
        w: u32,
        phi_worlds: &RoaringBitmap,
        all_worlds: &RoaringBitmap,
    ) -> bool {
        let not_phi = all_worlds - phi_worlds;
        self.obligatory(w, &not_phi)
    }
}

// ============================================================================
// PathDB Integration
// ============================================================================

/// Modal extension for PathDB
pub struct ModalPathDB {
    /// Base PathDB
    pub pathdb: PathDB,
    /// Modal frames indexed by frame_id
    pub frames: HashMap<u32, ModalFrame>,
    /// Entity to world mapping (which frame, which world)
    pub entity_to_world: HashMap<u32, (u32, u32)>,
}

impl ModalPathDB {
    /// Create new modal PathDB
    pub fn new() -> Self {
        Self {
            pathdb: PathDB::new(),
            frames: HashMap::new(),
            entity_to_world: HashMap::new(),
        }
    }

    /// Add a modal frame
    pub fn add_frame(&mut self, frame: ModalFrame) {
        // Register worlds as entities
        for (world_id, world) in &frame.worlds {
            self.entity_to_world
                .insert(world.entity_id, (frame.frame_id, *world_id));
        }
        self.frames.insert(frame.frame_id, frame);
    }

    /// Get frame by ID
    pub fn get_frame(&self, frame_id: u32) -> Option<&ModalFrame> {
        self.frames.get(&frame_id)
    }

    /// Query: Find entities where modal property holds
    /// e.g., "All entities where Knowledge(machinist, dangerous_for_titanium)"
    pub fn modal_query(
        &self,
        frame_id: u32,
        modality: Modality,
        rel_type: StrId,
        phi_entities: &RoaringBitmap,
    ) -> RoaringBitmap {
        let Some(frame) = self.frames.get(&frame_id) else {
            return RoaringBitmap::new();
        };

        // Convert entity IDs to world IDs
        let phi_worlds: RoaringBitmap = phi_entities
            .iter()
            .filter_map(|e| self.entity_to_world.get(&e).map(|(_, w)| *w))
            .collect();

        // Evaluate modality
        let result_worlds = match modality {
            Modality::Box => frame.box_worlds(rel_type, &phi_worlds),
            Modality::Diamond => frame.diamond_worlds(rel_type, &phi_worlds),
        };

        // Convert back to entity IDs
        let world_to_entity: HashMap<u32, u32> = frame
            .worlds
            .values()
            .map(|w| (w.world_id, w.entity_id))
            .collect();

        result_worlds
            .iter()
            .filter_map(|w| world_to_entity.get(&w).copied())
            .collect()
    }
}

/// Modal operators
#[derive(Debug, Clone, Copy)]
pub enum Modality {
    /// Necessity: □ (true in all accessible worlds)
    Box,
    /// Possibility: ◇ (true in some accessible world)
    Diamond,
}

impl Default for ModalPathDB {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kripke_frame() {
        let mut frame = ModalFrame::new_kripke(1);

        // Create worlds
        let mut props_w0 = RoaringBitmap::new();
        props_w0.insert(1); // p is true at w0

        let mut props_w1 = RoaringBitmap::new();
        props_w1.insert(2); // q is true at w1

        frame.add_world(ModalWorld {
            entity_id: 100,
            world_id: 0,
            true_props: props_w0,
            metadata: HashMap::new(),
        });

        frame.add_world(ModalWorld {
            entity_id: 101,
            world_id: 1,
            true_props: props_w1,
            metadata: HashMap::new(),
        });

        // w0 can access w1
        let acc_rel = StrId(10);
        frame.add_accessibility(acc_rel, 0, 1);

        // Test: At w0, Diamond(q) should be true
        let mut q_worlds = RoaringBitmap::new();
        q_worlds.insert(1); // w1 has q
        assert!(frame.eval_diamond(0, acc_rel, &q_worlds));

        // Test: At w0, Box(q) should be true (only accessible world is w1 where q holds)
        assert!(frame.eval_box(0, acc_rel, &q_worlds));
    }

    #[test]
    fn test_frame_properties() {
        let mut frame = ModalFrame::new_kripke(1);
        let acc_rel = StrId(10);

        // Add worlds
        for i in 0..3 {
            frame.add_world(ModalWorld {
                entity_id: 100 + i,
                world_id: i,
                true_props: RoaringBitmap::new(),
                metadata: HashMap::new(),
            });
        }

        // Make reflexive
        frame.add_accessibility(acc_rel, 0, 0);
        frame.add_accessibility(acc_rel, 1, 1);
        frame.add_accessibility(acc_rel, 2, 2);
        assert!(frame.is_reflexive(acc_rel));

        // Make symmetric
        frame.add_accessibility(acc_rel, 0, 1);
        frame.add_accessibility(acc_rel, 1, 0);
        assert!(frame.is_symmetric(acc_rel));

        // Make transitive
        frame.add_accessibility(acc_rel, 1, 2);
        frame.add_accessibility(acc_rel, 0, 2);
        assert!(frame.is_transitive(acc_rel));
    }
}
