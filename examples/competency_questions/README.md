# Competency Questions

Competency-question fixtures encode the questions an ontology or behavior case
must answer. They are review gates, coverage drivers, and useful agent-facing
prompts for typed ontology repair.

Human-authored CQ suites use `.cq` text files so ontology engineers can read
and review coverage questions without hand-authoring JSON or raw queries. Each
file starts with `version competency_question_bundle_v1` and contains
question-first records:

```text
question shipment_release:
  ask: Shipment release should be traceable.
  expect: exists RegulatedLine.ShipmentFulfills(shipment=?s, order=?o, work_order=?wo, ctx=?c, time=?t)
  min_rows: 1
  weight: 2.0
```

The loader lowers simple `expect: exists Schema.Rel(role=value, ...)` and
`expect: instance of Schema.Type` records into executable typed queries. Less
structured `ask` / `about` / `given` / `expect` records still load, but reports
mark them as unresolved authoring obligations until a typed lowering exists.

JSON `competency_question_bundle_v1` remains supported as a tool/report boundary
and test harness format, not as the preferred authoring surface.

To inspect the lowered tool boundary explicitly:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover competency-questions examples/physics/PhysicsOntology.axi \
  --no-schema \
  --from-cq examples/competency_questions/physics.cq \
  --out build/examples/physics_competency_questions.json
```
