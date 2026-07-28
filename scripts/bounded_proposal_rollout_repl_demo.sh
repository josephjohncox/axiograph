#!/bin/bash
set -euo pipefail

# Bounded proposal rollout demo (REPL script).
#
# Run:
#   ./scripts/bounded_proposal_rollout_repl_demo.sh

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
OUT_DIR="$ROOT_DIR/build/bounded_proposal_rollout_repl_demo"
mkdir -p "$OUT_DIR"

echo "== Bounded proposal rollout REPL demo =="
echo "root: $ROOT_DIR"
echo "out:  $OUT_DIR"

echo ""
echo "-- Build (via Makefile)"
cd "$ROOT_DIR"
make binaries

if [ -z "${PREDICTIVE_PROPOSAL_BACKEND:-}" ]; then
	export PREDICTIVE_PROPOSAL_BACKEND="baseline"
fi
if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "baseline" ]; then
	export PREDICTIVE_PROPOSAL_MODEL="${PREDICTIVE_PROPOSAL_MODEL:-baseline_oracle}"
elif [ "$PREDICTIVE_PROPOSAL_BACKEND" = "openai" ] && [ -z "${OPENAI_API_KEY:-}" ]; then
	echo "error: OPENAI_API_KEY is required for PREDICTIVE_PROPOSAL_BACKEND=openai"
	exit 2
fi
if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "anthropic" ] && [ -z "${ANTHROPIC_API_KEY:-}" ]; then
	echo "error: ANTHROPIC_API_KEY is required for PREDICTIVE_PROPOSAL_BACKEND=anthropic"
	exit 2
fi
if [ "$PREDICTIVE_PROPOSAL_BACKEND" = "ollama" ] && [ -z "${OLLAMA_HOST:-}" ] && [ -z "${OLLAMA_MODEL:-}" ]; then
	echo "error: OLLAMA_HOST or OLLAMA_MODEL is required for PREDICTIVE_PROPOSAL_BACKEND=ollama"
	exit 2
fi
ADAPTER_MODEL="${PREDICTIVE_PROPOSAL_MODEL:-${OPENAI_MODEL:-${ANTHROPIC_MODEL:-${OLLAMA_MODEL:-}}}}"
if [ -z "$ADAPTER_MODEL" ]; then
	echo "error: PREDICTIVE_PROPOSAL_MODEL (or OPENAI_MODEL / ANTHROPIC_MODEL / OLLAMA_MODEL) is required"
	exit 2
fi
export PREDICTIVE_PROPOSAL_MODEL="$ADAPTER_MODEL"

AXIOGRAPH="$ROOT_DIR/bin/axiograph"
if [ ! -x "$AXIOGRAPH" ]; then
	echo "error: expected executable at $ROOT_DIR/bin/axiograph"
	exit 2
fi

echo ""
echo "-- Run REPL script"
"$AXIOGRAPH" repl --quiet --script examples/repl_scripts/bounded_proposal_rollout_demo.repl

if [ -f "$ROOT_DIR/build/bounded_proposal_rollout_demo_plan.json" ]; then
	cp "$ROOT_DIR/build/bounded_proposal_rollout_demo_plan.json" "$OUT_DIR/"
fi

echo ""
echo "Done."
echo "Outputs:"
if [ -f "$OUT_DIR/bounded_proposal_rollout_demo_plan.json" ]; then
	echo "  $OUT_DIR/bounded_proposal_rollout_demo_plan.json"
fi
