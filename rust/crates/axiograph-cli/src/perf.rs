//! Performance harnesses for Axiograph.
//!
//! This is intentionally **not** a microbenchmark framework (no Criterion).
//! It's a practical CLI tool to answer questions like:
//! - How fast can we ingest N entities + M relations into PathDB?
//! - How expensive is `build_indexes()` at different index depths?
//! - What is the throughput of common path queries?
//!
//! Run in release mode for meaningful results:
//!
//! ```bash
//! cargo run -p axiograph-cli --release -- perf pathdb --entities 200000 --edges-per-entity 8
//! ```

use anyhow::{anyhow, Result};
use clap::Subcommand;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axiograph_pathdb::axi_meta::{ATTR_AXI_RELATION, ATTR_AXI_SCHEMA};
use axiograph_pathdb::{PathDB, PathSig};
use roaring::RoaringBitmap;
use std::collections::{HashMap, HashSet};

#[derive(Subcommand)]
pub enum PerfCommands {
    /// Synthetic PathDB ingestion + indexing + querying.
    Pathdb {
        /// Number of entities to create.
        #[arg(long, default_value_t = 100_000)]
        entities: usize,

        /// Outgoing edges per entity.
        #[arg(long, default_value_t = 8)]
        edges_per_entity: usize,

        /// Number of relation types to use (chosen uniformly at random).
        #[arg(long, default_value_t = 8)]
        rel_types: usize,

        /// Maximum hop depth to build a PathIndex for.
        #[arg(long, default_value_t = 3)]
        index_depth: usize,

        /// Path length for each query (if > index_depth, queries will partially fall back).
        #[arg(long, default_value_t = 3)]
        path_len: usize,

        /// Number of queries to run.
        #[arg(long, default_value_t = 50_000)]
        queries: usize,

        /// RNG seed (deterministic).
        #[arg(long, default_value_t = 1)]
        seed: u64,

        /// Persist the generated database to `.axpd`.
        #[arg(long)]
        out_axpd: Option<PathBuf>,

        /// Export the generated database to the reversible `.axi` snapshot schema (`PathDBExportV1`).
        #[arg(long)]
        out_axi: Option<PathBuf>,
    },

    /// Synthetic AxQL querying over a generated PathDB.
    Axql {
        /// Number of entities to create.
        #[arg(long, default_value_t = 100_000)]
        entities: usize,

        /// Outgoing edges per entity.
        #[arg(long, default_value_t = 8)]
        edges_per_entity: usize,

        /// Number of relation types to use (chosen uniformly at random).
        #[arg(long, default_value_t = 8)]
        rel_types: usize,

        /// Maximum hop depth to build a PathIndex for.
        #[arg(long, default_value_t = 3)]
        index_depth: usize,

        /// AxQL query mode: `path`, `star`, or `plus`.
        #[arg(long, default_value = "path")]
        mode: String,

        /// Path length for `mode=path`.
        #[arg(long, default_value_t = 3)]
        path_len: usize,

        /// Max rows per AxQL query (limits work; large values can be expensive for `star`).
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Number of queries to run.
        #[arg(long, default_value_t = 50_000)]
        queries: usize,

        /// RNG seed (deterministic).
        #[arg(long, default_value_t = 1)]
        seed: u64,
    },

    /// Scenario-based (typed) synthetic ingestion + `.axpd` roundtrip + workload.
    ///
    /// This is intended to approximate “real model” structure better than a
    /// uniform random graph.
    Scenario {
        /// Scenario name (try: proto_api | proto_api_business | enterprise_large_api | economic_flows | machinist_learning | schema_evolution | social_network | supply_chain)
        #[arg(long, default_value = "proto_api")]
        scenario: String,

        /// Scenario scale (roughly “number of repeated bundles”, scenario-specific).
        #[arg(long, default_value_t = 10_000)]
        scale: usize,

        /// Maximum hop depth to build a PathIndex for.
        #[arg(long, default_value_t = 3)]
        index_depth: usize,

        /// RNG seed (deterministic).
        #[arg(long, default_value_t = 1)]
        seed: u64,

        /// Number of `follow_path` queries to run in the workload.
        #[arg(long, default_value_t = 100_000)]
        path_queries: usize,

        /// Number of AxQL queries to run in the workload.
        #[arg(long, default_value_t = 10_000)]
        axql_queries: usize,

        /// Max rows per AxQL query.
        #[arg(long, default_value_t = 25)]
        axql_limit: usize,

        /// Persist the generated database to `.axpd`.
        #[arg(long)]
        out_axpd: Option<PathBuf>,

        /// Write a JSON performance report to this path.
        #[arg(long)]
        out_json: Option<PathBuf>,
    },

    /// Synthetic cache/index workload (fact/text caches + path LRU).
    Indexes {
        /// Number of entities to create.
        #[arg(long, default_value_t = 100_000)]
        entities: usize,

        /// Outgoing edges per entity.
        #[arg(long, default_value_t = 8)]
        edges_per_entity: usize,

        /// Number of relation types to use (chosen uniformly at random).
        #[arg(long, default_value_t = 8)]
        rel_types: usize,

        /// Maximum hop depth to build a PathIndex for.
        #[arg(long, default_value_t = 2)]
        index_depth: usize,

        /// Path length for queries (use > index_depth to exercise LRU).
        #[arg(long, default_value_t = 4)]
        path_len: usize,

        /// Number of path queries to run (warm).
        #[arg(long, default_value_t = 20_000)]
        path_queries: usize,

        /// Number of fact-cache queries to run (warm).
        #[arg(long, default_value_t = 20_000)]
        fact_queries: usize,

        /// Number of text-cache queries to run (warm).
        #[arg(long, default_value_t = 20_000)]
        text_queries: usize,

        /// LRU capacity (number of cached path signatures).
        #[arg(long, default_value_t = 256)]
        lru_capacity: usize,

        /// Enable async updates for the path LRU.
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        lru_async: bool,

        /// Async queue size for LRU updates.
        #[arg(long, default_value_t = 1024)]
        lru_queue: usize,

        /// Index build mode: `async` (default) or `sync`.
        #[arg(long, default_value = "async")]
        index_mode: String,

        /// Wait up to N seconds for async indexes to finish (used by --verify).
        #[arg(long, default_value_t = 20)]
        async_wait_secs: u64,

        /// Perform consistency checks between fallback and indexed results.
        #[arg(long, default_value_t = false)]
        verify: bool,

        /// Add N new entities after warmup to exercise invalidation paths.
        #[arg(long, default_value_t = 0)]
        mutations: usize,

        /// RNG seed (deterministic).
        #[arg(long, default_value_t = 1)]
        seed: u64,

        /// Write a JSON performance report to this path.
        #[arg(long)]
        out_json: Option<PathBuf>,
    },

    /// World model MPC/eval harness (untrusted; evidence-plane rollouts).
    WorldModel {
        /// Input `.axi` (preferred for eval) or `.axpd` snapshot.
        #[arg(long)]
        input: PathBuf,

        /// World model plugin executable (speaks `axiograph_world_model_v1`).
        #[arg(long)]
        world_model_plugin: Option<PathBuf>,

        /// Extra args for `--world-model-plugin` (repeatable).
        #[arg(long)]
        world_model_plugin_arg: Vec<String>,

        /// Use stub world model backend (emits no proposals).
        #[arg(long)]
        world_model_stub: bool,

        /// Optional world model model name (provenance only).
        #[arg(long)]
        world_model_model: Option<String>,

        /// MPC horizon steps.
        #[arg(long, default_value_t = 3)]
        horizon_steps: usize,

        /// Number of rollouts per MPC step (best cost is chosen).
        #[arg(long, default_value_t = 1)]
        rollouts: usize,

        /// Max proposals to keep per rollout (0 = no cap).
        #[arg(long, default_value_t = 50)]
        max_new_proposals: usize,

        /// Guardrail profile: off|fast|strict.
        #[arg(long, default_value = "fast")]
        guardrail_profile: String,

        /// Guardrail plane: meta|data|both.
        #[arg(long, default_value = "both")]
        guardrail_plane: String,

        /// Override guardrail weights (repeatable): key=value.
        #[arg(long)]
        guardrail_weight: Vec<String>,

        /// Task cost terms (repeatable): name=value[:weight[:unit]].
        #[arg(long)]
        task_cost: Vec<String>,

        /// JEPA export: instance filter (only for `.axi` inputs).
        #[arg(long)]
        export_instance: Option<String>,

        /// JEPA export: max items (0 = no cap).
        #[arg(long, default_value_t = 0)]
        export_max_items: usize,

        /// JEPA export: mask fields per tuple.
        #[arg(long, default_value_t = 1)]
        export_mask_fields: usize,

        /// JEPA export: RNG seed.
        #[arg(long, default_value_t = 1)]
        export_seed: u64,

        /// Holdout fraction for eval (only for `.axi` inputs).
        #[arg(long, default_value_t = 0.0)]
        holdout_frac: f64,

        /// Holdout max count (only for `.axi` inputs).
        #[arg(long, default_value_t = 0)]
        holdout_max: usize,

        /// RNG seed (deterministic).
        #[arg(long, default_value_t = 1)]
        seed: u64,

        /// Write a JSON report to this path.
        #[arg(long)]
        out_json: Option<PathBuf>,
    },
}

pub fn cmd_perf(command: PerfCommands) -> Result<()> {
    match command {
        PerfCommands::Pathdb {
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            path_len,
            queries,
            seed,
            out_axpd,
            out_axi,
        } => cmd_perf_pathdb(
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            path_len,
            queries,
            seed,
            out_axpd.as_ref(),
            out_axi.as_ref(),
        ),
        PerfCommands::Axql {
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            mode,
            path_len,
            limit,
            queries,
            seed,
        } => cmd_perf_axql(
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            &mode,
            path_len,
            limit,
            queries,
            seed,
        ),
        PerfCommands::Scenario {
            scenario,
            scale,
            index_depth,
            seed,
            path_queries,
            axql_queries,
            axql_limit,
            out_axpd,
            out_json,
        } => cmd_perf_scenario(
            &scenario,
            scale,
            index_depth,
            seed,
            path_queries,
            axql_queries,
            axql_limit,
            out_axpd.as_ref(),
            out_json.as_ref(),
        ),
        PerfCommands::Indexes {
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            path_len,
            path_queries,
            fact_queries,
            text_queries,
            lru_capacity,
            lru_async,
            lru_queue,
            index_mode,
            async_wait_secs,
            verify,
            mutations,
            seed,
            out_json,
        } => cmd_perf_indexes(
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            path_len,
            path_queries,
            fact_queries,
            text_queries,
            lru_capacity,
            lru_async,
            lru_queue,
            &index_mode,
            async_wait_secs,
            verify,
            mutations,
            seed,
            out_json.as_ref(),
        ),
        PerfCommands::WorldModel {
            input,
            world_model_plugin,
            world_model_plugin_arg,
            world_model_stub,
            world_model_model,
            horizon_steps,
            rollouts,
            max_new_proposals,
            guardrail_profile,
            guardrail_plane,
            guardrail_weight,
            task_cost,
            export_instance,
            export_max_items,
            export_mask_fields,
            export_seed,
            holdout_frac,
            holdout_max,
            seed,
            out_json,
        } => cmd_perf_world_model(
            &input,
            world_model_plugin.as_ref(),
            &world_model_plugin_arg,
            world_model_stub,
            world_model_model.as_deref(),
            horizon_steps,
            rollouts,
            max_new_proposals,
            &guardrail_profile,
            &guardrail_plane,
            &guardrail_weight,
            &task_cost,
            export_instance.as_deref(),
            export_max_items,
            export_mask_fields,
            export_seed,
            holdout_frac,
            holdout_max,
            seed,
            out_json.as_ref(),
        ),
    }
}

fn cmd_perf_pathdb(
    entities: usize,
    edges_per_entity: usize,
    rel_types: usize,
    index_depth: usize,
    path_len: usize,
    queries: usize,
    seed: u64,
    out_axpd: Option<&PathBuf>,
    out_axi: Option<&PathBuf>,
) -> Result<()> {
    if path_len == 0 {
        return Err(anyhow!("--path-len must be > 0"));
    }

    println!("perf/pathdb");
    println!(
        "  entities={} edges_per_entity={} rel_types={} index_depth={} path_len={} queries={} seed={}",
        entities, edges_per_entity, rel_types, index_depth, path_len, queries, seed
    );

    let ingest = crate::synthetic_pathdb::build_synthetic_pathdb_ingest(
        entities,
        edges_per_entity,
        rel_types,
        index_depth,
        seed,
    )?;
    let crate::synthetic_pathdb::SyntheticPathDbIngest {
        mut db,
        relation_type_names,
        entity_time,
        relation_time,
        edge_count,
    } = ingest;

    // ---------------------------------------------------------------------
    // Build indexes.
    // ---------------------------------------------------------------------
    let start = Instant::now();
    db.build_indexes();
    let index_time = start.elapsed();

    // ---------------------------------------------------------------------
    // Run queries.
    // ---------------------------------------------------------------------
    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed.wrapping_add(1));
    let start = Instant::now();
    let mut total_hits: u64 = 0;
    for _ in 0..queries {
        let start_id = rng.gen_range_usize(entities) as u32;
        let mut path: Vec<&str> = Vec::with_capacity(path_len);
        for _ in 0..path_len {
            let rel = &relation_type_names[rng.gen_range_usize(rel_types)];
            path.push(rel.as_str());
        }
        let hits = db.follow_path(start_id, &path);
        total_hits = total_hits.wrapping_add(hits.len());
    }
    let query_time = start.elapsed();

    // ---------------------------------------------------------------------
    // Optional persistence.
    // ---------------------------------------------------------------------
    if let Some(out) = out_axpd {
        let start = Instant::now();
        let bytes = db.to_bytes()?;
        fs::write(out, bytes)?;
        println!("  wrote_axpd={} ({:?})", out.display(), start.elapsed());
    }

    if let Some(out) = out_axi {
        let start = Instant::now();
        let axi = axiograph_pathdb::axi_export::export_pathdb_to_axi_v1(&db)?;
        fs::write(out, axi)?;
        println!("  wrote_axi={} ({:?})", out.display(), start.elapsed());
    }

    // ---------------------------------------------------------------------
    // Report.
    // ---------------------------------------------------------------------
    println!(
        "  ingest_entities={:?} ({:.1} entities/sec)",
        entity_time,
        rate(entities, entity_time)
    );
    println!(
        "  ingest_relations={:?} ({:.1} edges/sec)",
        relation_time,
        rate(edge_count, relation_time)
    );
    println!("  build_indexes={:?}", index_time);
    println!(
        "  queries={:?} ({:.1} queries/sec)",
        query_time,
        rate(queries, query_time)
    );
    println!("  total_hits={total_hits}");

    Ok(())
}

#[derive(Debug, Serialize, Clone)]
struct GuardrailSummaryOut {
    total_cost: f64,
    error_count: usize,
    warning_count: usize,
    info_count: usize,
    axi_fact_errors: usize,
    rewrite_rule_errors: usize,
    context_errors: usize,
    modal_errors: usize,
}

#[derive(Debug, Serialize, Clone, Default)]
struct PrecisionRecallOut {
    tp: usize,
    fp: usize,
    fn_count: usize,
    precision: Option<f64>,
    recall: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
struct WorldModelPerfStepV1 {
    step: usize,
    rollouts: usize,
    proposals: usize,
    guardrail_before: GuardrailSummaryOut,
    guardrail_after: GuardrailSummaryOut,
    guardrail_delta: f64,
    task_cost_total: f64,
    total_cost: f64,
    validation_ok: bool,
    validation_errors: usize,
    precision_recall: Option<PrecisionRecallOut>,
}

#[derive(Debug, Serialize)]
struct WorldModelPerfReportV1 {
    version: String,
    input: String,
    horizon_steps: usize,
    rollouts: usize,
    max_new_proposals: usize,
    guardrail_profile: String,
    guardrail_plane: String,
    guardrail_weights: crate::world_model::GuardrailCostWeightsV1,
    task_costs: Vec<crate::world_model::WorldModelTaskCostV1>,
    task_cost_total: f64,
    holdout_count: usize,
    steps: Vec<WorldModelPerfStepV1>,
}

fn guardrail_summary(report: &crate::world_model::GuardrailCostReportV1) -> GuardrailSummaryOut {
    GuardrailSummaryOut {
        total_cost: report.summary.total_cost,
        error_count: report.quality.error_count,
        warning_count: report.quality.warning_count,
        info_count: report.quality.info_count,
        axi_fact_errors: report.checked.axi_fact_errors,
        rewrite_rule_errors: report.checked.rewrite_rule_errors,
        context_errors: report.checked.context_errors,
        modal_errors: report.checked.modal_errors,
    }
}

fn proposals_digest(file: &axiograph_ingest_docs::ProposalsFileV1) -> Result<String> {
    let bytes = serde_json::to_vec(file)
        .map_err(|e| anyhow!("failed to serialize proposals for digest: {e}"))?;
    Ok(axiograph_dsl::digest::fnv1a64_digest_bytes(&bytes))
}

fn apply_proposals(db: &mut PathDB, proposals: &axiograph_ingest_docs::ProposalsFileV1) -> Result<()> {
    let digest = proposals_digest(proposals)?;
    let _summary =
        crate::proposals_import::import_proposals_file_into_pathdb(db, proposals, &digest)?;
    Ok(())
}

fn clone_db(db: &PathDB) -> Result<PathDB> {
    let bytes = db.to_bytes()?;
    Ok(PathDB::from_bytes(&bytes)?)
}

fn infer_binary_endpoint_fields(
    fields: &[axiograph_dsl::schema_v1::FieldDeclV1],
) -> Option<(String, String)> {
    let names: Vec<&str> = fields.iter().map(|f| f.field.as_str()).collect();
    if names.contains(&"from") && names.contains(&"to") {
        return Some(("from".to_string(), "to".to_string()));
    }
    if names.contains(&"source") && names.contains(&"target") {
        return Some(("source".to_string(), "target".to_string()));
    }
    if names.contains(&"lhs") && names.contains(&"rhs") {
        return Some(("lhs".to_string(), "rhs".to_string()));
    }
    if names.contains(&"child") && names.contains(&"parent") {
        return Some(("child".to_string(), "parent".to_string()));
    }
    if fields.len() >= 2 {
        return Some((
            fields[0].field.clone(),
            fields[1].field.clone(),
        ));
    }
    None
}

fn build_holdout_module(
    module: &axiograph_dsl::schema_v1::SchemaV1Module,
    holdout_frac: f64,
    holdout_max: usize,
    seed: u64,
) -> Result<(axiograph_dsl::schema_v1::SchemaV1Module, HashSet<String>)> {
    if holdout_frac <= 0.0 && holdout_max == 0 {
        return Ok((module.clone(), HashSet::new()));
    }

    let mut rel_fields: HashMap<(String, String), (String, String)> = HashMap::new();
    for schema in &module.schemas {
        for rel in &schema.relations {
            if let Some((src, dst)) = infer_binary_endpoint_fields(&rel.fields) {
                rel_fields.insert((schema.name.clone(), rel.name.clone()), (src, dst));
            }
        }
    }

    let mut candidates: Vec<(String, usize, usize, usize)> = Vec::new();
    for (inst_idx, inst) in module.instances.iter().enumerate() {
        for (assign_idx, assign) in inst.assignments.iter().enumerate() {
            let Some((src_field, dst_field)) =
                rel_fields.get(&(inst.schema.clone(), assign.name.clone()))
            else {
                continue;
            };
            for (item_idx, item) in assign.value.items.iter().enumerate() {
                let axiograph_dsl::schema_v1::SetItemV1::Tuple { fields } = item else {
                    continue;
                };
                let mut map: HashMap<&str, &str> = HashMap::new();
                for (k, v) in fields {
                    map.insert(k.as_str(), v.as_str());
                }
                let Some(src_val) = map.get(src_field.as_str()) else {
                    continue;
                };
                let Some(dst_val) = map.get(dst_field.as_str()) else {
                    continue;
                };
                let key = format!("{}|{}|{}", assign.name, src_val, dst_val);
                candidates.push((key, inst_idx, assign_idx, item_idx));
            }
        }
    }

    let mut holdout_count =
        ((candidates.len() as f64) * holdout_frac).round() as usize;
    if holdout_max > 0 {
        holdout_count = holdout_count.min(holdout_max);
    }
    holdout_count = holdout_count.min(candidates.len());
    if holdout_count == 0 {
        return Ok((module.clone(), HashSet::new()));
    }

    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed);
    for i in 0..holdout_count {
        let j = i + rng.gen_range_usize(candidates.len() - i);
        candidates.swap(i, j);
    }

    let mut remove: HashSet<(usize, usize, usize)> = HashSet::new();
    let mut heldout: HashSet<String> = HashSet::new();
    for (key, inst_idx, assign_idx, item_idx) in candidates.into_iter().take(holdout_count) {
        remove.insert((inst_idx, assign_idx, item_idx));
        heldout.insert(key);
    }

    let mut out = module.clone();
    for (inst_idx, inst) in out.instances.iter_mut().enumerate() {
        for (assign_idx, assign) in inst.assignments.iter_mut().enumerate() {
            let mut kept: Vec<axiograph_dsl::schema_v1::SetItemV1> = Vec::new();
            for (item_idx, item) in assign.value.items.iter().enumerate() {
                if remove.contains(&(inst_idx, assign_idx, item_idx)) {
                    continue;
                }
                kept.push(item.clone());
            }
            assign.value.items = kept;
        }
    }

    Ok((out, heldout))
}

fn proposal_relation_keys(file: &axiograph_ingest_docs::ProposalsFileV1) -> HashSet<String> {
    let mut out = HashSet::new();
    for p in &file.proposals {
        if let axiograph_ingest_docs::ProposalV1::Relation {
            rel_type, source, target, ..
        } = p
        {
            out.insert(format!("{}|{}|{}", rel_type, source, target));
        }
    }
    out
}

fn precision_recall(
    proposals: &axiograph_ingest_docs::ProposalsFileV1,
    truth: &HashSet<String>,
) -> PrecisionRecallOut {
    let keys = proposal_relation_keys(proposals);
    let mut tp = 0usize;
    for k in &keys {
        if truth.contains(k) {
            tp += 1;
        }
    }
    let fp = keys.len().saturating_sub(tp);
    let fn_count = truth.len().saturating_sub(tp);
    let precision = if keys.is_empty() {
        None
    } else {
        Some(tp as f64 / keys.len() as f64)
    };
    let recall = if truth.is_empty() {
        None
    } else {
        Some(tp as f64 / truth.len() as f64)
    };
    PrecisionRecallOut {
        tp,
        fp,
        fn_count,
        precision,
        recall,
    }
}

fn cmd_perf_world_model(
    input: &PathBuf,
    world_model_plugin: Option<&PathBuf>,
    world_model_plugin_arg: &[String],
    world_model_stub: bool,
    world_model_model: Option<&str>,
    horizon_steps: usize,
    rollouts: usize,
    max_new_proposals: usize,
    guardrail_profile: &str,
    guardrail_plane: &str,
    guardrail_weight: &[String],
    task_cost: &[String],
    export_instance: Option<&str>,
    export_max_items: usize,
    export_mask_fields: usize,
    export_seed: u64,
    holdout_frac: f64,
    holdout_max: usize,
    seed: u64,
    out_json: Option<&PathBuf>,
) -> Result<()> {
    if world_model_stub && world_model_plugin.is_some() {
        return Err(anyhow!(
            "perf world-model: choose at most one backend: --world-model-stub or --world-model-plugin"
        ));
    }
    if !world_model_stub && world_model_plugin.is_none() {
        return Err(anyhow!(
            "perf world-model: missing backend (use --world-model-plugin or --world-model-stub)"
        ));
    }

    let guardrail_profile = guardrail_profile.trim().to_ascii_lowercase();
    let guardrail_plane = guardrail_plane.trim().to_ascii_lowercase();
    let guardrail_weights = if guardrail_weight.is_empty() {
        crate::world_model::GuardrailCostWeightsV1::defaults()
    } else {
        crate::world_model::parse_guardrail_weights(guardrail_weight)?
    };
    let task_costs = crate::world_model::parse_task_costs(task_cost)?;
    let task_cost_total: f64 = task_costs.iter().map(|t| t.value * t.weight).sum();

    let ext = input.extension().and_then(|s| s.to_str()).unwrap_or("");
    let mut heldout: HashSet<String> = HashSet::new();
    let mut axi_text: Option<String> = None;
    let mut axi_digest: Option<String> = None;
    let mut jepa_export: Option<crate::world_model::JepaExportFileV1> = None;

    let mut db = if ext.eq_ignore_ascii_case("axi") {
        let text = fs::read_to_string(input)?;
        axi_digest = Some(axiograph_dsl::digest::axi_digest_v1(&text));
        axi_text = Some(text.clone());
        let module = axiograph_dsl::axi_v1::parse_axi_v1(&text)?;
        let (module, holdout_set) =
            build_holdout_module(&module, holdout_frac, holdout_max, seed)?;
        heldout = holdout_set;

        let opts = crate::world_model::JepaExportOptions {
            instance_filter: export_instance.map(|s| s.to_string()),
            max_items: export_max_items,
            mask_fields: export_mask_fields,
            seed: export_seed,
        };
        jepa_export = Some(crate::world_model::build_jepa_export_from_axi_text(&text, &opts)?);

        let mut db = PathDB::new();
        let _summary =
            axiograph_pathdb::axi_module_import::import_axi_schema_v1_module_into_pathdb(
                &mut db, &module,
            )?;
        db
    } else if ext.eq_ignore_ascii_case("axpd") {
        let bytes = fs::read(input)?;
        PathDB::from_bytes(&bytes)?
    } else {
        return Err(anyhow!(
            "perf world-model: unsupported input `{}` (expected .axi or .axpd)",
            input.display()
        ));
    };

    let mut wm = crate::world_model::WorldModelState::default();
    if world_model_stub {
        wm.backend = crate::world_model::WorldModelBackend::Stub;
    } else if let Some(plugin) = world_model_plugin {
        wm.backend = crate::world_model::WorldModelBackend::Command {
            program: plugin.clone(),
            args: world_model_plugin_arg.to_vec(),
        };
    }
    wm.model = world_model_model.map(|s| s.to_string());

    let mut steps: Vec<WorldModelPerfStepV1> = Vec::new();

    for step in 0..horizon_steps {
        let guardrail_before = crate::world_model::compute_guardrail_costs(
            &db,
            &format!("perf_world_model:step{step}"),
            &guardrail_profile,
            &guardrail_plane,
            &guardrail_weights,
        )?;

        let mut best: Option<(
            axiograph_ingest_docs::ProposalsFileV1,
            crate::world_model::GuardrailCostReportV1,
            PrecisionRecallOut,
            bool,
            usize,
            f64,
        )> = None;

        for rollout in 0..rollouts {
            let mut input = crate::world_model::WorldModelInputV1::default();
            input.axi_digest_v1 = axi_digest.clone();
            input.axi_module_text = axi_text.clone();
            input.export = jepa_export.clone();
            input.guardrail = Some(guardrail_before.clone());
            input.notes.push(format!("source=perf_world_model step={step} rollout={rollout}"));

            let mut options = crate::world_model::WorldModelOptionsV1::default();
            options.max_new_proposals = max_new_proposals;
            options.seed = Some(seed.wrapping_add((step as u64) * 1_000 + rollout as u64));
            options.task_costs = task_costs.clone();
            options.horizon_steps = Some(horizon_steps);

            let req = crate::world_model::make_world_model_request(input, options);
            let mut response = wm.propose(&req)?;
            if let Some(err) = response.error.take() {
                return Err(anyhow!("world model error: {err}"));
            }

            let provenance = crate::world_model::WorldModelProvenance {
                trace_id: response.trace_id.clone(),
                backend: wm.backend_label(),
                model: wm.model.clone(),
                axi_digest_v1: axi_digest.clone(),
                guardrail_total_cost: Some(guardrail_before.summary.total_cost),
                guardrail_profile: if guardrail_profile == "off" {
                    None
                } else {
                    Some(guardrail_profile.clone())
                },
                guardrail_plane: if guardrail_profile == "off" {
                    None
                } else {
                    Some(guardrail_plane.clone())
                },
            };
            let mut proposals =
                crate::world_model::apply_world_model_provenance(response.proposals, &provenance);
            if max_new_proposals > 0 && proposals.proposals.len() > max_new_proposals {
                proposals.proposals.truncate(max_new_proposals);
            }

            let mut candidate = clone_db(&db)?;
            apply_proposals(&mut candidate, &proposals)?;
            let guardrail_after = crate::world_model::compute_guardrail_costs(
                &candidate,
                &format!("perf_world_model:step{step}:rollout{rollout}"),
                &guardrail_profile,
                &guardrail_plane,
                &guardrail_weights,
            )?;

            let validation = crate::proposals_validate::validate_proposals_v1(
                &db,
                &proposals,
                "fast",
                "both",
            )?;
            let validation_ok = validation.ok;
            let validation_errors = validation.quality_delta.summary.error_count;

            let pr = if !heldout.is_empty() {
                precision_recall(&proposals, &heldout)
            } else {
                PrecisionRecallOut::default()
            };

            let total_cost = guardrail_after.summary.total_cost + task_cost_total;

            let candidate_tuple = (
                proposals,
                guardrail_after,
                pr,
                validation_ok,
                validation_errors,
                total_cost,
            );

            let better = match best.as_ref() {
                None => true,
                Some((_, _, _, _, _, best_cost)) => total_cost < *best_cost,
            };
            if better {
                best = Some(candidate_tuple);
            }
        }

        let (proposals, guardrail_after, pr, validation_ok, validation_errors, total_cost) =
            best.ok_or_else(|| anyhow!("perf world-model: no rollout produced proposals"))?;

        apply_proposals(&mut db, &proposals)?;

        let step_report = WorldModelPerfStepV1 {
            step,
            rollouts,
            proposals: proposals.proposals.len(),
            guardrail_before: guardrail_summary(&guardrail_before),
            guardrail_after: guardrail_summary(&guardrail_after),
            guardrail_delta: guardrail_after.summary.total_cost - guardrail_before.summary.total_cost,
            task_cost_total,
            total_cost,
            validation_ok,
            validation_errors,
            precision_recall: if heldout.is_empty() {
                None
            } else {
                Some(pr)
            },
        };
        steps.push(step_report);
    }

    let report = WorldModelPerfReportV1 {
        version: "perf_world_model_v1".to_string(),
        input: input.display().to_string(),
        horizon_steps,
        rollouts,
        max_new_proposals,
        guardrail_profile,
        guardrail_plane,
        guardrail_weights,
        task_costs,
        task_cost_total,
        holdout_count: heldout.len(),
        steps,
    };

    let json = serde_json::to_string_pretty(&report)?;
    if let Some(path) = out_json {
        fs::write(path, &json)?;
        println!("wrote {}", path.display());
    } else {
        println!("{json}");
    }
    Ok(())
}

fn cmd_perf_axql(
    entities: usize,
    edges_per_entity: usize,
    rel_types: usize,
    index_depth: usize,
    mode: &str,
    path_len: usize,
    limit: usize,
    queries: usize,
    seed: u64,
) -> Result<()> {
    if mode == "path" && path_len == 0 {
        return Err(anyhow!("--path-len must be > 0 when --mode=path"));
    }

    println!("perf/axql");
    println!(
        "  entities={} edges_per_entity={} rel_types={} index_depth={} mode={} path_len={} limit={} queries={} seed={}",
        entities, edges_per_entity, rel_types, index_depth, mode, path_len, limit, queries, seed
    );

    let ingest = crate::synthetic_pathdb::build_synthetic_pathdb_ingest(
        entities,
        edges_per_entity,
        rel_types,
        index_depth,
        seed,
    )?;
    let crate::synthetic_pathdb::SyntheticPathDbIngest {
        mut db,
        relation_type_names,
        entity_time,
        relation_time,
        edge_count,
    } = ingest;

    let start = Instant::now();
    db.build_indexes();
    let index_time = start.elapsed();

    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed.wrapping_add(1));
    let start = Instant::now();

    let mut total_rows: u64 = 0;

    for _ in 0..queries {
        let start_id = rng.gen_range_usize(entities) as u32;

        let path = match mode {
            "path" => {
                let mut rels: Vec<String> = Vec::with_capacity(path_len);
                for _ in 0..path_len {
                    rels.push(
                        relation_type_names[rng.gen_range_usize(rel_types)]
                            .as_str()
                            .to_string(),
                    );
                }
                crate::axql::AxqlPathExpr::seq(rels)
            }
            "star" => {
                let rel = relation_type_names[rng.gen_range_usize(rel_types)]
                    .as_str()
                    .to_string();
                crate::axql::AxqlPathExpr::star(rel)
            }
            "plus" => {
                let rel = relation_type_names[rng.gen_range_usize(rel_types)]
                    .as_str()
                    .to_string();
                crate::axql::AxqlPathExpr::plus(rel)
            }
            other => {
                return Err(anyhow!(
                    "unknown --mode `{other}` (expected: path, star, plus)"
                ))
            }
        };

        let query = crate::axql::AxqlQuery {
            select_vars: vec!["?y".to_string()],
            disjuncts: vec![vec![crate::axql::AxqlAtom::Edge {
                left: crate::axql::AxqlTerm::Const(start_id),
                path,
                right: crate::axql::AxqlTerm::Var("?y".to_string()),
            }]],
            limit,
            contexts: Vec::new(),
            max_hops: None,
            min_confidence: None,
        };

        let res = crate::axql::execute_axql_query(&db, &query)?;
        total_rows = total_rows.wrapping_add(res.rows.len() as u64);
    }

    let query_time = start.elapsed();

    println!(
        "  ingest_entities={:?} ({:.1} entities/sec)",
        entity_time,
        rate(entities, entity_time)
    );
    println!(
        "  ingest_relations={:?} ({:.1} edges/sec)",
        relation_time,
        rate(edge_count, relation_time)
    );
    println!("  build_indexes={:?}", index_time);
    println!(
        "  queries={:?} ({:.1} queries/sec)",
        query_time,
        rate(queries, query_time)
    );
    println!("  total_rows={total_rows}");

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct PerfIndexesReport {
    entities: usize,
    edges_per_entity: usize,
    rel_types: usize,
    index_depth: usize,
    path_len: usize,
    path_queries: usize,
    fact_queries: usize,
    text_queries: usize,
    seed: u64,
    ingest_entities_secs: f64,
    ingest_relations_secs: f64,
    build_indexes_secs: f64,
    path_cold_secs: f64,
    path_warm_secs: f64,
    path_total_hits: u64,
    fact_cold_secs: f64,
    fact_warm_secs: f64,
    fact_total_hits: u64,
    text_cold_secs: f64,
    text_warm_secs: f64,
    text_total_hits: u64,
    mutations: usize,
    mutation_relations_added: usize,
}

#[derive(Debug)]
struct PerfIndexesWorkload {
    path_cold_time: Duration,
    path_warm_time: Duration,
    path_total_hits: u64,
    fact_cold_time: Duration,
    fact_warm_time: Duration,
    fact_total_hits: u64,
    text_cold_time: Duration,
    text_warm_time: Duration,
    text_total_hits: u64,
    fact_cold_hits: RoaringBitmap,
    fact_warm_hits: RoaringBitmap,
    text_cold_hits: RoaringBitmap,
    text_warm_hits: RoaringBitmap,
    path_cold_hits: RoaringBitmap,
    path_warm_hits: Option<RoaringBitmap>,
}

fn cmd_perf_indexes(
    entities: usize,
    edges_per_entity: usize,
    rel_types: usize,
    index_depth: usize,
    path_len: usize,
    path_queries: usize,
    fact_queries: usize,
    text_queries: usize,
    lru_capacity: usize,
    lru_async: bool,
    lru_queue: usize,
    index_mode: &str,
    async_wait_secs: u64,
    verify: bool,
    mutations: usize,
    seed: u64,
    out_json: Option<&PathBuf>,
) -> Result<()> {
    if entities == 0 {
        return Err(anyhow!("--entities must be > 0"));
    }
    if rel_types == 0 {
        return Err(anyhow!("--rel-types must be > 0"));
    }
    if path_len == 0 {
        return Err(anyhow!("--path-len must be > 0"));
    }

    let async_indexes = match index_mode.to_ascii_lowercase().as_str() {
        "async" => true,
        "sync" => false,
        other => return Err(anyhow!("unknown --index-mode `{other}` (expected: async, sync)")),
    };

    println!("perf/indexes");
    println!(
        "  entities={} edges_per_entity={} rel_types={} index_depth={} path_len={} path_queries={} fact_queries={} text_queries={} seed={}",
        entities, edges_per_entity, rel_types, index_depth, path_len, path_queries, fact_queries, text_queries, seed
    );
    println!(
        "  lru_capacity={} lru_async={} lru_queue={} index_mode={} async_wait_secs={} verify={} mutations={}",
        lru_capacity, lru_async, lru_queue, if async_indexes { "async" } else { "sync" }, async_wait_secs, verify, mutations
    );

    let relation_type_names: Vec<String> =
        (0..rel_types).map(|i| format!("rel_{i}")).collect();
    let schema_count = rel_types.clamp(1, 4);
    let schema_names: Vec<String> = (0..schema_count)
        .map(|i| format!("schema_{i}"))
        .collect();
    let tokens = [
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "kappa", "lambda",
    ];

    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed);
    let mut db = PathDB::new();
    db.path_index = axiograph_pathdb::PathIndex::new(index_depth);

    // ---------------------------------------------------------------------
    // Ingest entities (with fact + text attributes).
    // ---------------------------------------------------------------------
    let start = Instant::now();
    for i in 0..entities {
        let rel_name = &relation_type_names[i % rel_types];
        let schema_name = &schema_names[i % schema_count];
        let tok_a = tokens[i % tokens.len()];
        let tok_b = tokens[(i * 7 + 3) % tokens.len()];
        let name = format!("node_{i} {tok_a} {tok_b}");
        db.add_entity(
            "Fact",
            vec![
                (ATTR_AXI_RELATION, rel_name.as_str()),
                (ATTR_AXI_SCHEMA, schema_name.as_str()),
                ("name", name.as_str()),
            ],
        );
    }
    let entity_time = start.elapsed();

    // ---------------------------------------------------------------------
    // Ingest relations.
    // ---------------------------------------------------------------------
    let start = Instant::now();
    for source in 0..entities {
        let source_id = source as u32;
        for _ in 0..edges_per_entity {
            let target_id = rng.gen_range_usize(entities) as u32;
            let rel = &relation_type_names[rng.gen_range_usize(rel_types)];
            db.add_relation(rel, source_id, target_id, 0.9, Vec::new());
        }
    }
    let relation_time = start.elapsed();
    let edge_count = entities.saturating_mul(edges_per_entity);

    // ---------------------------------------------------------------------
    // Build indexes (PathIndex only).
    // ---------------------------------------------------------------------
    let start = Instant::now();
    if index_depth > 0 {
        db.build_indexes_with_depth(index_depth);
    }
    let index_time = start.elapsed();

    // ---------------------------------------------------------------------
    // Configure LRU + async sources.
    // ---------------------------------------------------------------------
    if lru_capacity > 0 {
        db.set_path_index_lru_capacity(lru_capacity);
        if lru_async {
            db.enable_path_index_lru_async(lru_queue);
        }
    }

    let (mut db, workload) = if async_indexes {
        let db = Arc::new(db);
        db.attach_async_index_source(Arc::downgrade(&db));
        let workload = run_index_workload(
            &db,
            &relation_type_names,
            &schema_names,
            tokens.as_slice(),
            entities,
            rel_types,
            index_depth,
            path_len,
            path_queries,
            fact_queries,
            text_queries,
            lru_capacity,
            async_wait_secs,
            seed,
        );
        if lru_async {
            let _ = db.path_index.flush_async();
        }
        match Arc::try_unwrap(db) {
            Ok(db) => (db, workload),
            Err(arc) => {
                eprintln!("warn: async indexing still active; cloning db for mutation stage");
                let bytes = arc.to_bytes()?;
                (PathDB::from_bytes(&bytes)?, workload)
            }
        }
    } else {
        let workload = run_index_workload(
            &db,
            &relation_type_names,
            &schema_names,
            tokens.as_slice(),
            entities,
            rel_types,
            index_depth,
            path_len,
            path_queries,
            fact_queries,
            text_queries,
            lru_capacity,
            async_wait_secs,
            seed,
        );
        (db, workload)
    };

    if verify {
        if workload.fact_cold_hits != workload.fact_warm_hits {
            return Err(anyhow!("fact cache mismatch: fallback != indexed"));
        }
        if workload.text_cold_hits != workload.text_warm_hits {
            return Err(anyhow!("text cache mismatch: fallback != indexed"));
        }
        if let Some(path_warm_hits) = workload.path_warm_hits.clone() {
            if workload.path_cold_hits != path_warm_hits {
                return Err(anyhow!("path LRU mismatch: fallback != cached"));
            }
        }
    }

    // ---------------------------------------------------------------------
    // Mutation checks (optional).
    // ---------------------------------------------------------------------
    let mut mutation_relations_added = 0usize;
    if mutations > 0 {
        let cold_token = tokens[0];
        let mut_ids: Vec<u32> = {
            let mut ids_by_rel: std::collections::HashMap<String, Vec<u32>> =
                std::collections::HashMap::new();
            let mut ids_by_schema_rel: std::collections::HashMap<(String, String), Vec<u32>> =
                std::collections::HashMap::new();
            let mut ids = Vec::with_capacity(mutations);
            for i in 0..mutations {
                let rel = &relation_type_names[i % rel_types];
                let schema = &schema_names[i % schema_count];
                let name = format!("node_mut_{i} {cold_token}");
                let id = db.add_entity(
                    "Fact",
                    vec![
                        (ATTR_AXI_RELATION, rel.as_str()),
                        (ATTR_AXI_SCHEMA, schema.as_str()),
                        ("name", name.as_str()),
                    ],
                );
                let source = (i % entities) as u32;
                db.add_relation(rel, source, id, 0.9, Vec::new());
                mutation_relations_added += 1;
                ids.push(id);
                ids_by_rel
                    .entry(rel.to_string())
                    .or_default()
                    .push(id);
                ids_by_schema_rel
                    .entry((schema.to_string(), rel.to_string()))
                    .or_default()
                    .push(id);
            }
            for (rel, ids_for_rel) in &ids_by_rel {
                let hits = db.fact_nodes_by_axi_relation(rel);
                for id in ids_for_rel {
                    if !hits.contains(*id) {
                        return Err(anyhow!(
                            "mutation: fact index missing new entity {id} for rel {rel}"
                        ));
                    }
                }
            }
            for ((schema, rel), ids_for_pair) in &ids_by_schema_rel {
                let hits = db.fact_nodes_by_axi_schema_relation(schema, rel);
                for id in ids_for_pair {
                    if !hits.contains(*id) {
                        return Err(anyhow!(
                            "mutation: fact index missing new entity {id} for schema {schema} rel {rel}"
                        ));
                    }
                }
            }
            ids
        };

        let hits = db.entities_with_attr_fts("name", cold_token);
        for id in &mut_ids {
            if !hits.contains(*id) {
                return Err(anyhow!("mutation: text index missing new entity {id}"));
            }
        }
    }

    // ---------------------------------------------------------------------
    // Report.
    // ---------------------------------------------------------------------
    println!(
        "  ingest_entities={:?} ({:.1} entities/sec)",
        entity_time,
        rate(entities, entity_time)
    );
    println!(
        "  ingest_relations={:?} ({:.1} edges/sec)",
        relation_time,
        rate(edge_count, relation_time)
    );
    println!("  build_indexes={:?}", index_time);
    println!(
        "  path_queries={:?} ({:.1} queries/sec)",
        workload.path_warm_time,
        rate(path_queries, workload.path_warm_time)
    );
    println!(
        "  fact_queries={:?} ({:.1} queries/sec)",
        workload.fact_warm_time,
        rate(fact_queries, workload.fact_warm_time)
    );
    println!(
        "  text_queries={:?} ({:.1} queries/sec)",
        workload.text_warm_time,
        rate(text_queries, workload.text_warm_time)
    );
    println!(
        "  cold_samples: path={:?} fact={:?} text={:?}",
        workload.path_cold_time, workload.fact_cold_time, workload.text_cold_time
    );
    println!(
        "  total_hits: path={} fact={} text={}",
        workload.path_total_hits, workload.fact_total_hits, workload.text_total_hits
    );

    if let Some(out) = out_json {
        let report = PerfIndexesReport {
            entities,
            edges_per_entity,
            rel_types,
            index_depth,
            path_len,
            path_queries,
            fact_queries,
            text_queries,
            seed,
            ingest_entities_secs: entity_time.as_secs_f64(),
            ingest_relations_secs: relation_time.as_secs_f64(),
            build_indexes_secs: index_time.as_secs_f64(),
            path_cold_secs: workload.path_cold_time.as_secs_f64(),
            path_warm_secs: workload.path_warm_time.as_secs_f64(),
            path_total_hits: workload.path_total_hits,
            fact_cold_secs: workload.fact_cold_time.as_secs_f64(),
            fact_warm_secs: workload.fact_warm_time.as_secs_f64(),
            fact_total_hits: workload.fact_total_hits,
            text_cold_secs: workload.text_cold_time.as_secs_f64(),
            text_warm_secs: workload.text_warm_time.as_secs_f64(),
            text_total_hits: workload.text_total_hits,
            mutations,
            mutation_relations_added,
        };
        fs::write(out, serde_json::to_string_pretty(&report)?)?;
        println!("  wrote_json={}", out.display());
    }

    Ok(())
}

fn rate(items: usize, dt: std::time::Duration) -> f64 {
    let secs = dt.as_secs_f64();
    if secs <= 0.0 {
        return f64::INFINITY;
    }
    (items as f64) / secs
}

fn time_call<R>(f: impl FnOnce() -> R) -> (Duration, R) {
    let start = Instant::now();
    let out = f();
    (start.elapsed(), out)
}

fn wait_for(mut f: impl FnMut() -> bool, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    false
}

fn path_sig(db: &PathDB, rels: &[&str]) -> PathSig {
    let mut ids = Vec::with_capacity(rels.len());
    for rel in rels {
        let id = db
            .interner
            .id_of(rel)
            .unwrap_or_else(|| panic!("missing relation {rel}"));
        ids.push(id);
    }
    PathSig::new(ids)
}

fn run_index_workload(
    db: &PathDB,
    relation_type_names: &[String],
    schema_names: &[String],
    tokens: &[&str],
    entities: usize,
    rel_types: usize,
    index_depth: usize,
    path_len: usize,
    path_queries: usize,
    fact_queries: usize,
    text_queries: usize,
    lru_capacity: usize,
    async_wait_secs: u64,
    seed: u64,
) -> PerfIndexesWorkload {
    let cold_rel = relation_type_names[0].as_str();
    let cold_token = tokens[0];
    let mut cold_path: Vec<&str> = Vec::with_capacity(path_len);
    for i in 0..path_len {
        cold_path.push(relation_type_names[i % rel_types].as_str());
    }

    let (path_cold_time, path_cold_hits) = time_call(|| db.follow_path(0, &cold_path));
    let (fact_cold_time, fact_cold_hits) = time_call(|| db.fact_nodes_by_axi_relation(cold_rel));
    let (text_cold_time, text_cold_hits) =
        time_call(|| db.entities_with_attr_fts("name", cold_token));

    let name_id = db.interner.id_of("name");
    let async_timeout = Duration::from_secs(async_wait_secs);
    if async_wait_secs > 0 {
        let _ = wait_for(
            || {
                let sidecar = db.snapshot_index_sidecar(None);
                let fact_ready = sidecar.fact_index.is_some();
                let text_ready = name_id
                    .and_then(|id| sidecar.text_indexes.get(&id))
                    .is_some();
                fact_ready && text_ready
            },
            async_timeout,
        );
    }

    if lru_capacity > 0 && path_len > index_depth {
        let sig = path_sig(db, &cold_path);
        let _ = wait_for(|| db.path_index_lru_contains(&sig), async_timeout);
    }

    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed.wrapping_add(1));

    let start = Instant::now();
    let mut path_total_hits: u64 = 0;
    for _ in 0..path_queries {
        let start_id = rng.gen_range_usize(entities) as u32;
        let mut path: Vec<&str> = Vec::with_capacity(path_len);
        for _ in 0..path_len {
            path.push(relation_type_names[rng.gen_range_usize(rel_types)].as_str());
        }
        let hits = db.follow_path(start_id, &path);
        path_total_hits = path_total_hits.wrapping_add(hits.len());
    }
    let path_warm_time = start.elapsed();

    let start = Instant::now();
    let mut fact_total_hits: u64 = 0;
    for _ in 0..fact_queries {
        let rel_idx = rng.gen_range_usize(rel_types);
        let rel = &relation_type_names[rel_idx];
        let hits = if rng.gen_range_usize(2) == 0 {
            db.fact_nodes_by_axi_relation(rel)
        } else {
            let schema = &schema_names[rel_idx % schema_names.len().max(1)];
            db.fact_nodes_by_axi_schema_relation(schema, rel)
        };
        fact_total_hits = fact_total_hits.wrapping_add(hits.len());
    }
    let fact_warm_time = start.elapsed();

    let start = Instant::now();
    let mut text_total_hits: u64 = 0;
    for _ in 0..text_queries {
        let tok_a = tokens[rng.gen_range_usize(tokens.len())];
        let tok_b = tokens[rng.gen_range_usize(tokens.len())];
        let query = if rng.gen_range_usize(2) == 0 {
            tok_a.to_string()
        } else {
            format!("{tok_a} {tok_b}")
        };
        let hits = db.entities_with_attr_fts("name", &query);
        text_total_hits = text_total_hits.wrapping_add(hits.len());
    }
    let text_warm_time = start.elapsed();

    let fact_warm_hits = db.fact_nodes_by_axi_relation(cold_rel);
    let text_warm_hits = db.entities_with_attr_fts("name", cold_token);
    let path_warm_hits = if lru_capacity > 0 && path_len > index_depth {
        Some(db.follow_path(0, &cold_path))
    } else {
        None
    };

    PerfIndexesWorkload {
        path_cold_time,
        path_warm_time,
        path_total_hits,
        fact_cold_time,
        fact_warm_time,
        fact_total_hits,
        text_cold_time,
        text_warm_time,
        text_total_hits,
        fact_cold_hits,
        fact_warm_hits,
        text_cold_hits,
        text_warm_hits,
        path_cold_hits,
        path_warm_hits,
    }
}

#[derive(Debug, Clone, Serialize)]
struct PerfScenarioWorkloadReport {
    path_queries: usize,
    path_total_hits: u64,
    path_time_secs: f64,
    axql_queries: usize,
    axql_total_rows: u64,
    axql_time_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
struct PerfScenarioReport {
    scenario: String,
    scale: usize,
    index_depth: usize,
    seed: u64,
    entities: usize,
    relations: usize,
    equivalence_pairs: usize,
    ingest_entities_secs: f64,
    ingest_relations_secs: f64,
    build_indexes_secs: f64,
    axpd_bytes: usize,
    axpd_serialize_secs: f64,
    axpd_write_secs: Option<f64>,
    axpd_read_secs: Option<f64>,
    axpd_load_secs: f64,
    workload_original: PerfScenarioWorkloadReport,
    workload_reloaded: PerfScenarioWorkloadReport,
}

fn cmd_perf_scenario(
    scenario: &str,
    scale: usize,
    index_depth: usize,
    seed: u64,
    path_queries: usize,
    axql_queries: usize,
    axql_limit: usize,
    out_axpd: Option<&PathBuf>,
    out_json: Option<&PathBuf>,
) -> Result<()> {
    if scale == 0 {
        return Err(anyhow!("--scale must be > 0"));
    }
    if index_depth == 0 {
        return Err(anyhow!("--index-depth must be > 0"));
    }

    println!("perf/scenario");
    println!(
        "  scenario={} scale={} index_depth={} seed={} path_queries={} axql_queries={} axql_limit={}",
        scenario, scale, index_depth, seed, path_queries, axql_queries, axql_limit
    );

    // ---------------------------------------------------------------------
    // Ingest scenario.
    // ---------------------------------------------------------------------
    let ingest =
        crate::synthetic_pathdb::build_scenario_pathdb_ingest(scenario, scale, index_depth, seed)?;
    let crate::synthetic_pathdb::SyntheticScenarioIngest {
        scenario_name,
        description,
        entity_type_names: _,
        relation_type_names,
        mut db,
        entity_time,
        relation_time,
        example_commands: _,
    } = ingest;
    println!("  scenario_name={scenario_name}");
    println!("  {description}");
    let entities = db.entities.len();
    let relations = db.relations.len();
    let equiv_pairs = count_equivalence_pairs(&db);

    // ---------------------------------------------------------------------
    // Build indexes.
    // ---------------------------------------------------------------------
    let start = Instant::now();
    db.build_indexes();
    let index_time = start.elapsed();

    // ---------------------------------------------------------------------
    // Run workload on the in-memory DB.
    // ---------------------------------------------------------------------
    let workload_original = run_scenario_workload(
        &scenario_name,
        &relation_type_names,
        &db,
        seed,
        path_queries,
        axql_queries,
        axql_limit,
        index_depth,
    )?;

    // ---------------------------------------------------------------------
    // `.axpd` roundtrip.
    // ---------------------------------------------------------------------
    let start = Instant::now();
    let bytes = db.to_bytes()?;
    let serialize_time = start.elapsed();

    let mut write_time = None;
    if let Some(out) = out_axpd {
        let start = Instant::now();
        fs::write(out, &bytes)?;
        write_time = Some(start.elapsed());
        println!(
            "  wrote_axpd={} ({} bytes, {:?})",
            out.display(),
            bytes.len(),
            write_time.expect("set above")
        );
    }

    let (read_time, load_bytes) = if let Some(out) = out_axpd {
        let start = Instant::now();
        let loaded = fs::read(out)?;
        (Some(start.elapsed()), loaded)
    } else {
        (None, bytes.clone())
    };

    let start = Instant::now();
    let db2 = axiograph_pathdb::PathDB::from_bytes(&load_bytes)?;
    let load_time = start.elapsed();

    // ---------------------------------------------------------------------
    // Workload after reload (this is the “PathDB snapshot is useful” story).
    // ---------------------------------------------------------------------
    let workload_reloaded = run_scenario_workload(
        &scenario_name,
        &relation_type_names,
        &db2,
        seed,
        path_queries,
        axql_queries,
        axql_limit,
        index_depth,
    )?;

    // ---------------------------------------------------------------------
    // Report.
    // ---------------------------------------------------------------------
    println!("  entities={entities} relations={relations} equivalence_pairs={equiv_pairs}");
    println!(
        "  ingest_entities={:?} ({:.1} entities/sec)",
        entity_time,
        rate(entities, entity_time)
    );
    println!(
        "  ingest_relations={:?} ({:.1} relations/sec)",
        relation_time,
        rate(relations, relation_time)
    );
    println!("  build_indexes={index_time:?}");
    println!(
        "  axpd_serialize={serialize_time:?} ({} bytes)",
        bytes.len()
    );
    if let Some(read_time) = read_time {
        println!("  axpd_read={read_time:?}");
    }
    println!("  axpd_load={load_time:?}");

    print_workload("workload_original", &workload_original);
    print_workload("workload_reloaded", &workload_reloaded);

    if let Some(out) = out_json {
        let report = PerfScenarioReport {
            scenario: scenario_name,
            scale,
            index_depth,
            seed,
            entities,
            relations,
            equivalence_pairs: equiv_pairs,
            ingest_entities_secs: entity_time.as_secs_f64(),
            ingest_relations_secs: relation_time.as_secs_f64(),
            build_indexes_secs: index_time.as_secs_f64(),
            axpd_bytes: bytes.len(),
            axpd_serialize_secs: serialize_time.as_secs_f64(),
            axpd_write_secs: write_time.map(|d| d.as_secs_f64()),
            axpd_read_secs: read_time.map(|d| d.as_secs_f64()),
            axpd_load_secs: load_time.as_secs_f64(),
            workload_original,
            workload_reloaded,
        };
        let json = serde_json::to_vec_pretty(&report)?;
        fs::write(out, json)?;
        println!("  wrote_json={} ", out.display());
    }

    Ok(())
}

fn count_equivalence_pairs(db: &axiograph_pathdb::PathDB) -> usize {
    let mut edges: usize = 0;
    for v in db.equivalences.values() {
        edges = edges.saturating_add(v.len());
    }
    // `add_equivalence` inserts symmetric links, so divide by 2.
    edges / 2
}

fn print_workload(label: &str, w: &PerfScenarioWorkloadReport) {
    let path_qps = if w.path_time_secs > 0.0 {
        (w.path_queries as f64) / w.path_time_secs
    } else {
        f64::INFINITY
    };
    let axql_qps = if w.axql_time_secs > 0.0 {
        (w.axql_queries as f64) / w.axql_time_secs
    } else {
        f64::INFINITY
    };

    println!("  {label}:");
    println!(
        "    follow_path: queries={} time={:.3}s ({:.1} qps) total_hits={}",
        w.path_queries, w.path_time_secs, path_qps, w.path_total_hits
    );
    println!(
        "    axql:        queries={} time={:.3}s ({:.1} qps) total_rows={}",
        w.axql_queries, w.axql_time_secs, axql_qps, w.axql_total_rows
    );
}

fn run_scenario_workload(
    scenario_name: &str,
    relation_type_names: &[String],
    db: &axiograph_pathdb::PathDB,
    seed: u64,
    path_queries: usize,
    axql_queries: usize,
    axql_limit: usize,
    index_depth: usize,
) -> Result<PerfScenarioWorkloadReport> {
    let name = scenario_name.trim().to_ascii_lowercase();
    match name.as_str() {
        "proto_api" | "protoapi" | "proto" | "api" => {
            run_proto_api_workload(db, seed, path_queries, axql_queries, axql_limit)
        }
        _ => run_generic_workload(
            db,
            relation_type_names,
            seed,
            path_queries,
            axql_queries,
            axql_limit,
            index_depth,
        ),
    }
}

fn run_proto_api_workload(
    db: &axiograph_pathdb::PathDB,
    seed: u64,
    path_queries: usize,
    axql_queries: usize,
    axql_limit: usize,
) -> Result<PerfScenarioWorkloadReport> {
    let services = ids_of_type(db, "ProtoService");
    let rpcs = ids_of_type(db, "ProtoRpc");
    let docs = ids_of_type(db, "Doc");

    if services.is_empty() {
        return Err(anyhow!("proto_api workload: missing ProtoService nodes"));
    }
    if rpcs.is_empty() {
        return Err(anyhow!("proto_api workload: missing ProtoRpc nodes"));
    }
    if docs.is_empty() {
        return Err(anyhow!("proto_api workload: missing Doc nodes"));
    }

    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed.wrapping_add(7));

    // ------------------------------------------------------------------
    // Path workload (exercise the PathIndex).
    // ------------------------------------------------------------------
    let start = Instant::now();
    let mut total_hits: u64 = 0;

    for _ in 0..path_queries {
        match (rng.next_u64() % 4) as u8 {
            // service -> rpc
            0 => {
                let svc = services[rng.gen_range_usize(services.len())];
                let hits = db.follow_path(svc, &["proto_service_has_rpc"]);
                total_hits = total_hits.wrapping_add(hits.len() as u64);
            }
            // service -> rpc -> endpoint
            1 => {
                let svc = services[rng.gen_range_usize(services.len())];
                let hits =
                    db.follow_path(svc, &["proto_service_has_rpc", "proto_rpc_http_endpoint"]);
                total_hits = total_hits.wrapping_add(hits.len() as u64);
            }
            // doc -> http endpoint -> rpc  (two-step identification path)
            2 => {
                let doc = docs[rng.gen_range_usize(docs.len())];
                let hits = db.follow_path(
                    doc,
                    &["mentions_http_endpoint", "proto_http_endpoint_of_rpc"],
                );
                total_hits = total_hits.wrapping_add(hits.len() as u64);
            }
            // service -> calls -> calls -> calls (cross-bundle traversal)
            _ => {
                let svc = services[rng.gen_range_usize(services.len())];
                let hits = db.follow_path(svc, &["calls", "calls", "calls"]);
                total_hits = total_hits.wrapping_add(hits.len() as u64);
            }
        }
    }

    let path_time = start.elapsed();

    // ------------------------------------------------------------------
    // AxQL workload (exercise query planning + joins + RPQ paths).
    // ------------------------------------------------------------------
    let start = Instant::now();
    let mut total_rows: u64 = 0;

    for _ in 0..axql_queries {
        let query_kind = (rng.next_u64() % 3) as u8;
        let query = match query_kind {
            // List rpcs for a chosen service.
            0 => {
                let svc = services[rng.gen_range_usize(services.len())];
                crate::axql::AxqlQuery {
                    select_vars: vec!["?rpc".to_string()],
                    disjuncts: vec![vec![
                        crate::axql::AxqlAtom::Type {
                            term: crate::axql::AxqlTerm::Const(svc),
                            type_name: "ProtoService".to_string(),
                        },
                        crate::axql::AxqlAtom::Edge {
                            left: crate::axql::AxqlTerm::Const(svc),
                            path: crate::axql::AxqlPathExpr::seq(vec![
                                "proto_service_has_rpc".to_string()
                            ]),
                            right: crate::axql::AxqlTerm::Var("?rpc".to_string()),
                        },
                    ]],
                    limit: axql_limit,
                    contexts: Vec::new(),
                    max_hops: None,
                    min_confidence: None,
                }
            }
            // Join: service -> rpc -> http endpoint.
            1 => {
                let svc = services[rng.gen_range_usize(services.len())];
                crate::axql::AxqlQuery {
                    select_vars: vec!["?ep".to_string()],
                    disjuncts: vec![vec![
                        crate::axql::AxqlAtom::Type {
                            term: crate::axql::AxqlTerm::Const(svc),
                            type_name: "ProtoService".to_string(),
                        },
                        crate::axql::AxqlAtom::Edge {
                            left: crate::axql::AxqlTerm::Const(svc),
                            path: crate::axql::AxqlPathExpr::seq(vec![
                                "proto_service_has_rpc".to_string()
                            ]),
                            right: crate::axql::AxqlTerm::Var("?rpc".to_string()),
                        },
                        crate::axql::AxqlAtom::Edge {
                            left: crate::axql::AxqlTerm::Var("?rpc".to_string()),
                            path: crate::axql::AxqlPathExpr::seq(vec![
                                "proto_rpc_http_endpoint".to_string()
                            ]),
                            right: crate::axql::AxqlTerm::Var("?ep".to_string()),
                        },
                    ]],
                    limit: axql_limit,
                    contexts: Vec::new(),
                    max_hops: None,
                    min_confidence: None,
                }
            }
            // Doc -> endpoint -> rpc (a 2-hop path expression).
            _ => {
                let doc = docs[rng.gen_range_usize(docs.len())];
                crate::axql::AxqlQuery {
                    select_vars: vec!["?rpc".to_string()],
                    disjuncts: vec![vec![
                        crate::axql::AxqlAtom::Type {
                            term: crate::axql::AxqlTerm::Const(doc),
                            type_name: "Doc".to_string(),
                        },
                        crate::axql::AxqlAtom::Edge {
                            left: crate::axql::AxqlTerm::Const(doc),
                            path: crate::axql::AxqlPathExpr::seq(vec![
                                "mentions_http_endpoint".to_string(),
                                "proto_http_endpoint_of_rpc".to_string(),
                            ]),
                            right: crate::axql::AxqlTerm::Var("?rpc".to_string()),
                        },
                    ]],
                    limit: axql_limit,
                    contexts: Vec::new(),
                    max_hops: None,
                    min_confidence: None,
                }
            }
        };

        let res = crate::axql::execute_axql_query(db, &query)?;
        total_rows = total_rows.wrapping_add(res.rows.len() as u64);
    }

    let axql_time = start.elapsed();

    Ok(PerfScenarioWorkloadReport {
        path_queries,
        path_total_hits: total_hits,
        path_time_secs: path_time.as_secs_f64(),
        axql_queries,
        axql_total_rows: total_rows,
        axql_time_secs: axql_time.as_secs_f64(),
    })
}

fn ids_of_type(db: &axiograph_pathdb::PathDB, type_name: &str) -> Vec<u32> {
    db.find_by_type(type_name)
        .map(|bm| bm.iter().collect())
        .unwrap_or_default()
}

fn run_generic_workload(
    db: &axiograph_pathdb::PathDB,
    relation_type_names: &[String],
    seed: u64,
    path_queries: usize,
    axql_queries: usize,
    axql_limit: usize,
    index_depth: usize,
) -> Result<PerfScenarioWorkloadReport> {
    if db.entities.is_empty() {
        return Err(anyhow!("generic scenario workload: empty DB"));
    }
    if relation_type_names.is_empty() {
        return Err(anyhow!(
            "generic scenario workload: no relation types recorded (this should not happen)"
        ));
    }

    let max_len = index_depth.max(1).min(3);
    let rels: Vec<&str> = relation_type_names.iter().map(|s| s.as_str()).collect();

    let mut rng = crate::synthetic_pathdb::XorShift64::new(seed.wrapping_add(11));

    // ------------------------------------------------------------------
    // Path workload (random relation sequences).
    // ------------------------------------------------------------------
    let start = Instant::now();
    let mut total_hits: u64 = 0;

    for _ in 0..path_queries {
        let start_id = rng.gen_range_usize(db.entities.len()) as u32;
        let len = (rng.next_u64() as usize % max_len) + 1;
        let r1 = rels[rng.gen_range_usize(rels.len())];
        let r2 = rels[rng.gen_range_usize(rels.len())];
        let r3 = rels[rng.gen_range_usize(rels.len())];

        let hits = match len {
            1 => db.follow_path(start_id, &[r1]),
            2 => db.follow_path(start_id, &[r1, r2]),
            _ => db.follow_path(start_id, &[r1, r2, r3]),
        };
        total_hits = total_hits.wrapping_add(hits.len() as u64);
    }

    let path_time = start.elapsed();

    // ------------------------------------------------------------------
    // AxQL workload (random edge/path atoms).
    // ------------------------------------------------------------------
    let start = Instant::now();
    let mut total_rows: u64 = 0;

    for _ in 0..axql_queries {
        let start_id = rng.gen_range_usize(db.entities.len()) as u32;
        let len = (rng.next_u64() as usize % max_len) + 1;
        let r1 = rels[rng.gen_range_usize(rels.len())].to_string();
        let r2 = rels[rng.gen_range_usize(rels.len())].to_string();
        let r3 = rels[rng.gen_range_usize(rels.len())].to_string();

        let path = match len {
            1 => crate::axql::AxqlPathExpr::seq(vec![r1]),
            2 => crate::axql::AxqlPathExpr::seq(vec![r1, r2]),
            _ => crate::axql::AxqlPathExpr::seq(vec![r1, r2, r3]),
        };

        let query = crate::axql::AxqlQuery {
            select_vars: vec!["?y".to_string()],
            disjuncts: vec![vec![crate::axql::AxqlAtom::Edge {
                left: crate::axql::AxqlTerm::Const(start_id),
                path,
                right: crate::axql::AxqlTerm::Var("?y".to_string()),
            }]],
            limit: axql_limit,
            contexts: Vec::new(),
            max_hops: None,
            min_confidence: None,
        };

        let res = crate::axql::execute_axql_query(db, &query)?;
        total_rows = total_rows.wrapping_add(res.rows.len() as u64);
    }

    let axql_time = start.elapsed();

    Ok(PerfScenarioWorkloadReport {
        path_queries,
        path_total_hits: total_hits,
        path_time_secs: path_time.as_secs_f64(),
        axql_queries,
        axql_total_rows: total_rows,
        axql_time_secs: axql_time.as_secs_f64(),
    })
}
