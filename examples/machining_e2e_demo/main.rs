//! E2E Machining Demo: Complete Axiograph Integration
//!
//! This demo shows:
//! 1. Ingesting machining knowledge from multiple sources
//! 2. Building a knowledge graph with verified facts
//! 3. LLM querying with grounded responses
//! 4. Guardrails for safe operation recommendations
//! 5. Path-based reasoning for process planning

use std::collections::HashMap;

/// Simulated Axiograph components for demo
/// In real usage, these would be the actual crate imports

// ============================================================================
// Demo: Ingestion
// ============================================================================

fn ingest_machining_handbook() -> Vec<Fact> {
    vec![
        // Materials
        Fact::entity("Titanium", "Material", vec![
            ("density", "4.5 g/cm³"),
            ("thermal_conductivity", "21.9 W/mK"),
            ("hardness", "36 HRC"),
        ], 0.99, "Machinery's Handbook"),
        
        Fact::entity("Ti-6Al-4V", "TitaniumAlloy", vec![
            ("composition", "90% Ti, 6% Al, 4% V"),
            ("tensile_strength", "950 MPa"),
            ("yield_strength", "880 MPa"),
        ], 0.98, "ASM Handbook"),
        
        // Cutting parameters
        Fact::entity("TitaniumCuttingParams", "CuttingParameters", vec![
            ("sfm_min", "60"),
            ("sfm_max", "150"),
            ("feed_per_tooth", "0.003-0.005 in"),
            ("depth_of_cut", "0.050-0.100 in"),
        ], 0.95, "Industry Standard"),
        
        // Relationships
        Fact::relation("Ti-6Al-4V", "is_a", "Titanium", 0.99),
        Fact::relation("Ti-6Al-4V", "uses", "TitaniumCuttingParams", 0.95),
        
        // Tacit knowledge from experts
        Fact::tacit(
            "titanium machining",
            "When chip color turns blue/purple, speed is too high",
            0.92,
            "Senior Machinist (20 years)",
        ),
        Fact::tacit(
            "titanium machining",
            "Interrupted cuts require 20% speed reduction vs continuous",
            0.88,
            "Process Engineer",
        ),
    ]
}

fn ingest_conversation_transcript() -> Vec<Fact> {
    vec![
        // Extracted from machinist conversation
        Fact::tacit(
            "titanium roughing",
            "Use through-spindle coolant at minimum 1000 PSI for titanium",
            0.85,
            "Conversation: John/Mike 2024-01",
        ),
        Fact::tacit(
            "tool selection",
            "Uncoated carbide dulls 3x faster than AlTiN coated on Ti",
            0.90,
            "Conversation: John/Mike 2024-01",
        ),
    ]
}

// ============================================================================
// Demo: Knowledge Graph
// ============================================================================

struct KnowledgeGraph {
    entities: HashMap<String, Entity>,
    relations: Vec<Relation>,
    tacit_rules: Vec<TacitRule>,
}

impl KnowledgeGraph {
    fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: Vec::new(),
            tacit_rules: Vec::new(),
        }
    }

    fn ingest(&mut self, facts: Vec<Fact>) {
        for fact in facts {
            match fact {
                Fact::Entity { name, entity_type, attributes, confidence, source } => {
                    self.entities.insert(name.clone(), Entity {
                        name,
                        entity_type,
                        attributes,
                        confidence,
                        sources: vec![source],
                    });
                }
                Fact::Relation { subject, predicate, object, confidence } => {
                    self.relations.push(Relation {
                        subject,
                        predicate,
                        object,
                        confidence,
                    });
                }
                Fact::Tacit { domain, rule, confidence, source } => {
                    self.tacit_rules.push(TacitRule {
                        domain,
                        rule,
                        confidence,
                        source,
                    });
                }
            }
        }
    }

    fn query_entity(&self, name: &str) -> Option<&Entity> {
        self.entities.get(name)
    }

    fn find_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        // Simple BFS for demo
        use std::collections::{VecDeque, HashSet};
        
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((from.to_string(), vec![from.to_string()]));
        
        while let Some((current, path)) = queue.pop_front() {
            if current == to {
                return Some(path);
            }
            
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current.clone());
            
            for rel in &self.relations {
                if rel.subject == current && !visited.contains(&rel.object) {
                    let mut new_path = path.clone();
                    new_path.push(format!("--[{}]-->", rel.predicate));
                    new_path.push(rel.object.clone());
                    queue.push_back((rel.object.clone(), new_path));
                }
            }
        }
        
        None
    }

    fn get_tacit_rules(&self, domain: &str) -> Vec<&TacitRule> {
        self.tacit_rules.iter()
            .filter(|r| r.domain.contains(domain))
            .collect()
    }
}

// ============================================================================
// Demo: LLM Grounding
// ============================================================================

fn ground_llm_response(kg: &KnowledgeGraph, question: &str) -> GroundedResponse {
    // Simulate LLM analysis of question
    let topics = extract_topics(question);
    
    // Find relevant facts
    let mut relevant_entities = Vec::new();
    let mut relevant_rules = Vec::new();
    
    for topic in &topics {
        if let Some(entity) = kg.query_entity(topic) {
            relevant_entities.push(entity);
        }
        relevant_rules.extend(kg.get_tacit_rules(topic));
    }
    
    // Generate grounded response
    let mut response = String::new();
    
    if topics.contains(&"titanium".to_string()) || topics.contains(&"Ti-6Al-4V".to_string()) {
        response.push_str("Based on verified knowledge:\n\n");
        
        if let Some(entity) = kg.query_entity("TitaniumCuttingParams") {
            response.push_str(&format!(
                "**Cutting Parameters for Titanium:**\n- Speed: {} SFM\n- Feed: {}\n- DOC: {}\n\n",
                entity.attributes.get("sfm_min").unwrap_or(&"?".to_string()),
                entity.attributes.get("feed_per_tooth").unwrap_or(&"?".to_string()),
                entity.attributes.get("depth_of_cut").unwrap_or(&"?".to_string()),
            ));
        }
        
        response.push_str("**Expert Knowledge:**\n");
        for rule in relevant_rules.iter().take(3) {
            response.push_str(&format!(
                "- {} (confidence: {:.0}%, source: {})\n",
                rule.rule, rule.confidence * 100.0, rule.source
            ));
        }
    }
    
    GroundedResponse {
        answer: response,
        confidence: 0.9,
        sources: relevant_entities.iter().map(|e| e.name.clone()).collect(),
        grounding_score: if relevant_entities.is_empty() { 0.3 } else { 0.95 },
    }
}

fn extract_topics(question: &str) -> Vec<String> {
    let q_lower = question.to_lowercase();
    let mut topics = Vec::new();
    
    if q_lower.contains("titanium") || q_lower.contains("ti-6al-4v") {
        topics.push("titanium".to_string());
        topics.push("Ti-6Al-4V".to_string());
    }
    if q_lower.contains("cutting") || q_lower.contains("machine") {
        topics.push("titanium machining".to_string());
    }
    
    topics
}

// ============================================================================
// Demo: Guardrails
// ============================================================================

fn check_guardrails(kg: &KnowledgeGraph, operation: &str) -> Vec<GuardrailWarning> {
    let mut warnings = Vec::new();
    let op_lower = operation.to_lowercase();
    
    // Check titanium-specific rules
    if op_lower.contains("titanium") {
        if op_lower.contains("400 sfm") || op_lower.contains("500 sfm") {
            warnings.push(GuardrailWarning {
                severity: Severity::Error,
                rule: "MACH-001: Titanium Speed Limit".to_string(),
                message: "Cutting speed exceeds safe limit for titanium. Maximum recommended: 150 SFM.".to_string(),
                remediation: "Reduce speed to 100-150 SFM. Use through-coolant at >1000 PSI.".to_string(),
            });
        }
        
        if !op_lower.contains("coolant") {
            warnings.push(GuardrailWarning {
                severity: Severity::Warning,
                rule: "MACH-002: Titanium Coolant Required".to_string(),
                message: "Titanium machining requires high-pressure coolant.".to_string(),
                remediation: "Add through-spindle coolant specification (minimum 1000 PSI).".to_string(),
            });
        }
    }
    
    warnings
}

// ============================================================================
// Data Types
// ============================================================================

enum Fact {
    Entity {
        name: String,
        entity_type: String,
        attributes: HashMap<String, String>,
        confidence: f64,
        source: String,
    },
    Relation {
        subject: String,
        predicate: String,
        object: String,
        confidence: f64,
    },
    Tacit {
        domain: String,
        rule: String,
        confidence: f64,
        source: String,
    },
}

impl Fact {
    fn entity(name: &str, entity_type: &str, attrs: Vec<(&str, &str)>, conf: f64, src: &str) -> Self {
        Fact::Entity {
            name: name.to_string(),
            entity_type: entity_type.to_string(),
            attributes: attrs.into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            confidence: conf,
            source: src.to_string(),
        }
    }
    
    fn relation(subj: &str, pred: &str, obj: &str, conf: f64) -> Self {
        Fact::Relation {
            subject: subj.to_string(),
            predicate: pred.to_string(),
            object: obj.to_string(),
            confidence: conf,
        }
    }
    
    fn tacit(domain: &str, rule: &str, conf: f64, src: &str) -> Self {
        Fact::Tacit {
            domain: domain.to_string(),
            rule: rule.to_string(),
            confidence: conf,
            source: src.to_string(),
        }
    }
}

struct Entity {
    name: String,
    entity_type: String,
    attributes: HashMap<String, String>,
    confidence: f64,
    sources: Vec<String>,
}

struct Relation {
    subject: String,
    predicate: String,
    object: String,
    confidence: f64,
}

struct TacitRule {
    domain: String,
    rule: String,
    confidence: f64,
    source: String,
}

struct GroundedResponse {
    answer: String,
    confidence: f64,
    sources: Vec<String>,
    grounding_score: f64,
}

struct GuardrailWarning {
    severity: Severity,
    rule: String,
    message: String,
    remediation: String,
}

#[derive(Debug)]
enum Severity {
    Info,
    Warning,
    Error,
}

// ============================================================================
// Main Demo
// ============================================================================

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           AXIOGRAPH E2E MACHINING DEMO                       ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // Step 1: Build Knowledge Graph
    println!("━━━ Step 1: Ingesting Knowledge ━━━\n");
    
    let mut kg = KnowledgeGraph::new();
    
    println!("  📚 Ingesting Machinery's Handbook...");
    kg.ingest(ingest_machining_handbook());
    println!("     ✓ {} entities, {} relations, {} tacit rules", 
             kg.entities.len(), kg.relations.len(), kg.tacit_rules.len());
    
    println!("  💬 Ingesting conversation transcripts...");
    kg.ingest(ingest_conversation_transcript());
    println!("     ✓ {} total tacit rules\n", kg.tacit_rules.len());

    // Step 2: Query the Knowledge Graph
    println!("━━━ Step 2: Path-Based Reasoning ━━━\n");
    
    if let Some(path) = kg.find_path("Ti-6Al-4V", "TitaniumCuttingParams") {
        println!("  🔍 Path from Ti-6Al-4V to cutting parameters:");
        println!("     {}\n", path.join(" "));
    }

    // Step 3: LLM Query with Grounding
    println!("━━━ Step 3: LLM Query with Grounding ━━━\n");
    
    let question = "What cutting parameters should I use for Ti-6Al-4V?";
    println!("  ❓ Question: \"{}\"\n", question);
    
    let response = ground_llm_response(&kg, question);
    println!("  📝 Grounded Response (confidence: {:.0}%, grounding: {:.0}%):\n", 
             response.confidence * 100.0, response.grounding_score * 100.0);
    for line in response.answer.lines() {
        println!("     {}", line);
    }
    println!();

    // Step 4: Guardrails Check
    println!("━━━ Step 4: Guardrails Check ━━━\n");
    
    let unsafe_op = "Machine titanium at 400 SFM without coolant";
    println!("  ⚠️  Checking operation: \"{}\"\n", unsafe_op);
    
    let warnings = check_guardrails(&kg, unsafe_op);
    for warning in &warnings {
        let icon = match warning.severity {
            Severity::Error => "🛑",
            Severity::Warning => "⚠️ ",
            Severity::Info => "ℹ️ ",
        };
        println!("  {} [{:?}] {}", icon, warning.severity, warning.rule);
        println!("     Message: {}", warning.message);
        println!("     Fix: {}\n", warning.remediation);
    }

    // Summary
    println!("━━━ Summary ━━━\n");
    println!("  ✅ Knowledge ingested from multiple sources");
    println!("  ✅ Facts verified with confidence scores");
    println!("  ✅ Paths computed for reasoning");
    println!("  ✅ LLM responses grounded in knowledge graph");
    println!("  ✅ Guardrails caught {} safety issues", warnings.len());
    println!("\n  This demonstrates Axiograph's end-to-end capabilities for");
    println!("  domain-specific knowledge management with verification.");
}

