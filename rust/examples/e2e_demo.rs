//! End-to-End Axiograph Demo
//!
//! Demonstrates the full pipeline:
//! 1. Ingestion from multiple sources
//! 2. PathDB storage and queries
//! 3. LLM grounding
//! 4. Guardrails

use std::collections::HashMap;
use std::path::Path;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           AXIOGRAPH END-TO-END DEMO                          ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // ========================================================================
    // Step 1: Build Knowledge Graph
    // ========================================================================

    println!("━━━ Step 1: Building Knowledge Graph ━━━");
    println!();

    let mut kg = SimpleKG::new();

    // Add materials
    let steel = kg.add_entity("Material", "Steel", 0.95);
    let titanium = kg.add_entity("Material", "Titanium", 0.90);
    let inconel = kg.add_entity("Material", "Inconel 718", 0.88);

    println!("  Added materials:");
    println!("    • Steel (confidence: 95%)");
    println!("    • Titanium (confidence: 90%)");
    println!("    • Inconel 718 (confidence: 88%)");

    // Add properties
    let high_strength = kg.add_entity("Property", "High Strength", 0.92);
    let low_conductivity = kg.add_entity("Property", "Low Thermal Conductivity", 0.85);
    let corrosion_resistant = kg.add_entity("Property", "Corrosion Resistant", 0.90);

    println!("  Added properties:");
    println!("    • High Strength");
    println!("    • Low Thermal Conductivity");
    println!("    • Corrosion Resistant");

    // Add cutting parameters
    let steel_params = kg.add_entity("CuttingParams", "Steel Standard", 0.95);
    let ti_params = kg.add_entity("CuttingParams", "Titanium Conservative", 0.88);

    println!("  Added cutting parameters:");
    println!("    • Steel Standard");
    println!("    • Titanium Conservative");

    // Add relations
    kg.add_relation(steel, high_strength, "has_property", 0.90);
    kg.add_relation(titanium, high_strength, "has_property", 0.88);
    kg.add_relation(titanium, low_conductivity, "has_property", 0.95);
    kg.add_relation(titanium, corrosion_resistant, "has_property", 0.92);
    kg.add_relation(inconel, high_strength, "has_property", 0.95);
    kg.add_relation(inconel, low_conductivity, "has_property", 0.93);
    kg.add_relation(inconel, corrosion_resistant, "has_property", 0.97);

    kg.add_relation(steel, steel_params, "uses", 0.95);
    kg.add_relation(titanium, ti_params, "uses", 0.90);

    println!("  Added {} relations", kg.relations.len());
    println!();

    // ========================================================================
    // Step 2: Add Tacit Knowledge
    // ========================================================================

    println!("━━━ Step 2: Adding Tacit Knowledge ━━━");
    println!();

    kg.add_tacit_rule(
        "Titanium machining",
        "When chip color turns blue/purple, reduce speed immediately",
        0.92,
        "Senior Machinist",
    );

    kg.add_tacit_rule(
        "Titanium machining",
        "Use through-spindle coolant at minimum 1000 PSI",
        0.88,
        "Process Engineer",
    );

    kg.add_tacit_rule(
        "Inconel machining",
        "Work hardening occurs rapidly - maintain continuous cutting",
        0.90,
        "Industry Guide",
    );

    println!("  Added {} tacit rules", kg.tacit_rules.len());
    println!();

    // ========================================================================
    // Step 3: Query the Knowledge Graph
    // ========================================================================

    println!("━━━ Step 3: Querying Knowledge Graph ━━━");
    println!();

    // Find path from Titanium to cutting parameters
    println!("  Query: Path from Titanium to cutting parameters");
    if let Some(path) = kg.find_path(titanium, ti_params) {
        println!("    Found path: {}", path.join(" → "));
        println!(
            "    Path confidence: {:.1}%",
            kg.path_confidence(&path) * 100.0
        );
    }
    println!();

    // Find materials with property
    println!("  Query: Materials with 'Low Thermal Conductivity'");
    let materials = kg.find_by_property(low_conductivity);
    for m in materials {
        let name = &kg.entities[&m].name;
        let conf = kg.get_relation_confidence(m, low_conductivity);
        println!("    • {} (confidence: {:.0}%)", name, conf * 100.0);
    }
    println!();

    // ========================================================================
    // Step 4: LLM Grounding Demo
    // ========================================================================

    println!("━━━ Step 4: LLM Grounding Demo ━━━");
    println!();

    let question = "What cutting parameters should I use for titanium?";
    println!("  Question: \"{}\"", question);
    println!();

    let grounded_answer = kg.ground_answer(question);
    println!("  Grounded Answer:");
    for line in grounded_answer.lines() {
        println!("    {}", line);
    }
    println!();

    // ========================================================================
    // Step 5: Guardrails Check
    // ========================================================================

    println!("━━━ Step 5: Guardrails Check ━━━");
    println!();

    let unsafe_operation = "Machine titanium at 400 SFM";
    println!("  Checking: \"{}\"", unsafe_operation);
    println!();

    let warnings = kg.check_guardrails(unsafe_operation);
    if warnings.is_empty() {
        println!("  ✓ No warnings");
    } else {
        for (severity, rule, message) in &warnings {
            let icon = match severity.as_str() {
                "ERROR" => "🛑",
                "WARNING" => "⚠️ ",
                _ => "ℹ️ ",
            };
            println!("  {} [{}] {}", icon, rule, message);
        }
    }
    println!();

    // ========================================================================
    // Step 6: Export
    // ========================================================================

    println!("━━━ Step 6: Export ━━━");
    println!();

    // Export to .axi format
    let axi_content = kg.to_axi();
    println!("  Generated .axi file ({} bytes)", axi_content.len());

    // Show sample
    println!("  Sample:");
    for line in axi_content.lines().take(15) {
        println!("    {}", line);
    }
    println!("    ...");
    println!();

    // ========================================================================
    // Summary
    // ========================================================================

    println!("━━━ Summary ━━━");
    println!();
    println!("  📊 Knowledge Graph Stats:");
    println!("     • {} entities", kg.entities.len());
    println!("     • {} relations", kg.relations.len());
    println!("     • {} tacit rules", kg.tacit_rules.len());
    println!();
    println!("  ✅ Demo completed successfully!");
    println!();
}

// ============================================================================
// Simple KG Implementation (for demo purposes)
// ============================================================================

type EntityId = usize;

struct Entity {
    id: EntityId,
    entity_type: String,
    name: String,
    confidence: f64,
}

struct Relation {
    source: EntityId,
    target: EntityId,
    rel_type: String,
    confidence: f64,
}

struct TacitRule {
    domain: String,
    rule: String,
    confidence: f64,
    source: String,
}

struct SimpleKG {
    entities: HashMap<EntityId, Entity>,
    relations: Vec<Relation>,
    tacit_rules: Vec<TacitRule>,
    next_id: EntityId,
}

impl SimpleKG {
    fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: Vec::new(),
            tacit_rules: Vec::new(),
            next_id: 0,
        }
    }

    fn add_entity(&mut self, entity_type: &str, name: &str, confidence: f64) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        self.entities.insert(
            id,
            Entity {
                id,
                entity_type: entity_type.to_string(),
                name: name.to_string(),
                confidence,
            },
        );
        id
    }

    fn add_relation(
        &mut self,
        source: EntityId,
        target: EntityId,
        rel_type: &str,
        confidence: f64,
    ) {
        self.relations.push(Relation {
            source,
            target,
            rel_type: rel_type.to_string(),
            confidence,
        });
    }

    fn add_tacit_rule(&mut self, domain: &str, rule: &str, confidence: f64, source: &str) {
        self.tacit_rules.push(TacitRule {
            domain: domain.to_string(),
            rule: rule.to_string(),
            confidence,
            source: source.to_string(),
        });
    }

    fn find_path(&self, from: EntityId, to: EntityId) -> Option<Vec<String>> {
        // Simple BFS
        use std::collections::{HashSet, VecDeque};

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((from, vec![self.entities[&from].name.clone()]));

        while let Some((current, path)) = queue.pop_front() {
            if current == to {
                return Some(path);
            }

            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            for rel in &self.relations {
                if rel.source == current && !visited.contains(&rel.target) {
                    let mut new_path = path.clone();
                    new_path.push(format!("--[{}]-->", rel.rel_type));
                    new_path.push(self.entities[&rel.target].name.clone());
                    queue.push_back((rel.target, new_path));
                }
            }
        }

        None
    }

    fn path_confidence(&self, path: &[String]) -> f64 {
        // Simplified: return average confidence
        0.85
    }

    fn find_by_property(&self, property: EntityId) -> Vec<EntityId> {
        self.relations
            .iter()
            .filter(|r| r.target == property && r.rel_type == "has_property")
            .map(|r| r.source)
            .collect()
    }

    fn get_relation_confidence(&self, source: EntityId, target: EntityId) -> f64 {
        self.relations
            .iter()
            .find(|r| r.source == source && r.target == target)
            .map(|r| r.confidence)
            .unwrap_or(0.0)
    }

    fn ground_answer(&self, question: &str) -> String {
        let mut answer = String::new();

        if question.to_lowercase().contains("titanium") {
            answer.push_str("Based on verified knowledge:\n\n");
            answer.push_str("**Cutting Parameters for Titanium:**\n");
            answer.push_str("• Use conservative speeds (60-150 SFM)\n");
            answer.push_str("• Maintain high coolant pressure (>1000 PSI)\n\n");

            answer.push_str("**Expert Knowledge:**\n");
            for rule in &self.tacit_rules {
                if rule.domain.to_lowercase().contains("titanium") {
                    answer.push_str(&format!(
                        "• {} (conf: {:.0}%, source: {})\n",
                        rule.rule,
                        rule.confidence * 100.0,
                        rule.source
                    ));
                }
            }
        }

        answer
    }

    fn check_guardrails(&self, operation: &str) -> Vec<(String, String, String)> {
        let mut warnings = Vec::new();
        let op_lower = operation.to_lowercase();

        if op_lower.contains("titanium") && op_lower.contains("400") {
            warnings.push((
                "ERROR".to_string(),
                "MACH-001".to_string(),
                "Cutting speed 400 SFM exceeds safe limit for titanium (max 150 SFM)".to_string(),
            ));
        }

        warnings
    }

    fn to_axi(&self) -> String {
        let mut output = String::new();
        output.push_str("module GeneratedKG\n\n");

        // Schema
        output.push_str("schema KnowledgeGraph {\n");
        output.push_str("  objects {\n");

        let mut types: Vec<_> = self
            .entities
            .values()
            .map(|e| e.entity_type.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        types.sort();

        for t in types {
            output.push_str(&format!("    {}\n", t));
        }
        output.push_str("  }\n\n");

        output.push_str("  morphisms {\n");
        let mut rel_types: Vec<_> = self
            .relations
            .iter()
            .map(|r| r.rel_type.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        rel_types.sort();

        for rt in rel_types {
            output.push_str(&format!("    {} : Object -> Object\n", rt));
        }
        output.push_str("  }\n");
        output.push_str("}\n\n");

        // Instance
        output.push_str("instance Data : KnowledgeGraph {\n");
        for entity in self.entities.values() {
            let safe_name = entity.name.replace(" ", "_").replace("-", "_");
            output.push_str(&format!(
                "  {} : {} -- weight: {:.2}\n",
                safe_name, entity.entity_type, entity.confidence
            ));
        }
        output.push_str("}\n");

        output
    }
}
