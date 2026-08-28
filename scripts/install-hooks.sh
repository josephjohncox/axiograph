#!/usr/bin/env bash
# Install local Git hooks for Axiograph.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOKS_DIR="$PROJECT_ROOT/.git/hooks"

if [ ! -d "$PROJECT_ROOT/.git" ]; then
  echo "error: $PROJECT_ROOT is not a Git worktree"
  exit 1
fi

mkdir -p "$HOOKS_DIR"

if [ -e "$HOOKS_DIR/pre-commit" ] && [ ! -L "$HOOKS_DIR/pre-commit" ]; then
  backup="$HOOKS_DIR/pre-commit.bak.$(date +%Y%m%d%H%M%S)"
  echo "Existing pre-commit hook found; backing up to $backup"
  mv "$HOOKS_DIR/pre-commit" "$backup"
fi

chmod +x "$SCRIPT_DIR/pre-commit.sh"
ln -sf "$SCRIPT_DIR/pre-commit.sh" "$HOOKS_DIR/pre-commit"

echo "Installed Axiograph pre-commit hook."
echo "It runs Rust formatting/checks, optional Lean trusted-kernel build, AGENTS.md hygiene, and git diff whitespace checks."
