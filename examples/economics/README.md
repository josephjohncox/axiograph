# Economics Examples

`EconomicFlows.axi` teaches a compact business/economic ontology with typed flows.
It is a good first example after `examples/Family.axi` because the domain is
easy to inspect while still demonstrating business-facing typed relations.

Useful command:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/economics/EconomicFlows.axi
```
