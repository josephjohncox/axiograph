#!/usr/bin/env bash
# Read-only error example: the CLI transport succeeds; the JSON validation must fail.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bin="${AXIOGRAPH_BIN:-$repo_root/rust/target/debug/axiograph}"
workspace="$(mktemp -d)"
trap 'rm -rf "$workspace"' EXIT
fixture="$repo_root/examples/software_authoring/source_diagnostics"
cp "$fixture/Base.axi.source" "$workspace/Base.axi"
cp "$fixture/Root.axi.source" "$workspace/Root.axi"
cp "$fixture/request.json" "$workspace/request.json"
"$bin" authoring workspace --workspace "$workspace" --request "$workspace/request.json"
