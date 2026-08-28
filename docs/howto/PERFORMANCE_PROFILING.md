# Performance profiling

Axiograph has in-memory PathDB/AxQL performance runners and an optional CLI CPU
profiler. Authenticated `.axpd` materialization currently has focused correctness
tests rather than a public benchmark command.

## In-memory PathDB and AxQL runners

Build and run in release mode from `rust/`:

```bash
cargo run -p axiograph-cli --release -- \
  tools perf pathdb \
  --entities 200000 \
  --edges-per-entity 8 \
  --rel-types 8 \
  --queries 50000

cargo run -p axiograph-cli --release -- \
  tools perf axql \
  --entities 200000 \
  --edges-per-entity 8 \
  --rel-types 8 \
  --mode star \
  --queries 2000 \
  --limit 200

cargo run -p axiograph-cli --release -- \
  tools perf scenario \
  --scenario proto_api \
  --scale 10000 \
  --index-depth 3
```

These runners construct process-local query state. They do not publish or open
`.axpd` files.

## Authenticated materialization measurements

Use release-mode focused tests when profiling deterministic SQLite construction,
verification, and hydration:

```bash
cargo test --release -p axiograph-store --test materialization -- --nocapture
cargo test --release -p axiograph-pathdb --test materialization_tests -- --nocapture
```

A valid production benchmark must measure these phases separately:

1. canonical row validation and sorting;
2. in-memory SQLite population;
3. deterministic backup and fsync;
4. exact-image hashing;
5. read-only schema/anchor/digest verification;
6. PathDB hydration and process-local index rebuild.

Do not benchmark removed bincode, sectioned-binary, WAL checkout, hardlink/copy,
or sidecar-index paths. They are not supported formats.

## Predictive proposal rollout/eval runner

The runner exercises bounded evidence-plane proposal rollouts and reports
basic precision/recall when holdouts are available:

```bash
axiograph tools perf proposal-rollout \
  --input examples/Family.axi \
  --proposal-adapter-plugin scripts/axiograph_predictive_proposal_plugin_baseline.py \
  --proposal-adapter-plugin-arg=--strategy \
  --proposal-adapter-plugin-arg=oracle \
  --horizon-steps 3 \
  --rollouts 2 \
  --holdout-frac 0.2 \
  --out-json build/predictive_proposal_perf.json
```

Proposal inputs must be exact canonical `.axi` bytes. A derived PathDB image is
not a source-recovery or adapter-input format.

## Built-in sampling profiles

Build the CLI with profiling enabled:

```bash
cargo build -p axiograph-cli --release --features profiling
```

Prefix any supported long-running CLI command with profiling flags:

```bash
./target/release/axiograph \
  --profile all \
  --profile-interval 10 \
  --profile-out ../build/profiles/axql \
  tools perf axql --entities 200000 --edges-per-entity 8 --rel-types 8 \
  --mode star --queries 2000 --limit 200
```

Formats:

- `--profile flamegraph` writes `<out>.svg`;
- `--profile pprof` writes `<out>.pb`;
- `--profile folded` writes `<out>.folded`;
- `--profile all` writes all formats.

`--profile-signal` additionally emits a live snapshot on `SIGUSR2` on Unix.

## External profilers

On Linux, `cargo-flamegraph` can wrap a supported perf runner:

```bash
cargo flamegraph -p axiograph-cli --bin axiograph --release -- \
  tools perf pathdb --entities 200000 --edges-per-entity 8 \
  --rel-types 8 --queries 50000
```

On macOS, use Instruments Time Profiler if system sampling restrictions prevent
`cargo-flamegraph` from attaching.
