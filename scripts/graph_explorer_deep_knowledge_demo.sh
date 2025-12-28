#!/usr/bin/env bash
set -euo pipefail

# Graph Explorer “deep knowledge” demo:
# - accepted plane: dozens of canonical `.axi` modules (meaning plane)
# - evidence plane: a large-scale, context-scoped observation log in the WAL
#   (100k+ MeasurementObs facts by default) + grounded DocChunks
# - multiple contexts/worlds (ObservedSensors vs Simulation vs Literature)
# - “type-theory-ish” artifacts from the corpus (HoTT kinship, modal supply chain)
# - optional reasoning help via the LLM tool-loop panel
#
# Run from repo root:
#   ./scripts/graph_explorer_deep_knowledge_demo.sh
#
# Scaling knobs (defaults are “big”; override for quick runs):
#   DATA_POINTS=100000 RUNS=200 TIMES=500 BINS=1024 SEED=1 \
#     ./scripts/graph_explorer_deep_knowledge_demo.sh
#
# LLM options:
#   LLM_BACKEND=mock ./scripts/graph_explorer_deep_knowledge_demo.sh
#   LLM_BACKEND=ollama LLM_MODEL=nemotron-3-nano KEEP_RUNNING=1 ./scripts/graph_explorer_deep_knowledge_demo.sh
#   LLM_BACKEND=openai LLM_MODEL=gpt-5.2 OPENAI_API_KEY=... KEEP_RUNNING=1 ./scripts/graph_explorer_deep_knowledge_demo.sh
#   LLM_BACKEND=anthropic LLM_MODEL=claude-3-7-sonnet-20250219 ANTHROPIC_API_KEY=... KEEP_RUNNING=1 ./scripts/graph_explorer_deep_knowledge_demo.sh
#
# Notes:
# - Deterministic token-hash retrieval is always available (built into PathDB).
# - Optionally, you can also commit snapshot-scoped *model embeddings* into the WAL
#   for hybrid retrieval (`semantic_search`), via `axiograph db accept pathdb-embed`.
#   Configure with:
#     - `EMBED_ENABLED=0` to disable
#     - `EMBED_BACKEND=ollama|openai|anthropic` (defaults: ollama when `LLM_BACKEND=ollama`, openai when `LLM_BACKEND=openai`)
#     - `EMBED_MODEL=...` (defaults: `nomic-embed-text` for ollama, `text-embedding-3-small` for openai)

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/graph_explorer_deep_knowledge_demo"
PLANE_DIR="$OUT_DIR/plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

DATA_POINTS="${DATA_POINTS:-100000}"
RUNS="${RUNS:-200}"
TIMES="${TIMES:-500}"
BINS="${BINS:-1024}"
SEED="${SEED:-1}"

EXTRA_AXI_MODULES="${EXTRA_AXI_MODULES:-24}"

echo "== Graph Explorer deep knowledge demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"
echo "scale: data_points=$DATA_POINTS runs=$RUNS times=$TIMES bins=$BINS seed=$SEED extra_axi_modules=$EXTRA_AXI_MODULES"

echo ""
echo "-- Build (via Makefile)"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

# LLM defaults (can be overridden by env vars).
LLM_BACKEND="${LLM_BACKEND:-ollama}"
LLM_MODEL="${LLM_MODEL:-nemotron-3-nano}"
SKIP_SERVER="${SKIP_SERVER:-0}"

echo ""
echo "-- A) Promote canonical .axi modules into the accepted plane (meaning plane)"

PROMOTE_LIST=(
  "$ROOT_DIR/examples/physics/PhysicsOntology.axi"
  "$ROOT_DIR/examples/physics/PhysicsMeasurements.axi"
  "$ROOT_DIR/examples/family/FamilyHoTT.axi"
  "$ROOT_DIR/examples/manufacturing/SupplyChainHoTT.axi"
  "$ROOT_DIR/examples/manufacturing/SupplyChainModalitiesHoTT.axi"
  "$ROOT_DIR/examples/economics/EconomicFlows.axi"
  "$ROOT_DIR/examples/learning/MachinistLearning.axi"
  "$ROOT_DIR/examples/machining/MachiningKnowledge.axi"
  "$ROOT_DIR/examples/machining/PhysicsKnowledge.axi"
  "$ROOT_DIR/examples/modal/Modalities.axi"
  "$ROOT_DIR/examples/ontology/OntologyRewrites.axi"
  "$ROOT_DIR/examples/ontology/SchemaEvolution.axi"
  "$ROOT_DIR/examples/social/SocialNetwork.axi"
  "$ROOT_DIR/examples/demo_data/machining.axi"
  "$ROOT_DIR/examples/Family.axi"
)

ACCEPTED_SNAPSHOT=""
PROMOTED_COUNT=0
for f in "${PROMOTE_LIST[@]}"; do
  if [ ! -f "$f" ]; then
    echo "warn: missing .axi module: $f"
    continue
  fi
  msg="demo: accept $(basename "$f")"
  ACCEPTED_SNAPSHOT="$("$AXIOGRAPH" db accept promote "$f" --dir "$PLANE_DIR" --message "$msg")"
  PROMOTED_COUNT=$((PROMOTED_COUNT + 1))
done

EXTRA_DIR="$OUT_DIR/extra_axi"
mkdir -p "$EXTRA_DIR"

python3 - "$EXTRA_DIR" "$EXTRA_AXI_MODULES" <<'PY'
import json, sys
from pathlib import Path

out_dir = Path(sys.argv[1])
count = int(sys.argv[2])

def write_module(i: int) -> None:
    mod = f"DeepExtra_{i}"
    schema = f"DeepExtraSchema_{i}"
    inst = f"DeepExtraInstance_{i}"

    # Keep these tiny but well-formed. The point is to demonstrate that the
    # accepted plane can hold many small domain modules without breaking the
    # exploration UX (meta-plane navigation, query elaboration, etc.).
    text = f"""\
-- Auto-generated demo module (deep knowledge corpus filler)
-- This is intentionally small; the *large* data lives in the PathDB WAL overlays.

module {mod}

schema {schema}:
  object Entity
  object Text
  object Context

  relation label(entity: Entity, text: Text)
  relation mentions(from: Entity, to: Entity) @context Context

theory {schema}Rules on {schema}:
  constraint key label(entity, text)
  constraint key mentions(from, to, ctx)

instance {inst} of {schema}:
  Context = {{DocContext_{i}}}
  Entity = {{Node_{i}_0, Node_{i}_1, Node_{i}_2}}
  Text = {{Text_{i}_0, Text_{i}_1}}

  label = {{
    (entity=Node_{i}_0, text=Text_{i}_0),
    (entity=Node_{i}_1, text=Text_{i}_1)
  }}

  mentions = {{
    (from=Node_{i}_0, to=Node_{i}_1, ctx=DocContext_{i}),
    (from=Node_{i}_1, to=Node_{i}_2, ctx=DocContext_{i})
  }}
"""
    (out_dir / f"{mod}.axi").write_text(text, encoding="utf-8")

for i in range(count):
    write_module(i)
PY

for f in "$EXTRA_DIR"/*.axi; do
  [ -f "$f" ] || continue
  msg="demo: accept extra $(basename "$f")"
  ACCEPTED_SNAPSHOT="$("$AXIOGRAPH" db accept promote "$f" --dir "$PLANE_DIR" --message "$msg")"
  PROMOTED_COUNT=$((PROMOTED_COUNT + 1))
done

if [ -z "$ACCEPTED_SNAPSHOT" ]; then
  echo "error: no accepted snapshot created"
  exit 2
fi

echo "accepted snapshot (head): $ACCEPTED_SNAPSHOT"
echo "accepted modules promoted: $PROMOTED_COUNT"

echo ""
echo "-- B) Evidence grounding: DocChunks (always-on citations / RAG pack input)"
CHUNKS="$OUT_DIR/chunks.json"
cat >"$CHUNKS" <<'EOF'
[
  {
    "chunk_id": "chunk_physics_forms_0",
    "document_id": "physics_notes.md",
    "page": null,
    "span_id": "forms_0",
    "text": "Differential forms: d: Ω^k(M) → Ω^{k+1}(M) and the wedge product Ω^k(M)×Ω^l(M)→Ω^{k+l}(M). In symplectic geometry, a symplectic form ω is a closed, non-degenerate 2-form; Hamiltonian mechanics lives on a symplectic manifold (PhaseSpace, ω).",
    "bbox": null,
    "metadata": {"kind":"textbook","context":"Literature","topic":"diff_forms_symplectic"}
  },
  {
    "chunk_id": "chunk_physics_gr_0",
    "document_id": "physics_notes.md",
    "page": null,
    "span_id": "gr_0",
    "text": "General relativity models spacetime as a Lorentzian manifold with a metric g and Levi-Civita connection ∇. Curvature (Riemann tensor) is derived from ∇. Einstein's equation ties curvature to stress-energy.",
    "bbox": null,
    "metadata": {"kind":"textbook","context":"Literature","topic":"general_relativity"}
  },
  {
    "chunk_id": "chunk_tacit_lab_0",
    "document_id": "lab_notebook.md",
    "page": null,
    "span_id": "tacit_0",
    "text": "Tacit note: if acceleration spikes and the temperature channel simultaneously drifts, suspect a loose sensor mount before changing feed/pressure. Re-seat the mount, re-run a short calibration sweep, and compare against a simulated baseline.",
    "bbox": null,
    "metadata": {"kind":"tacit","context":"TacitNotes","topic":"diagnostics"}
  },
  {
    "chunk_id": "chunk_measurements_log_0",
    "document_id": "observations.csv",
    "page": null,
    "span_id": "log_0",
    "text": "ObservedSensors: a large synthetic observation log. Each row records (run, quantity, time, unit, value). Values are stored as WAL attributes; value_bin is a coarse discretization for typed querying. This chunk is the provenance anchor for the bulk measurement overlay.",
    "bbox": null,
    "metadata": {"kind":"dataset","context":"ObservedSensors","topic":"measurement_log"}
  }
]
EOF

echo ""
echo "-- C) Generate large-scale WAL overlays (proposals)"

PROPOSALS_MEASUREMENTS="$OUT_DIR/proposals_measurements.json"
python3 - "$PROPOSALS_MEASUREMENTS" "$DATA_POINTS" "$RUNS" "$TIMES" "$BINS" "$SEED" <<'PY'
import json
import math
import random
import sys
from datetime import datetime

out_path = sys.argv[1]
data_points = int(sys.argv[2])
runs = int(sys.argv[3])
times = int(sys.argv[4])
bins = int(sys.argv[5])
seed = int(sys.argv[6])

random.seed(seed)

schema = "PhysicsMeasurements"

contexts = [
    ("ObservedSensors", 0.98),
    ("Simulation", 0.85),
    ("Literature", 0.70),
]

quantities = [
    ("PositionX", "Unit_Meter"),
    ("VelocityX", "Unit_Dimensionless"),
    ("AccelerationX", "Unit_Dimensionless"),
    ("Temperature", "Unit_Kelvin"),
    ("CurvatureScalar", "Unit_Dimensionless"),
]

def entity(kind, entity_id, entity_type, name, desc=None, attrs=None, evidence=None):
    if attrs is None:
        attrs = {}
    if evidence is None:
        evidence = [{"chunk_id": "chunk_measurements_log_0", "locator": "observations.csv", "span_id": "log_0"}]
    return {
        "kind": "Entity",
        "proposal_id": entity_id,
        "confidence": 0.95,
        "evidence": evidence,
        "public_rationale": f"Seed entity for {kind} in the measurement overlay.",
        "metadata": {"demo": "deep_knowledge"},
        "entity_id": entity_id,
        "entity_type": entity_type,
        "name": name,
        "attributes": attrs,
        "description": desc,
    }

def relation(rel_id, rel_type, source, target, confidence, attrs, evidence=None, rationale=""):
    if evidence is None:
        evidence = [{"chunk_id": "chunk_measurements_log_0", "locator": "observations.csv", "span_id": "log_0"}]
    return {
      "kind": "Relation",
        "proposal_id": rel_id,
        "confidence": confidence,
        "evidence": evidence,
        "public_rationale": rationale or "Measurement record extracted from the observation log.",
        "metadata": {"demo": "deep_knowledge", "schema": schema},
        "relation_id": rel_id,
        "rel_type": rel_type,
        "source": source,
        "target": target,
        "attributes": attrs,
    }

with open(out_path, "w", encoding="utf-8") as f:
    f.write('{"version":1,')
    f.write(f'"generated_at":{json.dumps(datetime.utcnow().isoformat())},')
    f.write('"source":{"source_type":"demo","locator":"graph_explorer_deep_knowledge_demo"},')
    f.write(f'"schema_hint":{json.dumps(schema)},')
    f.write('"proposals":[')

    first = [True]
    def emit(obj):
        if not first[0]:
            f.write(",\n")
        first[0] = False
        f.write(json.dumps(obj, ensure_ascii=False))

    # Runs + times + bins (bounded cardinalities; keep these reusable)
    for i in range(runs):
        rid = f"Run_{i}"
        emit(entity("run", rid, "Run", rid, attrs={"axi_schema": schema}))

    for t in range(times):
        tid = f"T{t:04d}"
        emit(entity("time", tid, "Time", tid, attrs={"axi_schema": schema}))

    for b in range(bins):
        bid = f"Bin_{b:04d}"
        # Keep bin metadata tiny; a real pipeline might include bounds as attrs.
        emit(entity("scalar_bin", bid, "ScalarBin", bid, attrs={"axi_schema": schema, "bin_index": str(b)}))

    # Note: contexts/quantities/units are intentionally *not* re-proposed here.
    # They already exist in the accepted plane (see `PhysicsMeasurementsSeed`),
    # and we want the evidence-plane overlay to *link to* those canonical nodes
    # by name+type (so exploration doesn’t produce duplicate “PositionX” etc).

    # Measurement observations (fact nodes + derived traversal edges).
    #
    # Each observation:
    # - is context-scoped (ctx)
    # - is time-scoped (time)
    # - carries a coarse typed value (value_bin)
    # - carries the raw float value as an *attribute* (value_f64) for UI display
    for i in range(data_points):
        run_id = f"Run_{i % runs}"
        time_id = f"T{(i % times):04d}"
        q_name, unit = quantities[i % len(quantities)]

        # Context mix: mostly ObservedSensors, some Simulation/Literature.
        if i % 20 == 0:
            ctx, base_conf = contexts[1]
        elif i % 97 == 0:
            ctx, base_conf = contexts[2]
        else:
            ctx, base_conf = contexts[0]

        # Deterministic-ish value: combine a smooth term + a tiny pseudo-random perturbation.
        smooth = math.sin(i / 17.0) * 0.5 + math.cos(i / 29.0) * 0.5
        noise = (random.random() - 0.5) * 0.02
        value = smooth + noise

        # Bin in [0, bins)
        bin_index = int((value + 1.0) * 0.5 * (bins - 1))
        if bin_index < 0:
            bin_index = 0
        if bin_index >= bins:
            bin_index = bins - 1
        bin_id = f"Bin_{bin_index:04d}"

        obs_id = f"demo:MeasurementObs:{i:07d}"
        attrs = {
            "ctx": ctx,
            "time": time_id,
            "unit": unit,
            "value_bin": bin_id,
            "value_f64": f"{value:.6f}",
        }
        emit(
            relation(
                obs_id,
                "MeasurementObs",
                run_id,
                q_name,
                base_conf,
                attrs,
                rationale=f"Row {i} in observations.csv (ctx={ctx}, time={time_id}, quantity={q_name}).",
            )
        )

    f.write("]}")
PY

PROPOSALS_TACIT="$OUT_DIR/proposals_tacit.json"
python3 - "$PROPOSALS_TACIT" <<'PY'
import json
import sys
from datetime import datetime

schema = "MachiningLearning"

chunks = [
    ("chunk_tacit_lab_0", "lab_notebook.md", "tacit_0"),
]

def ev_ptr(chunk_id, locator, span_id):
    return {"chunk_id": chunk_id, "locator": locator, "span_id": span_id}

proposals = []

for i in range(10):
    tacit_id = f"demo:tacit:{i}"
    text_id = f"demo:text:tacit:{i}"
    conf_name = "Conf_0_88" if i % 2 == 0 else "Conf_0_92"
    chunk_id, locator, span_id = chunks[0]

    proposals.append({
        "kind": "Entity",
        "proposal_id": tacit_id,
        "confidence": 0.80,
        "evidence": [ev_ptr(chunk_id, locator, span_id)],
        "public_rationale": "Tacit diagnostic heuristic extracted from lab notebook text.",
        "metadata": {"demo": "deep_knowledge"},
        "entity_id": tacit_id,
        "entity_type": "TacitKnowledge",
        "name": f"Tacit_{i}",
        "attributes": {"axi_schema": schema},
        "description": None,
    })

    proposals.append({
        "kind": "Entity",
        "proposal_id": text_id,
        "confidence": 0.90,
        "evidence": [ev_ptr(chunk_id, locator, span_id)],
        "public_rationale": "Supporting text pointer (identifier-only surface).",
        "metadata": {"demo": "deep_knowledge"},
        "entity_id": text_id,
        "entity_type": "Text",
        "name": f"Text_tacit_rule_{i}",
        "attributes": {"axi_schema": schema},
        "description": None,
    })

    proposals.append({
        "kind": "Relation",
        "proposal_id": f"demo:rel:tacitRule:{i}",
        "confidence": 0.75,
        "evidence": [ev_ptr(chunk_id, locator, span_id)],
        "public_rationale": "Lab notebook heuristic (tacit rule).",
        "metadata": {"demo": "deep_knowledge", "schema": schema},
        "relation_id": f"demo:rel:tacitRule:{i}",
        "rel_type": "tacitRule",
        "source": tacit_id,
        "target": text_id,
        "attributes": {},
    })

    proposals.append({
        "kind": "Relation",
        "proposal_id": f"demo:rel:tacitConfidence:{i}",
        "confidence": 0.70,
        "evidence": [ev_ptr(chunk_id, locator, span_id)],
        "public_rationale": "Confidence label for the heuristic (not calibrated truth-probability).",
        "metadata": {"demo": "deep_knowledge", "schema": schema},
        "relation_id": f"demo:rel:tacitConfidence:{i}",
        "rel_type": "tacitConfidence",
        "source": tacit_id,
        "target": conf_name,
        "attributes": {},
    })

out = {
    "version": 1,
    "generated_at": datetime.utcnow().isoformat(),
    "source": {"source_type": "demo", "locator": "graph_explorer_deep_knowledge_demo"},
    "schema_hint": schema,
    "proposals": proposals,
}
open(sys.argv[1], "w", encoding="utf-8").write(json.dumps(out, ensure_ascii=False, indent=2))
PY

echo ""
echo "-- D) Commit overlays into the PathDB WAL"
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$ACCEPTED_SNAPSHOT" --chunks "$CHUNKS" --message "demo: deep chunks overlay" >/dev/null

# Measurements are huge; keep them as a single WAL op by default.
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$ACCEPTED_SNAPSHOT" --proposals "$PROPOSALS_MEASUREMENTS" --message "demo: deep measurement overlay ($DATA_POINTS obs)" >"$OUT_DIR/commit_measurements.log"

# A second WAL op adds some grounded tacit/learning heuristics (small).
"$AXIOGRAPH" db accept pathdb-commit --dir "$PLANE_DIR" --accepted-snapshot "$ACCEPTED_SNAPSHOT" --proposals "$PROPOSALS_TACIT" --message "demo: deep tacit heuristics overlay" >/dev/null

EMBED_ENABLED="${EMBED_ENABLED:-1}"
EMBED_BACKEND="${EMBED_BACKEND:-}"
if [ -z "$EMBED_BACKEND" ]; then
  if [ "$LLM_BACKEND" = "ollama" ] || [ "$LLM_BACKEND" = "openai" ] || [ "$LLM_BACKEND" = "anthropic" ]; then
    EMBED_BACKEND="$LLM_BACKEND"
  fi
fi

EMBED_MODEL="${EMBED_MODEL:-}"
if [ -z "$EMBED_MODEL" ]; then
  if [ "$EMBED_BACKEND" = "ollama" ]; then
    EMBED_MODEL="${EMBED_OLLAMA_MODEL:-nomic-embed-text}"
  elif [ "$EMBED_BACKEND" = "openai" ]; then
    EMBED_MODEL="${EMBED_OPENAI_MODEL:-text-embedding-3-small}"
  fi
fi

if [ "$EMBED_ENABLED" = "1" ] && [ -n "${EMBED_BACKEND:-}" ] && [ -n "${EMBED_MODEL:-}" ]; then
  echo ""
  echo "-- D2) Compute snapshot-scoped DocChunk embeddings (PathDB WAL) (backend=$EMBED_BACKEND model=$EMBED_MODEL)"
  if [ "$EMBED_BACKEND" = "ollama" ]; then
    echo "note: make sure the embedding model is available: ollama pull $EMBED_MODEL"
  fi
  set +e
  "$AXIOGRAPH" db accept pathdb-embed \
      --dir "$PLANE_DIR" \
      --snapshot head \
      --target docchunks \
      --embed-backend "$EMBED_BACKEND" \
      --embed-model "$EMBED_MODEL" \
      --message "demo: deep snapshot-scoped embeddings (docchunks)" \
      >"$OUT_DIR/embed_snapshot_id.txt" 2>"$OUT_DIR/embed.log"
  if [ $? -ne 0 ]; then
    echo "warn: embedding step failed; continuing without stored embeddings (see $OUT_DIR/embed.log)"
  else
    EMBED_SNAPSHOT_ID="$(cat "$OUT_DIR/embed_snapshot_id.txt" 2>/dev/null || true)"
    if [ -n "$EMBED_SNAPSHOT_ID" ]; then
      echo "ok: embeddings committed (pathdb snapshot=$EMBED_SNAPSHOT_ID)"
    fi
  fi
  set -e
fi

echo ""
echo "-- E) Start axiograph db serve (store-backed, pathdb layer)"
if [ "$SKIP_SERVER" = "1" ]; then
  echo "SKIP_SERVER=1: skipping server start."
  echo "Plane dir: $PLANE_DIR"
  echo "Tip: start the server manually:"
  echo "  $AXIOGRAPH db serve --dir \"$PLANE_DIR\" --layer pathdb --snapshot head --role master --admin-token demo-admin-token --listen 127.0.0.1:8089 --llm-mock"
  exit 0
fi
READY="$OUT_DIR/ready.json"
LLM_HTTP_TIMEOUT_SECS="${LLM_HTTP_TIMEOUT_SECS:-240}"
ADMIN_TOKEN="${ADMIN_TOKEN:-demo-admin-token}"

LLM_FLAGS=()
if [ "$LLM_BACKEND" = "ollama" ]; then
  echo "note: requires: ollama serve  (and: ollama pull $LLM_MODEL)"
  LLM_FLAGS+=(--llm-ollama --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "openai" ]; then
  LLM_FLAGS+=(--llm-openai --llm-model "$LLM_MODEL")
elif [ "$LLM_BACKEND" = "anthropic" ]; then
  LLM_FLAGS+=(--llm-anthropic --llm-model "$LLM_MODEL")
else
  LLM_FLAGS+=(--llm-mock)
fi

"$AXIOGRAPH" db serve \
  --dir "$PLANE_DIR" \
  --layer pathdb \
  --snapshot head \
  --role master \
  --admin-token "$ADMIN_TOKEN" \
  --listen 127.0.0.1:0 \
  --ready-file "$READY" \
  "${LLM_FLAGS[@]}" \
  >"$OUT_DIR/server.log" 2>&1 &
SERVER_PID=$!
trap 'kill "$SERVER_PID" 2>/dev/null || true' EXIT

sleep 0.2
if ! kill -0 "$SERVER_PID" 2>/dev/null; then
  echo "error: server exited early; see $OUT_DIR/server.log"
  tail -n 80 "$OUT_DIR/server.log" || true
  exit 2
fi

python3 - "$READY" <<'PY' >"$OUT_DIR/addr.txt"
import json, time, sys
path = sys.argv[1]
deadline = time.time() + 60
while time.time() < deadline:
  try:
    with open(path) as f:
      j = json.load(f)
    if "addr" in j:
      print(j["addr"])
      sys.exit(0)
  except Exception:
    time.sleep(0.05)
print("error: server did not write ready file", file=sys.stderr)
sys.exit(2)
PY

ADDR="$(cat "$OUT_DIR/addr.txt")"

echo "server: http://$ADDR"
echo "admin token (paste into the Add tab to commit overlays): $ADMIN_TOKEN"
echo ""
echo "Open this in a browser (large graph; start with smaller max_nodes):"
echo "  http://$ADDR/viz?focus_name=Run_0&plane=both&typed_overlay=true&hops=2&max_nodes=500"
echo "  http://$ADDR/viz?focus_name=ObservedSensors&plane=both&typed_overlay=true&hops=2&max_nodes=500"
echo "  http://$ADDR/viz?focus_name=Alice&plane=accepted&typed_overlay=true&hops=3&max_nodes=650"
echo ""
echo "Try in the explorer:"
echo "  - Toggle planes: accepted/evidence"
echo "  - Filter by context: ObservedSensors vs Simulation vs Literature"
echo "  - Use snapshot dropdown (WAL commits) to time travel: before/after tacit overlay"
echo "  - In the Query tab (AxQL):"
echo "      * select ?q where name(\"Run_0\") -MeasurementObs-> ?q in ObservedSensors limit 20"
echo "      * select ?m ?t ?bin where ?m = MeasurementObs(run=Run_0, quantity=Temperature, time=?t, value_bin=?bin) in ObservedSensors limit 20"
echo "      * select ?tacit ?text where ?tacit is TacitKnowledge, ?tacit -tacitRule-> ?text limit 20"
echo "      * select ?eq ?a ?b where ?eq is PathEquivalence, ?eq -from-> ?a, ?eq -to-> ?b limit 10"
echo "  - Ask (LLM panel):"
echo "      * what contexts/worlds exist in this snapshot?"
echo "      * what quantities are measured in Run_0 in ObservedSensors?"
echo "      * show some Temperature observations for Run_0 (include time and bin)"
echo "      * explain the tacit heuristics and cite evidence chunks"
echo "      * explain the HoTT kinship path equivalences in FamilyHoTT"
echo ""

# Optional: call the LLM agent once (useful for CI scripts; skip for KEEP_RUNNING=1).
RUN_SAMPLES="${RUN_SAMPLES:-}"
if [ -z "$RUN_SAMPLES" ]; then
  if [ "${KEEP_RUNNING:-0}" = "1" ]; then
    RUN_SAMPLES=0
  else
    RUN_SAMPLES=1
  fi
fi
if [ "$RUN_SAMPLES" = "1" ]; then
  set +e
  python3 - "$ADDR" "$LLM_HTTP_TIMEOUT_SECS" <<'PY' >"$OUT_DIR/llm_agent_response.json"
import json, sys, urllib.request
addr = sys.argv[1]
timeout = int(sys.argv[2])
payload = {"question": "what contexts exist?", "max_steps": 8, "max_rows": 25}
data = json.dumps(payload).encode("utf-8")
req = urllib.request.Request(
  f"http://{addr}/llm/agent",
  data=data,
  headers={"Content-Type": "application/json"},
  method="POST",
)
resp = urllib.request.urlopen(req, timeout=timeout)
print(resp.read().decode("utf-8"))
PY
  if [ $? -ne 0 ]; then
    echo "warn: /llm/agent sample failed (see $OUT_DIR/server.log)"
  fi
  set -e
fi

echo ""
if [ "${KEEP_RUNNING:-0}" = "1" ]; then
  echo "Keeping the server running (KEEP_RUNNING=1). Press Ctrl-C to stop."
  wait "$SERVER_PID"
else
  echo "Note: this script stops the server when it exits."
  echo "Tip: keep it running by setting KEEP_RUNNING=1."
fi
