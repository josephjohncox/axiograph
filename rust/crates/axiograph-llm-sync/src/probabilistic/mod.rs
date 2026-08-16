//! Bounded evidence-plane confidence estimation with finite factor graphs.
//!
//! This module supports validated finite unary and binary potentials, optional
//! observations, loopy belief propagation, and score calibration. Beliefs on
//! cyclic graphs are approximate and report whether iteration converged. These
//! scores are non-authoritative evidence: they do not establish path equality,
//! accepted ontology meaning, or a trusted-kernel theorem.

use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Factor Graph Core
// ============================================================================

pub const MAX_FACTOR_DOMAIN_SIZE: usize = 4_096;

/// A variable in the factor graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariableId(pub Uuid);

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum FactorGraphError {
    #[error("factor-graph variable domains must be non-empty")]
    EmptyDomain,
    #[error("factor-graph variable domain {size} exceeds limit {maximum}")]
    DomainTooLarge { size: usize, maximum: usize },
    #[error("unknown factor-graph variable {0:?}")]
    UnknownVariable(VariableId),
    #[error("inconsistent factor graph: {reason}")]
    InconsistentGraph { reason: String },
    #[error("observation {value} is outside variable domain 0..{domain_size}")]
    ObservationOutOfRange { value: usize, domain_size: usize },
    #[error("invalid factor potential: {reason}")]
    InvalidPotential { reason: String },
    #[error("duplicate reconciliation fact id {0}")]
    DuplicateFact(Uuid),
    #[error("missing reconciliation fact id {0}")]
    MissingFact(Uuid),
}

/// A factor connecting variables
#[derive(Debug, Clone)]
pub struct Factor {
    pub id: Uuid,
    pub variables: Vec<VariableId>,
    pub potential: FactorPotential,
}

/// Factor potential function (unnormalized probability)
#[derive(Debug, Clone)]
pub enum FactorPotential {
    /// Unary factor: P(X)
    Unary(Vec<f64>),
    /// Binary factor: P(X, Y) as |X| x |Y| matrix
    Binary(Vec<Vec<f64>>),
}

// ============================================================================
// Factor Graph
// ============================================================================

/// Complete factor graph for probabilistic reasoning
#[derive(Debug)]
pub struct FactorGraph {
    variables: HashMap<VariableId, Variable>,
    factors: HashMap<Uuid, Factor>,
    /// Edges: variable -> factors containing it
    var_to_factors: HashMap<VariableId, Vec<Uuid>>,
    /// Edges: factor -> variables it contains
    factor_to_vars: HashMap<Uuid, Vec<VariableId>>,
}

#[derive(Debug, Clone)]
pub struct Variable {
    pub id: VariableId,
    pub name: String,
    pub domain_size: usize,
    pub observed: Option<usize>, // Observed value if any
}

impl Default for FactorGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl FactorGraph {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            factors: HashMap::new(),
            var_to_factors: HashMap::new(),
            factor_to_vars: HashMap::new(),
        }
    }

    /// Add a binary variable.
    pub fn add_variable(&mut self, name: &str) -> VariableId {
        self.insert_variable(name, 2)
    }

    /// Add a variable with an arbitrary, non-empty domain.
    pub fn add_variable_with_domain(
        &mut self,
        name: &str,
        domain_size: usize,
    ) -> Result<VariableId, FactorGraphError> {
        if domain_size == 0 {
            return Err(FactorGraphError::EmptyDomain);
        }
        if domain_size > MAX_FACTOR_DOMAIN_SIZE {
            return Err(FactorGraphError::DomainTooLarge {
                size: domain_size,
                maximum: MAX_FACTOR_DOMAIN_SIZE,
            });
        }
        Ok(self.insert_variable(name, domain_size))
    }

    fn insert_variable(&mut self, name: &str, domain_size: usize) -> VariableId {
        let id = VariableId(Uuid::new_v4());
        let var = Variable {
            id,
            name: name.to_string(),
            domain_size,
            observed: None,
        };
        self.variables.insert(id, var);
        self.var_to_factors.insert(id, Vec::new());
        id
    }

    /// Observe a variable (set its value).
    pub fn observe(&mut self, var: VariableId, value: usize) -> Result<(), FactorGraphError> {
        let variable = self
            .variables
            .get_mut(&var)
            .ok_or(FactorGraphError::UnknownVariable(var))?;
        if value >= variable.domain_size {
            return Err(FactorGraphError::ObservationOutOfRange {
                value,
                domain_size: variable.domain_size,
            });
        }
        variable.observed = Some(value);
        Ok(())
    }

    /// Add a unary factor (prior).
    pub fn add_prior(
        &mut self,
        var: VariableId,
        probabilities: Vec<f64>,
    ) -> Result<Uuid, FactorGraphError> {
        let variable = self
            .variables
            .get(&var)
            .ok_or(FactorGraphError::UnknownVariable(var))?;
        if probabilities.len() != variable.domain_size {
            return Err(FactorGraphError::InvalidPotential {
                reason: format!(
                    "unary potential has {} values for domain size {}",
                    probabilities.len(),
                    variable.domain_size
                ),
            });
        }
        validate_potential_values(probabilities.iter().copied())?;
        let factor = Factor {
            id: Uuid::new_v4(),
            variables: vec![var],
            potential: FactorPotential::Unary(probabilities),
        };
        Ok(self.add_factor(factor))
    }

    /// Add a binary factor (pairwise relationship).
    pub fn add_pairwise(
        &mut self,
        var1: VariableId,
        var2: VariableId,
        potential: Vec<Vec<f64>>,
    ) -> Result<Uuid, FactorGraphError> {
        let domain1 = self
            .variables
            .get(&var1)
            .ok_or(FactorGraphError::UnknownVariable(var1))?
            .domain_size;
        let domain2 = self
            .variables
            .get(&var2)
            .ok_or(FactorGraphError::UnknownVariable(var2))?
            .domain_size;
        if var1 == var2 {
            return Err(FactorGraphError::InvalidPotential {
                reason: "binary factor variables must be distinct".to_string(),
            });
        }
        if potential.len() != domain1 || potential.iter().any(|row| row.len() != domain2) {
            return Err(FactorGraphError::InvalidPotential {
                reason: format!("binary potential must have shape {domain1}x{domain2}"),
            });
        }
        validate_potential_values(potential.iter().flatten().copied())?;
        let factor = Factor {
            id: Uuid::new_v4(),
            variables: vec![var1, var2],
            potential: FactorPotential::Binary(potential),
        };
        Ok(self.add_factor(factor))
    }

    fn add_factor(&mut self, factor: Factor) -> Uuid {
        let id = factor.id;
        for &var in &factor.variables {
            self.var_to_factors.entry(var).or_default().push(id);
        }
        self.factor_to_vars.insert(id, factor.variables.clone());
        self.factors.insert(id, factor);
        id
    }

    /// Get variable by ID
    pub fn get_variable(&self, id: VariableId) -> Option<&Variable> {
        self.variables.get(&id)
    }

    /// Get all variables
    pub fn variables(&self) -> impl Iterator<Item = &Variable> {
        self.variables.values()
    }

    /// Get all factors
    pub fn factors(&self) -> impl Iterator<Item = &Factor> {
        self.factors.values()
    }

    /// Get factors for a variable
    pub fn factors_for_variable(&self, var: VariableId) -> &[Uuid] {
        self.var_to_factors
            .get(&var)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get variables for a factor
    pub fn variables_for_factor(&self, factor: Uuid) -> &[VariableId] {
        self.factor_to_vars
            .get(&factor)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
}

fn validate_potential_values(
    values: impl IntoIterator<Item = f64>,
) -> Result<(), FactorGraphError> {
    let mut has_positive = false;
    for value in values {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(FactorGraphError::InvalidPotential {
                reason: "potential values must be finite and in [0, 1]".to_string(),
            });
        }
        has_positive |= value > 0.0;
    }
    if !has_positive {
        return Err(FactorGraphError::InvalidPotential {
            reason: "a potential must contain at least one positive value".to_string(),
        });
    }
    Ok(())
}

// ============================================================================
// Belief Propagation
// ============================================================================

/// Message from variable to factor or factor to variable
#[derive(Debug, Clone)]
pub struct Message {
    pub values: Vec<f64>,
}

impl Message {
    fn uniform(size: usize) -> Result<Self, FactorGraphError> {
        if size == 0 {
            return Err(FactorGraphError::EmptyDomain);
        }
        Ok(Self {
            values: vec![1.0 / size as f64; size],
        })
    }

    pub fn normalize(&mut self) {
        let sum: f64 = self.values.iter().sum();
        if sum > 0.0 {
            for v in &mut self.values {
                *v /= sum;
            }
        }
    }

    fn multiply(&self, other: &Message) -> Result<Message, FactorGraphError> {
        if self.values.len() != other.values.len() {
            return Err(FactorGraphError::InconsistentGraph {
                reason: format!(
                    "cannot multiply messages with domains {} and {}",
                    self.values.len(),
                    other.values.len()
                ),
            });
        }
        let values = self
            .values
            .iter()
            .zip(&other.values)
            .map(|(left, right)| left * right)
            .collect();
        Ok(Message { values })
    }
}

/// Loopy Belief Propagation
pub struct BeliefPropagation {
    graph: FactorGraph,
    /// Messages from variable to factor
    var_to_factor: HashMap<(VariableId, Uuid), Message>,
    /// Messages from factor to variable
    factor_to_var: HashMap<(Uuid, VariableId), Message>,
    /// Marginal beliefs
    beliefs: HashMap<VariableId, Vec<f64>>,
    /// Convergence threshold
    epsilon: f64,
    /// Maximum iterations
    max_iter: usize,
}

impl BeliefPropagation {
    pub fn new(graph: FactorGraph) -> Self {
        Self {
            graph,
            var_to_factor: HashMap::new(),
            factor_to_var: HashMap::new(),
            beliefs: HashMap::new(),
            epsilon: 1e-6,
            max_iter: 100,
        }
    }

    /// Initialize messages
    fn initialize(&mut self) -> Result<(), FactorGraphError> {
        self.var_to_factor.clear();
        self.factor_to_var.clear();
        self.beliefs.clear();

        for var in self.graph.variables() {
            let msg = Message::uniform(var.domain_size)?;
            for &factor_id in self.graph.factors_for_variable(var.id) {
                self.var_to_factor.insert((var.id, factor_id), msg.clone());
            }
        }

        for factor in self.graph.factors() {
            for &var_id in &factor.variables {
                let var = self
                    .graph
                    .get_variable(var_id)
                    .ok_or(FactorGraphError::UnknownVariable(var_id))?;
                let msg = Message::uniform(var.domain_size)?;
                self.factor_to_var.insert((factor.id, var_id), msg);
            }
        }
        Ok(())
    }

    /// Run belief propagation.
    ///
    /// `Ok(true)` means iteration converged; `Ok(false)` means it reached the
    /// configured finite iteration bound. Structural inconsistencies fail
    /// closed instead of producing partial beliefs.
    pub fn run(&mut self) -> Result<bool, FactorGraphError> {
        self.initialize()?;

        for _iteration in 0..self.max_iter {
            let max_diff = self.iterate()?;
            if max_diff < self.epsilon {
                self.compute_beliefs()?;
                return Ok(true);
            }
        }

        self.compute_beliefs()?;
        Ok(false)
    }

    /// One iteration of message passing
    fn iterate(&mut self) -> Result<f64, FactorGraphError> {
        let mut max_diff = 0.0f64;

        // Variable to factor messages
        for var in self.graph.variables() {
            for &factor_id in self.graph.factors_for_variable(var.id) {
                let new_msg = self.compute_var_to_factor_message(var.id, factor_id)?;
                let old_msg = self
                    .var_to_factor
                    .get(&(var.id, factor_id))
                    .ok_or_else(|| FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "missing variable-to-factor message for {:?} -> {factor_id}",
                            var.id
                        ),
                    })?;
                if new_msg.values.len() != old_msg.values.len() {
                    return Err(FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "variable-to-factor message domain changed from {} to {} for {:?} -> {factor_id}",
                            old_msg.values.len(),
                            new_msg.values.len(),
                            var.id
                        ),
                    });
                }

                let diff: f64 = new_msg
                    .values
                    .iter()
                    .zip(old_msg.values.iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum();
                max_diff = max_diff.max(diff);

                self.var_to_factor.insert((var.id, factor_id), new_msg);
            }
        }

        // Factor to variable messages
        for factor in self.graph.factors() {
            for &var_id in &factor.variables {
                let new_msg = self.compute_factor_to_var_message(factor.id, var_id)?;
                let old_msg = self
                    .factor_to_var
                    .get(&(factor.id, var_id))
                    .ok_or_else(|| FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "missing factor-to-variable message for {} -> {var_id:?}",
                            factor.id
                        ),
                    })?;
                if new_msg.values.len() != old_msg.values.len() {
                    return Err(FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "factor-to-variable message domain changed from {} to {} for {} -> {var_id:?}",
                            old_msg.values.len(),
                            new_msg.values.len(),
                            factor.id
                        ),
                    });
                }

                let diff: f64 = new_msg
                    .values
                    .iter()
                    .zip(old_msg.values.iter())
                    .map(|(a, b)| (a - b).abs())
                    .sum();
                max_diff = max_diff.max(diff);

                self.factor_to_var.insert((factor.id, var_id), new_msg);
            }
        }

        Ok(max_diff)
    }

    /// Compute message from variable to factor
    fn compute_var_to_factor_message(
        &self,
        var: VariableId,
        factor: Uuid,
    ) -> Result<Message, FactorGraphError> {
        let v = self
            .graph
            .get_variable(var)
            .ok_or(FactorGraphError::UnknownVariable(var))?;

        // If observed, send delta message
        if let Some(obs) = v.observed {
            let mut values = vec![0.0; v.domain_size];
            let domain_size = values.len();
            let slot = values
                .get_mut(obs)
                .ok_or(FactorGraphError::ObservationOutOfRange {
                    value: obs,
                    domain_size,
                })?;
            *slot = 1.0;
            return Ok(Message { values });
        }

        // Product of incoming messages from other factors
        let mut msg = Message::uniform(v.domain_size)?;
        for &other_factor in self.graph.factors_for_variable(var) {
            if other_factor != factor {
                let incoming = self
                    .factor_to_var
                    .get(&(other_factor, var))
                    .ok_or_else(|| FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "missing factor-to-variable message for {other_factor} -> {var:?}"
                        ),
                    })?;
                msg = msg.multiply(incoming)?;
            }
        }
        msg.normalize();
        Ok(msg)
    }

    /// Compute message from factor to variable
    fn compute_factor_to_var_message(
        &self,
        factor_id: Uuid,
        target_var: VariableId,
    ) -> Result<Message, FactorGraphError> {
        let factor = self.graph.factors.get(&factor_id).ok_or_else(|| {
            FactorGraphError::InconsistentGraph {
                reason: format!("missing factor {factor_id}"),
            }
        })?;
        if !factor.variables.contains(&target_var) {
            return Err(FactorGraphError::InconsistentGraph {
                reason: format!("factor {factor_id} does not contain target {target_var:?}"),
            });
        }
        let target = self
            .graph
            .get_variable(target_var)
            .ok_or(FactorGraphError::UnknownVariable(target_var))?;

        match &factor.potential {
            FactorPotential::Unary(probs) => {
                if probs.len() != target.domain_size {
                    return Err(FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "unary factor {factor_id} has {} values for target domain {}",
                            probs.len(),
                            target.domain_size
                        ),
                    });
                }
                Ok(Message {
                    values: probs.clone(),
                })
            }
            FactorPotential::Binary(matrix) => {
                let other_var = factor
                    .variables
                    .iter()
                    .copied()
                    .find(|&variable| variable != target_var)
                    .ok_or_else(|| FactorGraphError::InconsistentGraph {
                        reason: format!("binary factor {factor_id} has no distinct peer variable"),
                    })?;
                let other_msg =
                    self.var_to_factor
                        .get(&(other_var, factor_id))
                        .ok_or_else(|| FactorGraphError::InconsistentGraph {
                            reason: format!(
                            "missing variable-to-factor message for {other_var:?} -> {factor_id}"
                        ),
                        })?;

                let is_first = factor.variables.first().copied() == Some(target_var);
                let mut values = vec![0.0; target.domain_size];

                if is_first {
                    // Sum over rows
                    for (i, value) in values.iter_mut().enumerate() {
                        let row =
                            matrix
                                .get(i)
                                .ok_or_else(|| FactorGraphError::InconsistentGraph {
                                    reason: format!(
                                        "binary factor {factor_id} is missing matrix row {i}"
                                    ),
                                })?;
                        if row.len() != other_msg.values.len() {
                            return Err(FactorGraphError::InconsistentGraph {
                                reason: format!(
                                    "binary factor {factor_id} row {i} has width {}, expected {}",
                                    row.len(),
                                    other_msg.values.len()
                                ),
                            });
                        }
                        for (&potential, &probability) in row.iter().zip(&other_msg.values) {
                            *value += potential * probability;
                        }
                    }
                } else {
                    // Sum over columns
                    if matrix.len() != other_msg.values.len() {
                        return Err(FactorGraphError::InconsistentGraph {
                            reason: format!(
                                "binary factor {factor_id} has {} rows, expected {}",
                                matrix.len(),
                                other_msg.values.len()
                            ),
                        });
                    }
                    for (column, value) in values.iter_mut().enumerate() {
                        for (row, &probability) in matrix.iter().zip(&other_msg.values) {
                            let potential = row.get(column).ok_or_else(|| {
                                FactorGraphError::InconsistentGraph {
                                    reason: format!(
                                        "binary factor {factor_id} is missing matrix column {column}"
                                    ),
                                }
                            })?;
                            *value += potential * probability;
                        }
                    }
                }

                let mut msg = Message { values };
                msg.normalize();
                Ok(msg)
            }
        }
    }

    /// Compute final beliefs
    fn compute_beliefs(&mut self) -> Result<(), FactorGraphError> {
        for var in self.graph.variables() {
            if let Some(obs) = var.observed {
                let mut belief = vec![0.0; var.domain_size];
                let domain_size = belief.len();
                let slot = belief
                    .get_mut(obs)
                    .ok_or(FactorGraphError::ObservationOutOfRange {
                        value: obs,
                        domain_size,
                    })?;
                *slot = 1.0;
                self.beliefs.insert(var.id, belief);
                continue;
            }

            let mut belief = vec![1.0; var.domain_size];
            for &factor_id in self.graph.factors_for_variable(var.id) {
                let msg = self
                    .factor_to_var
                    .get(&(factor_id, var.id))
                    .ok_or_else(|| FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "missing factor-to-variable message for {factor_id} -> {:?}",
                            var.id
                        ),
                    })?;
                if belief.len() != msg.values.len() {
                    return Err(FactorGraphError::InconsistentGraph {
                        reason: format!(
                            "factor {factor_id} message has domain {}, expected {} for {:?}",
                            msg.values.len(),
                            belief.len(),
                            var.id
                        ),
                    });
                }
                for (value, &message) in belief.iter_mut().zip(&msg.values) {
                    *value *= message;
                }
            }

            // Normalize
            let sum: f64 = belief.iter().sum();
            if sum > 0.0 {
                for v in &mut belief {
                    *v /= sum;
                }
            }

            self.beliefs.insert(var.id, belief);
        }
        Ok(())
    }

    /// Get belief for a variable
    pub fn belief(&self, var: VariableId) -> Option<&[f64]> {
        self.beliefs.get(&var).map(|v| v.as_slice())
    }

    /// Get probability of variable being true (for binary variables)
    pub fn prob_true(&self, var: VariableId) -> Option<f64> {
        self.beliefs.get(&var).and_then(|b| b.get(1).copied())
    }
}

// ============================================================================
// Calibration
// ============================================================================

/// Calibrate confidence scores using Platt scaling
pub struct PlattCalibrator {
    a: f64,
    b: f64,
    fitted: bool,
}

impl Default for PlattCalibrator {
    fn default() -> Self {
        Self::new()
    }
}

impl PlattCalibrator {
    pub fn new() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            fitted: false,
        }
    }

    /// Fit calibrator on validation data
    /// scores: raw confidence scores
    /// labels: true labels (0 or 1)
    pub fn fit(&mut self, scores: &[f64], labels: &[bool]) {
        if scores.len() != labels.len() || scores.is_empty() {
            return;
        }

        // Simple gradient descent for Platt scaling
        // P(y=1|s) = 1 / (1 + exp(a*s + b))
        let mut a = 0.0;
        let mut b = 0.0;
        let lr = 0.01;
        let epochs = 100;

        for _ in 0..epochs {
            let mut grad_a = 0.0;
            let mut grad_b = 0.0;

            for (&s, &y) in scores.iter().zip(labels.iter()) {
                let p = 1.0 / (1.0 + (-a * s - b).exp());
                let t = if y { 1.0 } else { 0.0 };
                let err = p - t;
                grad_a += err * s;
                grad_b += err;
            }

            a -= lr * grad_a / scores.len() as f64;
            b -= lr * grad_b / scores.len() as f64;
        }

        self.a = a;
        self.b = b;
        self.fitted = true;
    }

    /// Calibrate a raw score
    pub fn calibrate(&self, score: f64) -> f64 {
        if !self.fitted {
            return score;
        }
        1.0 / (1.0 + (-self.a * score - self.b).exp())
    }
}

// ============================================================================
// Knowledge Graph Integration
// ============================================================================

/// Build factor graph from knowledge graph facts
pub fn build_factor_graph_for_reconciliation(
    facts: &[(Uuid, f64, Vec<Uuid>)], // (fact_id, prior, supporting_fact_ids)
) -> Result<FactorGraph, FactorGraphError> {
    let mut graph = FactorGraph::new();
    let mut fact_vars: HashMap<Uuid, VariableId> = HashMap::new();

    // Create variable for each fact.
    for &(fact_id, prior, _) in facts {
        if !(0.0..=1.0).contains(&prior) {
            return Err(FactorGraphError::InvalidPotential {
                reason: format!("fact {fact_id} prior must be finite and in [0, 1]"),
            });
        }
        let var = graph.add_variable(&fact_id.to_string());
        if fact_vars.insert(fact_id, var).is_some() {
            return Err(FactorGraphError::DuplicateFact(fact_id));
        }
        graph.add_prior(var, vec![1.0 - prior, prior])?;
    }

    // Add pairwise factors for support relationships.
    for &(fact_id, _, ref supports) in facts {
        let var1 = *fact_vars
            .get(&fact_id)
            .ok_or(FactorGraphError::MissingFact(fact_id))?;
        for &support_id in supports {
            if let Some(&var2) = fact_vars.get(&support_id) {
                // If A supports B, P(B|A) > P(B).
                graph.add_pairwise(
                    var2,
                    var1,
                    vec![
                        vec![1.0, 0.5], // A=false: B less likely
                        vec![0.5, 1.0], // A=true: B more likely
                    ],
                )?;
            }
        }
    }

    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factor_graph_construction() -> Result<(), FactorGraphError> {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable("Y");

        graph.add_prior(x, vec![0.3, 0.7])?;
        graph.add_pairwise(x, y, vec![vec![0.9, 0.1], vec![0.2, 0.8]])?;

        assert_eq!(graph.variables().count(), 2);
        assert_eq!(graph.factors().count(), 2);
        Ok(())
    }

    #[test]
    fn test_belief_propagation() -> Result<(), FactorGraphError> {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable("Y");

        graph.add_prior(x, vec![0.3, 0.7])?;
        graph.add_pairwise(x, y, vec![vec![0.9, 0.1], vec![0.2, 0.8]])?;

        let mut bp = BeliefPropagation::new(graph);
        let converged = bp.run()?;
        assert!(converged);

        let x_belief = bp
            .belief(x)
            .ok_or_else(|| FactorGraphError::InconsistentGraph {
                reason: "missing X belief after convergence".to_string(),
            })?;
        let y_belief = bp
            .belief(y)
            .ok_or_else(|| FactorGraphError::InconsistentGraph {
                reason: "missing Y belief after convergence".to_string(),
            })?;

        assert!((x_belief[0] + x_belief[1] - 1.0).abs() < 1e-6);
        assert!((y_belief[0] + y_belief[1] - 1.0).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_observation() -> Result<(), FactorGraphError> {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable("Y");

        graph.add_prior(x, vec![0.5, 0.5])?;
        graph.add_pairwise(x, y, vec![vec![0.9, 0.1], vec![0.1, 0.9]])?;

        graph.observe(x, 1)?; // Observe X = true

        let mut bp = BeliefPropagation::new(graph);
        bp.run()?;

        // Y should be likely true given X = true
        let y_prob = bp
            .prob_true(y)
            .ok_or_else(|| FactorGraphError::InconsistentGraph {
                reason: "missing Y probability after observed inference".to_string(),
            })?;
        assert!(y_prob > 0.8);
        Ok(())
    }

    #[test]
    fn invalid_graph_shapes_are_rejected_before_inference() -> Result<(), FactorGraphError> {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable_with_domain("Y", 3)?;
        let unknown = VariableId(Uuid::new_v4());

        assert_eq!(
            graph.add_variable_with_domain("empty", 0),
            Err(FactorGraphError::EmptyDomain)
        );
        assert!(matches!(
            graph.add_variable_with_domain("huge", MAX_FACTOR_DOMAIN_SIZE + 1),
            Err(FactorGraphError::DomainTooLarge { .. })
        ));
        assert!(matches!(
            graph.observe(unknown, 0),
            Err(FactorGraphError::UnknownVariable(id)) if id == unknown
        ));
        assert!(matches!(
            graph.observe(y, 3),
            Err(FactorGraphError::ObservationOutOfRange { .. })
        ));
        assert!(graph.add_prior(x, vec![f64::NAN, 1.0]).is_err());
        assert!(graph.add_prior(y, vec![0.5, 0.5]).is_err());
        assert!(graph
            .add_pairwise(x, x, vec![vec![1.0, 0.0], vec![0.0, 1.0]])
            .is_err());
        assert!(graph
            .add_pairwise(x, y, vec![vec![1.0, 0.0], vec![0.0, 1.0]])
            .is_err());
        Ok(())
    }

    #[test]
    fn inference_rejects_inconsistent_internal_graph() -> Result<(), FactorGraphError> {
        let mut graph = FactorGraph::new();
        let variable = graph.add_variable("X");
        graph.add_prior(variable, vec![0.5, 0.5])?;
        graph.variables.remove(&variable);

        let mut propagation = BeliefPropagation::new(graph);
        assert!(matches!(
            propagation.run(),
            Err(FactorGraphError::UnknownVariable(id)) if id == variable
        ));
        assert!(propagation.belief(variable).is_none());
        Ok(())
    }

    #[test]
    fn reconciliation_graph_rejects_invalid_and_duplicate_facts() {
        let fact = Uuid::new_v4();
        assert!(build_factor_graph_for_reconciliation(&[(fact, f64::NAN, vec![])]).is_err());
        assert!(matches!(
            build_factor_graph_for_reconciliation(&[
                (fact, 0.5, vec![]),
                (fact, 0.7, vec![])
            ]),
            Err(FactorGraphError::DuplicateFact(id)) if id == fact
        ));
        assert!(build_factor_graph_for_reconciliation(&[(fact, 0.5, vec![fact])]).is_err());
    }

    #[test]
    fn test_calibration() {
        let mut calibrator = PlattCalibrator::new();

        // Training data: over-confident model
        let scores = vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1];
        let labels = vec![true, true, false, true, false, false, true, false, false];

        calibrator.fit(&scores, &labels);

        // Calibrated scores should be different from raw
        let raw = 0.8;
        let calibrated = calibrator.calibrate(raw);
        assert!(calibrated != raw);
    }
}
