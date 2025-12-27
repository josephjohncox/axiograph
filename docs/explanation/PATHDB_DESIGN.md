# PathDB: Efficient Binary Path-Indexed Knowledge Graph

**Diataxis:** Explanation  
**Audience:** contributors

PathDB is Axiograph's high-performance storage and query engine for knowledge graphs. It combines techniques from database research, succinct data structures, and graph algorithms.

For distributed-system evolution (replication, sharding, snapshot-scoped certificates, and literature), see `docs/explanation/DISTRIBUTED_PATHDB.md`.

## Research Foundation

PathDB draws from several areas of research:

### 1. Graph Database Query Optimization
- **Gubichev et al. (2013)**: "Query Processing and Optimization in Graph Databases" - Path indexing strategies
- **Zhao & Han (2010)**: "On Graph Query Optimization in Large Networks" - 531 citations, landmark paper on graph query optimization

### 2. Succinct Data Structures
- **Jacobson (1989)**: Succinct static data structures - compact representations with constant-time operations
- **Navarro (2016)**: "Compact Data Structures: A Practical Approach" - modern treatment

### 3. Bitmap Indexing
- **Lemire et al. (2016)**: "Roaring Bitmaps" - compressed bitmaps for fast set operations
- Used by: Apache Spark, Netflix, Pinot, Druid

### 4. Zero-Copy Serialization
- **rkyv**: Zero-copy deserialization for Rust
- **FlatBuffers/Cap'n Proto**: Efficient binary formats

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              PathDB Architecture                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────────┐   ┌──────────────────┐   ┌──────────────────────────┐ │
│  │  String Interner │   │  Entity Store    │   │   Relation Store         │ │
│  │                  │   │  (Columnar)      │   │   (Edge-List + Index)    │ │
│  │  "Person" → 0    │   │                  │   │                          │ │
│  │  "knows" → 1     │   │  types: [0,0,0]  │   │  Forward: (src,rel)→tgts │ │
│  │  "Alice" → 2     │   │  attrs: {...}    │   │  Backward: (tgt,rel)→srcs│ │
│  │  ...             │   │  type_idx: {...} │   │  Type: rel→bitmap        │ │
│  └────────┬─────────┘   └────────┬─────────┘   └────────────┬─────────────┘ │
│           │                      │                          │               │
│           └──────────────────────┼──────────────────────────┘               │
│                                  │                                          │
│                                  ▼                                          │
│                    ┌───────────────────────────┐                            │
│                    │       Path Index          │                            │
│                    │                           │                            │
│                    │  PathSig → (start → bitmap)│                           │
│                    │  [knows] → {0 → {1,2}}   │                            │
│                    │  [knows,knows] → {0 → {3}}│                           │
│                    └───────────────────────────┘                            │
│                                                                              │
│                    ┌───────────────────────────┐                            │
│                    │    Equivalence Index      │                            │
│                    │                           │                            │
│                    │  entity → [(equiv, type)] │                            │
│                    └───────────────────────────┘                            │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Binary Format

```
┌────────────────────────────────────────────────────────┐
│ Header (32 bytes)                                       │
│ ├─ Magic: "AXPD" (4 bytes)                             │
│ ├─ Version: u32 (4 bytes)                              │
│ ├─ String table offset: u64 (8 bytes)                  │
│ ├─ Entity table offset: u64 (8 bytes)                  │
│ └─ Relation table offset: u64 (8 bytes)                │
├────────────────────────────────────────────────────────┤
│ String Table                                            │
│ ├─ Count: u32                                          │
│ ├─ Offsets: [u32; count]                               │
│ └─ Data: concatenated strings                          │
├────────────────────────────────────────────────────────┤
│ Entity Table (Columnar)                                 │
│ ├─ Count: u32                                          │
│ ├─ Types: [StrId; count]                               │
│ ├─ Attr count per entity: [u16; count]                 │
│ └─ Attrs: [(StrId, StrId); total_attrs]                │
├────────────────────────────────────────────────────────┤
│ Relation Table                                          │
│ ├─ Count: u32                                          │
│ ├─ Relations: [(StrId, u32, u32, f32); count]          │
│ │             (type, source, target, confidence)        │
│ ├─ Forward Index: serialized HashMap                   │
│ └─ Backward Index: serialized HashMap                  │
├────────────────────────────────────────────────────────┤
│ Path Index                                              │
│ ├─ Sig count: u32                                      │
│ ├─ For each signature:                                 │
│ │   ├─ Path length: u8                                 │
│ │   ├─ Path: [StrId; length]                           │
│ │   ├─ Entry count: u32                                │
│ │   └─ Entries: [(u32, RoaringBitmap); entry_count]    │
├────────────────────────────────────────────────────────┤
│ Equivalence Index                                       │
│ └─ HashMap<u32, Vec<(u32, StrId)>>                     │
└────────────────────────────────────────────────────────┘
```

## Text snapshot export (`.axi`)

`.axpd` is optimized for performance and compactness. For **reviewability**, **diffability**, and
long-term audit trails, PathDB also supports a *reversible* textual snapshot format in `.axi`.

## Snapshot management (accepted plane + WAL)

For the single-node “append-only accepted `.axi` snapshots + derived PathDB WAL snapshots” store
used by the CLI, see `docs/howto/SNAPSHOT_STORE.md`.

### A) Lossless snapshot export (engine interchange): `PathDBExportV1`

This is a *reversible*, deterministic `.axi` representation of the **entire** PathDB state
(interned strings, entity ids, relation ids, confidences, etc).

- Export: `axiograph db pathdb export-axi <knowledge.axpd> -o <snapshot_export_v1.axi>`
- Import: `axiograph db pathdb import-axi <snapshot_export_v1.axi> -o <knowledge.axpd>`
- Rust↔Lean parse parity (exported snapshot): `make verify-pathdb-export-axi-v1`

The snapshot uses a fixed schema named `PathDBExportV1` and is designed to round-trip exactly:

- Interned strings are stored as a stable table (`StringId_N ↔ StrUtf8Hex_<utf8-bytes-as-hex>`).
- Entity ids (`Entity_N`) and relation ids (`Relation_N`) are preserved.
- Relation confidences are stored as `F32Hex_<ieee754-bits>` so there is no float parsing/rounding.

This snapshot schema is an *engineering interchange* format (for storage and verification pipelines),
distinct from the canonical domain `.axi` examples in `examples/`.

### B) Canonical module export (human-readable): `axi_v1` schema/theory/instance

If PathDB contains the `.axi` **meta-plane** produced by importing a canonical module, we can export
that module back into canonical `.axi` syntax:

- Export: `axiograph db pathdb export-module <knowledge.axpd> -o <module.axi> [--module <name>]`

This is the format you usually want for:
- review / version control diffs of accepted knowledge
- treating `.axi` as the canonical “meaning plane”

It is intentionally **not** lossless for arbitrary PathDB engine state; keep `.axpd` and/or
`PathDBExportV1` for full engine interchange when needed.

## Key Optimizations

### 1. String Interning

All strings are stored exactly once and referenced by 4-byte IDs:

```rust
// Before: 24+ bytes per String
struct Entity { type: String, ... }  // "Person" = 24 bytes

// After: 4 bytes per reference
struct Entity { type: StrId, ... }   // StrId(0) = 4 bytes
```

**Memory savings**: 80%+ for string-heavy data

### 2. Columnar Entity Storage

Entities stored column-wise for cache efficiency:

```rust
// Row-oriented (cache-unfriendly for type scans)
entities: Vec<Entity>  // Scattered memory access

// Columnar (cache-friendly)
types: Vec<StrId>       // Sequential access
attrs: HashMap<StrId, HashMap<u32, StrId>>
```

**Speedup**: 2-5x for type-filtered queries

### 3. Bitmap Joins

Set operations use Roaring bitmaps instead of hash sets:

```rust
// Hash set intersection: O(min(m,n))
let result: HashSet = a.intersection(&b).collect();

// Bitmap intersection: O(min(m,n)) but SIMD-accelerated
let result: RoaringBitmap = &a & &b;
```

**Speedup**: 10-100x for large sets

### 4. Path Index

Pre-computed reachability for common path lengths:

```rust
// Without index: O(|V| * |E|) per query
fn follow_path(start, path) {
    let mut current = vec![start];
    for rel in path {
        current = current.flat_map(|e| neighbors(e, rel));
    }
}

// With index: O(1) lookup
fn follow_path(start, path) {
    path_index.get(&PathSig(path)).get(&start).clone()
}
```

**Speedup**: 100-1000x for indexed paths

### 5. Memory Mapping

Large databases accessed via mmap without full load:

```rust
// Full load: O(file_size) memory
let db = PathDB::from_bytes(&std::fs::read("kg.axpd")?)?;

// Memory mapped: O(1) initial, pages loaded on demand
let mmap = unsafe { Mmap::map(&file)? };
let db = PathDB::from_mmap(&mmap)?;
```

**Memory savings**: Only accessed pages loaded

### 6. FactIndex (canonical `.axi` fact nodes)

Canonical `.axi` instances represent n-ary relation tuples like:

```axi
Flow = { (from=a, to=b), (from=a, to=c) }
```

PathDB imports these by **reifying** each tuple as a dedicated *fact node* with:

- `axi_relation = "Flow"`
- `axi_schema = "<schema name>"`
- edges `fact -from-> a`, `fact -to-> b`, ...

AxQL fact atoms (`Flow(from=a, to=b)`) filter heavily on `axi_relation`, so repeatedly
scanning the attribute column becomes a bottleneck for interactive workloads.

PathDB therefore maintains a rebuildable in-memory **FactIndex**:

- `(axi_schema, axi_relation) -> {fact nodes}` as Roaring bitmaps
- `axi_relation -> {fact nodes}` (union across schemas)
- optional key-based lookup derived from meta-plane key constraints:
  `(axi_schema, axi_relation, key_fields, key_values) -> {fact nodes}`
- optional context/world scoping (derived on import when canonical `.axi` uses `@context` / `ctx=...`):
  - `context_entity_id -> {fact nodes}`
  - `(context_entity_id, axi_schema, axi_relation) -> {fact nodes}`

Entry points:

- `PathDB::fact_nodes_by_axi_relation`
- `PathDB::fact_nodes_by_axi_schema_relation`
- `PathDB::fact_nodes_by_axi_key`
- `PathDB::fact_nodes_by_context`
- `PathDB::fact_nodes_by_context_axi_schema_relation`

Implementation: `rust/crates/axiograph-pathdb/src/fact_index.rs` (lazy, invalidated on DB mutation).

#### Example: schema + relation lookup (fast “WHERE axi_relation = …”)

```rust
use axiograph_pathdb::PathDB;

// ... import a canonical `.axi` module into `db` ...
let flow_facts = db.fact_nodes_by_axi_schema_relation("S", "Flow");
for fact in flow_facts.iter().take(5) {
    println!("Flow fact node id = {fact}");
}
```

If you *don’t* know the schema (or don’t care), you can also query by relation only:

```rust
let flow_facts_any_schema = db.fact_nodes_by_axi_relation("Flow");
```

#### Example: key lookup (turn some fact atoms into near-index lookups)

If the schema has a meta-plane key constraint like:

```axi
theory Keys on S:
  constraint key Flow(from, to)
```

then you can look up the (typically unique) fact node for a bound key:

```rust
// `a_id` / `b_id` are entity ids for `a` / `b` (e.g. found via `name` attr).
let hits = db
    .fact_nodes_by_axi_key("S", "Flow", &["from", "to"], &[a_id, b_id])
    .expect("key index exists for Flow(from,to)");

assert!(hits.len() <= 1);
```

#### Example: AxQL fact atoms benefit automatically

AxQL fact atoms like:

```text
q select ?f where ?f = Flow(from=a, to=b)
```

expand into an `axi_relation` filter plus field-edge constraints, and the executor:

- uses `FactIndex` to avoid scanning the attribute column for `axi_relation`, and
- when a key constraint is present *and* all key fields are bound to constants,
  uses a key lookup to aggressively prune the fact-node candidate set.

### 7. Compiled-query cache (REPL)

The REPL is optimized for repeated querying of a single snapshot. It keeps a small LRU
cache of **compiled AxQL queries**, keyed by:

- query IR digest (`axql_query_ir_digest_v1`), and
- the REPL's snapshot key (updated on `load` / `import_*` / `gen`)

Cached artifacts include:

- lowered query
- candidate bitmaps + join order
- compiled RPQ automata (plus per-source reachability cache)

This turns repeated queries into mostly “search only” work.

## Query Patterns

### 1. Type Query (SQL-like)
```rust
// SELECT * FROM entities WHERE type = 'Person'
let persons = db.find_by_type("Person");  // Returns bitmap
```

### 2. Relation Traversal
```rust
// SELECT target FROM relations WHERE source = ? AND type = 'knows'
let friends = db.follow_one(alice, "knows");
```

### 3. Path Query
```rust
// Follow path: alice -[knows]-> -[knows]-> ?
let friends_of_friends = db.follow_path(alice, &["knows", "knows"]);
```

### 4. Path Discovery
```rust
// Find how alice is related to bob
let paths = db.find_paths(alice, bob, 5);
// Returns: vec![[knows], [knows, knows], ...]
```

### 5. Equivalence Query (HoTT)
```rust
// Find equivalent suppliers
let equivalents = db.find_equivalent(supplier_a);
// Returns: vec![(supplier_b, "SupplierEquiv"), ...]
```

### 6. Hybrid Vector + Path Query
```rust
// Vector search finds relevant chunks
let vector_hits = vector_db.search("cutting titanium", 10);

// PathDB filters to those matching path constraints
let relevant = db.hybrid_query(
    vector_hits,
    &PathQuery::FollowPath { 
        start: titanium_id, 
        path: vec!["RecommendedFor".into()] 
    }
);
```

## Lean Integration (Certificates)

PathDB is the high-performance **untrusted engine**. The trusted meaning of:

- what counts as a valid path witness,
- how paths rewrite/normalize (groupoid/rewrite semantics),
- how confidence combines (deterministically),

is defined in **Lean (mathlib-backed)**. Rust is allowed to be clever and fast, but must emit
**certificates** that a small Lean checker validates.

Practically:

- PathDB operations (queries, normalization, migrations) can emit a versioned `CertificateV2`.
- Lean re-computes the corresponding semantic function and checks “result = recompute(input)”.
- This keeps the trusted boundary small and deterministic (no floats; fixed-point probabilities).

This also enables a clean **hybrid discovery** story:

- Vector / full-text retrieval provides candidate evidence (chunks) and is explicitly *approximate*.
- PathDB applies structural constraints and returns **certificate-carrying** answers when requested.

See:
- `docs/reference/CERTIFICATES.md` (certificate schema + examples)
- `docs/howto/FORMAL_VERIFICATION.md` (how Rust↔Lean checking is wired today)

## Performance Characteristics

| Operation | Time Complexity | Notes |
|-----------|-----------------|-------|
| Type lookup | O(1) | Bitmap retrieval |
| Single-hop | O(k) | k = out-degree |
| N-hop (indexed) | O(1) | Pre-computed |
| N-hop (unindexed) | O(|V|^n) | Worst case |
| Path discovery | O(|V| + |E|) | BFS |
| Bitmap AND | O(min(m,n)) | SIMD accelerated |
| Bitmap OR | O(m+n) | SIMD accelerated |
| Equivalence lookup | O(1) | Hash map |

## Comparison with Other Systems

| Feature | PathDB | Neo4j | TypeDB | PostgreSQL |
|---------|--------|-------|--------|------------|
| Path indexing | ✓ Pre-computed | ✓ At query time | ✓ Rule-based | ✗ JOINs |
| Binary format | ✓ Zero-copy | ✗ | ✗ | ✗ |
| Type system | ✓ Lean-checked certificates | ✗ | ✓ Types | ✓ SQL types |
| Bitmap joins | ✓ Roaring | ✗ | ✗ | ✓ Bitmap indexes |
| HoTT equivalences | ✓ Native | ✗ | ✗ | ✗ |
| Vector hybrid | ✓ Bridge API | Plugin | ✗ | pgvector |

## Future Work

1. **Incremental Path Index**: Update path index without full rebuild
2. **Distributed PathDB**: Sharding across machines
3. **GPU Acceleration**: Bitmap operations on GPU
4. **Adaptive Indexing**: Build indexes based on query patterns
5. **Streaming Updates**: Append-only binary format with compaction
6. **Certified discovery operators**: expand certificates beyond reachability into normalization/reconciliation
7. **Ingestion integration**: treat `.axi` as canonical facts, and treat indexes as derived and rebuildable
