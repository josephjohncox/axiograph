#!/usr/bin/env bash
set -euo pipefail

# Snapshot evolution + visualization demo (offline).
#
# This script shows how to:
# - promote multiple versions of a canonical `.axi` module into the accepted plane
# - build `.axpd` snapshots for historical accepted snapshot ids (time travel)
# - render one HTML viz per snapshot + a simple index.html
#
# Run from repo root:
#   ./scripts/snapshot_evolution_viz_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="$ROOT_DIR/build/snapshot_evolution_viz_demo"
PLANE_DIR="$OUT_DIR/accepted_plane"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "== Snapshot evolution + viz demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
make binaries

AXIOGRAPH="$ROOT_DIR/bin/axiograph-cli"
if [ ! -x "$AXIOGRAPH" ]; then
  AXIOGRAPH="$ROOT_DIR/bin/axiograph"
fi
if [ ! -x "$AXIOGRAPH" ]; then
  echo "error: expected executable at $ROOT_DIR/bin/axiograph-cli or $ROOT_DIR/bin/axiograph"
  exit 2
fi

echo ""
echo "-- A) Create 3 ticks of a module (OntologyRewrites)"
TICK0="$OUT_DIR/OntologyRewrites_tick0.axi"
TICK1="$OUT_DIR/OntologyRewrites_tick1.axi"
TICK2="$OUT_DIR/OntologyRewrites_tick2.axi"

cp "$ROOT_DIR/examples/ontology/OntologyRewrites.axi" "$TICK0"

python3 - "$TICK0" "$TICK1" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, "r", encoding="utf-8").read()
text = text.replace("Person = {Alice, Bob, Carol, Eve}", "Person = {Alice, Bob, Carol, Eve, Zoe}")
text = text.replace("(parent=Bob, child=Carol)\n  }", "(parent=Bob, child=Carol),\n    (parent=Carol, child=Zoe)\n  }")
open(dst, "w", encoding="utf-8").write(text)
PY

python3 - "$TICK1" "$TICK2" <<'PY'
import sys
src, dst = sys.argv[1], sys.argv[2]
text = open(src, "r", encoding="utf-8").read()
text = text.replace("(employee=Eve, manager=Bob)", "(employee=Eve, manager=Bob),\n    (employee=Zoe, manager=Eve)")
open(dst, "w", encoding="utf-8").write(text)
PY

echo ""
echo "-- B) Promote ticks into accepted plane (append-only snapshots)"
"$AXIOGRAPH" db accept promote "$TICK0" --dir "$PLANE_DIR" --message "demo: tick0 (seed OntologyRewrites)"
"$AXIOGRAPH" db accept promote "$TICK1" --dir "$PLANE_DIR" --message "demo: tick1 (add Zoe + Parent edge)"
"$AXIOGRAPH" db accept promote "$TICK2" --dir "$PLANE_DIR" --message "demo: tick2 (add ReportsTo edge)"

echo ""
echo "-- C) Enumerate accepted snapshots (most recent first)"
"$AXIOGRAPH" db accept list --dir "$PLANE_DIR" --layer accepted --full --limit 20 >"$OUT_DIR/snapshots.txt"
cat "$OUT_DIR/snapshots.txt"

echo ""
echo "-- D) Build .axpd + render HTML viz for each snapshot"
python3 - "$AXIOGRAPH" "$PLANE_DIR" "$OUT_DIR" <<'PY'
import json
import os
import subprocess
import sys

axiograph = sys.argv[1]
plane_dir = sys.argv[2]
out_dir = sys.argv[3]

def show_snapshot(snapshot: str) -> dict:
    raw = subprocess.check_output(
        [
            axiograph,
            "db",
            "accept",
            "show",
            "--dir",
            plane_dir,
            "--layer",
            "accepted",
            "--snapshot",
            snapshot,
            "--json",
            "--full",
        ],
        text=True,
    )
    return json.loads(raw)


manifests = []
manifest = show_snapshot("head")
while True:
    manifests.append(manifest)
    prev = manifest.get("previous_snapshot_id")
    if not prev:
        break
    manifest = show_snapshot(prev)

rows = []
for idx, manifest in enumerate(manifests, start=1):
    snap = manifest["snapshot_id"]
    snap_short = snap.replace(":", "_")
    axpd = os.path.join(out_dir, f"accepted_{idx:02d}_{snap_short}.axpd")
    viz = os.path.join(out_dir, f"viz_{idx:02d}_{snap_short}.html")

    subprocess.check_call(
        [axiograph, "db", "accept", "build-pathdb", "--dir", plane_dir, "--snapshot", snap, "--out", axpd]
    )
    subprocess.check_call(
        [
            axiograph,
            "tools",
            "viz",
            axpd,
            "--out",
            viz,
            "--format",
            "html",
            "--plane",
            "both",
            "--focus-name",
            "Alice",
            "--typed-overlay",
            "--hops",
            "3",
            "--max-nodes",
            "420",
        ]
    )

    module_names = sorted(manifest.get("modules", {}).keys())
    rows.append((snap, module_names, os.path.basename(viz)))

index = os.path.join(out_dir, "index.html")
with open(index, "w", encoding="utf-8") as f:
    f.write("<!doctype html>\\n<html><head><meta charset='utf-8'/>\\n")
    f.write("<title>Axiograph snapshot evolution demo</title>\\n")
    f.write("<style>body{font-family:system-ui, -apple-system, sans-serif; padding: 18px;} code{background:#f4f4f4; padding:2px 4px;} li{margin:10px 0;}</style>\\n")
    f.write("</head><body>\\n")
    f.write("<h1>Snapshot evolution demo</h1>\\n")
    f.write("<p>This folder contains one viz per accepted snapshot. Newest first.</p>\\n")
    f.write("<ol>\\n")
    for snap, module_names, viz_file in rows:
        modules_label = ", ".join(module_names) if module_names else "(no modules)"
        f.write(
            f"<li><div><a href='{viz_file}'>{viz_file}</a></div><div><code>{snap}</code></div><div>{modules_label}</div></li>\\n"
        )
    f.write("</ol>\\n")
    f.write("</body></html>\\n")

print(f"wrote: {index}")
PY

echo ""
echo "Done."
echo "Outputs:"
echo "  $OUT_DIR/snapshots.txt"
echo "  $OUT_DIR/index.html"
