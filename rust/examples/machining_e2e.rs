//! Machining Knowledge Graph E2E Example
//!
//! Demonstrates the full Axiograph pipeline for machining knowledge:
//! 1. Building a knowledge graph with materials, properties, and cutting parameters
//! 2. Adding tacit knowledge from experts
//! 3. Path finding and confidence calculation
//! 4. Bayesian belief updates with new evidence
//! 5. LLM grounding and guardrails
//!
//! This example uses the FFI-compatible types to ensure compatibility with Idris.

use std::collections::HashMap;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║        AXIOGRAPH MACHINING E2E DEMONSTRATION                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    // ========================================================================
    // Step 1: Build Knowledge Graph
    // ========================================================================

    println!("━━━ Step 1: Building Knowledge Graph ━━━");
    println!();

    let mut kg = KnowledgeGraph::new();

    // Materials with probabilistic confidence
    let steel = kg.add_entity(Entity::new("Material", "Steel", Prob::new(0.95)));
    let titanium = kg.add_entity(Entity::new("Material", "Titanium", Prob::new(0.90)));
    let inconel = kg.add_entity(Entity::new("Material", "Inconel 718", Prob::new(0.88)));
    let aluminum = kg.add_entity(Entity::new("Material", "6061 Aluminum", Prob::new(0.92)));

    println!("  Materials:");
    println!(
        "    • Steel       (conf: {:.0}%)",
        kg.get(steel).confidence.to_float() * 100.0
    );
    println!(
        "    • Titanium    (conf: {:.0}%)",
        kg.get(titanium).confidence.to_float() * 100.0
    );
    println!(
        "    • Inconel 718 (conf: {:.0}%)",
        kg.get(inconel).confidence.to_float() * 100.0
    );
    println!(
        "    • 6061 Alum   (conf: {:.0}%)",
        kg.get(aluminum).confidence.to_float() * 100.0
    );

    // Material properties (verified bounds)
    let high_strength = kg.add_entity(Entity::new("Property", "High Strength", Prob::new(0.92)));
    let low_conductivity = kg.add_entity(Entity::new(
        "Property",
        "Low Thermal Conductivity",
        Prob::new(0.88),
    ));
    let corrosion_resistant = kg.add_entity(Entity::new(
        "Property",
        "Corrosion Resistant",
        Prob::new(0.90),
    ));
    let lightweight = kg.add_entity(Entity::new("Property", "Lightweight", Prob::new(0.95)));
    let work_hardening = kg.add_entity(Entity::new("Property", "Work Hardening", Prob::new(0.85)));

    println!("\n  Properties:");
    println!("    • High Strength, Low Thermal Conductivity");
    println!("    • Corrosion Resistant, Lightweight, Work Hardening");

    // Cutting parameter sets
    let steel_standard = kg.add_entity(Entity::new(
        "CuttingParams",
        "Steel Standard",
        Prob::new(0.95),
    ));
    let ti_conservative = kg.add_entity(Entity::new(
        "CuttingParams",
        "Titanium Conservative",
        Prob::new(0.88),
    ));
    let inconel_careful = kg.add_entity(Entity::new(
        "CuttingParams",
        "Inconel Careful",
        Prob::new(0.82),
    ));
    let al_aggressive = kg.add_entity(Entity::new(
        "CuttingParams",
        "Aluminum Aggressive",
        Prob::new(0.93),
    ));

    println!("\n  Cutting Parameters:");
    println!("    • Steel Standard, Titanium Conservative");
    println!("    • Inconel Careful, Aluminum Aggressive");

    // Property relations
    kg.add_relation(steel, high_strength, "has_property", Prob::new(0.90));
    kg.add_relation(titanium, high_strength, "has_property", Prob::new(0.88));
    kg.add_relation(titanium, low_conductivity, "has_property", Prob::new(0.95));
    kg.add_relation(
        titanium,
        corrosion_resistant,
        "has_property",
        Prob::new(0.92),
    );
    kg.add_relation(titanium, lightweight, "has_property", Prob::new(0.97));
    kg.add_relation(inconel, high_strength, "has_property", Prob::new(0.95));
    kg.add_relation(inconel, low_conductivity, "has_property", Prob::new(0.93));
    kg.add_relation(
        inconel,
        corrosion_resistant,
        "has_property",
        Prob::new(0.97),
    );
    kg.add_relation(inconel, work_hardening, "has_property", Prob::new(0.92));
    kg.add_relation(aluminum, lightweight, "has_property", Prob::new(0.98));

    // Parameter usage relations
    kg.add_relation(steel, steel_standard, "uses_params", Prob::new(0.95));
    kg.add_relation(titanium, ti_conservative, "uses_params", Prob::new(0.90));
    kg.add_relation(inconel, inconel_careful, "uses_params", Prob::new(0.85));
    kg.add_relation(aluminum, al_aggressive, "uses_params", Prob::new(0.93));

    // Property implications for cutting
    kg.add_relation(
        low_conductivity,
        ti_conservative,
        "implies",
        Prob::new(0.85),
    );
    kg.add_relation(work_hardening, inconel_careful, "implies", Prob::new(0.88));

    println!("\n  Added {} relations", kg.relations.len());
    println!();

    // ========================================================================
    // Step 2: Add Tacit Knowledge
    // ========================================================================

    println!("━━━ Step 2: Adding Tacit Knowledge ━━━");
    println!();

    kg.add_tacit_knowledge(TacitKnowledge {
        domain: "Titanium".into(),
        rule: "Blue/purple chip color indicates overheating - reduce speed immediately".into(),
        confidence: Prob::new(0.92),
        source: "Senior Machinist (25 yrs)".into(),
        source_credibility: Prob::new(0.95),
    });

    kg.add_tacit_knowledge(TacitKnowledge {
        domain: "Titanium".into(),
        rule: "Use through-spindle coolant at minimum 1000 PSI for deep pockets".into(),
        confidence: Prob::new(0.88),
        source: "Process Engineer".into(),
        source_credibility: Prob::new(0.90),
    });

    kg.add_tacit_knowledge(TacitKnowledge {
        domain: "Inconel".into(),
        rule: "Never dwell in cut - work hardening occurs within seconds".into(),
        confidence: Prob::new(0.90),
        source: "Aerospace Manufacturing Guide".into(),
        source_credibility: Prob::new(0.92),
    });

    kg.add_tacit_knowledge(TacitKnowledge {
        domain: "Inconel".into(),
        rule: "Ceramic inserts only above 800 SFM; use carbide below".into(),
        confidence: Prob::new(0.85),
        source: "Tool Manufacturer Handbook".into(),
        source_credibility: Prob::new(0.88),
    });

    kg.add_tacit_knowledge(TacitKnowledge {
        domain: "Aluminum".into(),
        rule: "High helix angle (45°+) prevents chip welding".into(),
        confidence: Prob::new(0.91),
        source: "CNC Programmer".into(),
        source_credibility: Prob::new(0.85),
    });

    println!("  Added {} tacit knowledge rules", kg.tacit_knowledge.len());
    for tk in &kg.tacit_knowledge {
        println!(
            "    • [{}] {} (conf: {:.0}%)",
            tk.domain,
            &tk.rule[..tk.rule.len().min(50)],
            tk.confidence.to_float() * 100.0
        );
    }
    println!();

    // ========================================================================
    // Step 3: Path Finding & Confidence
    // ========================================================================

    println!("━━━ Step 3: Path Finding & Confidence ━━━");
    println!();

    // Find path from Titanium to recommended cutting parameters
    print!("  Query: Titanium → Cutting Parameters");
    if let Some(path) = kg.find_path(titanium, ti_conservative) {
        println!();
        println!("    Path: {}", format_path(&kg, &path));
        println!("    Confidence: {:.1}%", kg.path_confidence(&path) * 100.0);
    } else {
        println!(" (direct relation)");
        let conf = kg.get_relation_confidence(titanium, ti_conservative);
        println!("    Confidence: {:.1}%", conf * 100.0);
    }

    // Find path via property implication
    print!("\n  Query: Low Thermal Conductivity → Cutting Parameters");
    if let Some(path) = kg.find_path(low_conductivity, ti_conservative) {
        println!();
        println!("    Path: {}", format_path(&kg, &path));
        println!("    Confidence: {:.1}%", kg.path_confidence(&path) * 100.0);
    }
    println!();

    // ========================================================================
    // Step 4: Bayesian Belief Update
    // ========================================================================

    println!("━━━ Step 4: Bayesian Belief Updates ━━━");
    println!();

    // Scenario: We observe successful machining with conservative parameters
    // Update our belief in the cutting parameters recommendation

    let prior = kg.get_relation_confidence(titanium, ti_conservative);
    println!(
        "  Prior belief (Ti → Conservative Params): {:.1}%",
        prior * 100.0
    );

    // Evidence: 5 successful runs with these parameters
    // P(success | correct params) = 0.95
    // P(success | wrong params) = 0.3
    let likelihood_if_correct = Prob::new(0.95);
    let likelihood_if_wrong = Prob::new(0.30);

    let posterior = bayesian_update(Prob::new(prior), likelihood_if_correct, likelihood_if_wrong);

    println!("  Evidence: 5 successful machining runs");
    println!("    P(success | correct params) = 95%");
    println!("    P(success | wrong params)   = 30%");
    println!("  Posterior belief: {:.1}%", posterior.to_float() * 100.0);
    println!();

    // Second update with conflicting evidence
    let evidence2_likelihood = Prob::new(0.6); // Tool wore faster than expected
    let evidence2_not = Prob::new(0.4);

    let posterior2 = bayesian_update(posterior, evidence2_likelihood, evidence2_not);
    println!("  New Evidence: Tool wore faster than expected");
    println!("  Updated belief: {:.1}%", posterior2.to_float() * 100.0);
    println!();

    // ========================================================================
    // Step 5: LLM Grounding Demo
    // ========================================================================

    println!("━━━ Step 5: LLM Grounding ━━━");
    println!();

    let query = "What parameters should I use for machining titanium?";
    println!("  Query: \"{}\"", query);
    println!();

    let grounded = kg.ground_response(query);
    println!("  Grounded Response:");
    for line in grounded.lines() {
        println!("    {}", line);
    }
    println!();

    // ========================================================================
    // Step 6: Guardrails Check
    // ========================================================================

    println!("━━━ Step 6: Guardrails Check ━━━");
    println!();

    let dangerous_operation = "Machine titanium at 400 SFM with no coolant";
    println!("  Checking: \"{}\"", dangerous_operation);
    println!();

    let warnings = kg.check_guardrails(dangerous_operation);
    for (level, code, msg) in &warnings {
        let icon = match level.as_str() {
            "CRITICAL" => "🛑",
            "ERROR" => "❌",
            "WARNING" => "⚠️",
            _ => "ℹ️",
        };
        println!("  {} [{}] {}", icon, code, msg);
    }
    println!();

    // ========================================================================
    // Step 7: Export to .axi Format
    // ========================================================================

    println!("━━━ Step 7: Export to .axi Format ━━━");
    println!();

    let axi = kg.to_axi();
    println!("  Generated .axi schema ({} bytes)", axi.len());
    println!("  Preview:");
    for line in axi.lines().take(20) {
        println!("    {}", line);
    }
    println!("    ...");
    println!();

    // ========================================================================
    // Summary
    // ========================================================================

    println!("━━━ Summary ━━━");
    println!();
    println!("  📊 Knowledge Graph:");
    println!("     • {} entities", kg.entities.len());
    println!("     • {} relations", kg.relations.len());
    println!("     • {} tacit knowledge rules", kg.tacit_knowledge.len());
    println!();
    println!("  🔧 Verified Properties:");
    println!("     • All probabilities bounded in [0, 1]");
    println!("     • Path confidence computed with multiplication");
    println!("     • Bayesian updates preserve probability bounds");
    println!();
    println!("  ✅ E2E demonstration complete!");
    println!();
}

// ============================================================================
// Probability Type (FFI-Compatible, mirrors Idris VProb)
// ============================================================================

const PRECISION: u32 = 1_000_000;

#[derive(Clone, Copy, Debug)]
struct Prob {
    numerator: u32,
}

impl Prob {
    fn new(value: f64) -> Self {
        let clamped = value.clamp(0.0, 1.0);
        let num = (clamped * PRECISION as f64).round() as u32;
        Self {
            numerator: num.min(PRECISION),
        }
    }

    fn to_float(self) -> f64 {
        self.numerator as f64 / PRECISION as f64
    }

    fn mult(self, other: Self) -> Self {
        let product = (self.numerator as u64 * other.numerator as u64) / PRECISION as u64;
        Self {
            numerator: product.min(PRECISION as u64) as u32,
        }
    }
}

fn bayesian_update(prior: Prob, likelihood_true: Prob, likelihood_false: Prob) -> Prob {
    let p = prior.to_float();
    let lt = likelihood_true.to_float();
    let lf = likelihood_false.to_float();

    let numerator = lt * p;
    let denominator = numerator + lf * (1.0 - p);

    if denominator <= 0.0 {
        return prior;
    }

    Prob::new(numerator / denominator)
}

// ============================================================================
// Knowledge Graph Types
// ============================================================================

type EntityId = usize;

struct Entity {
    entity_type: String,
    name: String,
    confidence: Prob,
}

impl Entity {
    fn new(entity_type: &str, name: &str, confidence: Prob) -> Self {
        Self {
            entity_type: entity_type.into(),
            name: name.into(),
            confidence,
        }
    }
}

struct Relation {
    source: EntityId,
    target: EntityId,
    rel_type: String,
    confidence: Prob,
}

struct TacitKnowledge {
    domain: String,
    rule: String,
    confidence: Prob,
    source: String,
    source_credibility: Prob,
}

struct KnowledgeGraph {
    entities: Vec<Entity>,
    relations: Vec<Relation>,
    tacit_knowledge: Vec<TacitKnowledge>,
}

impl KnowledgeGraph {
    fn new() -> Self {
        Self {
            entities: Vec::new(),
            relations: Vec::new(),
            tacit_knowledge: Vec::new(),
        }
    }

    fn add_entity(&mut self, entity: Entity) -> EntityId {
        let id = self.entities.len();
        self.entities.push(entity);
        id
    }

    fn get(&self, id: EntityId) -> &Entity {
        &self.entities[id]
    }

    fn add_relation(
        &mut self,
        source: EntityId,
        target: EntityId,
        rel_type: &str,
        confidence: Prob,
    ) {
        self.relations.push(Relation {
            source,
            target,
            rel_type: rel_type.into(),
            confidence,
        });
    }

    fn add_tacit_knowledge(&mut self, tk: TacitKnowledge) {
        self.tacit_knowledge.push(tk);
    }

    fn get_relation_confidence(&self, source: EntityId, target: EntityId) -> f64 {
        self.relations
            .iter()
            .find(|r| r.source == source && r.target == target)
            .map(|r| r.confidence.to_float())
            .unwrap_or(0.0)
    }

    fn find_path(&self, from: EntityId, to: EntityId) -> Option<Vec<EntityId>> {
        use std::collections::{HashSet, VecDeque};

        if from == to {
            return Some(vec![from]);
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back((from, vec![from]));

        while let Some((current, path)) = queue.pop_front() {
            if path.len() > 5 {
                continue;
            }

            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            for rel in &self.relations {
                if rel.source == current {
                    if rel.target == to {
                        let mut result = path.clone();
                        result.push(to);
                        return Some(result);
                    }

                    if !visited.contains(&rel.target) {
                        let mut new_path = path.clone();
                        new_path.push(rel.target);
                        queue.push_back((rel.target, new_path));
                    }
                }
            }
        }

        None
    }

    fn path_confidence(&self, path: &[EntityId]) -> f64 {
        if path.len() < 2 {
            return 1.0;
        }

        let mut confidence = Prob::new(1.0);
        for window in path.windows(2) {
            let from = window[0];
            let to = window[1];
            let rel_conf = Prob::new(self.get_relation_confidence(from, to));
            confidence = confidence.mult(rel_conf);
        }
        confidence.to_float()
    }

    fn ground_response(&self, query: &str) -> String {
        let mut response = String::new();
        let query_lower = query.to_lowercase();

        // Find relevant material
        let material = if query_lower.contains("titanium") {
            Some("Titanium")
        } else if query_lower.contains("inconel") {
            Some("Inconel")
        } else if query_lower.contains("aluminum") {
            Some("Aluminum")
        } else if query_lower.contains("steel") {
            Some("Steel")
        } else {
            None
        };

        if let Some(mat) = material {
            response.push_str(&format!("**Recommendations for {}:**\n\n", mat));

            // Find entity
            if let Some(entity) = self.entities.iter().find(|e| e.name.contains(mat)) {
                response.push_str(&format!(
                    "Knowledge confidence: {:.0}%\n\n",
                    entity.confidence.to_float() * 100.0
                ));
            }

            // Add tacit knowledge
            response.push_str("**Expert Knowledge:**\n");
            for tk in &self.tacit_knowledge {
                if tk.domain.to_lowercase().contains(&mat.to_lowercase()) {
                    response.push_str(&format!(
                        "• {} (conf: {:.0}%, source: {})\n",
                        tk.rule,
                        tk.confidence.to_float() * 100.0,
                        tk.source
                    ));
                }
            }
        }

        response
    }

    fn check_guardrails(&self, operation: &str) -> Vec<(String, String, String)> {
        let mut warnings = Vec::new();
        let op = operation.to_lowercase();

        // Titanium checks
        if op.contains("titanium") {
            if op.contains("400") || op.contains("sfm") && !op.contains("conservative") {
                warnings.push((
                    "CRITICAL".into(),
                    "MACH-001".into(),
                    "400 SFM exceeds safe limit for titanium (recommended: 60-150 SFM)".into(),
                ));
            }
            if op.contains("no coolant") {
                warnings.push((
                    "CRITICAL".into(),
                    "COOL-001".into(),
                    "Titanium REQUIRES coolant - risk of fire and tool damage".into(),
                ));
            }
        }

        // Inconel checks
        if op.contains("inconel") && op.contains("dwell") {
            warnings.push((
                "ERROR".into(),
                "MACH-002".into(),
                "Never dwell in Inconel - causes work hardening".into(),
            ));
        }

        warnings
    }

    fn to_axi(&self) -> String {
        let mut out = String::new();
        out.push_str("module GeneratedMachiningKG\n\n");

        // Collect types
        let mut types: Vec<_> = self
            .entities
            .iter()
            .map(|e| e.entity_type.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        types.sort();

        out.push_str("schema MachiningSchema {\n  objects {\n");
        for t in &types {
            out.push_str(&format!("    {}\n", t));
        }
        out.push_str("  }\n\n  morphisms {\n");

        // Collect relation types
        let mut rel_types: Vec<_> = self
            .relations
            .iter()
            .map(|r| r.rel_type.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        rel_types.sort();

        for rt in &rel_types {
            out.push_str(&format!("    {} : Object -> Object\n", rt));
        }
        out.push_str("  }\n}\n\n");

        // Instance
        out.push_str("instance Data : MachiningSchema {\n");
        for e in &self.entities {
            let safe_name = e.name.replace(' ', "_").replace('-', "_");
            out.push_str(&format!(
                "  {} : {} -- confidence: {:.2}\n",
                safe_name,
                e.entity_type,
                e.confidence.to_float()
            ));
        }
        out.push_str("}\n");

        out
    }
}

fn format_path(kg: &KnowledgeGraph, path: &[EntityId]) -> String {
    path.iter()
        .map(|&id| kg.get(id).name.as_str())
        .collect::<Vec<_>>()
        .join(" → ")
}
