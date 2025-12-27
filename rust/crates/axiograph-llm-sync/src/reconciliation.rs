//! Fact Reconciliation: Handling Conflicts with Weighting and Bayesian Updates
//!
//! When facts conflict, we need principled ways to resolve them:
//!
//! 1. **Evidence Aggregation**: Combine multiple sources
//! 2. **Bayesian Updates**: Update beliefs with new evidence  
//! 3. **Voting/Weighting**: Up/down votes, source credibility
//! 4. **Temporal Decay**: Older facts lose weight
//! 5. **Expert Override**: High-trust sources can override
//!
//! ## Conflict Resolution Strategy
//!
//! ```text
//! New Fact ──┬──► No Conflict ──► Direct Integration
//!            │
//!            └──► Conflict Detected
//!                     │
//!                     ├──► Same Source ──► Replace (newer wins)
//!                     │
//!                     ├──► Different Sources
//!                     │         │
//!                     │         ├──► Bayesian Update (if probabilistic)
//!                     │         ├──► Weight Comparison (if deterministic)
//!                     │         └──► Human Review (if uncertain)
//!                     │
//!                     └──► Schema Conflict ──► Reject or Schema Extension
//! ```

#![allow(unused_imports)]

use crate::{Conflict, ConflictType, ExtractedFact, FactId, Resolution, StructuredFact};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Evidence & Weight Types
// ============================================================================

/// Weight assigned to a fact (0.0 to 1.0)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Weight(f32);

impl Weight {
    pub fn new(w: f32) -> Self {
        Self(w.clamp(0.0, 1.0))
    }

    pub fn value(&self) -> f32 {
        self.0
    }

    /// Combine weights (multiplicative)
    pub fn combine(&self, other: Weight) -> Weight {
        Weight::new(self.0 * other.0)
    }

    /// Bayesian update: P(H|E) = P(E|H) * P(H) / P(E)
    pub fn bayesian_update(&self, likelihood: f32, prior_evidence: f32) -> Weight {
        if prior_evidence <= 0.0 {
            return *self;
        }
        let posterior = (likelihood * self.0) / prior_evidence;
        Weight::new(posterior)
    }
}

impl Default for Weight {
    fn default() -> Self {
        Self(0.5) // Neutral weight
    }
}

/// Source credibility (how much we trust this source)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceCredibility {
    pub source_id: String,
    pub base_credibility: Weight,
    pub domain_expertise: HashMap<String, Weight>, // domain -> expertise weight
    pub track_record: TrackRecord,
}

impl SourceCredibility {
    pub fn new(source_id: &str, base: f32) -> Self {
        Self {
            source_id: source_id.to_string(),
            base_credibility: Weight::new(base),
            domain_expertise: HashMap::new(),
            track_record: TrackRecord::default(),
        }
    }

    /// Get effective credibility for a domain
    pub fn credibility_for(&self, domain: &str) -> Weight {
        let domain_weight = self
            .domain_expertise
            .get(domain)
            .copied()
            .unwrap_or(Weight::new(0.5));

        // Combine base credibility with domain expertise
        let combined = self.base_credibility.combine(domain_weight);

        // Adjust by track record only once we actually have outcomes; otherwise
        // keep "no data" neutral rather than penalizing the score.
        let total = self.track_record.correct + self.track_record.incorrect;
        if total == 0 {
            return combined;
        }

        let accuracy = self.track_record.accuracy();
        combined.combine(Weight::new(accuracy))
    }

    /// Update track record after verification
    pub fn record_outcome(&mut self, correct: bool) {
        if correct {
            self.track_record.correct += 1;
        } else {
            self.track_record.incorrect += 1;
        }
    }
}

/// Track record of source accuracy
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackRecord {
    pub correct: u32,
    pub incorrect: u32,
}

impl TrackRecord {
    pub fn accuracy(&self) -> f32 {
        let total = self.correct + self.incorrect;
        if total == 0 {
            return 0.5; // No track record = neutral
        }
        self.correct as f32 / total as f32
    }
}

/// Evidence supporting or refuting a fact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub id: Uuid,
    pub source_id: String,
    pub evidence_type: EvidenceType,
    pub strength: Weight,
    pub timestamp: DateTime<Utc>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvidenceType {
    Supports,
    Refutes,
    Neutral,
    Clarifies,
}

// ============================================================================
// Weighted Fact
// ============================================================================

/// A fact with associated weight and evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedFact {
    pub fact_id: FactId,
    pub content: StructuredFact,
    /// Current aggregated weight
    pub weight: Weight,
    /// Individual votes/evidence
    pub evidence: Vec<Evidence>,
    /// Source credibilities that contributed
    pub sources: Vec<String>,
    /// When first introduced
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// Number of supporting votes
    pub upvotes: u32,
    /// Number of refuting votes
    pub downvotes: u32,
}

impl WeightedFact {
    pub fn new(fact_id: FactId, content: StructuredFact, initial_weight: f32) -> Self {
        let now = Utc::now();
        Self {
            fact_id,
            content,
            weight: Weight::new(initial_weight),
            evidence: Vec::new(),
            sources: Vec::new(),
            created_at: now,
            updated_at: now,
            upvotes: 0,
            downvotes: 0,
        }
    }

    /// Add evidence and update weight
    pub fn add_evidence(&mut self, evidence: Evidence, source_credibility: &SourceCredibility) {
        let domain = self.infer_domain();
        let source_weight = source_credibility.credibility_for(&domain);

        match evidence.evidence_type {
            EvidenceType::Supports => {
                // Bayesian update: increase belief
                let support = source_weight.value() * evidence.strength.value();
                // Map support strength into a likelihood in [0.5, 1.0] where 0.5 is neutral.
                let likelihood = 0.5 + (support / 2.0);
                self.weight = self.weight.bayesian_update(
                    likelihood, 0.5, // Prior evidence (could be more sophisticated)
                );
                self.upvotes += 1;
            }
            EvidenceType::Refutes => {
                // Decrease belief
                let refutation = 1.0 - (source_weight.value() * evidence.strength.value());
                self.weight = Weight::new(self.weight.value() * refutation);
                self.downvotes += 1;
            }
            EvidenceType::Neutral | EvidenceType::Clarifies => {
                // No weight change, but record evidence
            }
        }

        if !self.sources.contains(&evidence.source_id) {
            self.sources.push(evidence.source_id.clone());
        }
        self.evidence.push(evidence);
        self.updated_at = Utc::now();
    }

    /// Apply temporal decay
    pub fn apply_decay(&mut self, half_life_days: f64) {
        let age = Utc::now().signed_duration_since(self.updated_at);
        let days = age.num_seconds() as f64 / 86400.0;

        // Exponential decay: w * 2^(-t/half_life)
        let decay_factor = 0.5_f64.powf(days / half_life_days);
        self.weight = Weight::new((self.weight.value() as f64 * decay_factor) as f32);
    }

    /// Upvote this fact
    pub fn upvote(&mut self, source_id: &str, strength: f32) {
        self.add_evidence(
            Evidence {
                id: Uuid::new_v4(),
                source_id: source_id.to_string(),
                evidence_type: EvidenceType::Supports,
                strength: Weight::new(strength),
                timestamp: Utc::now(),
                description: "Upvote".to_string(),
            },
            &SourceCredibility::new(source_id, 0.5),
        );
    }

    /// Downvote this fact
    pub fn downvote(&mut self, source_id: &str, strength: f32) {
        self.add_evidence(
            Evidence {
                id: Uuid::new_v4(),
                source_id: source_id.to_string(),
                evidence_type: EvidenceType::Refutes,
                strength: Weight::new(strength),
                timestamp: Utc::now(),
                description: "Downvote".to_string(),
            },
            &SourceCredibility::new(source_id, 0.5),
        );
    }

    /// Net vote count
    pub fn net_votes(&self) -> i32 {
        self.upvotes as i32 - self.downvotes as i32
    }

    /// Infer domain from fact content
    fn infer_domain(&self) -> String {
        match &self.content {
            StructuredFact::Entity { entity_type, .. } => entity_type.to_lowercase(),
            StructuredFact::Relation { rel_type, .. } => rel_type.to_lowercase(),
            StructuredFact::Constraint { .. } => "constraints".to_string(),
            StructuredFact::TacitKnowledge { domain, .. } => domain.clone(),
        }
    }
}

// ============================================================================
// Reconciliation Engine
// ============================================================================

/// Configuration for reconciliation behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationConfig {
    /// Minimum weight difference to auto-resolve
    pub auto_resolve_threshold: f32,
    /// Weight threshold below which facts are considered "dead"
    pub discard_threshold: f32,
    /// Half-life for temporal decay (days)
    pub decay_half_life: f64,
    /// Require human review for high-impact conflicts
    pub human_review_threshold: f32,
    /// Trust expert sources absolutely
    pub expert_override: bool,
    /// Domains where expert override applies
    pub expert_domains: Vec<String>,
}

impl Default for ReconciliationConfig {
    fn default() -> Self {
        Self {
            auto_resolve_threshold: 0.3,
            discard_threshold: 0.1,
            decay_half_life: 30.0,
            human_review_threshold: 0.7,
            expert_override: true,
            expert_domains: vec!["safety".to_string(), "constraints".to_string()],
        }
    }
}

/// The reconciliation engine
pub struct ReconciliationEngine {
    config: ReconciliationConfig,
    /// Source credibility registry
    sources: HashMap<String, SourceCredibility>,
    /// Active weighted facts
    facts: HashMap<FactId, WeightedFact>,
    /// Conflict history
    conflicts: Vec<ResolvedConflict>,
}

impl ReconciliationEngine {
    pub fn new(config: ReconciliationConfig) -> Self {
        Self {
            config,
            sources: HashMap::new(),
            facts: HashMap::new(),
            conflicts: Vec::new(),
        }
    }

    /// Register a source with credibility
    pub fn register_source(&mut self, source: SourceCredibility) {
        self.sources.insert(source.source_id.clone(), source);
    }

    /// Get or create source credibility
    fn get_source(&self, source_id: &str) -> SourceCredibility {
        self.sources
            .get(source_id)
            .cloned()
            .unwrap_or_else(|| SourceCredibility::new(source_id, 0.5))
    }

    /// Attempt to reconcile a new fact with existing knowledge
    pub fn reconcile(&mut self, new_fact: ExtractedFact) -> ReconciliationResult {
        // Check for conflicts
        let conflicts = self.find_conflicts(&new_fact);

        if conflicts.is_empty() {
            // No conflict - integrate directly
            let weighted = WeightedFact::new(
                new_fact.id,
                new_fact.structured.clone(),
                new_fact.confidence,
            );
            self.facts.insert(new_fact.id, weighted);

            return ReconciliationResult {
                action: ReconciliationAction::Integrated,
                fact_id: new_fact.id,
                weight: Weight::new(new_fact.confidence),
                conflicts_resolved: vec![],
                requires_review: false,
            };
        }

        // Resolve conflicts
        self.resolve_conflicts(new_fact, conflicts)
    }

    /// Find facts that conflict with the new fact
    fn find_conflicts(&self, new_fact: &ExtractedFact) -> Vec<(FactId, ConflictType)> {
        let mut conflicts = Vec::new();

        for (id, existing) in &self.facts {
            if let Some(conflict_type) =
                self.check_conflict(&new_fact.structured, &existing.content)
            {
                conflicts.push((*id, conflict_type));
            }
        }

        conflicts
    }

    /// Check if two facts conflict
    fn check_conflict(
        &self,
        new: &StructuredFact,
        existing: &StructuredFact,
    ) -> Option<ConflictType> {
        match (new, existing) {
            // Same entity, different attributes
            (
                StructuredFact::Entity {
                    entity_type: t1,
                    name: n1,
                    attributes: a1,
                },
                StructuredFact::Entity {
                    entity_type: t2,
                    name: n2,
                    attributes: a2,
                },
            ) if t1 == t2 && n1 == n2 => {
                // Check for attribute conflicts
                for (k, v1) in a1 {
                    if let Some(v2) = a2.get(k) {
                        if v1 != v2 {
                            return Some(ConflictType::AttributeMismatch);
                        }
                    }
                }
                None
            }

            // Same relation, different confidence or attributes
            (
                StructuredFact::Relation {
                    rel_type: t1,
                    source: s1,
                    target: tg1,
                    ..
                },
                StructuredFact::Relation {
                    rel_type: t2,
                    source: s2,
                    target: tg2,
                    ..
                },
            ) if t1 == t2 && s1 == s2 && tg1 == tg2 => Some(ConflictType::ConfidenceConflict),

            // Contradictory tacit knowledge
            (
                StructuredFact::TacitKnowledge {
                    rule: r1,
                    domain: d1,
                    ..
                },
                StructuredFact::TacitKnowledge {
                    rule: r2,
                    domain: d2,
                    ..
                },
            ) if d1 == d2 && self.rules_contradict(r1, r2) => Some(ConflictType::Contradiction),

            _ => None,
        }
    }

    /// Check if two rules contradict
    fn rules_contradict(&self, r1: &str, r2: &str) -> bool {
        // Simple heuristic: rules with "never" vs "always" on same topic
        let r1_lower = r1.to_lowercase();
        let r2_lower = r2.to_lowercase();

        (r1_lower.contains("always") && r2_lower.contains("never"))
            || (r1_lower.contains("never") && r2_lower.contains("always"))
    }

    /// Resolve conflicts between new fact and existing facts
    fn resolve_conflicts(
        &mut self,
        new_fact: ExtractedFact,
        conflicts: Vec<(FactId, ConflictType)>,
    ) -> ReconciliationResult {
        let source = self.get_source(&new_fact.source.provider.to_string());
        let domain = self.infer_domain(&new_fact.structured);
        let new_weight = Weight::new(new_fact.confidence).combine(source.credibility_for(&domain));

        let mut resolutions = Vec::new();
        let mut requires_review = false;

        for (existing_id, conflict_type) in conflicts {
            let existing = match self.facts.get(&existing_id) {
                Some(f) => f,
                None => continue,
            };

            let resolution =
                self.determine_resolution(&new_fact, new_weight, existing, &conflict_type, &source);

            match &resolution {
                Resolution::ReplaceOld => {
                    // Remove old, add new
                    self.facts.remove(&existing_id);
                }
                Resolution::KeepOld => {
                    // Do nothing, new fact discarded
                }
                Resolution::Merge { weights } => {
                    // Merge facts with weighted average
                    if let Some(merged) =
                        self.merge_facts(&new_fact, existing, weights.0, weights.1)
                    {
                        self.facts.insert(existing_id, merged);
                    }
                }
                Resolution::HumanReview => {
                    requires_review = true;
                }
            }

            resolutions.push(ResolvedConflict {
                new_fact_id: new_fact.id,
                existing_fact_id: existing_id,
                conflict_type: conflict_type.clone(),
                resolution: resolution.clone(),
                timestamp: Utc::now(),
            });
        }

        // If any conflict requires review, don't integrate automatically
        if requires_review {
            return ReconciliationResult {
                action: ReconciliationAction::PendingReview,
                fact_id: new_fact.id,
                weight: new_weight,
                conflicts_resolved: resolutions,
                requires_review: true,
            };
        }

        // Check if new fact should be integrated
        let should_integrate = resolutions.iter().all(|r| {
            matches!(
                r.resolution,
                Resolution::ReplaceOld | Resolution::Merge { .. }
            )
        });

        if should_integrate {
            let weighted = WeightedFact::new(new_fact.id, new_fact.structured, new_weight.value());
            self.facts.insert(new_fact.id, weighted);

            ReconciliationResult {
                action: ReconciliationAction::Integrated,
                fact_id: new_fact.id,
                weight: new_weight,
                conflicts_resolved: resolutions,
                requires_review: false,
            }
        } else {
            ReconciliationResult {
                action: ReconciliationAction::Discarded,
                fact_id: new_fact.id,
                weight: new_weight,
                conflicts_resolved: resolutions,
                requires_review: false,
            }
        }
    }

    /// Determine how to resolve a specific conflict
    fn determine_resolution(
        &self,
        new_fact: &ExtractedFact,
        new_weight: Weight,
        existing: &WeightedFact,
        conflict_type: &ConflictType,
        source: &SourceCredibility,
    ) -> Resolution {
        let domain = self.infer_domain(&new_fact.structured);
        let weight_diff = new_weight.value() - existing.weight.value();

        // Expert override for critical domains
        if self.config.expert_override
            && self.config.expert_domains.contains(&domain)
            && source.base_credibility.value() > 0.9
        {
            return Resolution::ReplaceOld;
        }

        // High-impact conflicts need human review
        if existing.weight.value() > self.config.human_review_threshold
            && matches!(
                conflict_type,
                ConflictType::Contradiction | ConflictType::SchemaViolation
            )
        {
            return Resolution::HumanReview;
        }

        // Auto-resolve if weight difference is clear
        if weight_diff.abs() > self.config.auto_resolve_threshold {
            if weight_diff > 0.0 {
                Resolution::ReplaceOld
            } else {
                Resolution::KeepOld
            }
        } else {
            // Close weights - merge
            let total = new_weight.value() + existing.weight.value();
            if total > 0.0 {
                Resolution::Merge {
                    weights: (new_weight.value() / total, existing.weight.value() / total),
                }
            } else {
                Resolution::HumanReview
            }
        }
    }

    /// Merge two facts with weights
    fn merge_facts(
        &self,
        new_fact: &ExtractedFact,
        existing: &WeightedFact,
        new_weight: f32,
        existing_weight: f32,
    ) -> Option<WeightedFact> {
        // For now, take weighted average of confidence
        let merged_weight =
            new_weight * new_fact.confidence + existing_weight * existing.weight.value();

        let mut merged = existing.clone();
        merged.weight = Weight::new(merged_weight);
        merged.updated_at = Utc::now();

        // Add new source
        if let Some(source) = new_fact
            .source
            .provider
            .to_string()
            .split_whitespace()
            .next()
        {
            if !merged.sources.contains(&source.to_string()) {
                merged.sources.push(source.to_string());
            }
        }

        Some(merged)
    }

    fn infer_domain(&self, fact: &StructuredFact) -> String {
        match fact {
            StructuredFact::Entity { entity_type, .. } => entity_type.to_lowercase(),
            StructuredFact::Relation { rel_type, .. } => rel_type.to_lowercase(),
            StructuredFact::Constraint { .. } => "constraints".to_string(),
            StructuredFact::TacitKnowledge { domain, .. } => domain.clone(),
        }
    }

    // ========================================================================
    // Voting API
    // ========================================================================

    /// Upvote a fact
    pub fn upvote(&mut self, fact_id: FactId, voter_id: &str, strength: f32) -> Option<Weight> {
        let source = self.get_source(voter_id);
        if let Some(fact) = self.facts.get_mut(&fact_id) {
            fact.add_evidence(
                Evidence {
                    id: Uuid::new_v4(),
                    source_id: voter_id.to_string(),
                    evidence_type: EvidenceType::Supports,
                    strength: Weight::new(strength),
                    timestamp: Utc::now(),
                    description: "User upvote".to_string(),
                },
                &source,
            );
            Some(fact.weight)
        } else {
            None
        }
    }

    /// Downvote a fact
    pub fn downvote(&mut self, fact_id: FactId, voter_id: &str, strength: f32) -> Option<Weight> {
        let source = self.get_source(voter_id);
        if let Some(fact) = self.facts.get_mut(&fact_id) {
            fact.add_evidence(
                Evidence {
                    id: Uuid::new_v4(),
                    source_id: voter_id.to_string(),
                    evidence_type: EvidenceType::Refutes,
                    strength: Weight::new(strength),
                    timestamp: Utc::now(),
                    description: "User downvote".to_string(),
                },
                &source,
            );
            Some(fact.weight)
        } else {
            None
        }
    }

    /// Apply Bayesian update to a fact given new evidence
    pub fn bayesian_update(
        &mut self,
        fact_id: FactId,
        likelihood_if_true: f32,
        likelihood_if_false: f32,
    ) -> Option<Weight> {
        if let Some(fact) = self.facts.get_mut(&fact_id) {
            let prior = fact.weight.value();

            // P(H|E) = P(E|H) * P(H) / P(E)
            // P(E) = P(E|H)*P(H) + P(E|¬H)*P(¬H)
            let p_evidence = likelihood_if_true * prior + likelihood_if_false * (1.0 - prior);

            if p_evidence > 0.0 {
                let posterior = (likelihood_if_true * prior) / p_evidence;
                fact.weight = Weight::new(posterior);
                fact.updated_at = Utc::now();
            }

            Some(fact.weight)
        } else {
            None
        }
    }

    // ========================================================================
    // Maintenance
    // ========================================================================

    /// Apply temporal decay to all facts
    pub fn decay_all(&mut self) {
        for fact in self.facts.values_mut() {
            fact.apply_decay(self.config.decay_half_life);
        }
    }

    /// Remove facts below discard threshold
    pub fn prune_dead_facts(&mut self) -> Vec<FactId> {
        let threshold = self.config.discard_threshold;
        let to_remove: Vec<FactId> = self
            .facts
            .iter()
            .filter(|(_, f)| f.weight.value() < threshold)
            .map(|(id, _)| *id)
            .collect();

        for id in &to_remove {
            self.facts.remove(id);
        }

        to_remove
    }

    /// Get fact by ID
    pub fn get_fact(&self, id: FactId) -> Option<&WeightedFact> {
        self.facts.get(&id)
    }

    /// Get all facts above a weight threshold
    pub fn get_confident_facts(&self, threshold: f32) -> Vec<&WeightedFact> {
        self.facts
            .values()
            .filter(|f| f.weight.value() >= threshold)
            .collect()
    }

    /// Update source credibility based on verification
    pub fn update_source_credibility(&mut self, source_id: &str, correct: bool) {
        if let Some(source) = self.sources.get_mut(source_id) {
            source.record_outcome(correct);
        }
    }

    // ========================================================================
    // Export/Import for E2E Interop
    // ========================================================================

    /// Export current state to binary format
    pub fn export_state(&self) -> crate::reconciliation_format::ReconciliationState {
        crate::reconciliation_format::ReconciliationState {
            sources: self.sources.values().cloned().collect(),
            facts: self.facts.values().cloned().collect(),
            conflicts: self.conflicts.clone(),
        }
    }

    /// Import state from binary format
    pub fn import_state(&mut self, state: crate::reconciliation_format::ReconciliationState) {
        // Import sources
        for source in state.sources {
            self.sources.insert(source.source_id.clone(), source);
        }

        // Import facts
        for fact in state.facts {
            self.facts.insert(fact.fact_id, fact);
        }

        // Import conflicts
        self.conflicts = state.conflicts;
    }

    /// Save state to file
    pub fn save_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let state = self.export_state();
        state.save(path)?;
        Ok(())
    }

    /// Load state from file
    pub fn load_from_file(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let state = crate::reconciliation_format::ReconciliationState::load(path)?;
        self.import_state(state);
        Ok(())
    }

    /// Get all sources
    pub fn sources(&self) -> &HashMap<String, SourceCredibility> {
        &self.sources
    }

    /// Get all weighted facts
    pub fn facts(&self) -> &HashMap<FactId, WeightedFact> {
        &self.facts
    }

    /// Get resolved conflicts history
    pub fn resolved_conflicts(&self) -> &Vec<ResolvedConflict> {
        &self.conflicts
    }
}

// ============================================================================
// Result Types
// ============================================================================

/// Result of reconciliation attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationResult {
    pub action: ReconciliationAction,
    pub fact_id: FactId,
    pub weight: Weight,
    pub conflicts_resolved: Vec<ResolvedConflict>,
    pub requires_review: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReconciliationAction {
    /// Fact integrated successfully
    Integrated,
    /// Fact merged with existing
    Merged,
    /// Fact discarded (existing wins)
    Discarded,
    /// Requires human review
    PendingReview,
}

/// Record of a resolved conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConflict {
    pub new_fact_id: FactId,
    pub existing_fact_id: FactId,
    pub conflict_type: ConflictType,
    pub resolution: Resolution,
    pub timestamp: DateTime<Utc>,
}

// ============================================================================
// Helper for LLMProvider Display
// ============================================================================

impl std::fmt::Display for crate::LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            crate::LLMProvider::OpenAI { model } => write!(f, "openai:{}", model),
            crate::LLMProvider::Anthropic { model } => write!(f, "anthropic:{}", model),
            crate::LLMProvider::Local { model_path } => write!(f, "local:{}", model_path),
            crate::LLMProvider::Custom { name, .. } => write!(f, "custom:{}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_weight_operations() {
        let w1 = Weight::new(0.8);
        let w2 = Weight::new(0.6);

        assert!((w1.value() - 0.8).abs() < 0.001);

        let combined = w1.combine(w2);
        assert!((combined.value() - 0.48).abs() < 0.001);
    }

    #[test]
    fn test_weight_clamping() {
        let over = Weight::new(1.5);
        assert!((over.value() - 1.0).abs() < 0.001);

        let under = Weight::new(-0.5);
        assert!((under.value() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_bayesian_update() {
        let prior = Weight::new(0.5);

        // Strong evidence
        let posterior = prior.bayesian_update(0.9, 0.5);
        assert!(posterior.value() > 0.5);

        // Weak evidence against
        let posterior = prior.bayesian_update(0.1, 0.5);
        assert!(posterior.value() < 0.5);
    }

    #[test]
    fn test_source_credibility() {
        let mut source = SourceCredibility::new("expert", 0.9);
        source
            .domain_expertise
            .insert("machining".to_string(), Weight::new(0.95));

        let cred = source.credibility_for("machining");
        assert!(cred.value() > 0.8);

        let unknown = source.credibility_for("unknown");
        assert!(unknown.value() < cred.value());
    }

    #[test]
    fn test_track_record() {
        let mut source = SourceCredibility::new("user1", 0.5);

        source.record_outcome(true);
        source.record_outcome(true);
        source.record_outcome(false);

        assert!((source.track_record.accuracy() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_weighted_fact_voting() {
        let mut fact = WeightedFact::new(
            Uuid::new_v4(),
            StructuredFact::Entity {
                entity_type: "Material".to_string(),
                name: "Steel".to_string(),
                attributes: HashMap::new(),
            },
            0.5,
        );

        let initial = fact.weight.value();

        fact.upvote("user1", 0.8);
        assert!(fact.weight.value() > initial);
        assert_eq!(fact.upvotes, 1);

        fact.downvote("user2", 0.6);
        assert_eq!(fact.downvotes, 1);
    }

    #[test]
    fn test_temporal_decay() {
        let mut fact = WeightedFact::new(
            Uuid::new_v4(),
            StructuredFact::Entity {
                entity_type: "Test".to_string(),
                name: "Test".to_string(),
                attributes: HashMap::new(),
            },
            1.0,
        );

        // Simulate aging
        fact.updated_at = Utc::now() - Duration::days(30);
        fact.apply_decay(30.0); // 30-day half-life

        // Should be approximately halved
        assert!((fact.weight.value() - 0.5).abs() < 0.1);
    }

    #[test]
    fn test_reconciliation_no_conflict() {
        let mut engine = ReconciliationEngine::new(ReconciliationConfig::default());

        let fact = ExtractedFact {
            id: Uuid::new_v4(),
            claim: "Steel is a Material".to_string(),
            structured: StructuredFact::Entity {
                entity_type: "Material".to_string(),
                name: "Steel".to_string(),
                attributes: HashMap::new(),
            },
            confidence: 0.9,
            source: crate::FactSource {
                session_id: Uuid::new_v4(),
                provider: crate::LLMProvider::Custom {
                    name: "test".to_string(),
                    endpoint: "local".to_string(),
                },
                conversation_turns: vec![],
                extraction_timestamp: Utc::now(),
                human_verified: false,
            },
            status: crate::FactStatus::Pending,
        };

        let result = engine.reconcile(fact);
        assert!(matches!(result.action, ReconciliationAction::Integrated));
    }
}
