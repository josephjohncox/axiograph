# Economics Examples

`EconomicFlows.axi` teaches a small business/economic ontology with typed flows.
It is a good first example after `examples/Family.axi` because the domain is
small but less toy-like.

Useful command:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  check validate examples/economics/EconomicFlows.axi
```
