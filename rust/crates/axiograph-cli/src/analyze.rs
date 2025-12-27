//! Analysis commands (tooling; not part of the trusted kernel).
//!
//! Motivation
//! ----------
//! Ontology engineering is not just about parsing and storing graphs; it is also
//! about *understanding* them:
//! - What are the hubs?
//! - Which parts are disconnected “islands”?
//! - What communities/topics exist?
//! - Which nodes are “bridges” between communities?
//!
//! These commands intentionally live in the CLI crate:
//! - they are untrusted/evidence-plane friendly,
//! - they should be easy to iterate on,
//! - and they should not bloat the PathDB core or the Lean checker.

use anyhow::{anyhow, Result};
use clap::Subcommand;
use roaring::RoaringBitmap;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;

use axiograph_pathdb::axi_meta::ATTR_AXI_RELATION;
use axiograph_pathdb::PathDB;

#[derive(Subcommand)]
pub enum AnalyzeCommands {
    /// Network analysis over a `.axpd` snapshot or imported `.axi` module.
    ///
    /// Output is a JSON or text report intended for ontology engineering.
    Network {
        /// Input `.axpd` or `.axi`.
        input: PathBuf,

        /// Output report path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Output format: json|text
        #[arg(long, default_value = "text")]
        format: String,

        /// Plane selection: data|meta|both
        #[arg(long, default_value = "data")]
        plane: String,

        /// Include equivalence edges in the analysis graph.
        #[arg(long)]
        include_equivalences: bool,

        /// Skip fact nodes (reified n-ary tuples). This reduces “bipartite explosion”
        /// when analyzing large canonical snapshots.
        #[arg(long)]
        skip_facts: bool,

        /// PageRank iterations.
        #[arg(long, default_value_t = 30)]
        pagerank_iters: usize,

        /// Damping factor (typical default is 0.85).
        #[arg(long, default_value_t = 0.85)]
        pagerank_damping: f64,

        /// Number of sources for approximate betweenness (Brandes sampled).
        #[arg(long, default_value_t = 64)]
        betweenness_sources: usize,

        /// RNG seed for sampling (deterministic).
        #[arg(long, default_value_t = 1)]
        seed: u64,

        /// Community detection (Louvain; undirected projection).
        #[arg(long)]
        communities: bool,

        /// Max number of nodes to include in expensive algorithms (PageRank/betweenness/Louvain).
        ///
        /// When exceeded, those sections are skipped but components/degree stats still run.
        #[arg(long, default_value_t = 200_000)]
        max_heavy_nodes: usize,

        /// Number of top nodes to print in summaries.
        #[arg(long, default_value_t = 25)]
        top: usize,
    },

    /// Measure “semantic drift” between two contexts/worlds inside a snapshot.
    ///
    /// This is untrusted tooling intended for ontology engineering loops:
    /// - “what changed between accepted vs evidence?”
    /// - “did a new ingest tick shift the meaning surface?”
    ///
    /// Today this computes divergence over the **distribution of relation names**
    /// (`axi_relation` on fact nodes) scoped to each context.
    ///
    /// Notes:
    /// - This is *not* certificate-checked.
    /// - We apply add-α smoothing to avoid infinite KL when a relation appears in
    ///   one context but not the other.
    ContextDrift {
        /// Input `.axpd` or `.axi`.
        input: PathBuf,

        /// Context A (entity id or name).
        #[arg(long)]
        ctx_a: String,

        /// Context B (entity id or name).
        #[arg(long)]
        ctx_b: String,

        /// Metric: `kl` or `js` (Jensen–Shannon).
        #[arg(long, default_value = "js")]
        metric: String,

        /// Add-α smoothing pseudo-count (Laplace when α=1).
        #[arg(long, default_value_t = 1.0)]
        alpha: f64,

        /// Output report path (defaults to stdout).
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Output format: json|text
        #[arg(long, default_value = "text")]
        format: String,

        /// Number of top relations to print (by absolute probability difference).
        #[arg(long, default_value_t = 25)]
        top: usize,
    },
}

pub fn cmd_analyze(command: AnalyzeCommands) -> Result<()> {
    match command {
        AnalyzeCommands::Network {
            input,
            out,
            format,
            plane,
            include_equivalences,
            skip_facts,
            pagerank_iters,
            pagerank_damping,
            betweenness_sources,
            seed,
            communities,
            max_heavy_nodes,
            top,
        } => cmd_analyze_network(
            &input,
            out.as_ref(),
            &format,
            &plane,
            include_equivalences,
            skip_facts,
            pagerank_iters,
            pagerank_damping,
            betweenness_sources,
            seed,
            communities,
            max_heavy_nodes,
            top,
        ),
        AnalyzeCommands::ContextDrift {
            input,
            ctx_a,
            ctx_b,
            metric,
            alpha,
            out,
            format,
            top,
        } => cmd_analyze_context_drift(
            &input,
            &ctx_a,
            &ctx_b,
            &metric,
            alpha,
            out.as_ref(),
            &format,
            top,
        ),
    }
}

// =============================================================================
// Report format
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAnalysisReportV1 {
    pub version: String,
    pub generated_at_unix_secs: u64,
    pub input: String,
    pub plane: String,
    pub include_equivalences: bool,
    pub skip_facts: bool,
    pub node_count: usize,
    pub edge_count: usize,

    pub weak_components: ComponentSummaryV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strong_components: Option<ComponentSummaryV1>,

    pub degree: DegreeSummaryV1,

    #[serde(default)]
    pub top_degree: Vec<NodeScoreV1>,
    #[serde(default)]
    pub top_pagerank: Vec<NodeScoreV1>,
    #[serde(default)]
    pub top_betweenness: Vec<NodeScoreV1>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub communities: Option<CommunitySummaryV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentSummaryV1 {
    pub component_count: usize,
    pub giant_component_size: usize,
    pub giant_component_ratio: f64,
    pub top_component_sizes: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegreeSummaryV1 {
    pub min_in: usize,
    pub max_in: usize,
    pub min_out: usize,
    pub max_out: usize,
    pub min_total: usize,
    pub max_total: usize,
    pub mean_in: f64,
    pub mean_out: f64,
    pub mean_total: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySummaryV1 {
    pub algorithm: String,
    pub community_count: usize,
    pub top_community_sizes: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScoreV1 {
    pub id: u32,
    pub entity_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextDriftReportV1 {
    pub version: String,
    pub generated_at_unix_secs: u64,
    pub input: String,

    pub context_a: String,
    pub context_b: String,
    pub context_a_id: u32,
    pub context_b_id: u32,

    pub metric: String,
    pub alpha: f64,

    pub fact_count_a: usize,
    pub fact_count_b: usize,
    pub relation_support: usize,
    pub divergence: f64,

    #[serde(default)]
    pub top_relation_diffs: Vec<RelationDiffV1>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationDiffV1 {
    pub relation: String,
    pub count_a: usize,
    pub count_b: usize,
    pub p: f64,
    pub q: f64,
    pub diff: f64,
}

// =============================================================================
// Implementation
// =============================================================================

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

fn node_name(db: &PathDB, id: u32) -> Option<String> {
    db.get_entity(id).and_then(|e| e.attrs.get("name").cloned())
}

fn node_type(db: &PathDB, id: u32) -> String {
    db.get_entity(id)
        .map(|e| e.entity_type)
        .unwrap_or_else(|| "Unknown".to_string())
}

fn describe_entity(db: &PathDB, id: u32) -> String {
    let Some(view) = db.get_entity(id) else {
        return format!("{id} (missing)");
    };
    let name = view.attrs.get("name").cloned().unwrap_or_default();
    if name.is_empty() {
        format!("{id} ({})", view.entity_type)
    } else {
        format!("{id} ({}, name={})", view.entity_type, name)
    }
}

fn resolve_entity_ref(db: &PathDB, token: &str) -> Result<u32> {
    if let Ok(id) = token.parse::<u32>() {
        if db.get_entity(id).is_some() {
            return Ok(id);
        }
        return Err(anyhow!("no entity with id {id}"));
    }

    let Some(key_id) = db.interner.id_of("name") else {
        return Err(anyhow!("database has no `name` attribute interned"));
    };
    let Some(value_id) = db.interner.id_of(token) else {
        return Err(anyhow!("no entity found with name `{token}`"));
    };

    let ids = db.entities.entities_with_attr_value(key_id, value_id);
    if ids.is_empty() {
        return Err(anyhow!("no entity found with name `{token}`"));
    }
    if ids.len() == 1 {
        return Ok(ids.iter().next().unwrap_or(0));
    }

    let mut examples: Vec<String> = Vec::new();
    for id in ids.iter().take(5) {
        examples.push(describe_entity(db, id));
    }
    Err(anyhow!(
        "ambiguous name `{token}` ({} matches). Pass a numeric id. Examples: {}",
        ids.len(),
        examples.join(", ")
    ))
}

fn should_include_node(db: &PathDB, id: u32, plane: &str, skip_facts: bool) -> bool {
    let Some(view) = db.get_entity(id) else {
        return false;
    };
    let is_meta = is_meta_plane_entity(&view.entity_type);
    let is_fact = view.attrs.contains_key(ATTR_AXI_RELATION);
    if skip_facts && is_fact {
        return false;
    }
    match plane {
        "data" => !is_meta,
        "meta" => is_meta,
        "both" => true,
        _ => true,
    }
}

fn cmd_analyze_context_drift(
    input: &PathBuf,
    ctx_a_token: &str,
    ctx_b_token: &str,
    metric: &str,
    alpha: f64,
    out: Option<&PathBuf>,
    format: &str,
    top: usize,
) -> Result<()> {
    if alpha < 0.0 || !alpha.is_finite() {
        return Err(anyhow!("--alpha must be a finite number ≥ 0"));
    }

    let db = crate::load_pathdb_for_cli(input)?;

    let ctx_a = resolve_entity_ref(&db, ctx_a_token)?;
    let ctx_b = resolve_entity_ref(&db, ctx_b_token)?;

    let facts_a = db.fact_nodes_by_context(ctx_a);
    let facts_b = db.fact_nodes_by_context(ctx_b);

    let (counts_a, total_a) = relation_counts_for_facts(&db, &facts_a);
    let (counts_b, total_b) = relation_counts_for_facts(&db, &facts_b);

    if total_a == 0 {
        return Err(anyhow!(
            "context A has no scoped facts (ctx_a={})",
            describe_entity(&db, ctx_a)
        ));
    }
    if total_b == 0 {
        return Err(anyhow!(
            "context B has no scoped facts (ctx_b={})",
            describe_entity(&db, ctx_b)
        ));
    }

    let mut support: Vec<String> = counts_a.keys().chain(counts_b.keys()).cloned().collect();
    support.sort();
    support.dedup();

    let k = support.len() as f64;
    let denom_a = (total_a as f64) + alpha * k;
    let denom_b = (total_b as f64) + alpha * k;

    let mut diffs: Vec<RelationDiffV1> = Vec::new();
    let mut p_vec: Vec<f64> = Vec::with_capacity(support.len());
    let mut q_vec: Vec<f64> = Vec::with_capacity(support.len());

    for rel in &support {
        let ca = *counts_a.get(rel).unwrap_or(&0);
        let cb = *counts_b.get(rel).unwrap_or(&0);
        let p = (ca as f64 + alpha) / denom_a;
        let q = (cb as f64 + alpha) / denom_b;
        diffs.push(RelationDiffV1 {
            relation: rel.clone(),
            count_a: ca,
            count_b: cb,
            p,
            q,
            diff: (p - q).abs(),
        });
        p_vec.push(p);
        q_vec.push(q);
    }

    let divergence = match metric.to_ascii_lowercase().as_str() {
        "kl" => kl_divergence(&p_vec, &q_vec),
        "js" | "jensen-shannon" => js_divergence(&p_vec, &q_vec),
        other => return Err(anyhow!("unknown --metric `{other}` (expected kl|js)")),
    };

    diffs.sort_by(|a, b| {
        b.diff
            .partial_cmp(&a.diff)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    diffs.truncate(top);

    let report = ContextDriftReportV1 {
        version: "context_drift_v1".to_string(),
        generated_at_unix_secs: now_unix_secs(),
        input: input.display().to_string(),
        context_a: ctx_a_token.to_string(),
        context_b: ctx_b_token.to_string(),
        context_a_id: ctx_a,
        context_b_id: ctx_b,
        metric: metric.to_string(),
        alpha,
        fact_count_a: total_a,
        fact_count_b: total_b,
        relation_support: support.len(),
        divergence,
        top_relation_diffs: diffs,
    };

    match format.to_ascii_lowercase().as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&report)?;
            if let Some(path) = out {
                std::fs::write(path, json)?;
                println!("wrote {}", path.display());
            } else {
                println!("{json}");
            }
        }
        "text" => {
            let mut s = String::new();
            s.push_str("== Context drift ==\n");
            s.push_str(&format!("input: {}\n", report.input));
            s.push_str(&format!(
                "ctx_a: {} ({})\n",
                report.context_a,
                describe_entity(&db, report.context_a_id)
            ));
            s.push_str(&format!(
                "ctx_b: {} ({})\n",
                report.context_b,
                describe_entity(&db, report.context_b_id)
            ));
            s.push_str(&format!(
                "metric: {}  alpha: {}\n",
                report.metric, report.alpha
            ));
            s.push_str(&format!(
                "facts: a={} b={}  support={}  divergence={:.6}\n",
                report.fact_count_a,
                report.fact_count_b,
                report.relation_support,
                report.divergence
            ));
            if !report.top_relation_diffs.is_empty() {
                s.push_str("\nTop relation deltas:\n");
                for d in &report.top_relation_diffs {
                    s.push_str(&format!(
                        "  - {:<32}  |p-q|={:.6}  p={:.6}  q={:.6}  counts=({},{})\n",
                        d.relation, d.diff, d.p, d.q, d.count_a, d.count_b
                    ));
                }
            }
            if let Some(path) = out {
                std::fs::write(path, s)?;
                println!("wrote {}", path.display());
            } else {
                print!("{s}");
            }
        }
        other => return Err(anyhow!("unknown --format `{other}` (expected json|text)")),
    }

    Ok(())
}

fn relation_counts_for_facts(
    db: &PathDB,
    facts: &RoaringBitmap,
) -> (HashMap<String, usize>, usize) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut total: usize = 0;

    for id in facts.iter() {
        let Some(view) = db.get_entity(id) else {
            continue;
        };
        let Some(rel) = view.attrs.get(ATTR_AXI_RELATION) else {
            continue;
        };
        total += 1;
        *counts.entry(rel.clone()).or_insert(0) += 1;
    }

    (counts, total)
}

fn kl_divergence(p: &[f64], q: &[f64]) -> f64 {
    let mut out: f64 = 0.0;
    for (pi, qi) in p.iter().copied().zip(q.iter().copied()) {
        if pi <= 0.0 {
            continue;
        }
        out += pi * (pi / qi).ln();
    }
    out
}

fn js_divergence(p: &[f64], q: &[f64]) -> f64 {
    let mut m: Vec<f64> = Vec::with_capacity(p.len());
    for (pi, qi) in p.iter().copied().zip(q.iter().copied()) {
        m.push(0.5 * (pi + qi));
    }
    0.5 * kl_divergence(p, &m) + 0.5 * kl_divergence(q, &m)
}

#[derive(Debug, Clone)]
struct EdgeList {
    edges: Vec<(u32, u32)>,
    node_mask: Vec<bool>,
    included_nodes: Vec<u32>,
}

fn build_edge_list(
    db: &PathDB,
    plane: &str,
    include_equivalences: bool,
    skip_facts: bool,
) -> EdgeList {
    let node_count = db.entities.len();
    let mut node_mask = vec![false; node_count];
    let mut included_nodes: Vec<u32> = Vec::new();
    for id in 0..(node_count as u32) {
        if should_include_node(db, id, plane, skip_facts) {
            node_mask[id as usize] = true;
            included_nodes.push(id);
        }
    }

    let mut edges: Vec<(u32, u32)> = Vec::new();
    for rel_id in 0..db.relations.len() as u32 {
        let Some(rel) = db.relations.get_relation(rel_id) else {
            continue;
        };
        if rel.source as usize >= node_mask.len() || rel.target as usize >= node_mask.len() {
            continue;
        }
        if !node_mask[rel.source as usize] || !node_mask[rel.target as usize] {
            continue;
        }
        edges.push((rel.source, rel.target));
    }

    if include_equivalences {
        for (&src, pairs) in &db.equivalences {
            if src as usize >= node_mask.len() || !node_mask[src as usize] {
                continue;
            }
            for &(dst, _equiv_type) in pairs {
                if dst as usize >= node_mask.len() || !node_mask[dst as usize] {
                    continue;
                }
                // Treat equivalences as bidirectional edges for directed metrics.
                edges.push((src, dst));
                edges.push((dst, src));
            }
        }
    }

    EdgeList {
        edges,
        node_mask,
        included_nodes,
    }
}

fn summarize_components(component_ids: &[usize]) -> ComponentSummaryV1 {
    if component_ids.is_empty() {
        return ComponentSummaryV1 {
            component_count: 0,
            giant_component_size: 0,
            giant_component_ratio: 0.0,
            top_component_sizes: Vec::new(),
        };
    }

    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for &cid in component_ids {
        *sizes.entry(cid).or_insert(0) += 1;
    }
    let mut size_list: Vec<usize> = sizes.values().copied().collect();
    size_list.sort_unstable_by(|a, b| b.cmp(a));
    let giant = size_list.first().copied().unwrap_or(0);
    let total = component_ids.len();
    ComponentSummaryV1 {
        component_count: sizes.len(),
        giant_component_size: giant,
        giant_component_ratio: if total == 0 {
            0.0
        } else {
            giant as f64 / total as f64
        },
        top_component_sizes: size_list.into_iter().take(25).collect(),
    }
}

fn weak_components_union_find(node_mask: &[bool], edges: &[(u32, u32)]) -> Vec<usize> {
    // Union-find over the included node ids.
    // Output: for each included node (in id order), its component id (root id compressed to a dense index).
    let n = node_mask.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank: Vec<u8> = vec![0; n];

    fn find(parent: &mut [usize], x: usize) -> usize {
        let mut x0 = x;
        while parent[x0] != x0 {
            x0 = parent[x0];
        }
        let root = x0;
        let mut x1 = x;
        while parent[x1] != x1 {
            let p = parent[x1];
            parent[x1] = root;
            x1 = p;
        }
        root
    }

    fn union(parent: &mut [usize], rank: &mut [u8], a: usize, b: usize) {
        let ra = find(parent, a);
        let rb = find(parent, b);
        if ra == rb {
            return;
        }
        let rka = rank[ra];
        let rkb = rank[rb];
        if rka < rkb {
            parent[ra] = rb;
        } else if rka > rkb {
            parent[rb] = ra;
        } else {
            parent[rb] = ra;
            rank[ra] = rka.saturating_add(1);
        }
    }

    for &(u, v) in edges {
        let ui = u as usize;
        let vi = v as usize;
        if ui >= n || vi >= n {
            continue;
        }
        if !node_mask[ui] || !node_mask[vi] {
            continue;
        }
        union(&mut parent, &mut rank, ui, vi);
        union(&mut parent, &mut rank, vi, ui);
    }

    // Assign dense component ids in increasing root order for determinism.
    let mut root_to_dense: BTreeMap<usize, usize> = BTreeMap::new();
    let mut next_dense: usize = 0;
    let mut out: Vec<usize> = Vec::new();
    for id in 0..n {
        if !node_mask[id] {
            continue;
        }
        let root = find(&mut parent, id);
        let dense = *root_to_dense.entry(root).or_insert_with(|| {
            let d = next_dense;
            next_dense += 1;
            d
        });
        out.push(dense);
    }
    out
}

fn build_adjacency(node_mask: &[bool], edges: &[(u32, u32)]) -> (Vec<Vec<u32>>, Vec<Vec<u32>>) {
    let n = node_mask.len();
    let mut out_adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut in_adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for &(u, v) in edges {
        let ui = u as usize;
        let vi = v as usize;
        if ui >= n || vi >= n {
            continue;
        }
        if !node_mask[ui] || !node_mask[vi] {
            continue;
        }
        out_adj[ui].push(v);
        in_adj[vi].push(u);
    }
    (out_adj, in_adj)
}

fn kosaraju_scc(node_mask: &[bool], out_adj: &[Vec<u32>], in_adj: &[Vec<u32>]) -> Vec<usize> {
    let n = node_mask.len();
    let mut visited = vec![false; n];
    let mut order: Vec<usize> = Vec::new();

    // Iterative DFS to compute finishing order.
    for start in 0..n {
        if !node_mask[start] || visited[start] {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
        visited[start] = true;
        while let Some((node, idx)) = stack.pop() {
            if idx < out_adj[node].len() {
                // Re-push current with next index.
                stack.push((node, idx + 1));
                let next = out_adj[node][idx] as usize;
                if next < n && node_mask[next] && !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                order.push(node);
            }
        }
    }

    // Second pass: reverse graph, assign components.
    let mut comp_id = vec![usize::MAX; n];
    let mut next_comp: usize = 0;
    for &start in order.iter().rev() {
        if !node_mask[start] || comp_id[start] != usize::MAX {
            continue;
        }
        let mut stack: Vec<usize> = vec![start];
        comp_id[start] = next_comp;
        while let Some(node) = stack.pop() {
            for &prev in &in_adj[node] {
                let p = prev as usize;
                if p < n && node_mask[p] && comp_id[p] == usize::MAX {
                    comp_id[p] = next_comp;
                    stack.push(p);
                }
            }
        }
        next_comp += 1;
    }

    // Compress to dense ids in a stable order.
    let mut map: BTreeMap<usize, usize> = BTreeMap::new();
    let mut dense_next = 0usize;
    let mut out: Vec<usize> = Vec::new();
    for id in 0..n {
        if !node_mask[id] {
            continue;
        }
        let cid = comp_id[id];
        let dense = *map.entry(cid).or_insert_with(|| {
            let d = dense_next;
            dense_next += 1;
            d
        });
        out.push(dense);
    }
    out
}

fn degree_summary(
    node_mask: &[bool],
    out_adj: &[Vec<u32>],
    in_adj: &[Vec<u32>],
) -> DegreeSummaryV1 {
    let mut min_in = usize::MAX;
    let mut max_in = 0usize;
    let mut min_out = usize::MAX;
    let mut max_out = 0usize;
    let mut min_total = usize::MAX;
    let mut max_total = 0usize;

    let mut sum_in: u64 = 0;
    let mut sum_out: u64 = 0;
    let mut sum_total: u64 = 0;
    let mut count: u64 = 0;

    for i in 0..node_mask.len() {
        if !node_mask[i] {
            continue;
        }
        let din = in_adj[i].len();
        let dout = out_adj[i].len();
        let dt = din + dout;
        min_in = min_in.min(din);
        max_in = max_in.max(din);
        min_out = min_out.min(dout);
        max_out = max_out.max(dout);
        min_total = min_total.min(dt);
        max_total = max_total.max(dt);
        sum_in += din as u64;
        sum_out += dout as u64;
        sum_total += dt as u64;
        count += 1;
    }

    if count == 0 {
        return DegreeSummaryV1 {
            min_in: 0,
            max_in: 0,
            min_out: 0,
            max_out: 0,
            min_total: 0,
            max_total: 0,
            mean_in: 0.0,
            mean_out: 0.0,
            mean_total: 0.0,
        };
    }

    DegreeSummaryV1 {
        min_in: if min_in == usize::MAX { 0 } else { min_in },
        max_in,
        min_out: if min_out == usize::MAX { 0 } else { min_out },
        max_out,
        min_total: if min_total == usize::MAX {
            0
        } else {
            min_total
        },
        max_total,
        mean_in: sum_in as f64 / count as f64,
        mean_out: sum_out as f64 / count as f64,
        mean_total: sum_total as f64 / count as f64,
    }
}

fn top_by_score(db: &PathDB, node_mask: &[bool], scores: &[f64], top: usize) -> Vec<NodeScoreV1> {
    let mut items: Vec<(u32, f64)> = Vec::new();
    for i in 0..node_mask.len() {
        if !node_mask[i] {
            continue;
        }
        let s = scores[i];
        if s.is_finite() {
            items.push((i as u32, s));
        }
    }
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    items
        .into_iter()
        .take(top)
        .map(|(id, score)| NodeScoreV1 {
            id,
            entity_type: node_type(db, id),
            name: node_name(db, id),
            score,
        })
        .collect()
}

fn pagerank(node_mask: &[bool], out_adj: &[Vec<u32>], iters: usize, damping: f64) -> Vec<f64> {
    let n = node_mask.len();
    let mut nodes: Vec<usize> = Vec::new();
    for i in 0..n {
        if node_mask[i] {
            nodes.push(i);
        }
    }
    let m = nodes.len();
    if m == 0 {
        return vec![0.0; n];
    }

    let init = 1.0 / m as f64;
    let mut rank = vec![0.0; n];
    for &i in &nodes {
        rank[i] = init;
    }

    for _ in 0..iters {
        let mut next = vec![0.0; n];
        let mut dangling_mass = 0.0;

        for &u in &nodes {
            let out: Vec<u32> = out_adj[u]
                .iter()
                .copied()
                .filter(|&v| node_mask[v as usize])
                .collect();
            if out.is_empty() {
                dangling_mass += rank[u];
                continue;
            }
            let share = rank[u] / out.len() as f64;
            for v in out {
                next[v as usize] += share;
            }
        }

        let teleport = (1.0 - damping) / m as f64;
        let dangling_share = dangling_mass / m as f64;
        for &i in &nodes {
            next[i] = teleport + damping * (next[i] + dangling_share);
        }
        rank = next;
    }

    rank
}

// Deterministic xorshift RNG for sampling.
#[derive(Debug, Clone)]
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn gen_range_usize(&mut self, n: usize) -> usize {
        if n <= 1 {
            return 0;
        }
        (self.next_u64() as usize) % n
    }
}

fn approximate_betweenness(
    node_mask: &[bool],
    undirected_adj: &[Vec<u32>],
    sources: usize,
    seed: u64,
) -> Vec<f64> {
    // Brandes algorithm sampled over a subset of sources (unweighted, undirected).
    let n = node_mask.len();
    let mut nodes: Vec<usize> = Vec::new();
    for i in 0..n {
        if node_mask[i] {
            nodes.push(i);
        }
    }
    let m = nodes.len();
    if m == 0 {
        return vec![0.0; n];
    }

    let k = sources.min(m);
    let mut rng = XorShift64::new(seed);

    // Pick sources deterministically (random sample without replacement via shuffle prefix).
    let mut sampled = nodes.clone();
    for i in 0..k {
        let j = i + rng.gen_range_usize(m - i);
        sampled.swap(i, j);
    }
    sampled.truncate(k);

    let mut cb = vec![0.0; n];

    for &s in &sampled {
        // Stack of nodes in order of non-decreasing distance from s.
        let mut stack: Vec<usize> = Vec::new();
        let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut sigma: Vec<f64> = vec![0.0; n];
        let mut dist: Vec<i32> = vec![-1; n];

        sigma[s] = 1.0;
        dist[s] = 0;

        let mut q: VecDeque<usize> = VecDeque::new();
        q.push_back(s);

        while let Some(v) = q.pop_front() {
            stack.push(v);
            let dv = dist[v];
            for &w_u32 in &undirected_adj[v] {
                let w = w_u32 as usize;
                if w >= n || !node_mask[w] {
                    continue;
                }
                if dist[w] < 0 {
                    dist[w] = dv + 1;
                    q.push_back(w);
                }
                if dist[w] == dv + 1 {
                    sigma[w] += sigma[v];
                    pred[w].push(v);
                }
            }
        }

        let mut delta: Vec<f64> = vec![0.0; n];
        while let Some(w) = stack.pop() {
            for &v in &pred[w] {
                if sigma[w] > 0.0 {
                    delta[v] += (sigma[v] / sigma[w]) * (1.0 + delta[w]);
                }
            }
            if w != s {
                cb[w] += delta[w];
            }
        }
    }

    // Normalize by number of sources.
    for i in 0..n {
        cb[i] /= k.max(1) as f64;
    }

    cb
}

fn build_undirected_adjacency(node_mask: &[bool], edges: &[(u32, u32)]) -> Vec<Vec<u32>> {
    let n = node_mask.len();
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for &(u, v) in edges {
        let ui = u as usize;
        let vi = v as usize;
        if ui >= n || vi >= n {
            continue;
        }
        if !node_mask[ui] || !node_mask[vi] {
            continue;
        }
        adj[ui].push(v);
        adj[vi].push(u);
    }
    adj
}

fn louvain_one_level(node_mask: &[bool], adj: &[Vec<u32>]) -> Vec<usize> {
    // A small, deterministic Louvain "first level" pass for unweighted, undirected graphs.
    // This is a pragmatic tooling heuristic, not a certified algorithm.
    let n = node_mask.len();
    let mut nodes: Vec<usize> = Vec::new();
    for i in 0..n {
        if node_mask[i] {
            nodes.push(i);
        }
    }
    if nodes.is_empty() {
        return vec![usize::MAX; n];
    }

    // Initial community assignment: each node in its own community.
    let mut community: Vec<usize> = vec![usize::MAX; n];
    for (idx, &node) in nodes.iter().enumerate() {
        community[node] = idx;
    }

    // Degrees and total edge weight (m2 = 2m).
    let mut degree: Vec<f64> = vec![0.0; n];
    let mut m2: f64 = 0.0;
    for &u in &nodes {
        let du = adj[u].iter().filter(|&&v| node_mask[v as usize]).count() as f64;
        degree[u] = du;
        m2 += du;
    }
    if m2 == 0.0 {
        return community;
    }

    // Community total degree.
    let mut tot: Vec<f64> = vec![0.0; nodes.len()];
    for &u in &nodes {
        let c = community[u];
        tot[c] += degree[u];
    }

    // Iterate local moving until convergence.
    let max_passes = 20;
    for _pass in 0..max_passes {
        let mut moved_any = false;
        for &u in &nodes {
            let cur = community[u];

            // Compute weights to neighboring communities.
            let mut neigh_weights: HashMap<usize, f64> = HashMap::new();
            for &v_u32 in &adj[u] {
                let v = v_u32 as usize;
                if v >= n || !node_mask[v] {
                    continue;
                }
                let c = community[v];
                *neigh_weights.entry(c).or_insert(0.0) += 1.0;
            }

            // Remove u from its community temporarily.
            tot[cur] -= degree[u];

            // Best community by modularity gain (simplified for unweighted graphs).
            let mut best = cur;
            let mut best_gain = 0.0;
            for (&c, &k_u_in) in &neigh_weights {
                let gain = k_u_in - (degree[u] * tot[c]) / m2;
                if gain > best_gain + 1e-12 {
                    best_gain = gain;
                    best = c;
                }
            }

            // Reassign.
            community[u] = best;
            tot[best] += degree[u];
            if best != cur {
                moved_any = true;
            }
        }
        if !moved_any {
            break;
        }
    }

    // Compress to dense community ids for stability.
    let mut map: BTreeMap<usize, usize> = BTreeMap::new();
    let mut next = 0usize;
    for &u in &nodes {
        let c = community[u];
        let dense = *map.entry(c).or_insert_with(|| {
            let d = next;
            next += 1;
            d
        });
        community[u] = dense;
    }

    community
}

fn cmd_analyze_network(
    input: &PathBuf,
    out: Option<&PathBuf>,
    format: &str,
    plane: &str,
    include_equivalences: bool,
    skip_facts: bool,
    pagerank_iters: usize,
    pagerank_damping: f64,
    betweenness_sources: usize,
    seed: u64,
    communities: bool,
    max_heavy_nodes: usize,
    top: usize,
) -> Result<()> {
    let db = crate::load_pathdb_for_cli(input)?;
    let report = analyze_network_report(
        &db,
        &input.display().to_string(),
        plane,
        include_equivalences,
        skip_facts,
        pagerank_iters,
        pagerank_damping,
        betweenness_sources,
        seed,
        communities,
        max_heavy_nodes,
        top,
    )?;

    let format = format.trim().to_ascii_lowercase();
    let rendered = match format.as_str() {
        "json" => serde_json::to_string_pretty(&report)?,
        "text" => render_network_report_text(&report),
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

    Ok(())
}

pub fn analyze_network_report(
    db: &PathDB,
    input: &str,
    plane: &str,
    include_equivalences: bool,
    skip_facts: bool,
    pagerank_iters: usize,
    pagerank_damping: f64,
    betweenness_sources: usize,
    seed: u64,
    communities: bool,
    max_heavy_nodes: usize,
    top: usize,
) -> Result<NetworkAnalysisReportV1> {
    let plane = plane.trim().to_ascii_lowercase();
    if !matches!(plane.as_str(), "data" | "meta" | "both") {
        return Err(anyhow!("unknown plane `{plane}` (expected data|meta|both)"));
    }
    if !(0.0..=1.0).contains(&pagerank_damping) {
        return Err(anyhow!("pagerank_damping must be in [0,1]"));
    }

    let edge_list = build_edge_list(db, plane.as_str(), include_equivalences, skip_facts);
    let node_count = edge_list.included_nodes.len();
    let edge_count = edge_list.edges.len();

    let (out_adj, in_adj) = build_adjacency(&edge_list.node_mask, &edge_list.edges);

    // Components (weak + strong).
    let weak_ids = weak_components_union_find(&edge_list.node_mask, &edge_list.edges);
    let weak_summary = summarize_components(&weak_ids);

    let strong_ids = if node_count <= max_heavy_nodes {
        Some(kosaraju_scc(&edge_list.node_mask, &out_adj, &in_adj))
    } else {
        None
    };
    let strong_summary = strong_ids
        .as_ref()
        .map(|ids| summarize_components(ids.as_slice()));

    // Degree stats + top hubs.
    let degree = degree_summary(&edge_list.node_mask, &out_adj, &in_adj);
    let mut degree_scores = vec![0.0f64; edge_list.node_mask.len()];
    for i in 0..edge_list.node_mask.len() {
        if !edge_list.node_mask[i] {
            continue;
        }
        degree_scores[i] = (out_adj[i].len() + in_adj[i].len()) as f64;
    }
    let top_degree = top_by_score(db, &edge_list.node_mask, &degree_scores, top);

    // PageRank + betweenness + communities can be expensive. Skip when huge.
    let mut top_pagerank: Vec<NodeScoreV1> = Vec::new();
    let mut top_betweenness: Vec<NodeScoreV1> = Vec::new();
    let mut communities_summary: Option<CommunitySummaryV1> = None;

    if node_count <= max_heavy_nodes {
        let pr = pagerank(
            &edge_list.node_mask,
            &out_adj,
            pagerank_iters,
            pagerank_damping,
        );
        top_pagerank = top_by_score(db, &edge_list.node_mask, &pr, top);

        let undirected_adj = build_undirected_adjacency(&edge_list.node_mask, &edge_list.edges);
        let btw = approximate_betweenness(
            &edge_list.node_mask,
            &undirected_adj,
            betweenness_sources,
            seed,
        );
        top_betweenness = top_by_score(db, &edge_list.node_mask, &btw, top);

        if communities {
            let comm = louvain_one_level(&edge_list.node_mask, &undirected_adj);
            let mut sizes: HashMap<usize, usize> = HashMap::new();
            for i in 0..comm.len() {
                if !edge_list.node_mask[i] {
                    continue;
                }
                let cid = comm[i];
                if cid == usize::MAX {
                    continue;
                }
                *sizes.entry(cid).or_insert(0) += 1;
            }
            let mut size_list: Vec<usize> = sizes.values().copied().collect();
            size_list.sort_unstable_by(|a, b| b.cmp(a));
            communities_summary = Some(CommunitySummaryV1 {
                algorithm: "louvain_v1_one_level".to_string(),
                community_count: sizes.len(),
                top_community_sizes: size_list.into_iter().take(25).collect(),
            });
        }
    }

    Ok(NetworkAnalysisReportV1 {
        version: "network_analysis_v1".to_string(),
        generated_at_unix_secs: now_unix_secs(),
        input: input.to_string(),
        plane: plane.to_string(),
        include_equivalences,
        skip_facts,
        node_count,
        edge_count,
        weak_components: weak_summary,
        strong_components: strong_summary,
        degree,
        top_degree,
        top_pagerank,
        top_betweenness,
        communities: communities_summary,
    })
}

pub fn render_network_report_text(r: &NetworkAnalysisReportV1) -> String {
    let mut out = String::new();
    out.push_str("analyze/network\n");
    out.push_str(&format!("  input: {}\n", r.input));
    out.push_str(&format!(
        "  nodes: {}  edges: {}  plane: {}  equivalences: {}  skip_facts: {}\n",
        r.node_count, r.edge_count, r.plane, r.include_equivalences, r.skip_facts
    ));

    out.push_str("\ncomponents\n");
    out.push_str(&format!(
        "  weak: count={} giant={} ({:.1}%)\n",
        r.weak_components.component_count,
        r.weak_components.giant_component_size,
        100.0 * r.weak_components.giant_component_ratio
    ));
    if let Some(s) = &r.strong_components {
        out.push_str(&format!(
            "  strong: count={} giant={} ({:.1}%)\n",
            s.component_count,
            s.giant_component_size,
            100.0 * s.giant_component_ratio
        ));
    } else {
        out.push_str("  strong: (skipped)\n");
    }

    out.push_str("\ndegree\n");
    out.push_str(&format!(
        "  in:    min={} max={} mean={:.2}\n",
        r.degree.min_in, r.degree.max_in, r.degree.mean_in
    ));
    out.push_str(&format!(
        "  out:   min={} max={} mean={:.2}\n",
        r.degree.min_out, r.degree.max_out, r.degree.mean_out
    ));
    out.push_str(&format!(
        "  total: min={} max={} mean={:.2}\n",
        r.degree.min_total, r.degree.max_total, r.degree.mean_total
    ));

    fn render_top(label: &str, items: &[NodeScoreV1], out: &mut String) {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("\n{label}\n"));
        for (i, it) in items.iter().enumerate() {
            let name = it.name.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "  {:>2}. {}#{} name={} score={:.6}\n",
                i + 1,
                it.entity_type,
                it.id,
                name,
                it.score
            ));
        }
    }

    render_top("top_degree", &r.top_degree, &mut out);
    render_top("top_pagerank", &r.top_pagerank, &mut out);
    render_top("top_betweenness", &r.top_betweenness, &mut out);

    if let Some(c) = &r.communities {
        out.push_str("\ncommunities\n");
        out.push_str(&format!(
            "  algorithm={} communities={} top_sizes={:?}\n",
            c.algorithm, c.community_count, c.top_community_sizes
        ));
    }

    out
}
