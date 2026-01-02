# Performance profiling

This repo includes a few built-in ways to profile "where the time went". Phase timings are dependency-free; deeper CPU profiling is available behind an optional feature flag.

## Profile PathDB WAL checkout vs rebuild

`axiograph db accept pathdb-build` has two important modes:

- **Checkpoint fast path (default):** if a snapshot checkpoint exists, it "checks out" the `.axpd` by hardlink/copy.
- **Rebuild slow path (`--rebuild`):** rebuilds from accepted `.axi` + replays WAL ops + rebuilds indexes.

Use phase timings (human-readable):

```bash
axiograph db accept pathdb-build \
  --dir build/accepted_plane \
  --snapshot head \
  --out build/head.axpd \
  --timings
```

Or write timings to JSON:

```bash
axiograph db accept pathdb-build \
  --dir build/accepted_plane \
  --snapshot head \
  --out build/head.axpd \
  --timings-json build/pathdb_build_timings.json
```

Force the rebuild hot path (useful for profiling):

```bash
axiograph db accept pathdb-build \
  --dir build/accepted_plane \
  --snapshot head \
  --out build/head_rebuild.axpd \
  --rebuild \
  --timings
```

### One-command demo script

The easiest way to get both "checkout" and "rebuild" timings:

```bash
./scripts/profile_pathdb_wal_build.sh
```

This will:

- generate a large demo snapshot store if needed (via `scripts/graph_explorer_deep_knowledge_demo.sh`),
- run `pathdb-build` once using the checkpoint fast path,
- run `pathdb-build --rebuild` to profile the slow path,
- write `timings_*.json` into `build/profile_pathdb_wal_build/`.

## Built-in sampling profiles (feature-gated)

For deeper detail than phase timings, you can enable the optional CPU profiler
in the CLI and emit flamegraphs / pprof data / folded callstack dumps.

Build with the profiling feature:

```bash
cd rust
cargo build -p axiograph-cli --release --features profiling
```

---

## World model MPC/eval harness

The perf harness can exercise JEPA/world-model rollouts and report guardrail
deltas plus basic precision/recall (when running against `.axi` with holdouts).

```bash
axiograph tools perf world-model \
  --input examples/Family.axi \
  --world-model-plugin scripts/axiograph_world_model_plugin_baseline.py \
  --world-model-plugin-arg --strategy oracle \
  --horizon-steps 3 \
  --rollouts 2 \
  --holdout-frac 0.2 \
  --out-json build/world_model_perf.json
```

Then run any command with `--profile`:

```bash
./target/release/axiograph \
  --profile flamegraph \
  --profile-out ../build/profiles/pathdb_build \
  db accept pathdb-build --dir ../build/accepted_plane --snapshot head --out ../build/head.axpd --rebuild
```

Formats:

- `--profile flamegraph` -> `<out>.svg`
- `--profile pprof` -> `<out>.pb` (use `go tool pprof -top` for callstack time dumps)
- `--profile folded` -> `<out>.folded` (collapsed stacks)
- `--profile all` -> all of the above

`--profile` with no value defaults to `flamegraph`.

Live snapshots while running:

- `--profile-interval <secs>` emits periodic snapshots.
- `--profile-signal` emits a snapshot on `SIGUSR2` (Unix only).
- `--profile-live-format <fmt>` controls snapshot format (default: `pprof`).

Example (periodic snapshots every 10s):

```bash
./target/release/axiograph \
  --profile all \
  --profile-interval 10 \
  --profile-out ../build/profiles/pathdb_build \
  db accept pathdb-build --dir ../build/accepted_plane --snapshot head --out ../build/head.axpd --rebuild
```

Example (signal-triggered):

```bash
./target/release/axiograph \
  --profile all \
  --profile-signal \
  --profile-out ../build/profiles/pathdb_build \
  db accept pathdb-build --dir ../build/accepted_plane --snapshot head --out ../build/head.axpd --rebuild
```

Then from another terminal:

```bash
kill -USR2 <pid>
```

## Flamegraphs (optional, external tools)

For deeper detail than phase timings, you can also use an external sampling profiler.

### Linux

`cargo-flamegraph` (requires `perf`):

```bash
cd rust
cargo flamegraph -p axiograph-cli --bin axiograph --release -- \
  db accept pathdb-build --dir ../build/accepted_plane --snapshot head --out ../build/head.axpd --rebuild
```

### macOS

`cargo-flamegraph` may require additional privileges (sampling restrictions vary by macOS version).
If it doesn't work for you, use Instruments (Time Profiler) to attach to the `axiograph` process.
