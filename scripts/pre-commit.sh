#!/usr/bin/env bash
# Git pre-commit hook for Axiograph.
#
# This hook intentionally runs the same Rust/Lean-facing checks documented for
# the canonical semantic spine. It does not install or run unrelated language
# checks.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

echo "Running Axiograph pre-commit checks..."

echo "  - Rust formatting"
cargo fmt --manifest-path rust/Cargo.toml --check

echo "  - Rust crate check"
cargo check --manifest-path rust/Cargo.toml \
  -p axiograph-cli \
  -p axiograph-pathdb \
  -p axiograph-tooling-overlays \
  -p axiograph-software-authoring

if command -v lake >/dev/null 2>&1 && [ -f lean/lakefile.lean ]; then
  echo "  - Lean trusted-kernel build"
  (
    cd lean
    lake build Axiograph.VerifyMain
  )
else
  echo "  - Lean build skipped (lake not found or lean/lakefile.lean missing)"
fi

echo "  - AGENTS.md harness checks"
agents_lines="$(wc -l < AGENTS.md | tr -d ' ')"
if [ "$agents_lines" -gt 150 ]; then
  echo "AGENTS.md has $agents_lines lines; keep it below 150 or document why"
  exit 1
fi
if rg "\\- \\[[ x~]\\]" AGENTS.md >/dev/null; then
  echo "AGENTS.md contains checkbox backlog markers; move them to docs/roadmaps"
  exit 1
fi

echo "  - whitespace"
git diff --check

echo "Axiograph pre-commit checks passed."
