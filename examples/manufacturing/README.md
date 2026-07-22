# Manufacturing Examples

Manufacturing examples focus on typed paths, modalities, supply-chain flow, and
HoTT-flavored witnesses.

- `SupplyChainHoTT.axi` is useful for certified query and path examples.
- `SupplyChainModalitiesHoTT.axi` adds observed/evidence/modal distinctions.

Useful question-first command:

```bash
cargo run --manifest-path rust/Cargo.toml -p axiograph-cli -- \
  discover competency-questions examples/manufacturing/SupplyChainHoTT.axi \
  --from-cq examples/competency_questions/supply_chain.cq \
  --no-schema \
  --out build/examples/supply_chain_competency_questions.json
```
