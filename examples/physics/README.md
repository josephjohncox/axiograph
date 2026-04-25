# Physics Examples

Physics examples are domain-ontology teaching fixtures. They show how to model
scientific concepts, measurement relations, and evidence-bearing domain facts
without making the domain model part of the Axiograph core runtime.

Start with:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/physics/PhysicsOntology.axi
```
