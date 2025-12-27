//! Quality checks / linting for `.axi` modules and `.axpd` snapshots.
//!
//! This is intentionally tooling-first:
//! - it produces an auditable report,
//! - it helps ontology engineering loops (discover → propose → review → accept),
//! - and it is not part of the trusted kernel (Lean).
//!
//! Some *subsets* of these checks can later be promoted into certificate-checked
//! gates (e.g. well-typed modules and core constraint satisfaction).

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use axiograph_pathdb::axi_meta::ATTR_AXI_RELATION;
use axiograph_pathdb::axi_semantics::{ConstraintDecl, MetaPlaneIndex};
use axiograph_pathdb::PathDB;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReportV1 {
    pub version: String,
    pub generated_at_unix_secs: u64,
    pub input: String,
    pub profile: String,
    pub plane: String,
    pub summary: QualitySummaryV1,
    pub findings: Vec<QualityFindingV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QualitySummaryV1 {
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityFindingV1 {
    pub level: String, // "error" | "warning" | "info"
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<u32>,
}

fn now_unix_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn is_meta_plane_entity(entity_type: &str) -> bool {
    entity_type.starts_with("AxiMeta")
}

fn node_is_fact(db: &PathDB, id: u32) -> bool {
    db.get_entity(id)
        .map(|e| e.attrs.contains_key(ATTR_AXI_RELATION))
        .unwrap_or(false)
}

fn node_name(db: &PathDB, id: u32) -> Option<String> {
    db.get_entity(id).and_then(|e| e.attrs.get("name").cloned())
}

pub fn cmd_quality(
    input: &PathBuf,
    out: Option<&PathBuf>,
    format: &str,
    profile: &str,
    plane: &str,
    no_fail: bool,
) -> Result<()> {
    let profile = profile.trim().to_ascii_lowercase();
    if !matches!(profile.as_str(), "fast" | "strict") {
        return Err(anyhow!(
            "unknown --profile `{profile}` (expected fast|strict)"
        ));
    }

    let plane = plane.trim().to_ascii_lowercase();
    if !matches!(plane.as_str(), "data" | "meta" | "both") {
        return Err(anyhow!(
            "unknown --plane `{plane}` (expected data|meta|both)"
        ));
    }

    let db = crate::load_pathdb_for_cli(input)?;
    let report = run_quality_checks(&db, input, &profile, &plane)?;

    let format = format.trim().to_ascii_lowercase();
    let rendered = match format.as_str() {
        "json" => serde_json::to_string_pretty(&report)?,
        "text" => render_quality_report_text(&report),
        other => return Err(anyhow!("unknown --format `{other}` (expected json|text)")),
    };

    match out {
        Some(path) => {
            std::fs::write(path, rendered)?;
            println!("wrote {}", path.display());
        }
        None => {
            println!("{rendered}");
        }
    }

    if report.summary.error_count > 0 && !no_fail {
        return Err(anyhow!(
            "quality checks found {} error(s)",
            report.summary.error_count
        ));
    }
    Ok(())
}

pub fn run_quality_checks(
    db: &PathDB,
    input: &PathBuf,
    profile: &str,
    plane: &str,
) -> Result<QualityReportV1> {
    let mut findings: Vec<QualityFindingV1> = Vec::new();

    let include_meta = plane == "meta" || plane == "both";
    let include_data = plane == "data" || plane == "both";

    // ---------------------------------------------------------------------
    // Meta-plane lint
    // ---------------------------------------------------------------------
    if include_meta {
        let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
        if meta.schemas.is_empty() {
            findings.push(QualityFindingV1 {
                level: "warning".to_string(),
                code: "meta_plane_missing".to_string(),
                message: "no meta-plane schemas found (this snapshot may be synthetic or imported without canonical `.axi`)".to_string(),
                schema: None,
                relation: None,
                entity_id: None,
            });
        } else {
            // Subtyping cycles (should generally be avoided; they confuse type-directed tooling).
            for (schema_name, schema) in &meta.schemas {
                // Build adjacency for subtype graph (sub -> sup).
                let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
                for st in &schema.subtype_decls {
                    adj.entry(&st.sub).or_default().push(&st.sup);
                }

                // DFS cycle detection.
                #[derive(Clone, Copy, PartialEq, Eq)]
                enum Mark {
                    Temp,
                    Perm,
                }
                let mut marks: HashMap<&str, Mark> = HashMap::new();
                let mut stack: Vec<&str> = Vec::new();

                fn visit<'a>(
                    node: &'a str,
                    adj: &HashMap<&'a str, Vec<&'a str>>,
                    marks: &mut HashMap<&'a str, Mark>,
                    stack: &mut Vec<&'a str>,
                    cycles: &mut Vec<Vec<&'a str>>,
                ) {
                    if matches!(marks.get(node), Some(Mark::Perm)) {
                        return;
                    }
                    if matches!(marks.get(node), Some(Mark::Temp)) {
                        // Found a back-edge; record the cycle slice.
                        if let Some(pos) = stack.iter().position(|&x| x == node) {
                            cycles.push(stack[pos..].to_vec());
                        }
                        return;
                    }
                    marks.insert(node, Mark::Temp);
                    stack.push(node);
                    if let Some(nexts) = adj.get(node) {
                        for &n in nexts {
                            visit(n, adj, marks, stack, cycles);
                        }
                    }
                    stack.pop();
                    marks.insert(node, Mark::Perm);
                }

                let mut cycles: Vec<Vec<&str>> = Vec::new();
                for t in &schema.object_types {
                    visit(t, &adj, &mut marks, &mut stack, &mut cycles);
                }
                for cycle in cycles {
                    findings.push(QualityFindingV1 {
                        level: "warning".to_string(),
                        code: "subtyping_cycle".to_string(),
                        message: format!("subtyping cycle detected: {}", cycle.join(" < ")),
                        schema: Some(schema_name.clone()),
                        relation: None,
                        entity_id: None,
                    });
                }
            }
        }
    }

    // ---------------------------------------------------------------------
    // Data-plane lint
    // ---------------------------------------------------------------------
    if include_data {
        // Dangling references: relations pointing to missing entities should never exist.
        for rel_id in 0..db.relations.len() as u32 {
            let Some(rel) = db.relations.get_relation(rel_id) else {
                continue;
            };
            if db.get_entity(rel.source).is_none() {
                findings.push(QualityFindingV1 {
                    level: "error".to_string(),
                    code: "dangling_source".to_string(),
                    message: format!(
                        "relation #{rel_id} has missing source entity {}",
                        rel.source
                    ),
                    schema: None,
                    relation: None,
                    entity_id: Some(rel.source),
                });
            }
            if db.get_entity(rel.target).is_none() {
                findings.push(QualityFindingV1 {
                    level: "error".to_string(),
                    code: "dangling_target".to_string(),
                    message: format!(
                        "relation #{rel_id} has missing target entity {}",
                        rel.target
                    ),
                    schema: None,
                    relation: None,
                    entity_id: Some(rel.target),
                });
            }
        }

        // Constraint checks (best-effort) when the meta-plane is available:
        // - key(...) duplicates
        // - functional(field -> field) violations
        let meta = MetaPlaneIndex::from_db(db).unwrap_or_default();
        if !meta.schemas.is_empty() {
            // Build a fact-node lookup: relation name -> fact ids.
            let mut facts_by_relation: HashMap<String, Vec<u32>> = HashMap::new();
            for id in 0..db.entities.len() as u32 {
                if !node_is_fact(db, id) {
                    continue;
                }
                let Some(view) = db.get_entity(id) else {
                    continue;
                };
                let Some(rel_name) = view.attrs.get(ATTR_AXI_RELATION).cloned() else {
                    continue;
                };
                facts_by_relation.entry(rel_name).or_default().push(id);
            }

            for (schema_name, schema) in &meta.schemas {
                for (rel_name, constraints) in &schema.constraints_by_relation {
                    let Some(facts) = facts_by_relation.get(rel_name) else {
                        continue;
                    };

                    // Gather all tuples as field -> value_id (best-effort: only the first edge per field).
                    let mut tuples: Vec<HashMap<String, u32>> = Vec::new();
                    for &fact_id in facts {
                        let mut t: HashMap<String, u32> = HashMap::new();
                        // We don't know the declared field set here without also looking at relation decls;
                        // for constraints we only care about specific key/src/dst fields.
                        if let Some(rel_decl) = schema.relation_decls.get(rel_name) {
                            for f in &rel_decl.fields {
                                let Some(field_rel_id) = db.interner.id_of(&f.field_name) else {
                                    continue;
                                };
                                let ids = db.relations.outgoing_relation_ids(fact_id, field_rel_id);
                                if let Some(&rid) = ids.first() {
                                    if let Some(r) = db.relations.get_relation(rid) {
                                        t.insert(f.field_name.clone(), r.target);
                                    }
                                }
                            }
                        }
                        tuples.push(t);
                    }

                    for c in constraints {
                        match c {
                            ConstraintDecl::Key { fields, .. } => {
                                if fields.is_empty() {
                                    continue;
                                }
                                let mut seen: HashMap<Vec<u32>, u32> = HashMap::new();
                                for (i, t) in tuples.iter().enumerate() {
                                    let mut key: Vec<u32> = Vec::new();
                                    let mut missing = false;
                                    for f in fields {
                                        let Some(v) = t.get(f) else {
                                            missing = true;
                                            break;
                                        };
                                        key.push(*v);
                                    }
                                    if missing {
                                        continue;
                                    }
                                    if let Some(prev_idx) = seen.get(&key) {
                                        findings.push(QualityFindingV1 {
                                            level: if profile == "strict" { "error".to_string() } else { "warning".to_string() },
                                            code: "key_violation".to_string(),
                                            message: format!("key violation on {rel_name}({}) at tuples {prev_idx} and {i}", fields.join(", ")),
                                            schema: Some(schema_name.clone()),
                                            relation: Some(rel_name.clone()),
                                            entity_id: None,
                                        });
                                    } else {
                                        seen.insert(key, i as u32);
                                    }
                                }
                            }
                            ConstraintDecl::Functional {
                                src_field,
                                dst_field,
                                ..
                            } => {
                                let mut map: HashMap<u32, u32> = HashMap::new();
                                for (i, t) in tuples.iter().enumerate() {
                                    let Some(src) = t.get(src_field) else {
                                        continue;
                                    };
                                    let Some(dst) = t.get(dst_field) else {
                                        continue;
                                    };
                                    if let Some(prev_dst) = map.get(src) {
                                        if prev_dst != dst {
                                            findings.push(QualityFindingV1 {
                                                level: if profile == "strict" { "error".to_string() } else { "warning".to_string() },
                                                code: "functional_violation".to_string(),
                                                message: format!("functional violation on {rel_name}.{src_field} -> {rel_name}.{dst_field} (src={} has multiple dsts: {} and {}; tuple={i})", src, prev_dst, dst),
                                                schema: Some(schema_name.clone()),
                                                relation: Some(rel_name.clone()),
                                                entity_id: None,
                                            });
                                        }
                                    } else {
                                        map.insert(*src, *dst);
                                    }
                                }
                            }
                            ConstraintDecl::Symmetric { .. }
                            | ConstraintDecl::Transitive { .. } => {
                                // Not yet checked here: can be expensive and many examples use conditional forms.
                            }
                            ConstraintDecl::Unknown { text, .. } => {
                                if profile == "strict" {
                                    // Surface unknown constraints as warnings so authors can tighten them into structured forms.
                                    findings.push(QualityFindingV1 {
                                        level: "warning".to_string(),
                                        code: "constraint_unknown".to_string(),
                                        message: format!("unknown/unsupported constraint: {text}"),
                                        schema: Some(schema_name.clone()),
                                        relation: Some(rel_name.clone()),
                                        entity_id: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Context scoping coverage: if a `Context` type exists, suggest putting facts into some world.
        //
        // This is a heuristic (context scoping is optional), so we only emit info/warn.
        if profile == "strict" {
            let has_context_type = db
                .find_by_type("Context")
                .map(|bm| !bm.is_empty())
                .unwrap_or(false);
            if has_context_type {
                // Count facts that have any axi_fact_in_context edge.
                let ctx_rel_id = db
                    .interner
                    .id_of(axiograph_pathdb::axi_meta::REL_AXI_FACT_IN_CONTEXT);
                if let Some(ctx_rel_id) = ctx_rel_id {
                    let mut fact_count = 0usize;
                    let mut with_ctx = 0usize;
                    for id in 0..db.entities.len() as u32 {
                        if !node_is_fact(db, id) {
                            continue;
                        }
                        fact_count += 1;
                        if !db
                            .relations
                            .outgoing_relation_ids(id, ctx_rel_id)
                            .is_empty()
                        {
                            with_ctx += 1;
                        }
                    }
                    if fact_count > 0 && with_ctx < fact_count {
                        findings.push(QualityFindingV1 {
                            level: "info".to_string(),
                            code: "context_scoping_coverage".to_string(),
                            message: format!("context scoping: {with_ctx}/{fact_count} fact nodes have a `axi_fact_in_context` edge (optional but recommended)"),
                            schema: None,
                            relation: None,
                            entity_id: None,
                        });
                    }
                }
            }
        }
    }

    // Summary counts.
    let mut summary = QualitySummaryV1::default();
    for f in &findings {
        match f.level.as_str() {
            "error" => summary.error_count += 1,
            "warning" => summary.warning_count += 1,
            _ => summary.info_count += 1,
        }
    }

    Ok(QualityReportV1 {
        version: "quality_report_v1".to_string(),
        generated_at_unix_secs: now_unix_secs(),
        input: input.display().to_string(),
        profile: profile.to_string(),
        plane: plane.to_string(),
        summary,
        findings,
    })
}

pub fn render_quality_report_text(r: &QualityReportV1) -> String {
    let mut out = String::new();
    out.push_str("quality\n");
    out.push_str(&format!("  input: {}\n", r.input));
    out.push_str(&format!("  profile: {}  plane: {}\n", r.profile, r.plane));
    out.push_str(&format!(
        "  summary: errors={} warnings={} infos={}\n",
        r.summary.error_count, r.summary.warning_count, r.summary.info_count
    ));

    if r.findings.is_empty() {
        out.push_str("  (no findings)\n");
        return out;
    }

    // Group by level for readability.
    let mut by_level: BTreeMap<&str, Vec<&QualityFindingV1>> = BTreeMap::new();
    for f in &r.findings {
        by_level.entry(&f.level).or_default().push(f);
    }

    for (level, items) in by_level {
        out.push_str(&format!("\n{level}\n"));
        for f in items {
            let mut ctx = String::new();
            if let Some(s) = &f.schema {
                ctx.push_str(&format!(" schema={s}"));
            }
            if let Some(rel) = &f.relation {
                ctx.push_str(&format!(" relation={rel}"));
            }
            if let Some(id) = f.entity_id {
                ctx.push_str(&format!(" entity={id}"));
            }
            out.push_str(&format!("  - {}: {}{}\n", f.code, f.message, ctx));
        }
    }

    out
}
