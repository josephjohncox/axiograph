# Distributed PathDB / Axiograph: Design Notes + Reading List

**Diataxis:** Explanation  
**Audience:** contributors

This document sketches how **PathDB** (and the broader Axiograph stack) can evolve into a **distributed** system without breaking the core project commitment:

> *Untrusted engines compute; a small trusted checker verifies.*

In a distributed setting the “untrusted engine” is an entire cluster. The trusted boundary must remain small, deterministic, and well-defined.

This doc is intentionally **architecture-first** and **certificate-first**:

- the distributed system should scale ingestion + storage + query,
- while continuing to return **proof-carrying results** that can be verified (Lean) against stable semantics.

For the base single-node design, see `docs/explanation/PATHDB_DESIGN.md`.

---

## 0. Executive summary

PathDB today is a single-node, binary, indexed KG store. The most robust path to distribution is:

1. Treat **facts** as the canonical replicated state (append-only log + snapshots).
2. Treat **PathDB indexes** (bitmaps/path indexes) as **derived, rebuildable state** per shard/replica.
3. Ensure every query result is bound to a **snapshot id** and accompanied by a **certificate**.
4. If you need offline/third-party verification, bind certificates to a **cryptographic commitment** (Merkle root / transparency log) of the snapshot.

In other words: distribute the *data plane*, keep the *meaning plane* stable.

### 0.1 Read replicas (v1, implemented)

We already have a practical, semantics-preserving “write master / read replica” primitive:

- treat an accepted-plane directory as the unit of replication (it contains both the accepted snapshot store and the PathDB WAL),
- replicate by copying missing **immutable, content-addressed objects** first,
- then update the small mutable `HEAD` pointers.

This is implemented as a filesystem-only command:

- `axiograph db accept sync --from <master_dir> --dir <replica_dir> --layer both`

See `docs/howto/SNAPSHOT_STORE.md` for usage and gotchas.

---

## 1. Design constraints (Axiograph-specific)

### 1.1 Trust boundary

- The distributed cluster is untrusted: it may return wrong answers.
- Therefore, clients (or an internal verifier service) must verify certificates produced by the cluster.
- Verification must be deterministic: avoid floats and platform-dependent behavior.

### 1.2 Certificates must be snapshot-scoped

Every answer certificate must say:

- which snapshot of the KG it is about (logical time / commit index),
- which semantics version it assumes (certificate kind + version),
- and how to validate any data dependencies.

In a distributed system, “what snapshot?” becomes the *hard part*.

**Practical note (current repo)**: for auditability and offline review we can represent a snapshot as:

- `.axpd` (binary PathDB, fast to load), and/or
- `.axi` using the reversible `PathDBExportV1` schema (`axiograph db pathdb export-axi`), which is diffable and can be hashed/committed.

Either way, certificates should bind to a stable snapshot identifier (commit index, hash, or Merkle root).

### 1.3 Partitioning must not change semantics

Sharding is a performance choice. It must not change the meaning of:

- path derivations,
- normalization/rewrite semantics,
- reconciliation policy semantics,
- confidence combination.

Partitioning only changes *how we find proofs*, not *what counts as a proof*.

---

## 2. Canonical distributed shape: log + snapshots + derived indexes

The cleanest approach is “event-sourced KG”:

1. **Canonical fact log**
   - append-only records: “add fact”, “retract fact”, “supersede”, “reconcile decision”, etc.
2. **Periodic snapshots**
   - a snapshot defines a closed set of facts at a commit index/time.
3. **Derived indexes per node**
   - PathDB-like adjacency + bitmap/path indexes are built from a snapshot.

This matches production DB practice: derived indexes can be rebuilt; the canonical log cannot be reconstructed if corrupted.

### 2.1 Why this fits proof-carrying results

Certificates become stable if they refer to:

- a snapshot id,
- and fact identifiers that are stable within that snapshot (or globally content-addressed).

This avoids the “my answer is true, but only in whatever inconsistent replica state you happened to read” failure mode.

---

## 3. Consistency models: pick deliberately

### Option A (recommended for v1): strongly consistent commit log

- Use a consensus protocol (Raft/Paxos) to serialize writes.
- All queries read at a chosen log index (linearizable or at least “read-your-writes”).

Pros:

- simplest semantics and easiest certificate anchoring (“snapshot = commit index N”)
- easier reconciliation (one canonical decision stream)

Cons:

- operational complexity of consensus
- write throughput bounded by leader and replication

### Option B: eventual consistency with CRDT state

- Facts are stored in CRDT sets/maps and merged across replicas.
- Conflicts are resolved by deterministic merge rules (or by explicit reconciliation events).

Pros:

- offline writes / multi-writer replication
- high availability

Cons (especially for verifiability):

- a certificate must specify which merged state it is about (vector clock / version vector)
- reconciliation semantics must be extremely explicit and certificate-checked
- “what is the truth?” becomes a policy question, not just data

### Option C: hybrid

- Strongly consistent canonical log for “accepted knowledge”.
- Eventually consistent edge caches and ingestion buffers for “proposals”.

This fits Axiograph’s current worldview well: LLM outputs and ingestion are untrusted proposals until reconciled and certified.

---

## 4. Sharding strategies for a knowledge graph

You will likely need different partitioning for different workloads.

### 4.1 Entity-hash (simple)

Partition by `entity_id mod num_shards`.

Pros:

- easy to rebalance
- even distribution for random ids

Cons:

- path queries cross shards frequently

### 4.2 Community/cluster partitioning (graph-aware)

Partition by graph structure to minimize cut edges.

Pros:

- fewer cross-shard traversals for locality-heavy queries

Cons:

- rebalancing is hard (graph changes)
- requires periodic repartition

### 4.3 Vertex-cut vs edge-cut

High-degree nodes (common in KGs) can destroy performance if placed naively. Frameworks like PowerGraph use vertex-cuts to handle skew.

Recommendation:

- start with entity-hash + replication of high-degree adjacency lists,
- then consider vertex-cut for “hub” entities when/if needed.

---

## 5. Query execution strategies (distributed)

### 5.1 “k-hop” / bounded path queries

Many practical KG queries are bounded-length (1–3 hops):

- use distributed joins / message passing
- use adjacency lists and local indexes

### 5.2 General reachability / path search

General reachability is expensive to index exactly at scale. Options:

- on-demand distributed BFS/bi-BFS with early cutoff
- precompute limited-length path indexes per shard
- maintain landmark/hub indexes to prune search
- accept approximate search, but keep soundness via certificates for returned paths

### 5.3 Incremental maintenance (streaming updates)

If you adopt a fact log, you can maintain derived indexes incrementally:

- incremental view maintenance
- dataflow-style computation (timely/differential)
- partial recomputation per shard

This is attractive if reconciliation and ingestion produce frequent updates.

### 5.4 Ingestion + discovery pipelines (continuous)

A distributed PathDB is only useful if it stays connected to the “real world” sources (code, docs, SQL, Confluence, tickets).
That means treating ingestion and discovery as *continuous* processes:

1. **Ingestion produces proposals**
   - Parsers and extractors (including LLMs) emit *proposal facts* with provenance + confidence.
   - Proposal facts are not “truth”; they are candidates for reconciliation/acceptance.
2. **Reconciliation produces accepted facts**
   - A policy (human-in-the-loop, automatic rules, or both) selects which proposals become accepted.
   - Accepted facts enter the canonical log and thus snapshots.
3. **Discovery consumes both planes**
   - Approximate discovery (RAG/vector search) runs on document chunks and proposal facts.
   - Certified discovery runs on accepted facts and returns certificate-carrying answers.

**Important:** “Chain of thought discovery” should be implemented as **structured discovery traces**:

- store evidence pointers (chunk ids, file paths, citations) and a short *public rationale*,
- store a certificate when the output is an accepted/certified inference,
- do **not** store raw model hidden reasoning (it is not stable, not auditable, and not meant to be logged).

For concrete pipeline shape and artifact formats, see `docs/tutorials/CONTINUOUS_INGEST_AND_DISCOVERY.md`.

---

## 6. Certificates in a distributed system

Certificates get more subtle once data is sharded and replicated.

### 6.1 What a client needs to verify

For a path witness certificate (reachability), a checker ultimately needs to know:

1. each edge/fact referenced exists in the snapshot,
2. the endpoints line up (well-formedness),
3. confidence/rewrite/reconciliation calculations follow the agreed semantics.

(2) and (3) are purely semantic and live comfortably in Lean.
(1) is a data-availability / integrity question.

### 6.2 Two verification modes

**Mode 1: online trust-but-verify-with-the-cluster**

- The verifier queries the cluster to confirm edge existence.
- This is not fully trustless: it assumes the cluster answers membership queries honestly.

This is still useful inside a single trust domain (one company cluster).

**Mode 2: offline / third-party verification via authenticated membership proofs**

- Bind every snapshot to a cryptographic commitment (Merkle root).
- Provide membership proofs for each referenced fact/edge in the certificate (or alongside it).
- The verifier checks membership proofs against the root.

This is the clean path if you want:

- auditability across organizational boundaries,
- tamper-evident history (“knowledge transparency”),
- portable certificates (verify without access to the cluster).

### 6.3 Transparency logs (recommended primitive for Mode 2)

If you already have an append-only fact log, transparency log ideas fit naturally:

- commit entries to an append-only Merkle tree,
- publish signed roots,
- and prove membership (inclusion) for facts referenced by certificates.

### 6.4 Certificate size control

Membership proofs can bloat certificates. Options:

- batch proofs (multi-inclusion proofs),
- hash-cons DAG-shaped proofs (Merkleize proof DAGs),
- cache verified inclusion proofs by hash.

---

## 7. Distributed reconciliation (conflicts) and provenance

Distributed systems amplify conflicts:

- concurrent writes,
- divergent ingestion sources,
- partial visibility across replicas.

Recommendations:

1. Make reconciliation decisions explicit events in the canonical log (“resolution chosen because …”).
2. Emit a reconciliation certificate whose meaning is defined in Lean (policy evaluation + any merge math).
3. Treat “proposal facts” as separate from “accepted facts” so the accepted snapshot stays coherent.

If you need multi-writer reconciliation, CRDT-like designs can work, but only if the merge policy is explicit and certificate-checked.

---

## 8. A staged migration plan (pragmatic)

### Stage 0: single-node PathDB + remote API

- Keep storage local.
- Add a gRPC/HTTP service boundary.
- Establish stable snapshot ids and deterministic certificate formats.

### Stage 1: replicated read replicas

- One writer node, N read replicas.
- Replicate the fact log (or snapshot files) and rebuild indexes locally.

### Stage 2: strongly consistent commit log

- Introduce Raft/Paxos for the canonical log of accepted facts.
- Snapshot at commit indices; queries specify which snapshot they want.

### Stage 3: sharded store

- Shard facts by entity id (initially).
- Keep a coordinator for multi-shard query planning.
- Return certificates; optionally include authenticated membership proofs later.

### Stage 4: “trustless-ish” verification

- Add transparency log / Merkle commitments for snapshots.
- Make certificates independently verifiable (membership proofs).

---

## 9. Literature and pointers (selected)

### 9.1 Distributed systems foundations

- Lamport — “Paxos Made Simple” (2001). https://lamport.azurewebsites.net/pubs/paxos-simple.pdf
- Ongaro & Ousterhout — “In Search of an Understandable Consensus Algorithm (Raft)” (2014). https://raft.github.io/raft.pdf
- Ghemawat, Gobioff, Leung — “The Google File System” (2003). https://research.google/pubs/pub51/
- Chang et al. — “Bigtable: A Distributed Storage System for Structured Data” (2006). https://www.usenix.org/legacy/event/osdi06/tech/chang/chang.pdf
- Corbett et al. — “Spanner: Google’s Globally-Distributed Database” (2012). https://research.google/pubs/pub39966/
- DeCandia et al. — “Dynamo: Amazon’s Highly Available Key-value Store” (2007). https://www.allthingsdistributed.com/files/amazon-dynamo-sosp2007.pdf
- Kleppmann — *Designing Data-Intensive Applications* (2017). https://dataintensive.net/

### 9.2 Distributed graph processing / graph databases

- Malewicz et al. — “Pregel: A System for Large-Scale Graph Processing” (2010). https://doi.org/10.1145/1807167.1807184
- Gonzalez et al. — “PowerGraph: Distributed Graph-Parallel Computation on Natural Graphs” (OSDI 2012). https://www.usenix.org/conference/osdi12/technical-sessions/presentation/gonzalez
- Shao, Wang, Li — “Trinity: A Distributed Graph Engine on a Memory Cloud” (SIGMOD 2013). https://doi.org/10.1145/2463676.2463705
- Zeng et al. — “Trinity.RDF: A Distributed In-Memory RDF Engine for Efficient Querying of Large Graphs” (VLDB 2013). https://doi.org/10.14778/2536222.2536232
- Xin et al. — “GraphX: Unifying Data-Parallel and Graph-Parallel Analytics” (2013). https://arxiv.org/abs/1402.2394

### 9.3 Incremental view maintenance / dataflow (helpful for streaming updates)

- Murray et al. — “Noria: dynamic, partially-stateful data-flow for high-performance web applications” (OSDI 2018). https://www.usenix.org/conference/osdi18/presentation/murray
- McSherry et al. — “Differential Dataflow” (CIDR 2013). https://www.cidrdb.org/cidr2013/Papers/CIDR13_Paper111.pdf
- McSherry et al. — “Timely Dataflow” (2015). https://doi.org/10.1145/2723372.2742785

### 9.4 CRDTs (if you pursue eventual consistency)

- Shapiro et al. — “Conflict-Free Replicated Data Types” (2011). https://doi.org/10.1007/978-3-642-24550-3_29
- CRDT literature hub: https://crdt.tech/papers.html

### 9.5 Authenticated data structures / transparency logs (for trustless-ish verification)

- Laurie, Langley, Kasper — RFC 6962 “Certificate Transparency” (2013). https://www.rfc-editor.org/rfc/rfc6962
- Trillian (transparent, tamper-evident logs): https://github.com/google/trillian
- Goodrich, Tamassia, Triandopoulos — “Authenticated Data Structures for Graph and Geometric Searching” (CT-RSA 2009). https://doi.org/10.1007/978-3-642-00862-7_18

### 9.6 Cryptographic query verification (optional, later)

If you need stronger guarantees than “certificate checking + authenticated membership proofs” (e.g., you want to prove *the query execution itself* was performed correctly on committed data across trust boundaries), there is a large literature on verifiable computation and ZK proofs for databases.

One recent pointer in the “ZK for SQL query results” direction:

- PoneglyphDB — “Verifiable Query Execution for Blockchain-based Databases” (preprint). https://kira.cs.umd.edu/papers/poneglyphdb_preprint.pdf

This is a *later-layer* for Axiograph: it adds heavy cryptographic complexity and is usually only justified for multi-organization deployments where the verifier cannot rely on the operator for honest execution.

---

## 10. What to implement first (Axiograph recommendation)

If the goal is “distributed, production-ready, and still verifiable”, the recommended first step is:

1. Strongly consistent canonical log for accepted facts (or single-writer log + replicas initially).
2. Snapshot ids integrated into certificates.
3. Derived PathDB indexes rebuilt per replica/shard.
4. Add transparency/Merkle commitments only when you need offline/third-party verification.
