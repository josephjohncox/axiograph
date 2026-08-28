# Axiograph

> **Proof-carrying ontology engineering**
>
> Build typed knowledge systems whose important claims carry explicit scope,
> provenance, and machine-checkable evidence.
>
> **Canonical meaning · Typed runtime · Lean-checked claims · Bounded AI**

Axiograph is a greenfield ontology workbench for software, business rules,
semantic version control, and high-value inference. It separates fast,
untrusted computation from a small checker that verifies the strongest
supported claims.

```text
accepted .axi bytes + import closure + accepted snapshot
  → CanonicalCompiler
  → CompiledKernelSnapshot
  → KernelSnapshotIr
  → runtime indexes, reports, and certificates
  → optional Lean verification for the supported finite fragment
```

## The contract

Three rules organize the system:

1. **Accepted `.axi` is the meaning plane.** PathDB, graph databases,
   embeddings, generated code, and model output are derived or advisory.
2. **The engine computes; the checker verifies.** Rust performs ingestion,
   query execution, reconciliation, storage, and certificate emission. Lean
   checks selected certificate and gate fragments.
3. **Claims carry limits.** Exact finite-query completeness is not open-world
   completeness. Runtime evidence is not accepted truth. Confidence is not
   path equality.

## Choose a reading path

| Path | What you will learn |
| --- | --- |
| [**Understand the system**](explanation/SYSTEM_OVERVIEW.md) | The architecture, semantic spine, and trust boundary. |
| [**Run a checked query**](tutorials/CERTIFIED_QUERYING_101.md) | How one result moves from canonical input through Rust and Lean. |
| [**Design an ontology**](explanation/TYPED_ONTOLOGY_ENGINEERING.md) | How types, relation objects, equations, contexts, and competency questions fit together. |
| [**Audit a claim**](reference/TRUSTED_KERNEL.md) | What is trusted, checked, operational, or outside scope. |

## First executable proof

From the repository root, run the regulated-shipment workflow:

```bash
make verify-regulated-shipment
```

The workflow compiles canonical modules, checks the supported finite theory and
query fragment, validates an independently executed Lean receipt, performs a
typed merge, reopens authenticated SQLite state, hydrates read-only PathDB
state, and builds receipt-bound grounding with stable identities.

For the complete workflow, read [The canonical semantic
spine](howto/CANONICAL_SEMANTIC_SPINE.md). For the exact trust envelope, read
[The trusted kernel](reference/TRUSTED_KERNEL.md).

## How this book is organized

| Part | Question |
| --- | --- |
| **The System** | What is Axiograph, and where does meaning live? |
| **Typed Semantics** | Which mathematical structures shape schemas, paths, and constraints? |
| **Proof-Carrying Claims** | What does the checker establish, and what remains operational? |
| **Build With Axiograph** | How do I author, query, ingest, and generate software? |
| **Evolve And Integrate** | How do accepted models change and project into other systems? |
| **Operate And Verify** | How do I test, deploy, inspect, and profile the implementation? |

The book is generated directly from the maintained documentation tree. The web
edition and repository docs therefore share one source of truth.
