//! Proper Probabilistic Reasoning with Factor Graphs
//!
//! Fixes naive probability handling by:
//! 1. Using factor graphs to model dependencies
//! 2. Belief propagation for inference
//! 3. Proper handling of correlated evidence
//! 4. Calibrated confidence scores

#![allow(unused_imports, unused_variables)]

use std::collections::{HashMap, HashSet};
use uuid::Uuid;

// ============================================================================
// Factor Graph Core
// ============================================================================

/// A variable in the factor graph
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariableId(pub Uuid);

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
    /// General factor: joint distribution over all states
    General(FactorTable),
}

/// General factor table for n-ary factors
#[derive(Debug, Clone)]
pub struct FactorTable {
    pub dimensions: Vec<usize>,
    pub values: Vec<f64>,
}

impl FactorTable {
    pub fn new(dimensions: Vec<usize>) -> Self {
        let size: usize = dimensions.iter().product();
        Self {
            dimensions,
            values: vec![1.0; size],
        }
    }

    pub fn get(&self, indices: &[usize]) -> f64 {
        let idx = self.flat_index(indices);
        self.values[idx]
    }

    pub fn set(&mut self, indices: &[usize], value: f64) {
        let idx = self.flat_index(indices);
        self.values[idx] = value;
    }

    fn flat_index(&self, indices: &[usize]) -> usize {
        let mut idx = 0;
        let mut stride = 1;
        for (i, &dim) in self.dimensions.iter().enumerate().rev() {
            idx += indices[i] * stride;
            stride *= dim;
        }
        idx
    }
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

impl FactorGraph {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            factors: HashMap::new(),
            var_to_factors: HashMap::new(),
            factor_to_vars: HashMap::new(),
        }
    }

    /// Add a binary variable
    pub fn add_variable(&mut self, name: &str) -> VariableId {
        self.add_variable_with_domain(name, 2)
    }

    /// Add a variable with arbitrary domain
    pub fn add_variable_with_domain(&mut self, name: &str, domain_size: usize) -> VariableId {
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

    /// Observe a variable (set its value)
    pub fn observe(&mut self, var: VariableId, value: usize) {
        if let Some(v) = self.variables.get_mut(&var) {
            v.observed = Some(value);
        }
    }

    /// Add a unary factor (prior)
    pub fn add_prior(&mut self, var: VariableId, probabilities: Vec<f64>) -> Uuid {
        let factor = Factor {
            id: Uuid::new_v4(),
            variables: vec![var],
            potential: FactorPotential::Unary(probabilities),
        };
        self.add_factor(factor)
    }

    /// Add a binary factor (pairwise relationship)
    pub fn add_pairwise(
        &mut self,
        var1: VariableId,
        var2: VariableId,
        potential: Vec<Vec<f64>>,
    ) -> Uuid {
        let factor = Factor {
            id: Uuid::new_v4(),
            variables: vec![var1, var2],
            potential: FactorPotential::Binary(potential),
        };
        self.add_factor(factor)
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

// ============================================================================
// Belief Propagation
// ============================================================================

/// Message from variable to factor or factor to variable
#[derive(Debug, Clone)]
pub struct Message {
    pub values: Vec<f64>,
}

impl Message {
    pub fn uniform(size: usize) -> Self {
        Self {
            values: vec![1.0 / size as f64; size],
        }
    }

    pub fn normalize(&mut self) {
        let sum: f64 = self.values.iter().sum();
        if sum > 0.0 {
            for v in &mut self.values {
                *v /= sum;
            }
        }
    }

    pub fn multiply(&self, other: &Message) -> Message {
        let values: Vec<f64> = self
            .values
            .iter()
            .zip(other.values.iter())
            .map(|(a, b)| a * b)
            .collect();
        Message { values }
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
    fn initialize(&mut self) {
        for var in self.graph.variables() {
            let msg = Message::uniform(var.domain_size);
            for &factor_id in self.graph.factors_for_variable(var.id) {
                self.var_to_factor.insert((var.id, factor_id), msg.clone());
            }
        }

        for factor in self.graph.factors() {
            for &var_id in &factor.variables {
                let var = self.graph.get_variable(var_id).unwrap();
                let msg = Message::uniform(var.domain_size);
                self.factor_to_var.insert((factor.id, var_id), msg);
            }
        }
    }

    /// Run belief propagation
    pub fn run(&mut self) -> bool {
        self.initialize();

        for iter in 0..self.max_iter {
            let max_diff = self.iterate();
            if max_diff < self.epsilon {
                self.compute_beliefs();
                return true;
            }
        }

        self.compute_beliefs();
        false // Did not converge
    }

    /// One iteration of message passing
    fn iterate(&mut self) -> f64 {
        let mut max_diff = 0.0f64;

        // Variable to factor messages
        for var in self.graph.variables() {
            for &factor_id in self.graph.factors_for_variable(var.id) {
                let new_msg = self.compute_var_to_factor_message(var.id, factor_id);
                let old_msg = self.var_to_factor.get(&(var.id, factor_id)).unwrap();

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
                let new_msg = self.compute_factor_to_var_message(factor.id, var_id);
                let old_msg = self.factor_to_var.get(&(factor.id, var_id)).unwrap();

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

        max_diff
    }

    /// Compute message from variable to factor
    fn compute_var_to_factor_message(&self, var: VariableId, factor: Uuid) -> Message {
        let v = self.graph.get_variable(var).unwrap();

        // If observed, send delta message
        if let Some(obs) = v.observed {
            let mut values = vec![0.0; v.domain_size];
            values[obs] = 1.0;
            return Message { values };
        }

        // Product of incoming messages from other factors
        let mut msg = Message::uniform(v.domain_size);
        for &other_factor in self.graph.factors_for_variable(var) {
            if other_factor != factor {
                if let Some(incoming) = self.factor_to_var.get(&(other_factor, var)) {
                    msg = msg.multiply(incoming);
                }
            }
        }
        msg.normalize();
        msg
    }

    /// Compute message from factor to variable
    fn compute_factor_to_var_message(&self, factor_id: Uuid, target_var: VariableId) -> Message {
        let factor = self.graph.factors.get(&factor_id).unwrap();
        let target = self.graph.get_variable(target_var).unwrap();

        match &factor.potential {
            FactorPotential::Unary(probs) => Message {
                values: probs.clone(),
            },
            FactorPotential::Binary(matrix) => {
                let other_var = factor.variables.iter().find(|&&v| v != target_var).unwrap();
                let other_msg = self.var_to_factor.get(&(*other_var, factor_id)).unwrap();

                let is_first = factor.variables[0] == target_var;
                let mut values = vec![0.0; target.domain_size];

                if is_first {
                    // Sum over rows
                    for i in 0..values.len() {
                        for (j, &prob) in other_msg.values.iter().enumerate() {
                            values[i] += matrix[i][j] * prob;
                        }
                    }
                } else {
                    // Sum over columns
                    for j in 0..values.len() {
                        for (i, &prob) in other_msg.values.iter().enumerate() {
                            values[j] += matrix[i][j] * prob;
                        }
                    }
                }

                let mut msg = Message { values };
                msg.normalize();
                msg
            }
            FactorPotential::General(table) => {
                // Marginalize over all variables except target
                // This is simplified - full implementation would be more complex
                Message::uniform(target.domain_size)
            }
        }
    }

    /// Compute final beliefs
    fn compute_beliefs(&mut self) {
        for var in self.graph.variables() {
            let v = self.graph.get_variable(var.id).unwrap();

            if let Some(obs) = v.observed {
                let mut belief = vec![0.0; v.domain_size];
                belief[obs] = 1.0;
                self.beliefs.insert(var.id, belief);
                continue;
            }

            let mut belief = vec![1.0; v.domain_size];
            for &factor_id in self.graph.factors_for_variable(var.id) {
                if let Some(msg) = self.factor_to_var.get(&(factor_id, var.id)) {
                    for (i, &v) in msg.values.iter().enumerate() {
                        belief[i] *= v;
                    }
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
) -> FactorGraph {
    let mut graph = FactorGraph::new();
    let mut fact_vars: HashMap<Uuid, VariableId> = HashMap::new();

    // Create variable for each fact
    for &(fact_id, prior, _) in facts {
        let var = graph.add_variable(&fact_id.to_string());
        fact_vars.insert(fact_id, var);

        // Add prior factor
        graph.add_prior(var, vec![1.0 - prior, prior]);
    }

    // Add pairwise factors for support relationships
    for &(fact_id, _, ref supports) in facts {
        let var1 = fact_vars[&fact_id];
        for &support_id in supports {
            if let Some(&var2) = fact_vars.get(&support_id) {
                // If A supports B, P(B|A) > P(B)
                graph.add_pairwise(
                    var2,
                    var1,
                    vec![
                        vec![1.0, 0.5], // A=false: B less likely
                        vec![0.5, 1.0], // A=true: B more likely
                    ],
                );
            }
        }
    }

    graph
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factor_graph_construction() {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable("Y");

        graph.add_prior(x, vec![0.3, 0.7]);
        graph.add_pairwise(x, y, vec![vec![0.9, 0.1], vec![0.2, 0.8]]);

        assert_eq!(graph.variables().count(), 2);
        assert_eq!(graph.factors().count(), 2);
    }

    #[test]
    fn test_belief_propagation() {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable("Y");

        graph.add_prior(x, vec![0.3, 0.7]);
        graph.add_pairwise(x, y, vec![vec![0.9, 0.1], vec![0.2, 0.8]]);

        let mut bp = BeliefPropagation::new(graph);
        let converged = bp.run();

        let x_belief = bp.belief(x).unwrap();
        let y_belief = bp.belief(y).unwrap();

        assert!((x_belief[0] + x_belief[1] - 1.0).abs() < 1e-6);
        assert!((y_belief[0] + y_belief[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_observation() {
        let mut graph = FactorGraph::new();
        let x = graph.add_variable("X");
        let y = graph.add_variable("Y");

        graph.add_prior(x, vec![0.5, 0.5]);
        graph.add_pairwise(x, y, vec![vec![0.9, 0.1], vec![0.1, 0.9]]);

        graph.observe(x, 1); // Observe X = true

        let mut bp = BeliefPropagation::new(graph);
        bp.run();

        // Y should be likely true given X = true
        let y_prob = bp.prob_true(y).unwrap();
        assert!(y_prob > 0.8);
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
