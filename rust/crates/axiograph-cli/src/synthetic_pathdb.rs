//! Synthetic PathDB generators used by CLI tooling.
//!
//! We keep this separate from the core `axiograph-pathdb` crate so that:
//! - performance harnesses can evolve quickly without polluting the library API
//! - REPL/demo tooling can share deterministic generators

#![allow(dead_code)]

use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

fn scenario_doc_id(scenario_name: &str) -> String {
    format!("synthetic/{scenario_name}.md")
}

fn record_scenario_docchunk_edge_types(builder: &mut ScenarioBuilder) {
    for ty in ["Document", "DocChunk"] {
        builder.entity_type_names.insert(ty.to_string());
    }
    for rel in [
        "document_has_chunk",
        "chunk_in_document",
        "doc_chunk_about",
        "has_doc_chunk",
    ] {
        builder.relation_type_names.insert(rel.to_string());
    }
}

fn import_scenario_docchunks(
    builder: &mut ScenarioBuilder,
    scenario_name: &str,
    description: &str,
    key_entities: &[(&str, &str)],
    mut extra_chunks: Vec<axiograph_ingest_docs::Chunk>,
) -> Result<()> {
    let document_id = scenario_doc_id(scenario_name);

    let mut chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    chunks.push(axiograph_ingest_docs::Chunk {
        chunk_id: format!("doc_{scenario_name}_overview_0"),
        document_id: document_id.clone(),
        page: None,
        span_id: "overview".to_string(),
        text: format!(
            "Scenario: {scenario_name}\n\n{description}\n\nThis is a synthetic graph used for demos and REPL exploration. It is not external ground truth; treat it as a structured test fixture.",
        ),
        bbox: None,
        metadata: HashMap::from([
            ("kind".to_string(), "scenario_overview".to_string()),
            ("scenario".to_string(), scenario_name.to_string()),
        ]),
    });

    for (i, (about_type, about_name)) in key_entities.iter().enumerate() {
        chunks.push(axiograph_ingest_docs::Chunk {
            chunk_id: format!("doc_{scenario_name}_key_{i}"),
            document_id: document_id.clone(),
            page: None,
            span_id: format!("key_{i}"),
            text: format!(
                "Key entity (synthetic): {about_type} \"{about_name}\".\n\nUse `describe` / `q` / `viz` to explore its neighborhood. This DocChunk exists to enable grounded demos (citations + open-source pointers)."
            ),
            bbox: None,
            metadata: HashMap::from([
                ("kind".to_string(), "scenario_key_entity".to_string()),
                ("scenario".to_string(), scenario_name.to_string()),
                ("about_type".to_string(), about_type.to_string()),
                ("about_name".to_string(), about_name.to_string()),
            ]),
        });
    }

    chunks.append(&mut extra_chunks);

    let _summary = crate::doc_chunks::import_chunks_into_pathdb(&mut builder.db, &chunks)?;
    record_scenario_docchunk_edge_types(builder);
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    pub(crate) fn new(seed: u64) -> Self {
        // Avoid the degenerate all-zero state.
        let state = if seed == 0 { 0x9e3779b97f4a7c15 } else { seed };
        Self { state }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        // xorshift64* (simple, fast, deterministic).
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub(crate) fn gen_range_usize(&mut self, upper: usize) -> usize {
        if upper == 0 {
            return 0;
        }
        (self.next_u64() % (upper as u64)) as usize
    }
}

pub(crate) struct SyntheticPathDbIngest {
    pub(crate) db: axiograph_pathdb::PathDB,
    pub(crate) relation_type_names: Vec<String>,
    pub(crate) entity_time: Duration,
    pub(crate) relation_time: Duration,
    pub(crate) edge_count: usize,
}

/// Build a synthetic PathDB *without* calling `build_indexes()`.
pub(crate) fn build_synthetic_pathdb_ingest(
    entities: usize,
    edges_per_entity: usize,
    rel_types: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticPathDbIngest> {
    if entities == 0 {
        return Err(anyhow!("--entities must be > 0"));
    }
    if rel_types == 0 {
        return Err(anyhow!("--rel-types must be > 0"));
    }
    if index_depth == 0 {
        return Err(anyhow!("--index-depth must be > 0"));
    }

    let relation_type_names: Vec<String> = (0..rel_types).map(|i| format!("rel_{i}")).collect();
    let edge_count = entities.saturating_mul(edges_per_entity);

    let mut rng = XorShift64::new(seed);
    let mut db = axiograph_pathdb::PathDB::new();
    db.path_index = axiograph_pathdb::PathIndex::new(index_depth);

    let start = Instant::now();
    for i in 0..entities {
        db.add_entity("Node", vec![("name", &format!("node_{i}"))]);
    }
    let entity_time = start.elapsed();

    let start = Instant::now();
    for source in 0..entities {
        let source_id = source as u32;
        for _ in 0..edges_per_entity {
            let target_id = rng.gen_range_usize(entities) as u32;
            let rel_type = &relation_type_names[rng.gen_range_usize(rel_types)];
            db.add_relation(rel_type, source_id, target_id, 0.9, Vec::new());
        }
    }
    let relation_time = start.elapsed();

    Ok(SyntheticPathDbIngest {
        db,
        relation_type_names,
        entity_time,
        relation_time,
        edge_count,
    })
}

// =============================================================================
// Scenario generator (realistic shapes + relations + homotopies)
// =============================================================================

pub(crate) struct SyntheticScenarioIngest {
    pub(crate) scenario_name: String,
    pub(crate) description: String,
    /// Distinct entity types used by the scenario generator.
    ///
    /// This is primarily for tooling (perf/workloads, REPL UX, viz overlays),
    /// not for the trusted kernel.
    pub(crate) entity_type_names: Vec<String>,
    /// Distinct relation types used by the scenario generator.
    pub(crate) relation_type_names: Vec<String>,
    pub(crate) db: axiograph_pathdb::PathDB,
    pub(crate) entity_time: Duration,
    pub(crate) relation_time: Duration,
    pub(crate) example_commands: Vec<String>,
}

pub(crate) fn build_scenario_pathdb_ingest(
    scenario: &str,
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    let name = scenario.trim().to_ascii_lowercase();
    match name.as_str() {
        "enterprise" | "realworld" | "real_world" => {
            build_enterprise_scenario(scale, index_depth, seed)
        }
        "enterprise_large_api" | "enterprise_large" | "enterprise_api" => {
            build_enterprise_large_api_scenario(scale, index_depth, seed)
        }
        "economic_flows" | "economicflows" | "economics" | "economy" => {
            build_economic_flows_scenario(scale, index_depth, seed)
        }
        "machinist_learning" | "machinistlearning" | "machining" | "learning" => {
            build_machinist_learning_scenario(scale, index_depth, seed)
        }
        "schema_evolution" | "schemaevolution" | "schema" | "evolution" => {
            build_schema_evolution_scenario(scale, index_depth, seed)
        }
        "proto_api" | "protoapi" | "proto" | "api" => {
            build_proto_api_scenario(scale, index_depth, seed)
        }
        "proto_api_business" | "proto_business" | "business_proto" | "enterprise_proto" => {
            build_proto_api_business_scenario(scale, index_depth, seed)
        }
        "social_network" | "socialnetwork" | "social" => {
            build_social_network_scenario(scale, index_depth, seed)
        }
        "supply_chain" | "supplychain" | "supply" | "manufacturing" => {
            build_supply_chain_scenario(scale, index_depth, seed)
        }
        other => Err(anyhow!(
            "unknown scenario `{other}` (try: enterprise | enterprise_large_api | proto_api | proto_api_business | economic_flows | machinist_learning | schema_evolution | social_network | supply_chain)"
        )),
    }
}

struct ScenarioBuilder {
    db: axiograph_pathdb::PathDB,
    rng: XorShift64,
    ids_by_name: HashMap<String, u32>,
    entity_type_names: HashSet<String>,
    relation_type_names: HashSet<String>,
}

impl ScenarioBuilder {
    fn new(seed: u64, index_depth: usize) -> Result<Self> {
        if index_depth == 0 {
            return Err(anyhow!("index_depth must be > 0"));
        }
        let mut db = axiograph_pathdb::PathDB::new();
        db.path_index = axiograph_pathdb::PathIndex::new(index_depth);
        Ok(Self {
            db,
            rng: XorShift64::new(seed),
            ids_by_name: HashMap::new(),
            entity_type_names: HashSet::new(),
            relation_type_names: HashSet::new(),
        })
    }

    fn add_named_entity(
        &mut self,
        type_name: &str,
        name: impl Into<String>,
        mut attrs: Vec<(String, String)>,
    ) -> u32 {
        self.entity_type_names.insert(type_name.to_string());
        let name = name.into();
        attrs.push(("name".to_string(), name.clone()));
        let attrs_ref: Vec<(&str, &str)> = attrs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let id = self.db.add_entity(type_name, attrs_ref);
        self.ids_by_name.insert(name, id);
        id
    }

    fn rel(&mut self, rel_type: &str, source: u32, target: u32, confidence: f32) -> u32 {
        self.relation_type_names.insert(rel_type.to_string());
        self.db
            .add_relation(rel_type, source, target, confidence, Vec::new())
    }

    fn equiv(&mut self, left: u32, right: u32, equiv_type: &str) {
        self.db.add_equivalence(left, right, equiv_type);
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.rng.gen_range_usize(xs.len())]
    }
}

fn add_flow_compose_fact(
    b: &mut ScenarioBuilder,
    flow_type_ids: &HashMap<String, u32>,
    f1: &str,
    f2: &str,
    result: &str,
) {
    let Some(a) = flow_type_ids.get(f1).copied() else {
        return;
    };
    let Some(b2) = flow_type_ids.get(f2).copied() else {
        return;
    };
    let Some(r) = flow_type_ids.get(result).copied() else {
        return;
    };

    let comp = b.add_named_entity(
        "FlowCompose",
        format!("compose_{f1}_{f2}_to_{result}"),
        vec![("repr".to_string(), format!("{f1} ; {f2} ≡ {result}"))],
    );
    b.rel("first", comp, a, 1.0);
    b.rel("second", comp, b2, 1.0);
    b.rel("result", comp, r, 1.0);
}

fn connect_migration(b: &mut ScenarioBuilder, from_schema: u32, migration: u32, to_schema: u32) {
    b.rel("outgoingMigration", from_schema, migration, 1.0);
    b.rel("incomingMigration", to_schema, migration, 1.0);
    b.rel("fromSchema", migration, from_schema, 1.0);
    b.rel("toSchema", migration, to_schema, 1.0);
}

fn build_enterprise_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    build_enterprise_scenario_named(
        scale,
        index_depth,
        seed,
        "enterprise",
        "Enterprise-ish KB: people/teams/services/endpoints/tables/docs + explicit homotopy objects (commuting diagrams) + equivalence classes.",
        |i| format!("svc_{i}"),
        vec![
            "q select ?svc where ?svc { is Service, ownedBy, storesIn, exposes } limit 10".to_string(),
            "q select ?svc where name(\"team_0\") -owns-> ?svc limit 10".to_string(),
            "q select ?svc where name(\"person_0_0\") -memberOf/owns-> ?svc limit 10".to_string(),
            "q select ?ep where name(\"svc_0\") -exposes-> ?ep limit 10".to_string(),
            "q select ?dst where name(\"svc_0.rpc_0\") -calls/calls-> ?dst limit 10".to_string(),
            "q select ?h where ?h is Homotopy, ?h -from-> name(\"doc_0_0\") limit 10".to_string(),
            "sq examples/semantic_queries/person_team_services.json".to_string(),
            "llm use mock".to_string(),
            "llm ask find Service named svc_0".to_string(),
        ],
    )
}

fn build_enterprise_large_api_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    // A variant of the enterprise scenario whose service names align with the
    // `examples/proto/large_api/` fixture. This makes it easy to demonstrate
    // proto import + reconciliation/matching in the REPL.
    let domains = ["users", "payments", "catalog"];

    build_enterprise_scenario_named(
        scale,
        index_depth,
        seed,
        "enterprise_large_api",
        "Enterprise-ish KB aligned with `examples/proto/large_api`: teams/services/docs plus a proto-shaped API surface that can be imported and matched.",
        |i| {
            let base = domains.get(i).copied().unwrap_or_else(|| "misc");
            format!("svc_{base}")
        },
        vec![
            "q select ?svc where ?svc is Service limit 10".to_string(),
            "q select ?psvc where ?psvc is ProtoService limit 10".to_string(),
            "q select ?rpc where name(\"team_0\") -owns/mapsToProtoService/proto_service_has_rpc-> ?rpc max_hops 6 limit 10".to_string(),
        ],
    )
}

fn build_enterprise_scenario_named(
    scale: usize,
    index_depth: usize,
    seed: u64,
    scenario_name: &str,
    description: &str,
    service_name_for_index: impl Fn(usize) -> String,
    example_commands: Vec<String>,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario inspired by common “GraphRAG → structured KG” pipelines:
    // - Teams own services
    // - People belong to teams (shapes)
    // - Services expose endpoints and store data in tables (shapes)
    // - Docs mention both services and endpoints (multi-path reachability)
    // - Explicit “homotopy” objects record commuting diagrams between paths
    //
    // Important: This is *not* claiming relations are invertible in the real world.
    // “Homotopy” here means “multiple derivations / multiple paths between the same
    // endpoints”, which is what our certificate layer later makes auditable.

    let people_per_team = 3usize;
    let endpoints_per_service = 3usize;
    let docs_per_team = 2usize;
    let columns_per_table = 4usize;

    let mut b = ScenarioBuilder::new(seed, index_depth)?;

    let languages = ["rust", "go", "python"];
    let tiers = ["backend", "infra", "ml"];
    let column_types = ["int", "text", "bool", "timestamp"];

    let start = Instant::now();

    let mut teams: Vec<u32> = Vec::with_capacity(scale);
    let mut people: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut services: Vec<u32> = Vec::with_capacity(scale);
    let mut service_aliases: Vec<u32> = Vec::with_capacity(scale);
    let mut tables: Vec<u32> = Vec::with_capacity(scale);
    let mut columns: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut endpoints: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut rpc_nodes: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut docs: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut doc_direct_paths: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut doc_via_paths: Vec<Vec<u32>> = Vec::with_capacity(scale);
    let mut homotopies: Vec<Vec<u32>> = Vec::with_capacity(scale);

    for i in 0..scale {
        let svc_name = service_name_for_index(i);
        let svc_fqn_segment = enterprise_service_fqn_segment(&svc_name);

        let team = b.add_named_entity(
            "Team",
            format!("team_{i}"),
            vec![("org".to_string(), "Acme".to_string())],
        );
        teams.push(team);

        // People
        let mut ps: Vec<u32> = Vec::with_capacity(people_per_team);
        for p in 0..people_per_team {
            let title = if p == 0 { "TechLead" } else { "Engineer" };
            let person = b.add_named_entity(
                "Person",
                format!("person_{i}_{p}"),
                vec![
                    ("email".to_string(), format!("person_{i}_{p}@acme.test")),
                    ("title".to_string(), title.to_string()),
                ],
            );
            ps.push(person);
        }
        people.push(ps);

        // Service + alias (for entity resolution / equivalence demos).
        let lang = b.pick(&languages).to_string();
        let tier = b.pick(&tiers).to_string();
        let svc = b.add_named_entity(
            "Service",
            svc_name.clone(),
            vec![("language".to_string(), lang), ("tier".to_string(), tier)],
        );
        services.push(svc);

        let alias = b.add_named_entity(
            "ServiceRef",
            format!("{svc_name}_alias"),
            vec![
                ("source".to_string(), "docs".to_string()),
                ("ref_kind".to_string(), "text_mention".to_string()),
            ],
        );
        service_aliases.push(alias);

        // Table + columns (a shape you can query via `hasColumn`).
        let table = b.add_named_entity(
            "Table",
            format!("{svc_name}.table"),
            vec![("engine".to_string(), "postgres".to_string())],
        );
        tables.push(table);

        let mut cols: Vec<u32> = Vec::with_capacity(columns_per_table);
        for c in 0..columns_per_table {
            let ty = b.pick(&column_types).to_string();
            let col = b.add_named_entity(
                "Column",
                format!("{svc_name}.table.col_{c}"),
                vec![("data_type".to_string(), ty)],
            );
            cols.push(col);
        }
        columns.push(cols);

        // Endpoints + equivalent RPC nodes.
        let mut eps: Vec<u32> = Vec::with_capacity(endpoints_per_service);
        let mut rpcs: Vec<u32> = Vec::with_capacity(endpoints_per_service);
        for e in 0..endpoints_per_service {
            let ep_name = format!("{svc_name}.rpc_{e}");
            let ep = b.add_named_entity(
                "Endpoint",
                ep_name.clone(),
                vec![
                    ("method".to_string(), "POST".to_string()),
                    ("path".to_string(), format!("/{svc_name}/rpc/{e}")),
                ],
            );
            eps.push(ep);

            let rpc = b.add_named_entity(
                "Rpc",
                format!("{ep_name}.Rpc"),
                vec![("fqn".to_string(), format!("acme.{svc_fqn_segment}.Rpc{e}"))],
            );
            rpcs.push(rpc);
        }
        endpoints.push(eps);
        rpc_nodes.push(rpcs);

        // Docs + explicit homotopy objects for a commuting diagram:
        //   doc -mentionsService-> service
        //   doc -mentionsEndpoint-> endpoint -belongsTo-> service
        let mut ds: Vec<u32> = Vec::with_capacity(docs_per_team);
        let mut direct_ps: Vec<u32> = Vec::with_capacity(docs_per_team);
        let mut via_ps: Vec<u32> = Vec::with_capacity(docs_per_team);
        let mut hs: Vec<u32> = Vec::with_capacity(docs_per_team);
        for d in 0..docs_per_team {
            let doc = b.add_named_entity(
                "Doc",
                format!("doc_{i}_{d}"),
                vec![("kind".to_string(), "rfc".to_string())],
            );
            ds.push(doc);

            let p_direct = b.add_named_entity(
                "PathWitness",
                format!("path_doc_{i}_{d}_direct"),
                vec![("repr".to_string(), "mentionsService".to_string())],
            );
            direct_ps.push(p_direct);

            let p_via = b.add_named_entity(
                "PathWitness",
                format!("path_doc_{i}_{d}_via_endpoint"),
                vec![("repr".to_string(), "mentionsEndpoint/belongsTo".to_string())],
            );
            via_ps.push(p_via);

            let hom = b.add_named_entity(
                "Homotopy",
                format!("homotopy_doc_{i}_{d}"),
                vec![(
                    "repr".to_string(),
                    "mentionsService ~ mentionsEndpoint/belongsTo".to_string(),
                )],
            );
            hs.push(hom);
        }
        docs.push(ds);
        doc_direct_paths.push(direct_ps);
        doc_via_paths.push(via_ps);
        homotopies.push(hs);
    }

    let entity_time = start.elapsed();

    let start = Instant::now();

    // People ↔ team membership; team owns service; service stores in table; service exposes endpoints.
    for i in 0..scale {
        let team = teams[i];
        let svc = services[i];
        let alias = service_aliases[i];
        let table = tables[i];

        b.rel("owns", team, svc, 0.95);
        b.rel("ownedBy", svc, team, 0.95);
        b.equiv(svc, alias, "SameService");

        for &person in &people[i] {
            b.rel("memberOf", person, team, 0.95);
            b.rel("hasMember", team, person, 0.95);
        }
        // Team lead is the first person.
        b.rel("onCallFor", people[i][0], svc, 0.8);

        b.rel("storesIn", svc, table, 0.9);
        for &col in &columns[i] {
            b.rel("hasColumn", table, col, 0.99);
        }

        for (j, &ep) in endpoints[i].iter().enumerate() {
            b.rel("exposes", svc, ep, 0.99);
            b.rel("belongsTo", ep, svc, 0.99);
            b.equiv(ep, rpc_nodes[i][j], "SameApiSurface");
        }

        // Documents mention service + endpoint + table, producing multi-path reachability.
        for d in 0..docs[i].len() {
            let doc = docs[i][d];
            let ep0 = endpoints[i][0];

            b.rel("mentionsService", doc, svc, 0.85);
            b.rel("mentionsEndpoint", doc, ep0, 0.85);
            b.rel("mentionsTable", doc, table, 0.8);

            // PathWitness endpoints.
            let direct = doc_direct_paths[i][d];
            let via = doc_via_paths[i][d];
            let hom = homotopies[i][d];

            b.rel("from", direct, doc, 1.0);
            b.rel("to", direct, svc, 1.0);

            b.rel("from", via, doc, 1.0);
            b.rel("to", via, svc, 1.0);
            b.rel("via", via, ep0, 1.0);

            b.rel("from", hom, doc, 1.0);
            b.rel("to", hom, svc, 1.0);
            b.rel("lhs", hom, direct, 1.0);
            b.rel("rhs", hom, via, 1.0);

            // Also record this as an entity-level equivalence between path witnesses.
            b.equiv(direct, via, "HomotopicPath");
        }
    }

    // Cross-service call graph (endpoints call endpoints).
    for i in 0..scale {
        let next = (i + 1) % scale;
        for e in 0..endpoints[i].len() {
            b.rel("calls", endpoints[i][e], endpoints[next][e], 0.7);
        }
    }

    let relation_time = start.elapsed();

    // Add a small evidence layer so REPL/LLM demos have DocChunks to cite and open
    // even when the user only ran `gen scenario ...` (no external docs imported).
    let key_service = service_name_for_index(0);
    import_scenario_docchunks(
        &mut b,
        scenario_name,
        description,
        &[("Service", key_service.as_str()), ("Person", "person_0_0"), ("Doc", "doc_0_0")],
        Vec::new(),
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = b;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: scenario_name.to_string(),
        description: description.to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn enterprise_service_fqn_segment(service_name: &str) -> String {
    // Preserve the old behavior for numeric service names:
    // - `svc_0` → `svc0` (no underscore)
    // but keep named services readable:
    // - `svc_users` → `svc_users`
    let Some(rest) = service_name.strip_prefix("svc_") else {
        return service_name.to_string();
    };
    if rest.chars().all(|c| c.is_ascii_digit()) {
        return format!("svc{rest}");
    }
    service_name.to_string()
}

fn build_economic_flows_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario aligned with `examples/economics/EconomicFlows.axi`:
    // - Agents: households/firms/bank/government
    // - Flow types and small flow algebra (inverse, composition)
    // - Two different “transaction paths” with the same net effect
    //   recorded explicitly as a Homotopy between PathWitness nodes.

    let mut b = ScenarioBuilder::new(seed, index_depth)?;

    let flow_types = [
        "Labor",
        "Wages",
        "Consumption",
        "Goods",
        "Savings",
        "Withdrawal",
        "Loans",
        "LoanRepayment",
        "Interest",
        "Taxes",
        "Transfers",
        "Identity",
        "Employment",
        "CreditPurchase",
        "Accumulation",
        "Redistribution",
    ];
    let amount_levels = ["Zero", "Small", "Medium", "Large"];
    let times = ["T1", "T2", "T3", "T4"];

    let start = Instant::now();

    let mut flow_type_ids: HashMap<String, u32> = HashMap::new();
    for ft in flow_types {
        let id = b.add_named_entity("FlowType", ft, Vec::new());
        flow_type_ids.insert(ft.to_string(), id);
    }

    for a in amount_levels {
        b.add_named_entity("Amount", a, Vec::new());
    }
    for t in times {
        b.add_named_entity("Time", t, Vec::new());
    }

    let bank = b.add_named_entity(
        "Bank",
        "bank_0",
        vec![("label".to_string(), "Bank Z".to_string())],
    );
    let gov = b.add_named_entity(
        "Government",
        "gov_0",
        vec![("label".to_string(), "Gov".to_string())],
    );

    // Accounts (stocks).
    let bank_acct = b.add_named_entity(
        "Account",
        "acct_bank_0",
        vec![("stockType".to_string(), "deposits".to_string())],
    );
    b.rel("hasAccount", bank, bank_acct, 1.0);
    b.rel("accountOf", bank_acct, bank, 1.0);

    let gov_acct = b.add_named_entity(
        "Account",
        "acct_gov_0",
        vec![("stockType".to_string(), "treasury".to_string())],
    );
    b.rel("hasAccount", gov, gov_acct, 1.0);
    b.rel("accountOf", gov_acct, gov, 1.0);

    let mut households: Vec<u32> = Vec::with_capacity(scale);
    let mut firms: Vec<u32> = Vec::with_capacity(scale);
    for i in 0..scale {
        let h = b.add_named_entity(
            "Household",
            format!("household_{i}"),
            vec![("sector".to_string(), "household".to_string())],
        );
        households.push(h);
        let f = b.add_named_entity(
            "Firm",
            format!("firm_{i}"),
            vec![("sector".to_string(), "production".to_string())],
        );
        firms.push(f);

        let h_acct = b.add_named_entity(
            "Account",
            format!("acct_household_{i}"),
            vec![("stockType".to_string(), "deposits".to_string())],
        );
        b.rel("hasAccount", h, h_acct, 1.0);
        b.rel("accountOf", h_acct, h, 1.0);

        let f_acct = b.add_named_entity(
            "Account",
            format!("acct_firm_{i}"),
            vec![("stockType".to_string(), "cash".to_string())],
        );
        b.rel("hasAccount", f, f_acct, 1.0);
        b.rel("accountOf", f_acct, f, 1.0);
    }

    let entity_time = start.elapsed();

    let start = Instant::now();

    // Flow inverses (groupoid-ish).
    if let (Some(loans), Some(repay)) = (
        flow_type_ids.get("Loans").copied(),
        flow_type_ids.get("LoanRepayment").copied(),
    ) {
        b.rel("inverseOf", loans, repay, 1.0);
        b.rel("inverseOf", repay, loans, 1.0);
    }
    if let (Some(sav), Some(wd)) = (
        flow_type_ids.get("Savings").copied(),
        flow_type_ids.get("Withdrawal").copied(),
    ) {
        b.rel("inverseOf", sav, wd, 1.0);
        b.rel("inverseOf", wd, sav, 1.0);
    }

    // A few composition facts (small “flow algebra”).
    add_flow_compose_fact(&mut b, &flow_type_ids, "Labor", "Wages", "Employment");
    add_flow_compose_fact(
        &mut b,
        &flow_type_ids,
        "Loans",
        "Consumption",
        "CreditPurchase",
    );
    add_flow_compose_fact(&mut b, &flow_type_ids, "Wages", "Savings", "Accumulation");
    add_flow_compose_fact(
        &mut b,
        &flow_type_ids,
        "Taxes",
        "Transfers",
        "Redistribution",
    );

    // Simple circular-flow-ish dynamics.
    for i in 0..scale {
        let h = households[i];
        let f = firms[i];

        b.rel("Labor", h, f, 0.9);
        b.rel("Wages", f, h, 0.9);

        b.rel("Consumption", h, f, 0.92);
        b.rel("Goods", f, h, 0.92);

        b.rel("Savings", h, bank, 0.85);
        b.rel("Withdrawal", bank, h, 0.7);

        b.rel("Loans", bank, f, 0.8);
        b.rel("LoanRepayment", f, bank, 0.75);
        b.rel("Interest", f, bank, 0.65);

        b.rel("Taxes", h, gov, 0.6);
        b.rel("Transfers", gov, h, 0.6);
    }

    // A concrete “two paths, same effect” example:
    // Household pays Firm either directly, or via a card transaction mediated by the bank.
    let h0 = households[0];
    let f0 = firms[0];
    b.rel("CardCharge", h0, bank, 0.85);
    b.rel("CardSettlement", bank, f0, 0.85);

    let p_direct = b.add_named_entity(
        "PathWitness",
        "path_household0_to_firm0_direct",
        vec![("repr".to_string(), "Consumption".to_string())],
    );
    b.rel("from", p_direct, h0, 1.0);
    b.rel("to", p_direct, f0, 1.0);

    let p_via = b.add_named_entity(
        "PathWitness",
        "path_household0_to_firm0_via_bank",
        vec![("repr".to_string(), "CardCharge/CardSettlement".to_string())],
    );
    b.rel("from", p_via, h0, 1.0);
    b.rel("to", p_via, f0, 1.0);
    b.rel("via", p_via, bank, 1.0);

    let hom = b.add_named_entity(
        "Homotopy",
        "homotopy_direct_vs_card",
        vec![(
            "repr".to_string(),
            "Consumption ~ CardCharge/CardSettlement".to_string(),
        )],
    );
    b.rel("from", hom, h0, 1.0);
    b.rel("to", hom, f0, 1.0);
    b.rel("lhs", hom, p_direct, 1.0);
    b.rel("rhs", hom, p_via, 1.0);

    b.equiv(p_direct, p_via, "HomotopicPath");

    let relation_time = start.elapsed();

    let example_commands = vec![
        "q select ?f where name(\"household_0\") -Consumption-> ?f limit 10".to_string(),
        "q select ?f where name(\"household_0\") -CardCharge/CardSettlement-> ?f max_hops 4 limit 10"
            .to_string(),
        "q select ?inv where name(\"Loans\") -inverseOf-> ?inv limit 10".to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"household_0\") limit 10".to_string(),
        "q select ?c where ?c is FlowCompose, ?c -first-> name(\"Labor\"), ?c -result-> ?r limit 10"
            .to_string(),
    ];

    import_scenario_docchunks(
        &mut b,
        "economic_flows",
        "Economic flows: households/firms/bank/government + flow algebra (inverse/compose) + explicit homotopy for path-equivalence of transactions.",
        &[("Household", "household_0"), ("Bank", "bank_0")],
        Vec::new(),
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = b;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "economic_flows".to_string(),
        description: "Economic flows: households/firms/bank/government + flow algebra (inverse/compose) + explicit homotopy for path-equivalence of transactions.".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn build_machinist_learning_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario aligned with `examples/learning/MachinistLearning.axi`:
    // - Materials, tools, operations, outcomes
    // - Concepts + guidelines (learning / guardrails)
    // - Two derivations from an operation to a guideline are recorded as a Homotopy.

    let mut b = ScenarioBuilder::new(seed, index_depth)?;

    let start = Instant::now();

    let outcome_success = b.add_named_entity("Outcome", "Success", Vec::new());
    let outcome_tool_wear = b.add_named_entity("Outcome", "ToolWear", Vec::new());
    let outcome_chatter = b.add_named_entity("Outcome", "Chatter", Vec::new());
    let _outcome_bue = b.add_named_entity("Outcome", "BuiltUpEdge", Vec::new());

    let concept_thermal = b.add_named_entity(
        "Concept",
        "ThermalConductivity",
        vec![
            ("difficulty".to_string(), "Beginner".to_string()),
            (
                "description".to_string(),
                "Heat flow from the cutting zone; titanium concentrates heat at the tool tip."
                    .to_string(),
            ),
        ],
    );
    let concept_work = b.add_named_entity(
        "Concept",
        "WorkHardening",
        vec![
            ("difficulty".to_string(), "Intermediate".to_string()),
            (
                "description".to_string(),
                "Surface hardens when rubbed; can accelerate tool wear.".to_string(),
            ),
        ],
    );
    let concept_chatter = b.add_named_entity(
        "Concept",
        "ChatterVibration",
        vec![
            ("difficulty".to_string(), "Advanced".to_string()),
            (
                "description".to_string(),
                "Self-excited vibration; damages surface finish and can break tools.".to_string(),
            ),
        ],
    );
    b.rel("requires", concept_work, concept_thermal, 1.0);
    b.rel("requires", concept_chatter, concept_work, 1.0);

    let guideline_ti = b.add_named_entity(
        "SafetyGuideline",
        "TitaniumSpeed",
        vec![("severity".to_string(), "Critical".to_string())],
    );
    let guideline_coolant = b.add_named_entity(
        "SafetyGuideline",
        "DeepHoleCoolant",
        vec![("severity".to_string(), "Warning".to_string())],
    );
    let guideline_thin_wall = b.add_named_entity(
        "SafetyGuideline",
        "ThinWallChatter",
        vec![("severity".to_string(), "Advisory".to_string())],
    );
    b.rel("explains", concept_thermal, guideline_ti, 0.95);
    b.rel("explains", concept_work, guideline_ti, 0.8);
    b.rel("explains", concept_chatter, guideline_thin_wall, 0.85);
    b.rel("prevents", guideline_ti, outcome_tool_wear, 0.9);
    b.rel("prevents", guideline_thin_wall, outcome_chatter, 0.85);
    b.rel("prevents", guideline_coolant, outcome_tool_wear, 0.8);

    let material_names = ["Titanium6Al4V", "Aluminum6061", "Steel4140"];
    let tool_materials = ["Carbide", "HSS", "Ceramic"];
    let coatings = ["TiN", "TiAlN", "Uncoated"];
    let op_types = ["Turning", "Milling", "Drilling"];

    let mut materials: Vec<u32> = Vec::with_capacity(scale);
    let mut tools: Vec<u32> = Vec::with_capacity(scale);
    let mut ops: Vec<u32> = Vec::with_capacity(scale);

    for i in 0..scale {
        let mat_name = material_names[i % material_names.len()];
        let hardness = match mat_name {
            "Titanium6Al4V" => "36",
            "Aluminum6061" => "15",
            "Steel4140" => "48",
            _ => "30",
        };
        let mat = b.add_named_entity(
            "Material",
            format!("mat_{i}_{mat_name}"),
            vec![
                ("kind".to_string(), mat_name.to_string()),
                ("hardness".to_string(), hardness.to_string()),
            ],
        );
        materials.push(mat);

        let tool_material = b.pick(&tool_materials).to_string();
        let coating = b.pick(&coatings).to_string();
        let tool = b.add_named_entity(
            "CuttingTool",
            format!("tool_{i}"),
            vec![
                ("material".to_string(), tool_material),
                ("coating".to_string(), coating),
            ],
        );
        tools.push(tool);

        let operation_type = b.pick(&op_types).to_string();
        let op = b.add_named_entity(
            "MachiningOperation",
            format!("op_{i}"),
            vec![
                ("operationType".to_string(), operation_type),
                ("cuttingSpeed".to_string(), "50".to_string()),
                ("feedRate".to_string(), "0.15".to_string()),
            ],
        );
        ops.push(op);

        let example = b.add_named_entity(
            "Example",
            format!("ex_{i}"),
            vec![("description".to_string(), "synthetic example".to_string())],
        );
        b.rel("material", example, mat, 1.0);
        b.rel("operation", example, op, 1.0);

        // Minimal outcome assignment for demos.
        let outcome = if mat_name == "Titanium6Al4V" {
            outcome_tool_wear
        } else {
            outcome_success
        };
        b.rel("outcome", example, outcome, 1.0);

        b.rel("hasMaterial", op, mat, 1.0);
        b.rel("usesTool", op, tool, 1.0);
        b.rel("suitableFor", tool, mat, 0.8);

        b.rel("demonstrates", example, concept_thermal, 0.6);
    }

    // Material → concept links (for “via concept” derivations).
    b.rel("involvesConcept", materials[0], concept_thermal, 0.9);
    b.rel("involvesConcept", materials[0], concept_work, 0.7);

    let entity_time = start.elapsed();

    let start = Instant::now();

    // Direct guardrails (an untrusted “engine” would compute these, later certified).
    b.rel("guardrailedBy", ops[0], guideline_ti, 0.95);
    b.rel("guardrailedBy", ops[0], guideline_coolant, 0.6);

    // Record two derivations from op_0 to TitaniumSpeed.
    let p_direct = b.add_named_entity(
        "PathWitness",
        "path_op0_to_titanium_speed_direct",
        vec![("repr".to_string(), "guardrailedBy".to_string())],
    );
    b.rel("from", p_direct, ops[0], 1.0);
    b.rel("to", p_direct, guideline_ti, 1.0);

    let p_via = b.add_named_entity(
        "PathWitness",
        "path_op0_to_titanium_speed_via_concept",
        vec![(
            "repr".to_string(),
            "hasMaterial/involvesConcept/explains".to_string(),
        )],
    );
    b.rel("from", p_via, ops[0], 1.0);
    b.rel("to", p_via, guideline_ti, 1.0);
    b.rel("via", p_via, materials[0], 1.0);
    b.rel("via", p_via, concept_thermal, 1.0);

    let hom = b.add_named_entity(
        "Homotopy",
        "homotopy_op0_guardrail_vs_concept",
        vec![(
            "repr".to_string(),
            "guardrailedBy ~ hasMaterial/involvesConcept/explains".to_string(),
        )],
    );
    b.rel("from", hom, ops[0], 1.0);
    b.rel("to", hom, guideline_ti, 1.0);
    b.rel("lhs", hom, p_direct, 1.0);
    b.rel("rhs", hom, p_via, 1.0);
    b.equiv(p_direct, p_via, "HomotopicPath");

    let relation_time = start.elapsed();

    let example_commands = vec![
        "q select ?g where name(\"op_0\") -guardrailedBy-> ?g limit 10".to_string(),
        "q select ?g where name(\"op_0\") -hasMaterial/involvesConcept/explains-> ?g max_hops 6 limit 10".to_string(),
        "q select ?c where name(\"ChatterVibration\") -requires-> ?c limit 10".to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"op_0\") limit 10".to_string(),
    ];

    import_scenario_docchunks(
        &mut b,
        "machinist_learning",
        "Machinist learning: materials/tools/operations + concepts + safety guidelines, with explicit homotopy between alternative derivations (direct guardrail vs via concept chain).",
        &[("MachiningOperation", "op_0"), ("Concept", "ChatterVibration")],
        Vec::new(),
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = b;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "machinist_learning".to_string(),
        description: "Machinist learning: materials/tools/operations + concepts + safety guidelines, with explicit homotopy between alternative derivations (direct guardrail vs via concept chain).".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn build_schema_evolution_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario aligned with `examples/ontology/SchemaEvolution.axi`:
    // - Schemas, migrations, schema equivalences
    // - Migration composition (commuting diagram) recorded as a Homotopy

    let mut b = ScenarioBuilder::new(seed, index_depth)?;

    let start = Instant::now();

    let proof_iso = b.add_named_entity("EquivProof", "IsoProof", Vec::new());
    let _proof_lossless = b.add_named_entity("EquivProof", "LosslessProof", Vec::new());
    let _proof_sem = b.add_named_entity("EquivProof", "SemanticEquiv", Vec::new());

    let change_types = [
        "AddTable",
        "DropTable",
        "AddColumn",
        "DropColumn",
        "Normalize",
        "Denormalize",
        "Rename",
        "TypeChange",
    ];
    let mut change_type_ids: HashMap<String, u32> = HashMap::new();
    for ct in change_types {
        let id = b.add_named_entity("ChangeType", ct, Vec::new());
        change_type_ids.insert(ct.to_string(), id);
    }

    // Change inverses (groupoid-ish).
    if let (Some(a), Some(b2)) = (
        change_type_ids.get("AddTable").copied(),
        change_type_ids.get("DropTable").copied(),
    ) {
        b.rel("inverseOf", a, b2, 1.0);
        b.rel("inverseOf", b2, a, 1.0);
    }
    if let (Some(a), Some(b2)) = (
        change_type_ids.get("AddColumn").copied(),
        change_type_ids.get("DropColumn").copied(),
    ) {
        b.rel("inverseOf", a, b2, 1.0);
        b.rel("inverseOf", b2, a, 1.0);
    }
    if let (Some(a), Some(b2)) = (
        change_type_ids.get("Normalize").copied(),
        change_type_ids.get("Denormalize").copied(),
    ) {
        b.rel("inverseOf", a, b2, 1.0);
        b.rel("inverseOf", b2, a, 1.0);
    }
    if let Some(rename) = change_type_ids.get("Rename").copied() {
        b.rel("inverseOf", rename, rename, 1.0);
    }

    let mut catalogs: Vec<(u32, u32, u32, u32, u32, u32)> = Vec::with_capacity(scale);

    for i in 0..scale {
        let s_v1 = b.add_named_entity("Schema", format!("ProductV1_{i}"), Vec::new());
        let s_v2 = b.add_named_entity("Schema", format!("ProductV2_{i}"), Vec::new());
        let s_v3 = b.add_named_entity("Schema", format!("ProductV3_{i}"), Vec::new());
        let s_v3a = b.add_named_entity("Schema", format!("ProductV3_alt_{i}"), Vec::new());
        let s_v4 = b.add_named_entity("Schema", format!("ProductV4_{i}"), Vec::new());

        let m_add = b.add_named_entity("Migration", format!("AddCategories_{i}"), Vec::new());
        let m_norm = b.add_named_entity("Migration", format!("NormalizeSKU_{i}"), Vec::new());
        let m_direct = b.add_named_entity("Migration", format!("DirectV1toV3_{i}"), Vec::new());

        connect_migration(&mut b, s_v1, m_add, s_v2);
        connect_migration(&mut b, s_v2, m_norm, s_v3);
        connect_migration(&mut b, s_v1, m_direct, s_v3);

        // A schema equivalence V3 ≃ V3_alt.
        let m_to_alt = b.add_named_entity("Migration", format!("V3toV3alt_{i}"), Vec::new());
        let m_from_alt = b.add_named_entity("Migration", format!("V3altToV3_{i}"), Vec::new());
        connect_migration(&mut b, s_v3, m_to_alt, s_v3a);
        connect_migration(&mut b, s_v3a, m_from_alt, s_v3);

        let eq = b.add_named_entity(
            "SchemaEquiv",
            format!("SchemaEquiv_V3_V3alt_{i}"),
            Vec::new(),
        );
        b.rel("left", eq, s_v3, 1.0);
        b.rel("right", eq, s_v3a, 1.0);
        b.rel("forward", eq, m_to_alt, 1.0);
        b.rel("backward", eq, m_from_alt, 1.0);
        b.rel("proof", eq, proof_iso, 1.0);
        b.equiv(s_v3, s_v3a, "SchemaEquiv");

        // Change typing for migrations.
        if let Some(add_table) = change_type_ids.get("AddTable").copied() {
            b.rel("changes", m_add, add_table, 1.0);
        }
        if let Some(norm) = change_type_ids.get("Normalize").copied() {
            b.rel("changes", m_norm, norm, 1.0);
        }
        if let Some(rename) = change_type_ids.get("Rename").copied() {
            b.rel("changes", m_to_alt, rename, 1.0);
        }

        // Instances + data migration.
        let inst_v1 = b.add_named_entity("Instance", format!("Products_Jan2020_{i}"), Vec::new());
        let inst_v3 = b.add_named_entity("Instance", format!("Products_Jan2023_{i}"), Vec::new());
        let inst_v3a = b.add_named_entity(
            "Instance",
            format!("Products_Jan2023_migrated_{i}"),
            Vec::new(),
        );
        b.rel("instanceOf", inst_v1, s_v1, 1.0);
        b.rel("instanceOf", inst_v3, s_v3, 1.0);
        b.rel("instanceOf", inst_v3a, s_v3a, 1.0);

        let dm = b.add_named_entity("DataMigration", format!("TransformJan2023_{i}"), Vec::new());
        b.rel("migration", dm, m_to_alt, 1.0);
        b.rel("sourceData", dm, inst_v3, 1.0);
        b.rel("targetData", dm, inst_v3a, 1.0);

        // A commuting diagram witness: AddCategories ; NormalizeSKU ≡ DirectV1toV3.
        let p_direct = b.add_named_entity(
            "PathWitness",
            format!("path_ProductV1_to_ProductV3_direct_{i}"),
            vec![("repr".to_string(), format!("DirectV1toV3_{i}"))],
        );
        b.rel("from", p_direct, s_v1, 1.0);
        b.rel("to", p_direct, s_v3, 1.0);
        b.rel("via", p_direct, m_direct, 1.0);

        let p_via = b.add_named_entity(
            "PathWitness",
            format!("path_ProductV1_to_ProductV3_via_ProductV2_{i}"),
            vec![(
                "repr".to_string(),
                format!("AddCategories_{i}/NormalizeSKU_{i}"),
            )],
        );
        b.rel("from", p_via, s_v1, 1.0);
        b.rel("to", p_via, s_v3, 1.0);
        b.rel("via", p_via, s_v2, 1.0);

        let hom = b.add_named_entity(
            "Homotopy",
            format!("homotopy_ProductV1_to_ProductV3_{i}"),
            vec![(
                "repr".to_string(),
                "AddCategories/NormalizeSKU ~ DirectV1toV3".to_string(),
            )],
        );
        b.rel("from", hom, s_v1, 1.0);
        b.rel("to", hom, s_v3, 1.0);
        b.rel("lhs", hom, p_direct, 1.0);
        b.rel("rhs", hom, p_via, 1.0);
        b.equiv(p_direct, p_via, "HomotopicPath");

        let m_denorm = b.add_named_entity("Migration", format!("Denormalize_{i}"), Vec::new());
        connect_migration(&mut b, s_v3, m_denorm, s_v4);

        catalogs.push((s_v1, s_v2, s_v3, s_v3a, s_v4, eq));
    }

    let entity_time = start.elapsed();

    // All relations are created during entity construction for this scenario.
    let relation_time = Duration::from_millis(0);

    let example_commands = vec![
        "q select ?s where name(\"ProductV1_0\") -outgoingMigration/toSchema-> ?s limit 10"
            .to_string(),
        "q select ?s where name(\"ProductV1_0\") -outgoingMigration/toSchema/outgoingMigration/toSchema-> ?s max_hops 6 limit 10".to_string(),
        "q select ?eq where ?eq is SchemaEquiv, ?eq -left-> name(\"ProductV3_0\") limit 10"
            .to_string(),
        "q select ?inv where name(\"Normalize\") -inverseOf-> ?inv limit 10".to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"ProductV1_0\") limit 10"
            .to_string(),
    ];

    let _ = catalogs;

    import_scenario_docchunks(
        &mut b,
        "schema_evolution",
        "Schema evolution: schemas/migrations/schema-equivalences + explicit homotopy for migration composition (commuting diagram).",
        &[("Schema", "ProductV1_0"), ("Migration", "AddCategories_0")],
        Vec::new(),
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = b;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "schema_evolution".to_string(),
        description: "Schema evolution: schemas/migrations/schema-equivalences + explicit homotopy for migration composition (commuting diagram).".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn build_proto_api_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario aligned with `axiograph-ingest-proto` and the fixture in
    // `examples/proto/large_api/descriptor.json`.
    //
    // Goals:
    // - typed API surface: Proto* entities + HttpEndpoint + ApiWorkflow
    // - both "documented" (HTTP annotations) and "tacit" (workflow grouping) structure
    // - explicit homotopies for "two ways to identify the same thing"
    //   (doc mentions RPC directly vs doc mentions HTTP endpoint)

    struct ProtoFieldSpec {
        message_id: u32,
        field_id: u32,
        field_type_message: Option<u32>,
    }

    struct ProtoRpcSpec {
        rpc_id: u32,
        request_message_id: u32,
        response_message_id: u32,
        http_endpoint_id: u32,
        http_method: String,
    }

    struct ServiceBundle {
        package_id: u32,
        file_id: u32,
        service_id: u32,
        workflow_id: u32,
        doc_id: u32,
        message_ids: Vec<u32>,
        field_specs: Vec<ProtoFieldSpec>,
        rpcs: Vec<ProtoRpcSpec>,
        // Doc → RPC homotopy.
        doc_direct_path_id: u32,
        doc_via_http_path_id: u32,
        doc_homotopy_id: u32,
        // Order homotopy (heuristic vs observed).
        order_suggested_path_id: u32,
        order_observed_path_id: u32,
        order_homotopy_id: u32,
    }

    let mut builder = ScenarioBuilder::new(seed, index_depth)?;

    let start = Instant::now();

    let mut bundles: Vec<ServiceBundle> = Vec::with_capacity(scale);

    for i in 0..scale {
        let package_name = format!("acme.svc{i}.v1");
        let file_name = format!("acme/svc{i}/v1/service{i}.proto");
        let service_fqn = format!("{package_name}.Service{i}");

        let package_id = builder.add_named_entity("ProtoPackage", package_name.clone(), Vec::new());
        let file_id = builder.add_named_entity(
            "ProtoFile",
            file_name.clone(),
            vec![
                ("package".to_string(), package_name.clone()),
                ("syntax".to_string(), "proto3".to_string()),
            ],
        );
        let service_id = builder.add_named_entity(
            "ProtoService",
            service_fqn.clone(),
            vec![
                ("package".to_string(), package_name.clone()),
                ("file".to_string(), file_name.clone()),
                ("fqn".to_string(), service_fqn.clone()),
            ],
        );

        // Resource message and a few request/response messages.
        let widget_fqn = format!("{package_name}.Widget");
        let widget_message_id = builder.add_named_entity(
            "ProtoMessage",
            widget_fqn.clone(),
            vec![
                ("package".to_string(), package_name.clone()),
                ("file".to_string(), file_name.clone()),
                ("fqn".to_string(), widget_fqn.clone()),
            ],
        );

        let message_names = [
            ("CreateWidgetRequest", "ProtoMessage"),
            ("CreateWidgetResponse", "ProtoMessage"),
            ("GetWidgetRequest", "ProtoMessage"),
            ("GetWidgetResponse", "ProtoMessage"),
            ("DeleteWidgetRequest", "ProtoMessage"),
            ("DeleteWidgetResponse", "ProtoMessage"),
        ];
        let mut message_ids: Vec<u32> = Vec::with_capacity(1 + message_names.len());
        message_ids.push(widget_message_id);

        let mut named_message_ids: HashMap<String, u32> = HashMap::new();
        named_message_ids.insert("Widget".to_string(), widget_message_id);

        for (suffix, ty) in message_names {
            let fqn = format!("{package_name}.{suffix}");
            let id = builder.add_named_entity(
                ty,
                fqn.clone(),
                vec![
                    ("package".to_string(), package_name.clone()),
                    ("file".to_string(), file_name.clone()),
                    ("fqn".to_string(), fqn.clone()),
                ],
            );
            message_ids.push(id);
            named_message_ids.insert(suffix.to_string(), id);
        }

        // Fields (minimal, but enough for graph structure).
        let mut field_specs: Vec<ProtoFieldSpec> = Vec::new();

        let widget_id_field = builder.add_named_entity(
            "ProtoField",
            format!("{widget_fqn}.widget_id"),
            vec![
                ("message_fqn".to_string(), widget_fqn.clone()),
                ("field_name".to_string(), "widget_id".to_string()),
                ("number".to_string(), "1".to_string()),
                ("type".to_string(), "TYPE_STRING".to_string()),
            ],
        );
        field_specs.push(ProtoFieldSpec {
            message_id: widget_message_id,
            field_id: widget_id_field,
            field_type_message: None,
        });

        let widget_status_field = builder.add_named_entity(
            "ProtoField",
            format!("{widget_fqn}.status"),
            vec![
                ("message_fqn".to_string(), widget_fqn.clone()),
                ("field_name".to_string(), "status".to_string()),
                ("number".to_string(), "2".to_string()),
                ("type".to_string(), "TYPE_STRING".to_string()),
            ],
        );
        field_specs.push(ProtoFieldSpec {
            message_id: widget_message_id,
            field_id: widget_status_field,
            field_type_message: None,
        });

        // request: widget_id, response: widget (typed)
        for (req, resp) in [
            ("CreateWidgetRequest", "CreateWidgetResponse"),
            ("GetWidgetRequest", "GetWidgetResponse"),
            ("DeleteWidgetRequest", "DeleteWidgetResponse"),
        ] {
            let req_id = *named_message_ids.get(req).expect("request message exists");
            let req_field = builder.add_named_entity(
                "ProtoField",
                format!("{package_name}.{req}.widget_id"),
                vec![
                    ("message_fqn".to_string(), format!("{package_name}.{req}")),
                    ("field_name".to_string(), "widget_id".to_string()),
                    ("number".to_string(), "1".to_string()),
                    ("type".to_string(), "TYPE_STRING".to_string()),
                ],
            );
            field_specs.push(ProtoFieldSpec {
                message_id: req_id,
                field_id: req_field,
                field_type_message: None,
            });

            let resp_id = *named_message_ids
                .get(resp)
                .expect("response message exists");
            let resp_field = builder.add_named_entity(
                "ProtoField",
                format!("{package_name}.{resp}.widget"),
                vec![
                    ("message_fqn".to_string(), format!("{package_name}.{resp}")),
                    ("field_name".to_string(), "widget".to_string()),
                    ("number".to_string(), "1".to_string()),
                    ("type".to_string(), "TYPE_MESSAGE".to_string()),
                    ("type_name".to_string(), widget_fqn.clone()),
                ],
            );
            field_specs.push(ProtoFieldSpec {
                message_id: resp_id,
                field_id: resp_field,
                field_type_message: Some(widget_message_id),
            });
        }

        // RPCs + HTTP endpoints.
        let rpc_specs = [
            (
                "CreateWidget",
                "CreateWidgetRequest",
                "CreateWidgetResponse",
                "POST",
                format!("/v1/svc{i}/widgets"),
            ),
            (
                "GetWidget",
                "GetWidgetRequest",
                "GetWidgetResponse",
                "GET",
                format!("/v1/svc{i}/widgets/{{widget_id}}"),
            ),
            (
                "DeleteWidget",
                "DeleteWidgetRequest",
                "DeleteWidgetResponse",
                "DELETE",
                format!("/v1/svc{i}/widgets/{{widget_id}}"),
            ),
        ];

        let mut rpcs: Vec<ProtoRpcSpec> = Vec::new();
        for (method_name, request_name, response_name, http_method, http_path) in rpc_specs {
            let rpc_fqn = format!("{service_fqn}.{method_name}");
            let request_fqn = format!("{package_name}.{request_name}");
            let response_fqn = format!("{package_name}.{response_name}");

            let rpc_id = builder.add_named_entity(
                "ProtoRpc",
                rpc_fqn.clone(),
                vec![
                    ("package".to_string(), package_name.clone()),
                    ("file".to_string(), file_name.clone()),
                    ("service_fqn".to_string(), service_fqn.clone()),
                    ("rpc_fqn".to_string(), rpc_fqn.clone()),
                    ("method_name".to_string(), method_name.to_string()),
                    ("input_type".to_string(), request_fqn.clone()),
                    ("output_type".to_string(), response_fqn.clone()),
                    ("http_method".to_string(), http_method.to_string()),
                    ("http_path".to_string(), http_path.clone()),
                ],
            );

            let endpoint_key = format!("{http_method} {http_path}");
            let http_endpoint_id = builder.add_named_entity(
                "HttpEndpoint",
                endpoint_key.clone(),
                vec![
                    ("method".to_string(), http_method.to_string()),
                    ("path".to_string(), http_path.clone()),
                ],
            );

            let request_message_id = *named_message_ids
                .get(request_name)
                .expect("request message exists");
            let response_message_id = *named_message_ids
                .get(response_name)
                .expect("response message exists");

            rpcs.push(ProtoRpcSpec {
                rpc_id,
                request_message_id,
                response_message_id,
                http_endpoint_id,
                http_method: http_method.to_string(),
            });
        }

        // Tacit workflow grouping (like `axiograph-ingest-proto`).
        let workflow_id = builder.add_named_entity(
            "ApiWorkflow",
            format!("WidgetLifecycle_{i}"),
            vec![("resource_fqn".to_string(), widget_fqn.clone())],
        );

        // A doc node that mentions both the rpc and its HTTP endpoint.
        let doc_id = builder.add_named_entity(
            "Doc",
            format!("doc_proto_api_{i}"),
            vec![("kind".to_string(), "api_docs".to_string())],
        );

        // Homotopy: doc → rpc (direct) vs doc → http endpoint → rpc.
        let doc_direct_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_doc_proto_api_{i}_direct"),
            vec![("repr".to_string(), "mentions_rpc".to_string())],
        );
        let doc_via_http_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_doc_proto_api_{i}_via_http"),
            vec![(
                "repr".to_string(),
                "mentions_http_endpoint/proto_http_endpoint_of_rpc".to_string(),
            )],
        );
        let doc_homotopy_id = builder.add_named_entity(
            "Homotopy",
            format!("homotopy_doc_proto_api_{i}"),
            vec![(
                "repr".to_string(),
                "mentions_rpc ~ mentions_http_endpoint/proto_http_endpoint_of_rpc".to_string(),
            )],
        );

        // Homotopy: heuristic ordering vs observed ordering.
        let order_suggested_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_CreateWidget_to_GetWidget_suggested_{i}"),
            vec![("repr".to_string(), "workflow_suggests_order".to_string())],
        );
        let order_observed_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_CreateWidget_to_GetWidget_observed_{i}"),
            vec![("repr".to_string(), "observed_next".to_string())],
        );
        let order_homotopy_id = builder.add_named_entity(
            "Homotopy",
            format!("homotopy_CreateWidget_to_GetWidget_{i}"),
            vec![(
                "repr".to_string(),
                "workflow_suggests_order ~ observed_next".to_string(),
            )],
        );

        bundles.push(ServiceBundle {
            package_id,
            file_id,
            service_id,
            workflow_id,
            doc_id,
            message_ids,
            field_specs,
            rpcs,
            doc_direct_path_id,
            doc_via_http_path_id,
            doc_homotopy_id,
            order_suggested_path_id,
            order_observed_path_id,
            order_homotopy_id,
        });
    }

    let entity_time = start.elapsed();

    let start = Instant::now();

    for (i, bundle) in bundles.iter().enumerate() {
        // file/package/service structure
        builder.rel(
            "proto_file_in_package",
            bundle.file_id,
            bundle.package_id,
            0.98,
        );
        builder.rel(
            "proto_file_declares_service",
            bundle.file_id,
            bundle.service_id,
            0.98,
        );

        // file → messages
        for &m in &bundle.message_ids {
            builder.rel("proto_file_declares_message", bundle.file_id, m, 0.98);
        }

        // message → fields
        for field in &bundle.field_specs {
            builder.rel(
                "proto_message_has_field",
                field.message_id,
                field.field_id,
                0.98,
            );
            if let Some(type_msg) = field.field_type_message {
                builder.rel("proto_field_type_message", field.field_id, type_msg, 0.98);
            }
        }

        // workflow
        builder.rel(
            "proto_service_has_workflow",
            bundle.service_id,
            bundle.workflow_id,
            0.60,
        );

        for rpc in &bundle.rpcs {
            builder.rel("proto_service_has_rpc", bundle.service_id, rpc.rpc_id, 0.98);
            builder.rel(
                "proto_rpc_request",
                rpc.rpc_id,
                rpc.request_message_id,
                0.98,
            );
            builder.rel(
                "proto_rpc_response",
                rpc.rpc_id,
                rpc.response_message_id,
                0.98,
            );
            builder.rel(
                "proto_rpc_http_endpoint",
                rpc.rpc_id,
                rpc.http_endpoint_id,
                0.98,
            );
            builder.rel(
                "proto_http_endpoint_of_rpc",
                rpc.http_endpoint_id,
                rpc.rpc_id,
                0.98,
            );

            // include in workflow (tacit)
            builder.rel(
                "workflow_includes_rpc",
                bundle.workflow_id,
                rpc.rpc_id,
                0.60,
            );
        }

        // Choose a deterministic "primary" rpc (GetWidget) for doc mentions.
        let get_rpc = bundle
            .rpcs
            .iter()
            .find(|r| r.http_method == "GET")
            .expect("GetWidget exists");
        builder.rel("mentions_rpc", bundle.doc_id, get_rpc.rpc_id, 0.85);
        builder.rel(
            "mentions_http_endpoint",
            bundle.doc_id,
            get_rpc.http_endpoint_id,
            0.85,
        );

        // Heuristic order: CreateWidget -> GetWidget -> DeleteWidget.
        let create_rpc = bundle
            .rpcs
            .iter()
            .find(|r| r.http_method == "POST")
            .expect("CreateWidget exists");
        let delete_rpc = bundle
            .rpcs
            .iter()
            .find(|r| r.http_method == "DELETE")
            .expect("DeleteWidget exists");

        builder.rel(
            "workflow_suggests_order",
            create_rpc.rpc_id,
            get_rpc.rpc_id,
            0.55,
        );
        builder.rel(
            "workflow_suggests_order",
            get_rpc.rpc_id,
            delete_rpc.rpc_id,
            0.55,
        );

        // Observed: for this synthetic scenario we assert the same ordering (but
        // the confidence could differ).
        builder.rel("observed_next", create_rpc.rpc_id, get_rpc.rpc_id, 0.70);
        builder.rel("observed_next", get_rpc.rpc_id, delete_rpc.rpc_id, 0.70);

        // Doc homotopy (direct mention vs via http endpoint).
        builder.rel("from", bundle.doc_direct_path_id, bundle.doc_id, 1.0);
        builder.rel("to", bundle.doc_direct_path_id, get_rpc.rpc_id, 1.0);

        builder.rel("from", bundle.doc_via_http_path_id, bundle.doc_id, 1.0);
        builder.rel("to", bundle.doc_via_http_path_id, get_rpc.rpc_id, 1.0);
        builder.rel(
            "via",
            bundle.doc_via_http_path_id,
            get_rpc.http_endpoint_id,
            1.0,
        );

        builder.rel("from", bundle.doc_homotopy_id, bundle.doc_id, 1.0);
        builder.rel("to", bundle.doc_homotopy_id, get_rpc.rpc_id, 1.0);
        builder.rel(
            "lhs",
            bundle.doc_homotopy_id,
            bundle.doc_direct_path_id,
            1.0,
        );
        builder.rel(
            "rhs",
            bundle.doc_homotopy_id,
            bundle.doc_via_http_path_id,
            1.0,
        );
        builder.equiv(
            bundle.doc_direct_path_id,
            bundle.doc_via_http_path_id,
            "HomotopicPath",
        );

        // Order homotopy (heuristic vs observed).
        builder.rel(
            "from",
            bundle.order_suggested_path_id,
            create_rpc.rpc_id,
            1.0,
        );
        builder.rel("to", bundle.order_suggested_path_id, get_rpc.rpc_id, 1.0);

        builder.rel(
            "from",
            bundle.order_observed_path_id,
            create_rpc.rpc_id,
            1.0,
        );
        builder.rel("to", bundle.order_observed_path_id, get_rpc.rpc_id, 1.0);

        builder.rel("from", bundle.order_homotopy_id, create_rpc.rpc_id, 1.0);
        builder.rel("to", bundle.order_homotopy_id, get_rpc.rpc_id, 1.0);
        builder.rel(
            "lhs",
            bundle.order_homotopy_id,
            bundle.order_suggested_path_id,
            1.0,
        );
        builder.rel(
            "rhs",
            bundle.order_homotopy_id,
            bundle.order_observed_path_id,
            1.0,
        );
        builder.equiv(
            bundle.order_suggested_path_id,
            bundle.order_observed_path_id,
            "HomotopicPath",
        );

        // A tiny cross-service link so "follow" can walk across bundles.
        let next = (i + 1) % bundles.len();
        builder.rel("calls", bundle.service_id, bundles[next].service_id, 0.40);
    }

    let relation_time = start.elapsed();

    let example_commands = vec![
        "q select ?svc where ?svc is ProtoService limit 10".to_string(),
        "q select ?rpc where name(\"acme.svc0.v1.Service0\") -proto_service_has_rpc-> ?rpc limit 10"
            .to_string(),
        "q select ?ep where name(\"acme.svc0.v1.Service0.GetWidget\") -proto_rpc_http_endpoint-> ?ep limit 10"
            .to_string(),
        "q select ?rpc where name(\"doc_proto_api_0\") -mentions_rpc-> ?rpc limit 10".to_string(),
        "q select ?rpc where name(\"doc_proto_api_0\") -mentions_http_endpoint/proto_http_endpoint_of_rpc-> ?rpc max_hops 3 limit 10".to_string(),
        "q select ?w where ?w is ApiWorkflow, ?w -workflow_includes_rpc-> name(\"acme.svc0.v1.Service0.CreateWidget\") limit 10".to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"doc_proto_api_0\") limit 10".to_string(),
        "q select ?dst where name(\"acme.svc0.v1.Service0\") -calls-> ?dst limit 10".to_string(),
    ];

    // Provide DocChunks that link to the generated proto surface so the LLM can
    // cite and users can `open chunk ...` in the REPL/HTML explorer.
    let mut extra_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    let doc_id = scenario_doc_id("proto_api");
    for svc_i in 0..scale.max(1).min(8) {
        let service_fqn = format!("acme.svc{svc_i}.v1.Service{svc_i}");
        let rpc_fqns = ["CreateWidget", "GetWidget", "DeleteWidget"]
            .into_iter()
            .map(|m| format!("{service_fqn}.{m}"))
            .collect::<Vec<_>>();

        extra_chunks.push(axiograph_ingest_docs::Chunk {
            chunk_id: format!("doc_proto_service_{svc_i}"),
            document_id: doc_id.clone(),
            page: None,
            span_id: format!("proto_service_{svc_i}"),
            text: format!(
                "Proto service: {service_fqn}\nRPCs: {}",
                rpc_fqns.iter().take(8).cloned().collect::<Vec<_>>().join(", ")
            ),
            bbox: None,
            metadata: HashMap::from([
                ("kind".to_string(), "proto_service".to_string()),
                ("fqn".to_string(), service_fqn.clone()),
                ("about_type".to_string(), "ProtoService".to_string()),
                ("about_name".to_string(), service_fqn.clone()),
            ]),
        });

        for (rpc_i, rpc_fqn) in rpc_fqns.iter().take(8).enumerate() {
            let rpc_id = builder.ids_by_name.get(rpc_fqn).copied();
            let (http_method, http_path) = rpc_id
                .and_then(|id| builder.db.get_entity(id))
                .map(|view| {
                    let method = view
                        .attrs
                        .get("http_method")
                        .cloned()
                        .unwrap_or_else(|| "?".to_string());
                    let path = view
                        .attrs
                        .get("http_path")
                        .cloned()
                        .unwrap_or_else(|| "?".to_string());
                    (method, path)
                })
                .unwrap_or_else(|| ("?".to_string(), "?".to_string()));

            extra_chunks.push(axiograph_ingest_docs::Chunk {
                chunk_id: format!("doc_proto_rpc_{svc_i}_{rpc_i}"),
                document_id: doc_id.clone(),
                page: None,
                span_id: format!("proto_rpc_{svc_i}_{rpc_i}"),
                text: format!(
                    "Proto RPC: {rpc_fqn}\nHTTP: {http_method} {http_path}\n\nThis is a synthetic API surface used for ontology engineering demos."
                ),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_rpc".to_string()),
                    ("fqn".to_string(), rpc_fqn.clone()),
                    ("about_type".to_string(), "ProtoRpc".to_string()),
                    ("about_name".to_string(), rpc_fqn.clone()),
                ]),
            });
        }
    }

    import_scenario_docchunks(
        &mut builder,
        "proto_api",
        "Proto/gRPC API surface: ProtoPackage/ProtoFile/ProtoService/ProtoRpc/ProtoMessage/ProtoField + HttpEndpoint + ApiWorkflow, with explicit homotopies for doc-identification paths and ordering.",
        &[
            ("ProtoService", "acme.svc0.v1.Service0"),
            ("ProtoRpc", "acme.svc0.v1.Service0.GetWidget"),
            ("Doc", "doc_proto_api_0"),
        ],
        extra_chunks,
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = builder;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "proto_api".to_string(),
        description: "Proto/gRPC API surface: ProtoPackage/ProtoFile/ProtoService/ProtoRpc/ProtoMessage/ProtoField + HttpEndpoint + ApiWorkflow, with explicit homotopies for doc-identification paths and ordering.".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn build_proto_api_business_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A proto/gRPC API fleet in a (somewhat) realistic microservice/business context.
    //
    // Why this scenario exists (vs `proto_api`)
    // -----------------------------------------
    // `proto_api` is designed to align with the proto ingestion fixture and to keep
    // names stable for tests (`acme.svc0.v1.Service0`, etc).
    //
    // For demos/tutorials/viz, it’s useful to have:
    // - human-recognizable service names (orders/payments/shipping/...),
    // - a richer cross-service call graph,
    // - DocChunks for *dozens* of services so LLM grounding has citations.

    struct ProtoFieldSpec {
        message_id: u32,
        field_id: u32,
        field_type_message: Option<u32>,
    }

    struct ProtoRpcSpec {
        rpc_id: u32,
        rpc_fqn: String,
        request_message_id: u32,
        response_message_id: u32,
        http_endpoint_id: u32,
        method_name: String,
        http_method: String,
        http_path: String,
    }

    struct ServiceBundle {
        domain: String,
        package_id: u32,
        file_id: u32,
        service_id: u32,
        workflow_id: u32,
        doc_id: u32,
        resource_fqn: String,
        resource_message_id: u32,
        message_ids: Vec<u32>,
        field_specs: Vec<ProtoFieldSpec>,
        rpcs: Vec<ProtoRpcSpec>,
        // Doc → RPC homotopy.
        doc_direct_path_id: u32,
        doc_via_http_path_id: u32,
        doc_homotopy_id: u32,
        // Order homotopy (heuristic vs observed).
        order_suggested_path_id: u32,
        order_observed_path_id: u32,
        order_homotopy_id: u32,
    }

    #[derive(Clone)]
    struct BusinessServiceSpec {
        domain: &'static str,
        service_name: &'static str,
        resource_name: &'static str,
        dependencies: &'static [&'static str],
    }

    // A minimal set of “business-shaped” services. If `scale` is bigger than
    // this list, we fall back to `acme.svcN.v1.ServiceN`-like names.
    let business: Vec<BusinessServiceSpec> = vec![
        BusinessServiceSpec {
            domain: "auth",
            service_name: "AuthService",
            resource_name: "Session",
            dependencies: &["users"],
        },
        BusinessServiceSpec {
            domain: "users",
            service_name: "UserService",
            resource_name: "User",
            dependencies: &[],
        },
        BusinessServiceSpec {
            domain: "catalog",
            service_name: "CatalogService",
            resource_name: "Product",
            dependencies: &[],
        },
        BusinessServiceSpec {
            domain: "pricing",
            service_name: "PricingService",
            resource_name: "PriceQuote",
            dependencies: &["catalog"],
        },
        BusinessServiceSpec {
            domain: "inventory",
            service_name: "InventoryService",
            resource_name: "StockReservation",
            dependencies: &["catalog"],
        },
        BusinessServiceSpec {
            domain: "orders",
            service_name: "OrderService",
            resource_name: "Order",
            dependencies: &["users", "catalog", "pricing", "inventory", "payments", "shipping"],
        },
        BusinessServiceSpec {
            domain: "payments",
            service_name: "PaymentService",
            resource_name: "Payment",
            dependencies: &["fraud", "ledger"],
        },
        BusinessServiceSpec {
            domain: "fraud",
            service_name: "FraudService",
            resource_name: "RiskAssessment",
            dependencies: &["users"],
        },
        BusinessServiceSpec {
            domain: "ledger",
            service_name: "LedgerService",
            resource_name: "LedgerEntry",
            dependencies: &[],
        },
        BusinessServiceSpec {
            domain: "shipping",
            service_name: "ShippingService",
            resource_name: "Shipment",
            dependencies: &["inventory", "notifications"],
        },
        BusinessServiceSpec {
            domain: "notifications",
            service_name: "NotificationService",
            resource_name: "Notification",
            dependencies: &[],
        },
        BusinessServiceSpec {
            domain: "fulfillment",
            service_name: "FulfillmentService",
            resource_name: "FulfillmentTask",
            dependencies: &["inventory", "shipping", "notifications"],
        },
        BusinessServiceSpec {
            domain: "returns",
            service_name: "ReturnsService",
            resource_name: "Return",
            dependencies: &["orders", "payments", "shipping", "notifications"],
        },
        BusinessServiceSpec {
            domain: "support",
            service_name: "SupportService",
            resource_name: "Ticket",
            dependencies: &["users"],
        },
        BusinessServiceSpec {
            domain: "analytics",
            service_name: "AnalyticsService",
            resource_name: "Event",
            dependencies: &[],
        },
    ];

    let mut deps_by_domain: HashMap<&'static str, &'static [&'static str]> = HashMap::new();
    for s in &business {
        deps_by_domain.insert(s.domain, s.dependencies);
    }

    let mut builder = ScenarioBuilder::new(seed, index_depth)?;

    let start = Instant::now();

    let mut bundles: Vec<ServiceBundle> = Vec::with_capacity(scale);

    for i in 0..scale {
        let domain: String;
        let service_name: String;
        let resource_name: String;

        if let Some(spec) = business.get(i) {
            domain = spec.domain.to_string();
            service_name = spec.service_name.to_string();
            resource_name = spec.resource_name.to_string();
        } else {
            domain = format!("svc{i}");
            service_name = format!("Service{i}");
            resource_name = "Widget".to_string();
        }

        let package_name = format!("acme.{domain}.v1");
        let file_name = format!("acme/{domain}/v1/{domain}.proto");
        let service_fqn = format!("{package_name}.{service_name}");

        let package_id = builder.add_named_entity("ProtoPackage", package_name.clone(), Vec::new());
        let file_id = builder.add_named_entity(
            "ProtoFile",
            file_name.clone(),
            vec![
                ("package".to_string(), package_name.clone()),
                ("syntax".to_string(), "proto3".to_string()),
            ],
        );
        let service_id = builder.add_named_entity(
            "ProtoService",
            service_fqn.clone(),
            vec![
                ("domain".to_string(), domain.clone()),
                ("package".to_string(), package_name.clone()),
                ("file".to_string(), file_name.clone()),
                ("fqn".to_string(), service_fqn.clone()),
            ],
        );

        // Resource message and a few request/response messages.
        let resource_fqn = format!("{package_name}.{resource_name}");
        let resource_message_id = builder.add_named_entity(
            "ProtoMessage",
            resource_fqn.clone(),
            vec![
                ("package".to_string(), package_name.clone()),
                ("file".to_string(), file_name.clone()),
                ("fqn".to_string(), resource_fqn.clone()),
            ],
        );

        // RPC set: CRUD-ish defaults, with a couple of domain-specific additions
        // for realism (payments/orders/shipping).
        let mut rpc_method_names: Vec<String> = Vec::new();

        if domain == "payments" {
            rpc_method_names.extend([
                "AuthorizePayment",
                "CapturePayment",
                "RefundPayment",
                "GetPayment",
            ].into_iter().map(|s| s.to_string()));
        } else if domain == "orders" {
            rpc_method_names.extend([
                "CreateOrder",
                "GetOrder",
                "CancelOrder",
                "ListOrders",
            ].into_iter().map(|s| s.to_string()));
        } else if domain == "shipping" {
            rpc_method_names.extend([
                "CreateShipment",
                "TrackShipment",
                "GetQuote",
                "CancelShipment",
            ].into_iter().map(|s| s.to_string()));
        } else {
            rpc_method_names.extend([
                format!("Create{resource_name}"),
                format!("Get{resource_name}"),
                format!("Delete{resource_name}"),
                format!("List{resource_name}s"),
            ]);
        }

        // Messages: request/response per RPC.
        let mut message_ids: Vec<u32> = Vec::new();
        message_ids.push(resource_message_id);

        let mut named_message_ids: HashMap<String, u32> = HashMap::new();
        named_message_ids.insert(resource_name.clone(), resource_message_id);

        for method in &rpc_method_names {
            for suffix in ["Request", "Response"] {
                let msg_name = format!("{method}{suffix}");
                let fqn = format!("{package_name}.{msg_name}");
                let id = builder.add_named_entity(
                    "ProtoMessage",
                    fqn.clone(),
                    vec![
                        ("package".to_string(), package_name.clone()),
                        ("file".to_string(), file_name.clone()),
                        ("fqn".to_string(), fqn.clone()),
                    ],
                );
                message_ids.push(id);
                named_message_ids.insert(msg_name, id);
            }
        }

        // Fields: give every resource an `id` and `status`, and every response a
        // typed `resource` payload.
        let mut field_specs: Vec<ProtoFieldSpec> = Vec::new();

        let resource_id_field = builder.add_named_entity(
            "ProtoField",
            format!("{resource_fqn}.id"),
            vec![
                ("message_fqn".to_string(), resource_fqn.clone()),
                ("field_name".to_string(), "id".to_string()),
                ("number".to_string(), "1".to_string()),
                ("type".to_string(), "TYPE_STRING".to_string()),
            ],
        );
        field_specs.push(ProtoFieldSpec {
            message_id: resource_message_id,
            field_id: resource_id_field,
            field_type_message: None,
        });

        let resource_status_field = builder.add_named_entity(
            "ProtoField",
            format!("{resource_fqn}.status"),
            vec![
                ("message_fqn".to_string(), resource_fqn.clone()),
                ("field_name".to_string(), "status".to_string()),
                ("number".to_string(), "2".to_string()),
                ("type".to_string(), "TYPE_STRING".to_string()),
            ],
        );
        field_specs.push(ProtoFieldSpec {
            message_id: resource_message_id,
            field_id: resource_status_field,
            field_type_message: None,
        });

        for method in &rpc_method_names {
            let req_name = format!("{method}Request");
            let resp_name = format!("{method}Response");
            let req_id = *named_message_ids.get(&req_name).expect("request message exists");
            let req_field = builder.add_named_entity(
                "ProtoField",
                format!("{package_name}.{req_name}.id"),
                vec![
                    (
                        "message_fqn".to_string(),
                        format!("{package_name}.{req_name}"),
                    ),
                    ("field_name".to_string(), "id".to_string()),
                    ("number".to_string(), "1".to_string()),
                    ("type".to_string(), "TYPE_STRING".to_string()),
                ],
            );
            field_specs.push(ProtoFieldSpec {
                message_id: req_id,
                field_id: req_field,
                field_type_message: None,
            });

            let resp_id = *named_message_ids
                .get(&resp_name)
                .expect("response message exists");
            let resp_field = builder.add_named_entity(
                "ProtoField",
                format!("{package_name}.{resp_name}.resource"),
                vec![
                    (
                        "message_fqn".to_string(),
                        format!("{package_name}.{resp_name}"),
                    ),
                    ("field_name".to_string(), "resource".to_string()),
                    ("number".to_string(), "1".to_string()),
                    ("type".to_string(), "TYPE_MESSAGE".to_string()),
                    ("type_name".to_string(), resource_fqn.clone()),
                ],
            );
            field_specs.push(ProtoFieldSpec {
                message_id: resp_id,
                field_id: resp_field,
                field_type_message: Some(resource_message_id),
            });
        }

        // RPCs + HTTP endpoints.
        let mut rpcs: Vec<ProtoRpcSpec> = Vec::new();
        for method_name in &rpc_method_names {
            let rpc_fqn = format!("{service_fqn}.{method_name}");
            let request_fqn = format!("{package_name}.{method_name}Request");
            let response_fqn = format!("{package_name}.{method_name}Response");

            let http_method = if method_name.starts_with("Get") || method_name.starts_with("List") || method_name == "TrackShipment" || method_name == "GetQuote" {
                "GET"
            } else if method_name.starts_with("Delete") || method_name.starts_with("Cancel") {
                "DELETE"
            } else if method_name.starts_with("Update") {
                "PATCH"
            } else {
                "POST"
            };

            let resource_path = domain.clone();
            let resource_plural = resource_name.to_ascii_lowercase() + "s";

            let http_path = if method_name.starts_with("List") || method_name == "GetQuote" {
                format!("/v1/{resource_path}/{resource_plural}")
            } else if method_name.starts_with("Get") || method_name == "TrackShipment" {
                format!("/v1/{resource_path}/{resource_plural}/{{id}}")
            } else if method_name == "CapturePayment" {
                "/v1/payments/{id}:capture".to_string()
            } else if method_name == "RefundPayment" {
                "/v1/payments/{id}:refund".to_string()
            } else if method_name.starts_with("Cancel") {
                format!("/v1/{resource_path}/{resource_plural}/{{id}}:cancel")
            } else {
                format!("/v1/{resource_path}/{resource_plural}")
            };

            let rpc_id = builder.add_named_entity(
                "ProtoRpc",
                rpc_fqn.clone(),
                vec![
                    ("domain".to_string(), domain.clone()),
                    ("package".to_string(), package_name.clone()),
                    ("file".to_string(), file_name.clone()),
                    ("service_fqn".to_string(), service_fqn.clone()),
                    ("rpc_fqn".to_string(), rpc_fqn.clone()),
                    ("method_name".to_string(), method_name.to_string()),
                    ("input_type".to_string(), request_fqn.clone()),
                    ("output_type".to_string(), response_fqn.clone()),
                    ("http_method".to_string(), http_method.to_string()),
                    ("http_path".to_string(), http_path.clone()),
                ],
            );

            let endpoint_key = format!("{http_method} {http_path}");
            let http_endpoint_id = builder.add_named_entity(
                "HttpEndpoint",
                endpoint_key.clone(),
                vec![
                    ("method".to_string(), http_method.to_string()),
                    ("path".to_string(), http_path.clone()),
                ],
            );

            let request_message_id = *named_message_ids
                .get(&format!("{method_name}Request"))
                .expect("request message exists");
            let response_message_id = *named_message_ids
                .get(&format!("{method_name}Response"))
                .expect("response message exists");

            rpcs.push(ProtoRpcSpec {
                rpc_id,
                rpc_fqn,
                request_message_id,
                response_message_id,
                http_endpoint_id,
                method_name: method_name.to_string(),
                http_method: http_method.to_string(),
                http_path,
            });
        }

        // Tacit workflow grouping.
        let workflow_id = builder.add_named_entity(
            "ApiWorkflow",
            format!("{resource_name}Lifecycle_{domain}"),
            vec![
                ("domain".to_string(), domain.clone()),
                ("resource_fqn".to_string(), resource_fqn.clone()),
            ],
        );

        // A doc node that mentions both the rpc and its HTTP endpoint.
        let doc_id = builder.add_named_entity(
            "Doc",
            format!("doc_{domain}_api"),
            vec![
                ("kind".to_string(), "api_docs".to_string()),
                ("domain".to_string(), domain.clone()),
                ("about_service".to_string(), service_fqn.clone()),
            ],
        );

        // Homotopy: doc → rpc (direct) vs doc → http endpoint → rpc.
        let doc_direct_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_doc_{domain}_direct"),
            vec![("repr".to_string(), "mentions_rpc".to_string())],
        );
        let doc_via_http_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_doc_{domain}_via_http"),
            vec![(
                "repr".to_string(),
                "mentions_http_endpoint/proto_http_endpoint_of_rpc".to_string(),
            )],
        );
        let doc_homotopy_id = builder.add_named_entity(
            "Homotopy",
            format!("homotopy_doc_{domain}"),
            vec![(
                "repr".to_string(),
                "mentions_rpc ~ mentions_http_endpoint/proto_http_endpoint_of_rpc".to_string(),
            )],
        );

        // Homotopy: heuristic ordering vs observed ordering.
        let order_suggested_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_Create_to_Get_suggested_{domain}"),
            vec![("repr".to_string(), "workflow_suggests_order".to_string())],
        );
        let order_observed_path_id = builder.add_named_entity(
            "PathWitness",
            format!("path_Create_to_Get_observed_{domain}"),
            vec![("repr".to_string(), "observed_next".to_string())],
        );
        let order_homotopy_id = builder.add_named_entity(
            "Homotopy",
            format!("homotopy_Create_to_Get_{domain}"),
            vec![("repr".to_string(), "workflow_suggests_order ~ observed_next".to_string())],
        );

        bundles.push(ServiceBundle {
            domain,
            package_id,
            file_id,
            service_id,
            workflow_id,
            doc_id,
            resource_fqn,
            resource_message_id,
            message_ids,
            field_specs,
            rpcs,
            doc_direct_path_id,
            doc_via_http_path_id,
            doc_homotopy_id,
            order_suggested_path_id,
            order_observed_path_id,
            order_homotopy_id,
        });
    }

    let entity_time = start.elapsed();

    let start = Instant::now();

    let mut domain_to_service: HashMap<String, u32> = HashMap::new();
    for b in &bundles {
        domain_to_service.insert(b.domain.clone(), b.service_id);
    }

    for (i, bundle) in bundles.iter().enumerate() {
        // file/package/service structure
        builder.rel(
            "proto_file_in_package",
            bundle.file_id,
            bundle.package_id,
            0.98,
        );
        builder.rel(
            "proto_file_declares_service",
            bundle.file_id,
            bundle.service_id,
            0.98,
        );

        // file → messages
        for &m in &bundle.message_ids {
            builder.rel("proto_file_declares_message", bundle.file_id, m, 0.98);
        }

        // message → fields
        for field in &bundle.field_specs {
            builder.rel("proto_message_has_field", field.message_id, field.field_id, 0.98);
            if let Some(type_msg) = field.field_type_message {
                builder.rel("proto_field_type_message", field.field_id, type_msg, 0.98);
            }
        }

        // workflow
        builder.rel(
            "proto_service_has_workflow",
            bundle.service_id,
            bundle.workflow_id,
            0.60,
        );

        for rpc in &bundle.rpcs {
            builder.rel("proto_service_has_rpc", bundle.service_id, rpc.rpc_id, 0.98);
            builder.rel("proto_rpc_request", rpc.rpc_id, rpc.request_message_id, 0.98);
            builder.rel("proto_rpc_response", rpc.rpc_id, rpc.response_message_id, 0.98);
            builder.rel("proto_rpc_http_endpoint", rpc.rpc_id, rpc.http_endpoint_id, 0.98);
            builder.rel("proto_http_endpoint_of_rpc", rpc.http_endpoint_id, rpc.rpc_id, 0.98);

            // include in workflow (tacit)
            builder.rel("workflow_includes_rpc", bundle.workflow_id, rpc.rpc_id, 0.60);
        }

        // Pick a deterministic “primary” rpc for doc mentions:
        // prefer Get*, otherwise fall back to the first.
        let primary_rpc = bundle
            .rpcs
            .iter()
            .find(|r| r.method_name.starts_with("Get"))
            .or_else(|| bundle.rpcs.first())
            .expect("at least one rpc exists");

        builder.rel("mentions_rpc", bundle.doc_id, primary_rpc.rpc_id, 0.85);
        builder.rel(
            "mentions_http_endpoint",
            bundle.doc_id,
            primary_rpc.http_endpoint_id,
            0.85,
        );

        // Heuristic order: Create* -> Get* (when present).
        let create_rpc = bundle.rpcs.iter().find(|r| r.method_name.starts_with("Create"));
        let get_rpc = bundle.rpcs.iter().find(|r| r.method_name.starts_with("Get"));
        if let (Some(create), Some(get)) = (create_rpc, get_rpc) {
            builder.rel("workflow_suggests_order", create.rpc_id, get.rpc_id, 0.55);
            builder.rel("observed_next", create.rpc_id, get.rpc_id, 0.70);

            builder.rel("from", bundle.order_suggested_path_id, create.rpc_id, 1.0);
            builder.rel("to", bundle.order_suggested_path_id, get.rpc_id, 1.0);

            builder.rel("from", bundle.order_observed_path_id, create.rpc_id, 1.0);
            builder.rel("to", bundle.order_observed_path_id, get.rpc_id, 1.0);

            builder.rel("from", bundle.order_homotopy_id, create.rpc_id, 1.0);
            builder.rel("to", bundle.order_homotopy_id, get.rpc_id, 1.0);
            builder.rel("lhs", bundle.order_homotopy_id, bundle.order_suggested_path_id, 1.0);
            builder.rel("rhs", bundle.order_homotopy_id, bundle.order_observed_path_id, 1.0);
            builder.equiv(
                bundle.order_suggested_path_id,
                bundle.order_observed_path_id,
                "HomotopicPath",
            );
        }

        // Doc homotopy (direct mention vs via http endpoint).
        builder.rel("from", bundle.doc_direct_path_id, bundle.doc_id, 1.0);
        builder.rel("to", bundle.doc_direct_path_id, primary_rpc.rpc_id, 1.0);

        builder.rel("from", bundle.doc_via_http_path_id, bundle.doc_id, 1.0);
        builder.rel("to", bundle.doc_via_http_path_id, primary_rpc.rpc_id, 1.0);
        builder.rel("via", bundle.doc_via_http_path_id, primary_rpc.http_endpoint_id, 1.0);

        builder.rel("from", bundle.doc_homotopy_id, bundle.doc_id, 1.0);
        builder.rel("to", bundle.doc_homotopy_id, primary_rpc.rpc_id, 1.0);
        builder.rel("lhs", bundle.doc_homotopy_id, bundle.doc_direct_path_id, 1.0);
        builder.rel("rhs", bundle.doc_homotopy_id, bundle.doc_via_http_path_id, 1.0);
        builder.equiv(
            bundle.doc_direct_path_id,
            bundle.doc_via_http_path_id,
            "HomotopicPath",
        );

        // Cross-service “calls” edges: use domain dependency hints when present,
        // otherwise fall back to a simple ring (keeps graph connected).
        if let Some(deps) = deps_by_domain.get(bundle.domain.as_str()) {
            for &dep in *deps {
                if let Some(&dst) = domain_to_service.get(dep) {
                    builder.rel("calls", bundle.service_id, dst, 0.75);
                }
            }
        } else if !bundles.is_empty() {
            let next = (i + 1) % bundles.len();
            builder.rel("calls", bundle.service_id, bundles[next].service_id, 0.60);
        }
    }

    let relation_time = start.elapsed();

    let example_commands = vec![
        "q select ?svc where ?svc is ProtoService limit 20".to_string(),
        "q select ?rpc where name(\"acme.orders.v1.OrderService\") -proto_service_has_rpc-> ?rpc limit 20"
            .to_string(),
        "q select ?dst where name(\"acme.orders.v1.OrderService\") -calls-> ?dst limit 20".to_string(),
        "q select ?ep where name(\"acme.payments.v1.PaymentService.AuthorizePayment\") -proto_rpc_http_endpoint-> ?ep limit 10"
            .to_string(),
        "q select ?rpc where name(\"doc_orders_api\") -mentions_rpc-> ?rpc limit 10".to_string(),
        "q select ?rpc where name(\"doc_orders_api\") -mentions_http_endpoint/proto_http_endpoint_of_rpc-> ?rpc max_hops 3 limit 10"
            .to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"doc_orders_api\") limit 10".to_string(),
    ];

    // DocChunks for grounding + exploration. Cap so perf runs don’t accidentally
    // generate hundreds of thousands of chunks.
    let mut extra_chunks: Vec<axiograph_ingest_docs::Chunk> = Vec::new();
    let doc_id = scenario_doc_id("proto_api_business");
    let chunk_cap = scale.max(1).min(64);
    for svc_i in 0..chunk_cap {
        let b = &bundles[svc_i];

        let service_fqn = builder
            .db
            .get_entity(b.service_id)
            .and_then(|v| v.attrs.get("name").cloned())
            .unwrap_or_else(|| "<unknown>".to_string());

        let rpc_fqns = b
            .rpcs
            .iter()
            .map(|r| r.rpc_fqn.clone())
            .collect::<Vec<_>>();

        extra_chunks.push(axiograph_ingest_docs::Chunk {
            chunk_id: format!("doc_proto_business_service_{svc_i}"),
            document_id: doc_id.clone(),
            page: None,
            span_id: format!("proto_business_service_{svc_i}"),
            text: format!(
                "Business proto service: {service_fqn}\nDomain: {}\nResource: {}\nRPCs: {}",
                b.domain,
                b.resource_fqn,
                rpc_fqns.iter().cloned().collect::<Vec<_>>().join(", ")
            ),
            bbox: None,
            metadata: HashMap::from([
                ("kind".to_string(), "proto_service".to_string()),
                ("domain".to_string(), b.domain.clone()),
                ("about_type".to_string(), "ProtoService".to_string()),
                ("about_name".to_string(), service_fqn.clone()),
            ]),
        });

        for (rpc_i, rpc) in b.rpcs.iter().take(12).enumerate() {
            extra_chunks.push(axiograph_ingest_docs::Chunk {
                chunk_id: format!("doc_proto_business_rpc_{svc_i}_{rpc_i}"),
                document_id: doc_id.clone(),
                page: None,
                span_id: format!("proto_business_rpc_{svc_i}_{rpc_i}"),
                text: format!(
                    "Business proto RPC: {}\nHTTP: {} {}\n\nThis is a synthetic enterprise-style API surface used for proto visualizer demos.",
                    rpc.rpc_fqn, rpc.http_method, rpc.http_path
                ),
                bbox: None,
                metadata: HashMap::from([
                    ("kind".to_string(), "proto_rpc".to_string()),
                    ("domain".to_string(), b.domain.clone()),
                    ("about_type".to_string(), "ProtoRpc".to_string()),
                    ("about_name".to_string(), rpc.rpc_fqn.clone()),
                ]),
            });
        }
    }

    // A narrative chunk for a “checkout” cross-service workflow, so the explorer
    // has something human-readable to ground questions in.
    extra_chunks.push(axiograph_ingest_docs::Chunk {
        chunk_id: "doc_proto_business_checkout_0".to_string(),
        document_id: doc_id.clone(),
        page: None,
        span_id: "checkout_0".to_string(),
        text: "Checkout workflow (synthetic): OrderService.CreateOrder typically consults PricingService, reserves stock via InventoryService, authorizes payment via PaymentService (which may call FraudService and record to LedgerService), and finally requests shipment via ShippingService; NotificationService emits user-visible updates.\n\nThis is a demo narrative chunk; it is not ground truth.".to_string(),
        bbox: None,
        metadata: HashMap::from([
            ("kind".to_string(), "workflow_narrative".to_string()),
            ("about_type".to_string(), "ApiWorkflow".to_string()),
            ("about_name".to_string(), "CheckoutWorkflow".to_string()),
        ]),
    });

    import_scenario_docchunks(
        &mut builder,
        "proto_api_business",
        "Enterprise proto fleet: dozens of ProtoServices + RPCs + HTTP endpoints + workflows, with doc/homotopy artifacts for grounded ontology/viz demos.",
        &[
            ("ProtoService", "acme.orders.v1.OrderService"),
            ("ProtoRpc", "acme.orders.v1.OrderService.CreateOrder"),
            ("Doc", "doc_orders_api"),
        ],
        extra_chunks,
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = builder;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "proto_api_business".to_string(),
        description: "Enterprise proto fleet: dozens of ProtoServices + RPCs + HTTP endpoints + workflows, with doc/homotopy artifacts for grounded ontology/viz demos.".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn build_social_network_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario aligned with `examples/social/SocialNetwork.axi`:
    // - people/organizations/communities
    // - relationship types + transformations (a "higher groupoid" story)
    // - explicit HistoryEquivalence + Homotopy objects for "same social state via different histories"

    struct SocialCluster {
        alice: u32,
        bob: u32,
        carol: u32,
        dave: u32,
        techcorp: u32,
        university: u32,
        makerspace: u32,
        bookclub: u32,
        neighborhood: u32,
        path_work_then_friend: u32,
        path_friend_then_work: u32,
        history_homotopy: u32,
        history_equiv: u32,
    }

    let mut builder = ScenarioBuilder::new(seed, index_depth)?;
    let start = Instant::now();

    // Shared vocab.
    let relation_types = [
        "Stranger",
        "Acquaintance",
        "Friend",
        "CloseFriend",
        "Colleague",
        "Family",
        "Mentor",
    ];
    let mut relation_type_ids: HashMap<String, u32> = HashMap::new();
    for rt in relation_types {
        let id = builder.add_named_entity("RelationType", rt, Vec::new());
        relation_type_ids.insert(rt.to_string(), id);
    }

    let trust_levels = ["None", "Low", "Medium", "High", "Complete"];
    let mut trust_level_ids: HashMap<String, u32> = HashMap::new();
    for tl in trust_levels {
        let id = builder.add_named_entity("TrustLevel", tl, Vec::new());
        trust_level_ids.insert(tl.to_string(), id);
    }

    let time_points = ["T0", "T1", "T2", "T3"];
    let mut time_ids: HashMap<String, u32> = HashMap::new();
    for t in time_points {
        let id = builder.add_named_entity("Time", t, Vec::new());
        time_ids.insert(t.to_string(), id);
    }

    let transform_names = [
        "Strengthen",
        "Weaken",
        "Formalize",
        "Deformalize",
        "DeepTrust",
        "MeetIntro",
        "Drift",
        // "Composite" transformations as named results.
        "BecameFriends",
        "BecameClose",
        "BecameColleagues",
    ];
    let mut transform_ids: HashMap<String, u32> = HashMap::new();
    for tr in transform_names {
        let id = builder.add_named_entity("RelTransformation", tr, Vec::new());
        transform_ids.insert(tr.to_string(), id);
    }

    // A few composition facts (associativity/equational reasoning demos later).
    let compose_meet_intro = builder.add_named_entity(
        "TransformCompose",
        "Compose_MeetIntro_Strengthen",
        Vec::new(),
    );
    let compose_strengthen = builder.add_named_entity(
        "TransformCompose",
        "Compose_Strengthen_DeepTrust",
        Vec::new(),
    );
    let compose_formalize = builder.add_named_entity(
        "TransformCompose",
        "Compose_MeetIntro_Formalize",
        Vec::new(),
    );

    let witness_same_friendship = builder.add_named_entity("Text", "SameFriendship", Vec::new());

    let mut clusters: Vec<SocialCluster> = Vec::with_capacity(scale);

    for i in 0..scale {
        let alice = builder.add_named_entity("Person", format!("Alice_{i}"), Vec::new());
        let bob = builder.add_named_entity("Person", format!("Bob_{i}"), Vec::new());
        let carol = builder.add_named_entity("Person", format!("Carol_{i}"), Vec::new());
        let dave = builder.add_named_entity("Person", format!("Dave_{i}"), Vec::new());

        let techcorp =
            builder.add_named_entity("Organization", format!("TechCorp_{i}"), Vec::new());
        let university =
            builder.add_named_entity("Organization", format!("University_{i}"), Vec::new());

        let makerspace =
            builder.add_named_entity("Community", format!("MakerSpace_{i}"), Vec::new());
        let bookclub = builder.add_named_entity("Community", format!("BookClub_{i}"), Vec::new());
        let neighborhood =
            builder.add_named_entity("Community", format!("Neighborhood_{i}"), Vec::new());

        let path_work_then_friend = builder.add_named_entity(
            "PathWitness",
            format!("path_Alice_to_Carol_work_then_friend_{i}"),
            vec![(
                "repr".to_string(),
                "met at work → colleague → friend".to_string(),
            )],
        );
        let path_friend_then_work = builder.add_named_entity(
            "PathWitness",
            format!("path_Alice_to_Carol_friend_then_work_{i}"),
            vec![(
                "repr".to_string(),
                "met socially → friend → colleague-friend".to_string(),
            )],
        );
        let history_homotopy = builder.add_named_entity(
            "Homotopy",
            format!("homotopy_Alice_to_Carol_history_{i}"),
            vec![(
                "repr".to_string(),
                "WorkThenFriend ~ FriendThenWork".to_string(),
            )],
        );
        let history_equiv = builder.add_named_entity(
            "HistoryEquivalence",
            format!("HistoryEquivalence_Alice_to_Carol_{i}"),
            vec![("witness".to_string(), "SameFriendship".to_string())],
        );

        clusters.push(SocialCluster {
            alice,
            bob,
            carol,
            dave,
            techcorp,
            university,
            makerspace,
            bookclub,
            neighborhood,
            path_work_then_friend,
            path_friend_then_work,
            history_homotopy,
            history_equiv,
        });
    }

    let entity_time = start.elapsed();

    let start = Instant::now();

    // Shared transform composition edges.
    let meet_intro = *transform_ids.get("MeetIntro").expect("transform exists");
    let strengthen = *transform_ids.get("Strengthen").expect("transform exists");
    let deep_trust = *transform_ids.get("DeepTrust").expect("transform exists");
    let formalize = *transform_ids.get("Formalize").expect("transform exists");
    let became_friends = *transform_ids
        .get("BecameFriends")
        .expect("transform exists");
    let became_close = *transform_ids.get("BecameClose").expect("transform exists");
    let became_colleagues = *transform_ids
        .get("BecameColleagues")
        .expect("transform exists");

    builder.rel("t1", compose_meet_intro, meet_intro, 1.0);
    builder.rel("t2", compose_meet_intro, strengthen, 1.0);
    builder.rel("result", compose_meet_intro, became_friends, 1.0);

    builder.rel("t1", compose_strengthen, strengthen, 1.0);
    builder.rel("t2", compose_strengthen, deep_trust, 1.0);
    builder.rel("result", compose_strengthen, became_close, 1.0);

    builder.rel("t1", compose_formalize, meet_intro, 1.0);
    builder.rel("t2", compose_formalize, formalize, 1.0);
    builder.rel("result", compose_formalize, became_colleagues, 1.0);

    let friend = *relation_type_ids.get("Friend").expect("rel exists");
    let colleague = *relation_type_ids.get("Colleague").expect("rel exists");
    let acquaintance = *relation_type_ids.get("Acquaintance").expect("rel exists");
    let stranger = *relation_type_ids.get("Stranger").expect("rel exists");

    for (i, c) in clusters.iter().enumerate() {
        // Membership / participation
        builder.rel("memberOf", c.alice, c.university, 0.9);
        builder.rel("memberOf", c.bob, c.techcorp, 0.9);
        builder.rel("memberOf", c.carol, c.techcorp, 0.9);
        builder.rel("participatesIn", c.alice, c.bookclub, 0.8);
        builder.rel("participatesIn", c.bob, c.neighborhood, 0.7);
        builder.rel("participatesIn", c.carol, c.makerspace, 0.75);

        // Current relationships (as direct edges).
        builder.rel("Friend", c.alice, c.bob, 0.9);
        builder.rel("Colleague", c.alice, c.carol, 0.8);
        builder.rel("Acquaintance", c.bob, c.carol, 0.6);
        builder.rel("Mentor", c.carol, c.dave, 0.7);

        // Relationship evolution as explicit objects.
        let rel_path_0 = builder.add_named_entity(
            "RelationshipPath",
            format!("RelationshipPath_Alice_Bob_{i}_T0"),
            Vec::new(),
        );
        builder.rel("from", rel_path_0, c.alice, 1.0);
        builder.rel("to", rel_path_0, c.bob, 1.0);
        builder.rel("startRel", rel_path_0, stranger, 1.0);
        builder.rel("endRel", rel_path_0, acquaintance, 1.0);
        builder.rel("transform", rel_path_0, meet_intro, 1.0);
        builder.rel(
            "time",
            rel_path_0,
            *time_ids.get("T0").expect("T0 exists"),
            1.0,
        );

        let rel_path_1 = builder.add_named_entity(
            "RelationshipPath",
            format!("RelationshipPath_Alice_Bob_{i}_T1"),
            Vec::new(),
        );
        builder.rel("from", rel_path_1, c.alice, 1.0);
        builder.rel("to", rel_path_1, c.bob, 1.0);
        builder.rel("startRel", rel_path_1, acquaintance, 1.0);
        builder.rel("endRel", rel_path_1, friend, 1.0);
        builder.rel("transform", rel_path_1, strengthen, 1.0);
        builder.rel(
            "time",
            rel_path_1,
            *time_ids.get("T1").expect("T1 exists"),
            1.0,
        );

        // Trust paths (typed objects).
        let trust_path = builder.add_named_entity(
            "TrustPath",
            format!("TrustPath_Alice_to_Bob_{i}"),
            Vec::new(),
        );
        builder.rel("from", trust_path, c.alice, 1.0);
        builder.rel("to", trust_path, c.bob, 1.0);
        builder.rel(
            "level",
            trust_path,
            *trust_level_ids.get("High").expect("High exists"),
            1.0,
        );
        builder.rel("witnesses", trust_path, c.bookclub, 1.0);

        // History equivalence / homotopy between two "histories" from Alice to Carol.
        builder.rel("from", c.path_work_then_friend, c.alice, 1.0);
        builder.rel("to", c.path_work_then_friend, c.carol, 1.0);
        builder.rel("via", c.path_work_then_friend, c.techcorp, 1.0);
        builder.rel("via", c.path_work_then_friend, colleague, 1.0);
        builder.rel("via", c.path_work_then_friend, friend, 1.0);

        builder.rel("from", c.path_friend_then_work, c.alice, 1.0);
        builder.rel("to", c.path_friend_then_work, c.carol, 1.0);
        builder.rel("via", c.path_friend_then_work, c.makerspace, 1.0);
        builder.rel("via", c.path_friend_then_work, friend, 1.0);
        builder.rel("via", c.path_friend_then_work, colleague, 1.0);

        builder.rel("from", c.history_homotopy, c.alice, 1.0);
        builder.rel("to", c.history_homotopy, c.carol, 1.0);
        builder.rel("lhs", c.history_homotopy, c.path_work_then_friend, 1.0);
        builder.rel("rhs", c.history_homotopy, c.path_friend_then_work, 1.0);
        builder.equiv(
            c.path_work_then_friend,
            c.path_friend_then_work,
            "HomotopicPath",
        );

        builder.rel("from", c.history_equiv, c.alice, 1.0);
        builder.rel("to", c.history_equiv, c.carol, 1.0);
        builder.rel("path1", c.history_equiv, c.path_work_then_friend, 1.0);
        builder.rel("path2", c.history_equiv, c.path_friend_then_work, 1.0);
        builder.rel("witness", c.history_equiv, witness_same_friendship, 1.0);

        // Cross-cluster social ties (just one link per cluster).
        let next = (i + 1) % clusters.len();
        builder.rel("Acquaintance", c.bob, clusters[next].alice, 0.4);
        builder.rel("Mentor", clusters[next].carol, c.dave, 0.3);
    }

    let relation_time = start.elapsed();

    let example_commands = vec![
        "q select ?p where ?p is Person limit 10".to_string(),
        "q select ?x where name(\"Alice_0\") -Friend-> ?x limit 10".to_string(),
        "q select ?p where ?p is RelationshipPath, ?p -from-> name(\"Alice_0\") limit 10"
            .to_string(),
        "q select ?t where ?t is TrustPath, ?t -from-> name(\"Alice_0\") limit 10".to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"Alice_0\") limit 10".to_string(),
        "q select ?e where ?e is HistoryEquivalence, ?e -from-> name(\"Alice_0\") limit 10"
            .to_string(),
    ];

    import_scenario_docchunks(
        &mut builder,
        "social_network",
        "Social network: Person/Organization/Community + RelationType/RelTransformation + relationship paths, trust paths, and explicit HistoryEquivalence/Homotopy artifacts.",
        &[("Person", "Alice_0"), ("Person", "Bob_0"), ("Community", "BookClub_0")],
        Vec::new(),
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = builder;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "social_network".to_string(),
        description: "Social network: Person/Organization/Community + RelationType/RelTransformation + relationship paths, trust paths, and explicit HistoryEquivalence/Homotopy artifacts.".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

fn build_supply_chain_scenario(
    scale: usize,
    index_depth: usize,
    seed: u64,
) -> Result<SyntheticScenarioIngest> {
    if scale == 0 {
        return Err(anyhow!("scale must be > 0"));
    }

    // A scenario aligned with `examples/manufacturing/SupplyChainHoTT.axi`:
    // - suppliers/warehouses/factories/customers
    // - two routes to the same delivery outcome
    // - explicit homotopies for "path independence"

    struct SupplyLane {
        supplier: u32,
        supplier_alt: u32,
        warehouse: u32,
        factory: u32,
        customer: u32,
        direct_path: u32,
        via_warehouse_path: u32,
        route_homotopy: u32,
    }

    let mut builder = ScenarioBuilder::new(seed, index_depth)?;
    let start = Instant::now();

    let material_widget = builder.add_named_entity("Material", "Widget", Vec::new());
    let proof_route = builder.add_named_entity("RouteProof", "RouteProof", Vec::new());

    let mut lanes: Vec<SupplyLane> = Vec::with_capacity(scale);

    for i in 0..scale {
        let supplier = builder.add_named_entity("Supplier", format!("supplier_{i}"), Vec::new());
        let supplier_alt =
            builder.add_named_entity("Supplier", format!("supplier_{i}_alt"), Vec::new());
        let warehouse = builder.add_named_entity("Warehouse", format!("warehouse_{i}"), Vec::new());
        let factory = builder.add_named_entity("Factory", format!("factory_{i}"), Vec::new());
        let customer = builder.add_named_entity("Customer", format!("customer_{i}"), Vec::new());

        let direct_path = builder.add_named_entity(
            "PathWitness",
            format!("path_supplier_to_factory_direct_{i}"),
            vec![("repr".to_string(), "supplies".to_string())],
        );
        let via_warehouse_path = builder.add_named_entity(
            "PathWitness",
            format!("path_supplier_to_factory_via_warehouse_{i}"),
            vec![("repr".to_string(), "suppliesVia/transfers".to_string())],
        );
        let route_homotopy = builder.add_named_entity(
            "Homotopy",
            format!("homotopy_supplier_to_factory_{i}"),
            vec![(
                "repr".to_string(),
                "supplies ~ suppliesVia/transfers".to_string(),
            )],
        );

        lanes.push(SupplyLane {
            supplier,
            supplier_alt,
            warehouse,
            factory,
            customer,
            direct_path,
            via_warehouse_path,
            route_homotopy,
        });
    }

    let entity_time = start.elapsed();

    let start = Instant::now();

    for (i, lane) in lanes.iter().enumerate() {
        builder.rel("supplies", lane.supplier, lane.factory, 0.85);
        builder.rel("suppliesVia", lane.supplier, lane.warehouse, 0.85);
        builder.rel("transfers", lane.warehouse, lane.factory, 0.85);
        builder.rel("deliversTo", lane.factory, lane.customer, 0.8);

        builder.rel("suppliesMaterial", lane.supplier, material_widget, 0.9);
        builder.rel("suppliesMaterial", lane.supplier_alt, material_widget, 0.9);

        // Supplier equivalence: two suppliers can be substituted for Widget.
        builder.equiv(lane.supplier, lane.supplier_alt, "SupplierEquiv");

        // Route equivalence witness (as explicit homotopy artifacts).
        builder.rel("from", lane.direct_path, lane.supplier, 1.0);
        builder.rel("to", lane.direct_path, lane.factory, 1.0);

        builder.rel("from", lane.via_warehouse_path, lane.supplier, 1.0);
        builder.rel("to", lane.via_warehouse_path, lane.factory, 1.0);
        builder.rel("via", lane.via_warehouse_path, lane.warehouse, 1.0);

        builder.rel("from", lane.route_homotopy, lane.supplier, 1.0);
        builder.rel("to", lane.route_homotopy, lane.factory, 1.0);
        builder.rel("lhs", lane.route_homotopy, lane.direct_path, 1.0);
        builder.rel("rhs", lane.route_homotopy, lane.via_warehouse_path, 1.0);
        builder.rel("proof", lane.route_homotopy, proof_route, 1.0);
        builder.equiv(lane.direct_path, lane.via_warehouse_path, "HomotopicPath");

        // Cross-lane coupling (make the graph less disjoint).
        let next = (i + 1) % lanes.len();
        builder.rel("suppliesVia", lane.supplier, lanes[next].warehouse, 0.3);
    }

    let relation_time = start.elapsed();

    let example_commands = vec![
        "q select ?f where name(\"supplier_0\") -supplies-> ?f limit 10".to_string(),
        "q select ?f where name(\"supplier_0\") -suppliesVia/transfers-> ?f max_hops 4 limit 10"
            .to_string(),
        "q select ?h where ?h is Homotopy, ?h -from-> name(\"supplier_0\") limit 10".to_string(),
        "q select ?m where name(\"supplier_0\") -suppliesMaterial-> ?m limit 10".to_string(),
    ];

    import_scenario_docchunks(
        &mut builder,
        "supply_chain",
        "Supply chain: Supplier/Warehouse/Factory/Customer + route alternatives with explicit homotopy (path independence) and SupplierEquiv substitutions.",
        &[("Supplier", "supplier_0"), ("Factory", "factory_0")],
        Vec::new(),
    )?;

    let ScenarioBuilder {
        db,
        entity_type_names,
        relation_type_names,
        ..
    } = builder;
    let mut entity_type_names: Vec<String> = entity_type_names.into_iter().collect();
    entity_type_names.sort();
    let mut relation_type_names: Vec<String> = relation_type_names.into_iter().collect();
    relation_type_names.sort();

    Ok(SyntheticScenarioIngest {
        scenario_name: "supply_chain".to_string(),
        description: "Supply chain: Supplier/Warehouse/Factory/Customer + route alternatives with explicit homotopy (path independence) and SupplierEquiv substitutions.".to_string(),
        entity_type_names,
        relation_type_names,
        db,
        entity_time,
        relation_time,
        example_commands,
    })
}

#[cfg(test)]
mod scenario_tests {
    use super::*;

    fn id_by_name(db: &axiograph_pathdb::PathDB, name: &str) -> Option<u32> {
        let attr = db.interner.id_of("name")?;
        let value = db.interner.id_of(name)?;
        db.entities
            .entities_with_attr_value(attr, value)
            .iter()
            .next()
    }

    #[test]
    fn enterprise_scenario_has_expected_shapes_and_homotopies() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("enterprise", 2, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("Person").is_some());
        assert!(db.find_by_type("Team").is_some());
        assert!(db.find_by_type("Service").is_some());
        assert!(db.find_by_type("Endpoint").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let svc0 = id_by_name(db, "svc_0").expect("svc_0 exists");
        let equivs = db.find_equivalent(svc0);
        assert!(
            equivs
                .iter()
                .any(|(_, t)| db.interner.lookup(*t) == Some("SameService".to_string())),
            "svc_0 should have SameService equivalence"
        );

        Ok(())
    }

    #[test]
    fn economic_flows_scenario_has_expected_types_and_homotopies() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("economic_flows", 2, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("Household").is_some());
        assert!(db.find_by_type("Firm").is_some());
        assert!(db.find_by_type("Bank").is_some());
        assert!(db.find_by_type("FlowType").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let h0 = id_by_name(db, "household_0").expect("household_0 exists");
        let targets = db.follow_path(h0, &["Consumption"]);
        assert!(
            !targets.is_empty(),
            "household_0 should consume from some firm"
        );

        Ok(())
    }

    #[test]
    fn machinist_learning_scenario_has_expected_types_and_homotopies() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("machinist_learning", 2, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("Material").is_some());
        assert!(db.find_by_type("MachiningOperation").is_some());
        assert!(db.find_by_type("Concept").is_some());
        assert!(db.find_by_type("SafetyGuideline").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let op0 = id_by_name(db, "op_0").expect("op_0 exists");
        let targets = db.follow_path(op0, &["guardrailedBy"]);
        assert!(
            !targets.is_empty(),
            "op_0 should have at least one direct guardrail"
        );

        Ok(())
    }

    #[test]
    fn schema_evolution_scenario_has_expected_types_and_homotopies() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("schema_evolution", 1, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("Schema").is_some());
        assert!(db.find_by_type("Migration").is_some());
        assert!(db.find_by_type("SchemaEquiv").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let v1 = id_by_name(db, "ProductV1_0").expect("ProductV1_0 exists");
        let targets = db.follow_path(v1, &["outgoingMigration", "toSchema"]);
        assert!(
            !targets.is_empty(),
            "ProductV1_0 should have at least one outgoing migration"
        );

        Ok(())
    }

    #[test]
    fn proto_api_scenario_has_expected_types_and_paths() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("proto_api", 1, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("ProtoService").is_some());
        assert!(db.find_by_type("ProtoRpc").is_some());
        assert!(db.find_by_type("HttpEndpoint").is_some());
        assert!(db.find_by_type("ApiWorkflow").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let svc0 = id_by_name(db, "acme.svc0.v1.Service0").expect("Service0 exists");
        let rpcs = db.follow_path(svc0, &["proto_service_has_rpc"]);
        assert!(!rpcs.is_empty(), "Proto service should have some rpcs");

        let doc0 = id_by_name(db, "doc_proto_api_0").expect("doc exists");
        let mentioned = db.follow_path(doc0, &["mentions_rpc"]);
        assert!(
            !mentioned.is_empty(),
            "doc should mention at least one rpc directly"
        );

        Ok(())
    }

    #[test]
    fn proto_api_business_scenario_has_expected_types_and_paths() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("proto_api_business", 8, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("ProtoService").is_some());
        assert!(db.find_by_type("ProtoRpc").is_some());
        assert!(db.find_by_type("HttpEndpoint").is_some());
        assert!(db.find_by_type("ApiWorkflow").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let orders = id_by_name(db, "acme.orders.v1.OrderService").expect("OrderService exists");
        let rpcs = db.follow_path(orders, &["proto_service_has_rpc"]);
        assert!(!rpcs.is_empty(), "OrderService should have some rpcs");

        let doc = id_by_name(db, "doc_orders_api").expect("orders doc exists");
        let mentioned = db.follow_path(doc, &["mentions_rpc"]);
        assert!(
            !mentioned.is_empty(),
            "orders doc should mention at least one rpc directly"
        );

        Ok(())
    }

    #[test]
    fn social_network_scenario_has_expected_types_and_edges() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("social_network", 1, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("Person").is_some());
        assert!(db.find_by_type("Organization").is_some());
        assert!(db.find_by_type("Community").is_some());
        assert!(db.find_by_type("RelationType").is_some());
        assert!(db.find_by_type("RelTransformation").is_some());
        assert!(db.find_by_type("RelationshipPath").is_some());
        assert!(db.find_by_type("TrustPath").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let alice = id_by_name(db, "Alice_0").expect("Alice_0 exists");
        let friends = db.follow_path(alice, &["Friend"]);
        assert!(
            !friends.is_empty(),
            "Alice_0 should have at least one friend"
        );

        Ok(())
    }

    #[test]
    fn supply_chain_scenario_has_expected_types_and_routes() -> Result<()> {
        let scenario = build_scenario_pathdb_ingest("supply_chain", 1, 3, 1)?;
        let db = &scenario.db;

        assert!(db.find_by_type("Supplier").is_some());
        assert!(db.find_by_type("Warehouse").is_some());
        assert!(db.find_by_type("Factory").is_some());
        assert!(db.find_by_type("Customer").is_some());
        assert!(db.find_by_type("Homotopy").is_some());

        let supplier = id_by_name(db, "supplier_0").expect("supplier_0 exists");
        let direct = db.follow_path(supplier, &["supplies"]);
        assert!(
            !direct.is_empty(),
            "supplier_0 should supply at least one factory directly"
        );

        Ok(())
    }
}
