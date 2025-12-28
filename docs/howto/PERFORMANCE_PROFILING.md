# Performance profiling

This repo includes a few built-in ways to profile “where the time went”, without introducing new Rust dependencies.

## Profile PathDB WAL checkout vs rebuild

`axiograph db accept pathdb-build` has two important modes:

- **Checkpoint fast path (default):** if a snapshot checkpoint exists, it “checks out” the `.axpd` by hardlink/copy.
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

The easiest way to get both “checkout” and “rebuild” timings:

```bash
./scripts/profile_pathdb_wal_build.sh
```

This will:

- generate a large demo snapshot store if needed (via `scripts/graph_explorer_deep_knowledge_demo.sh`),
- run `pathdb-build` once using the checkpoint fast path,
- run `pathdb-build --rebuild` to profile the slow path,
- write `timings_*.json` into `build/profile_pathdb_wal_build/`.

## Flamegraphs (optional)

For deeper detail than phase timings, use a sampling profiler.

### Linux

`cargo-flamegraph` (requires `perf`):

```bash
cd rust
cargo flamegraph -p axiograph-cli --bin axiograph --release -- \
  db accept pathdb-build --dir ../build/accepted_plane --snapshot head --out ../build/head.axpd --rebuild
```

### macOS

`cargo-flamegraph` may require additional privileges (sampling restrictions vary by macOS version).
If it doesn’t work for you, use Instruments (Time Profiler) to attach to the `axiograph` process.

