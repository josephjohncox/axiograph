//! Guardrails and Learning Support for PathDB
//!
//! This module provides safety guardrails and educational support for users
//! who are inexperienced in a domain. Key features:
//!
//! - **Constraint Validation**: Check actions against domain knowledge
//! - **Safety Checks**: Identify potentially dangerous operations
//! - **Learning Hints**: Suggest related knowledge when gaps detected
//! - **Explanation Generation**: Provide reasoning for recommendations
//! - **Progressive Disclosure**: Show information at appropriate levels
//!
//! Designed to make domain expertise accessible and prevent costly mistakes.

#![allow(unused_imports, unused_variables, dead_code)]

use crate::verified::{ReachabilityProof, VerifiedProb};
use crate::{PathDB, PathQuery, PathSig, StrId};
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ============================================================================
// Guardrail Types
// ============================================================================

/// Severity level for guardrail violations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Informational: good to know
    Info,
    /// Advisory: should consider
    Advisory,
    /// Warning: likely problematic
    Warning,
    /// Critical: dangerous, should not proceed
    Critical,
    /// Blocking: system will prevent action
    Blocking,
}

/// A guardrail rule that checks conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailRule {
    /// Rule identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Description of what the rule checks
    pub description: String,
    /// Severity if violated
    pub severity: Severity,
    /// Domain this rule applies to (e.g., "machining", "safety", "economics")
    pub domain: String,
    /// Entity types this rule applies to
    pub applicable_types: Vec<String>,
    /// Condition: path pattern that indicates violation
    pub violation_pattern: Option<ViolationPattern>,
    /// Required relations that must exist
    pub required_relations: Vec<String>,
    /// Forbidden relations that must not exist
    pub forbidden_relations: Vec<String>,
    /// Confidence threshold for rule to apply
    pub min_confidence: f32,
}

/// Pattern for detecting violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationPattern {
    /// Relation path that indicates violation
    pub path: Vec<String>,
    /// Required properties at end of path
    pub target_type: Option<String>,
    /// Value constraints
    pub constraints: Vec<Constraint>,
}

/// A constraint on values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Constraint {
    /// Value must equal
    Equals(String, String),
    /// Value must be greater than
    GreaterThan(String, f64),
    /// Value must be less than
    LessThan(String, f64),
    /// Value must be in range
    InRange(String, f64, f64),
    /// Value must be one of
    OneOf(String, Vec<String>),
}

/// Result of checking a guardrail
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailViolation {
    /// Rule that was violated
    pub rule_id: String,
    /// Severity
    pub severity: Severity,
    /// Human-readable explanation
    pub explanation: String,
    /// Entities involved
    pub entities: Vec<u32>,
    /// Evidence paths
    pub evidence: Vec<Vec<String>>,
    /// Suggested actions
    pub suggestions: Vec<String>,
    /// Related knowledge to learn
    pub learning_resources: Vec<LearningResource>,
}

/// A learning resource reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningResource {
    /// Resource type
    pub resource_type: LearningResourceType,
    /// Title
    pub title: String,
    /// Description
    pub description: String,
    /// Path in knowledge graph
    pub kg_path: Option<Vec<String>>,
    /// External URL if applicable
    pub url: Option<String>,
    /// Confidence/relevance score
    pub relevance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearningResourceType {
    /// Concept definition in KG
    Concept,
    /// Example entity in KG
    Example,
    /// Related safety rule
    SafetyRule,
    /// External documentation
    Documentation,
    /// Tutorial or guide
    Tutorial,
    /// Expert recommendation
    ExpertAdvice,
}

// ============================================================================
// Guardrail Engine
// ============================================================================

/// Engine for checking guardrails against PathDB
pub struct GuardrailEngine {
    /// Active rules
    rules: Vec<GuardrailRule>,
    /// Domain-specific rule indices
    domain_rules: HashMap<String, Vec<usize>>,
    /// Type-specific rule indices
    type_rules: HashMap<String, Vec<usize>>,
    /// Learning content providers
    learning_providers: Vec<Box<dyn LearningProvider>>,
}

impl GuardrailEngine {
    /// Create new engine with rules
    pub fn new(rules: Vec<GuardrailRule>) -> Self {
        let mut domain_rules: HashMap<String, Vec<usize>> = HashMap::new();
        let mut type_rules: HashMap<String, Vec<usize>> = HashMap::new();

        for (i, rule) in rules.iter().enumerate() {
            domain_rules.entry(rule.domain.clone()).or_default().push(i);

            for ty in &rule.applicable_types {
                type_rules.entry(ty.clone()).or_default().push(i);
            }
        }

        Self {
            rules,
            domain_rules,
            type_rules,
            learning_providers: Vec::new(),
        }
    }

    /// Add a learning provider
    pub fn add_learning_provider(&mut self, provider: Box<dyn LearningProvider>) {
        self.learning_providers.push(provider);
    }

    /// Check all applicable rules for an entity
    pub fn check_entity(
        &self,
        db: &PathDB,
        entity_id: u32,
        entity_type: &str,
        context: &CheckContext,
    ) -> Vec<GuardrailViolation> {
        let mut violations = Vec::new();

        // Get applicable rules
        let mut applicable_rule_ids: HashSet<usize> = HashSet::new();

        if let Some(domain_rules) = self.domain_rules.get(&context.domain) {
            applicable_rule_ids.extend(domain_rules);
        }

        if let Some(type_rules) = self.type_rules.get(entity_type) {
            applicable_rule_ids.extend(type_rules);
        }

        // Check each applicable rule
        for &rule_idx in &applicable_rule_ids {
            let rule = &self.rules[rule_idx];

            if let Some(violation) = self.check_rule(db, entity_id, rule, context) {
                violations.push(violation);
            }
        }

        // Sort by severity (most severe first)
        violations.sort_by(|a, b| b.severity.cmp(&a.severity));
        violations
    }

    /// Check a specific rule
    fn check_rule(
        &self,
        db: &PathDB,
        entity_id: u32,
        rule: &GuardrailRule,
        context: &CheckContext,
    ) -> Option<GuardrailViolation> {
        // Check required relations
        for rel in &rule.required_relations {
            let targets = db.follow_one(entity_id, rel);
            if targets.is_empty() {
                return Some(GuardrailViolation {
                    rule_id: rule.id.clone(),
                    severity: rule.severity,
                    explanation: format!(
                        "Missing required relation '{}': {}",
                        rel, rule.description
                    ),
                    entities: vec![entity_id],
                    evidence: vec![],
                    suggestions: vec![format!(
                        "Add a '{}' relation to specify this information",
                        rel
                    )],
                    learning_resources: self.find_learning_resources(db, rel, context),
                });
            }
        }

        // Check forbidden relations
        for rel in &rule.forbidden_relations {
            let targets = db.follow_one(entity_id, rel);
            if !targets.is_empty() {
                return Some(GuardrailViolation {
                    rule_id: rule.id.clone(),
                    severity: rule.severity,
                    explanation: format!(
                        "Forbidden relation '{}' exists: {}",
                        rel, rule.description
                    ),
                    entities: std::iter::once(entity_id).chain(targets.iter()).collect(),
                    evidence: vec![vec![rel.clone()]],
                    suggestions: vec![format!(
                        "Remove the '{}' relation or use an alternative",
                        rel
                    )],
                    learning_resources: self.find_learning_resources(db, rel, context),
                });
            }
        }

        // Check violation patterns
        if let Some(pattern) = &rule.violation_pattern {
            let path_refs: Vec<&str> = pattern.path.iter().map(|s| s.as_str()).collect();
            let reached = db.follow_path(entity_id, &path_refs);

            if !reached.is_empty() {
                // Pattern matched - this is a violation
                return Some(GuardrailViolation {
                    rule_id: rule.id.clone(),
                    severity: rule.severity,
                    explanation: format!(
                        "Violation detected via path {:?}: {}",
                        pattern.path, rule.description
                    ),
                    entities: std::iter::once(entity_id).chain(reached.iter()).collect(),
                    evidence: vec![pattern.path.clone()],
                    suggestions: self.generate_suggestions(rule, &pattern.path),
                    learning_resources: self.find_learning_resources(
                        db,
                        pattern.path.first().map(|s| s.as_str()).unwrap_or(""),
                        context,
                    ),
                });
            }
        }

        None
    }

    /// Find relevant learning resources
    fn find_learning_resources(
        &self,
        db: &PathDB,
        topic: &str,
        context: &CheckContext,
    ) -> Vec<LearningResource> {
        let mut resources = Vec::new();

        for provider in &self.learning_providers {
            resources.extend(provider.find_resources(db, topic, context));
        }

        // Sort by relevance
        resources.sort_by(|a, b| {
            b.relevance
                .partial_cmp(&a.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Limit to top N
        resources.truncate(5);
        resources
    }

    /// Generate suggestions for a violation
    fn generate_suggestions(&self, rule: &GuardrailRule, path: &[String]) -> Vec<String> {
        let mut suggestions = Vec::new();

        suggestions.push(format!(
            "Review the '{}' rule in domain '{}'",
            rule.name, rule.domain
        ));

        if !path.is_empty() {
            suggestions.push(format!(
                "Check the relationship path: {}",
                path.join(" -> ")
            ));
        }

        suggestions.push("Consult with a domain expert before proceeding".to_string());

        suggestions
    }
}

/// Context for checking guardrails
#[derive(Debug, Clone, Default)]
pub struct CheckContext {
    /// Domain being operated in
    pub domain: String,
    /// User experience level (0.0 = novice, 1.0 = expert)
    pub experience_level: f32,
    /// Specific operation being performed
    pub operation: Option<String>,
    /// Additional context tags
    pub tags: Vec<String>,
}

/// Provider of learning resources
pub trait LearningProvider: Send + Sync {
    /// Find relevant learning resources for a topic
    fn find_resources(
        &self,
        db: &PathDB,
        topic: &str,
        context: &CheckContext,
    ) -> Vec<LearningResource>;
}

// ============================================================================
// Built-in Learning Providers
// ============================================================================

/// Learning provider that uses PathDB's knowledge graph
pub struct KGLearningProvider {
    /// Types that are considered "educational"
    educational_types: HashSet<String>,
    /// Relation that points to explanations
    explanation_rel: String,
    /// Relation that points to examples
    example_rel: String,
}

impl KGLearningProvider {
    pub fn new() -> Self {
        let mut educational_types = HashSet::new();
        educational_types.insert("Concept".to_string());
        educational_types.insert("Definition".to_string());
        educational_types.insert("Tutorial".to_string());
        educational_types.insert("Example".to_string());
        educational_types.insert("SafetyGuideline".to_string());
        educational_types.insert("BestPractice".to_string());

        Self {
            educational_types,
            explanation_rel: "hasExplanation".to_string(),
            example_rel: "hasExample".to_string(),
        }
    }
}

impl Default for KGLearningProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LearningProvider for KGLearningProvider {
    fn find_resources(
        &self,
        db: &PathDB,
        topic: &str,
        context: &CheckContext,
    ) -> Vec<LearningResource> {
        let mut resources = Vec::new();

        // Find concepts related to the topic
        if let Some(concepts) = db.find_by_type("Concept") {
            for concept_id in concepts.iter().take(10) {
                // Check if this concept is related to the topic
                // (simplified: in production, would use semantic search)
                let related = db.follow_one(concept_id, "relatedTo");

                resources.push(LearningResource {
                    resource_type: LearningResourceType::Concept,
                    title: format!("Concept #{}", concept_id),
                    description: format!("Related to '{}'", topic),
                    kg_path: Some(vec!["Concept".to_string(), concept_id.to_string()]),
                    url: None,
                    relevance: 0.7,
                });
            }
        }

        // Find safety guidelines
        if let Some(guidelines) = db.find_by_type("SafetyGuideline") {
            for guideline_id in guidelines.iter().take(5) {
                resources.push(LearningResource {
                    resource_type: LearningResourceType::SafetyRule,
                    title: format!("Safety Guideline #{}", guideline_id),
                    description: "Safety-related guidance".to_string(),
                    kg_path: Some(vec![
                        "SafetyGuideline".to_string(),
                        guideline_id.to_string(),
                    ]),
                    url: None,
                    relevance: 0.9, // Safety always high relevance
                });
            }
        }

        resources
    }
}

// ============================================================================
// Progressive Disclosure
// ============================================================================

/// Level of detail for information disclosure
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisclosureLevel {
    /// Just the essentials
    Minimal,
    /// Basic information with key context
    Basic,
    /// Standard detail level
    Standard,
    /// Full details with explanations
    Detailed,
    /// Expert-level with all technical details
    Expert,
}

impl DisclosureLevel {
    /// Determine appropriate level based on user experience
    pub fn from_experience(experience: f32) -> Self {
        if experience < 0.2 {
            DisclosureLevel::Minimal
        } else if experience < 0.4 {
            DisclosureLevel::Basic
        } else if experience < 0.6 {
            DisclosureLevel::Standard
        } else if experience < 0.8 {
            DisclosureLevel::Detailed
        } else {
            DisclosureLevel::Expert
        }
    }
}

/// Progressive disclosure formatter
pub struct ProgressiveDisclosure {
    level: DisclosureLevel,
}

impl ProgressiveDisclosure {
    pub fn new(level: DisclosureLevel) -> Self {
        Self { level }
    }

    pub fn from_experience(experience: f32) -> Self {
        Self::new(DisclosureLevel::from_experience(experience))
    }

    /// Format a guardrail violation for the user
    pub fn format_violation(&self, violation: &GuardrailViolation) -> String {
        match self.level {
            DisclosureLevel::Minimal => {
                format!(
                    "⚠️ {}: {}",
                    self.severity_emoji(violation.severity),
                    self.truncate(&violation.explanation, 50)
                )
            }
            DisclosureLevel::Basic => {
                let mut output = format!(
                    "{} {}\n{}\n",
                    self.severity_emoji(violation.severity),
                    violation.rule_id,
                    violation.explanation
                );
                if let Some(suggestion) = violation.suggestions.first() {
                    output.push_str(&format!("💡 {}\n", suggestion));
                }
                output
            }
            DisclosureLevel::Standard => {
                let mut output = format!(
                    "{} **{}** ({})\n\n{}\n\n",
                    self.severity_emoji(violation.severity),
                    violation.rule_id,
                    self.severity_name(violation.severity),
                    violation.explanation
                );

                if !violation.suggestions.is_empty() {
                    output.push_str("**Suggestions:**\n");
                    for (i, suggestion) in violation.suggestions.iter().enumerate() {
                        output.push_str(&format!("{}. {}\n", i + 1, suggestion));
                    }
                }

                if !violation.learning_resources.is_empty() {
                    output.push_str("\n**Learn more:**\n");
                    for resource in violation.learning_resources.iter().take(3) {
                        output
                            .push_str(&format!("- {}: {}\n", resource.title, resource.description));
                    }
                }

                output
            }
            DisclosureLevel::Detailed | DisclosureLevel::Expert => {
                let mut output = format!(
                    "# {} {} ({})\n\n## Description\n{}\n\n",
                    self.severity_emoji(violation.severity),
                    violation.rule_id,
                    self.severity_name(violation.severity),
                    violation.explanation
                );

                output.push_str(&format!(
                    "## Affected Entities\n{:?}\n\n",
                    violation.entities
                ));

                if !violation.evidence.is_empty() {
                    output.push_str("## Evidence Paths\n");
                    for path in &violation.evidence {
                        output.push_str(&format!("- `{}`\n", path.join(" → ")));
                    }
                    output.push('\n');
                }

                if !violation.suggestions.is_empty() {
                    output.push_str("## Recommendations\n");
                    for (i, suggestion) in violation.suggestions.iter().enumerate() {
                        output.push_str(&format!("{}. {}\n", i + 1, suggestion));
                    }
                    output.push('\n');
                }

                if !violation.learning_resources.is_empty() {
                    output.push_str("## Learning Resources\n");
                    for resource in &violation.learning_resources {
                        output.push_str(&format!(
                            "### {} (relevance: {:.0}%)\n{}\n",
                            resource.title,
                            resource.relevance * 100.0,
                            resource.description
                        ));
                        if let Some(url) = &resource.url {
                            output.push_str(&format!("URL: {}\n", url));
                        }
                        output.push('\n');
                    }
                }

                output
            }
        }
    }

    fn severity_emoji(&self, severity: Severity) -> &'static str {
        match severity {
            Severity::Info => "ℹ️",
            Severity::Advisory => "💭",
            Severity::Warning => "⚠️",
            Severity::Critical => "🚨",
            Severity::Blocking => "🛑",
        }
    }

    fn severity_name(&self, severity: Severity) -> &'static str {
        match severity {
            Severity::Info => "Info",
            Severity::Advisory => "Advisory",
            Severity::Warning => "Warning",
            Severity::Critical => "Critical",
            Severity::Blocking => "Blocking",
        }
    }

    fn truncate(&self, s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            s.to_string()
        } else {
            format!("{}...", &s[..max_len])
        }
    }
}

// ============================================================================
// Pre-built Domain Rules
// ============================================================================

/// Create machining safety rules
pub fn machining_safety_rules() -> Vec<GuardrailRule> {
    vec![
        GuardrailRule {
            id: "MACH-001".to_string(),
            name: "Missing material specification".to_string(),
            description: "Every machining operation must specify the workpiece material".to_string(),
            severity: Severity::Critical,
            domain: "machining".to_string(),
            applicable_types: vec!["MachiningOperation".to_string()],
            violation_pattern: None,
            required_relations: vec!["hasMaterial".to_string()],
            forbidden_relations: vec![],
            min_confidence: 0.8,
        },
        GuardrailRule {
            id: "MACH-002".to_string(),
            name: "Titanium with high speed".to_string(),
            description: "Titanium requires reduced cutting speeds to prevent tool wear and work hardening".to_string(),
            severity: Severity::Critical,
            domain: "machining".to_string(),
            applicable_types: vec!["MachiningOperation".to_string()],
            violation_pattern: Some(ViolationPattern {
                path: vec!["hasMaterial".to_string(), "isTitanium".to_string()],
                target_type: None,
                constraints: vec![],
            }),
            required_relations: vec![],
            forbidden_relations: vec!["hasHighSpeed".to_string()],
            min_confidence: 0.9,
        },
        GuardrailRule {
            id: "MACH-003".to_string(),
            name: "Missing coolant for deep holes".to_string(),
            description: "Deep hole drilling (>3xD) requires through-spindle coolant".to_string(),
            severity: Severity::Warning,
            domain: "machining".to_string(),
            applicable_types: vec!["DrillingOperation".to_string()],
            violation_pattern: Some(ViolationPattern {
                path: vec!["hasDepth".to_string()],
                target_type: Some("DeepHole".to_string()),
                constraints: vec![Constraint::GreaterThan("depthRatio".to_string(), 3.0)],
            }),
            required_relations: vec!["hasThroughCoolant".to_string()],
            forbidden_relations: vec![],
            min_confidence: 0.8,
        },
        GuardrailRule {
            id: "MACH-004".to_string(),
            name: "Chatter risk at thin walls".to_string(),
            description: "Thin-walled parts (<2mm) are prone to chatter; reduce depth of cut and increase feed".to_string(),
            severity: Severity::Advisory,
            domain: "machining".to_string(),
            applicable_types: vec!["MillingOperation".to_string()],
            violation_pattern: Some(ViolationPattern {
                path: vec!["hasFeature".to_string(), "isThinWall".to_string()],
                target_type: None,
                constraints: vec![Constraint::LessThan("wallThickness".to_string(), 2.0)],
            }),
            required_relations: vec![],
            forbidden_relations: vec![],
            min_confidence: 0.7,
        },
        GuardrailRule {
            id: "MACH-005".to_string(),
            name: "Missing tool specification".to_string(),
            description: "Every cutting operation must specify the cutting tool".to_string(),
            severity: Severity::Warning,
            domain: "machining".to_string(),
            applicable_types: vec!["CuttingOperation".to_string()],
            violation_pattern: None,
            required_relations: vec!["usesTool".to_string()],
            forbidden_relations: vec![],
            min_confidence: 0.9,
        },
    ]
}

/// Create financial/economic rules
pub fn economic_safety_rules() -> Vec<GuardrailRule> {
    vec![
        GuardrailRule {
            id: "ECON-001".to_string(),
            name: "Missing risk assessment".to_string(),
            description: "Financial transactions above threshold require risk assessment"
                .to_string(),
            severity: Severity::Critical,
            domain: "economics".to_string(),
            applicable_types: vec!["Transaction".to_string(), "Investment".to_string()],
            violation_pattern: None,
            required_relations: vec!["hasRiskAssessment".to_string()],
            forbidden_relations: vec![],
            min_confidence: 0.9,
        },
        GuardrailRule {
            id: "ECON-002".to_string(),
            name: "Circular dependency detected".to_string(),
            description: "Circular financial dependencies can indicate fraud or instability"
                .to_string(),
            severity: Severity::Warning,
            domain: "economics".to_string(),
            applicable_types: vec!["Entity".to_string()],
            violation_pattern: Some(ViolationPattern {
                path: vec!["owes".to_string(), "owes".to_string(), "owes".to_string()],
                target_type: None,
                constraints: vec![],
            }),
            required_relations: vec![],
            forbidden_relations: vec![],
            min_confidence: 0.85,
        },
    ]
}

// ============================================================================
// Query Validator
// ============================================================================

/// Validates queries against guardrails before execution
pub struct QueryValidator {
    engine: GuardrailEngine,
    /// Block queries that would violate critical rules
    block_on_critical: bool,
}

impl QueryValidator {
    pub fn new(engine: GuardrailEngine) -> Self {
        Self {
            engine,
            block_on_critical: true,
        }
    }

    /// Validate a query before execution
    pub fn validate_query(
        &self,
        db: &PathDB,
        query: &PathQuery,
        context: &CheckContext,
    ) -> QueryValidationResult {
        let warnings: Vec<GuardrailViolation> = Vec::new();
        let errors: Vec<GuardrailViolation> = Vec::new();

        // In production: analyze query structure for potentially dangerous patterns
        // e.g., queries that might expose sensitive data, circular queries, etc.

        if errors.is_empty() {
            QueryValidationResult::Approved { warnings }
        } else if self.block_on_critical && errors.iter().any(|v| v.severity >= Severity::Critical)
        {
            QueryValidationResult::Blocked { errors, warnings }
        } else {
            QueryValidationResult::ApprovedWithWarnings { errors, warnings }
        }
    }
}

/// Result of query validation
#[derive(Debug)]
pub enum QueryValidationResult {
    /// Query is safe to execute
    Approved { warnings: Vec<GuardrailViolation> },
    /// Query approved but has warnings
    ApprovedWithWarnings {
        errors: Vec<GuardrailViolation>,
        warnings: Vec<GuardrailViolation>,
    },
    /// Query blocked due to critical violations
    Blocked {
        errors: Vec<GuardrailViolation>,
        warnings: Vec<GuardrailViolation>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machining_rules() {
        let rules = machining_safety_rules();
        assert!(!rules.is_empty());

        // Check that critical rules exist
        assert!(rules.iter().any(|r| r.severity == Severity::Critical));
    }

    #[test]
    fn test_disclosure_levels() {
        assert_eq!(
            DisclosureLevel::from_experience(0.1),
            DisclosureLevel::Minimal
        );
        assert_eq!(
            DisclosureLevel::from_experience(0.5),
            DisclosureLevel::Standard
        );
        assert_eq!(
            DisclosureLevel::from_experience(0.9),
            DisclosureLevel::Expert
        );
    }

    #[test]
    fn test_progressive_disclosure_formatting() {
        let violation = GuardrailViolation {
            rule_id: "TEST-001".to_string(),
            severity: Severity::Warning,
            explanation: "Test violation for formatting".to_string(),
            entities: vec![1, 2],
            evidence: vec![vec!["rel1".to_string(), "rel2".to_string()]],
            suggestions: vec!["Fix this".to_string()],
            learning_resources: vec![],
        };

        let minimal = ProgressiveDisclosure::new(DisclosureLevel::Minimal);
        let minimal_output = minimal.format_violation(&violation);
        assert!(minimal_output.len() < 100);

        let detailed = ProgressiveDisclosure::new(DisclosureLevel::Detailed);
        let detailed_output = detailed.format_violation(&violation);
        assert!(detailed_output.len() > minimal_output.len());
    }
}
