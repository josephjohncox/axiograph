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
use std::time::Instant;

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
        /// Scenario name (try: proto_api | enterprise_large_api | economic_flows | machinist_learning | schema_evolution | social_network | supply_chain)
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

fn rate(items: usize, dt: std::time::Duration) -> f64 {
    let secs = dt.as_secs_f64();
    if secs <= 0.0 {
        return f64::INFINITY;
    }
    (items as f64) / secs
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
