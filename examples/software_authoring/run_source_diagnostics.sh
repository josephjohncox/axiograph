#!/usr/bin/env bash
# Read-only error example: the CLI transport succeeds; the JSON validation must fail.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bin="${AXIOGRAPH_BIN:-$repo_root/rust/target/debug/axiograph}"
workspace="$(mktemp -d)"
trap 'rm -rf "$workspace"' EXIT
cat > "$workspace/Company.axi" <<'AXI'
module CompanyExample
# Compny in this comment is not the failing occurrence.
schema Hiring:
  object Company
  relation Employment(company: Compny)
AXI
cat > "$workspace/request.json" <<'JSON'
{"version":"authoring_workspace_request_v1","operation":"validate","axi_path":"Company.axi","presentation":{"detail":"full"}}
JSON
"$bin" authoring workspace --workspace "$workspace" --request "$workspace/request.json"
