# Manufacturing Examples

Manufacturing examples focus on typed paths, modalities, supply-chain flow, and
HoTT-flavored witnesses.

- `SupplyChainHoTT.axi` is useful for certified query and path examples.
- `SupplyChainModalitiesHoTT.axi` adds observed/evidence/modal distinctions.

Useful command:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  cert query examples/manufacturing/SupplyChainHoTT.axi \
  --lang axql \
  'select ?to where name("RawMetal_A") -Flow-> ?to limit 10' \
  --out build/examples/supply_chain_query_cert.json
```
