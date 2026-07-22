#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo test --manifest-path "$ROOT/rust/Cargo.toml" -p axiograph-kernel
cargo build --manifest-path "$ROOT/rust/Cargo.toml" \
	-p axiograph-dsl --bin axiograph_parse_axi_v1 \
	-p axiograph-pathdb --bin axiograph_typecheck_axi
(
	cd "$ROOT/lean"
	lake build axiograph_axi_v1_parse axiograph_axi_v1_typecheck
)

ROOT="$ROOT" node <<'NODE'
const fs = require('fs');
const path = require('path');
const cp = require('child_process');
const root = process.env.ROOT;
const corpus = JSON.parse(
  fs.readFileSync(path.join(root, 'fixtures/canonical/w02/corpus.json'), 'utf8')
);
const bins = {
  rustParse: path.join(root, 'rust/target/debug/axiograph_parse_axi_v1'),
  leanParse: path.join(root, 'lean/.lake/build/bin/axiograph_axi_v1_parse'),
  rustTypecheck: path.join(root, 'rust/target/debug/axiograph_typecheck_axi'),
  leanTypecheck: path.join(root, 'lean/.lake/build/bin/axiograph_axi_v1_typecheck'),
};
function accepts(binary, relativePath) {
  try {
    cp.execFileSync(binary, [path.join(root, relativePath)], {
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    return true;
  } catch (_) {
    return false;
  }
}
const failures = [];
for (const testCase of corpus.cases) {
  const actual = {
    rustParse: accepts(bins.rustParse, testCase.path),
    leanParse: accepts(bins.leanParse, testCase.path),
    rustTypecheck: accepts(bins.rustTypecheck, testCase.path),
    leanTypecheck: accepts(bins.leanTypecheck, testCase.path),
  };
  const passed =
    actual.rustParse === testCase.parse &&
    actual.leanParse === testCase.parse &&
    actual.rustTypecheck === testCase.typecheck &&
    actual.leanTypecheck === testCase.typecheck;
  console.log(
    `${passed ? 'PASS' : 'FAIL'} ${testCase.path} ` +
      `parse=${actual.rustParse}/${actual.leanParse} ` +
      `typecheck=${actual.rustTypecheck}/${actual.leanTypecheck}`
  );
  if (!passed) failures.push({ testCase, actual });
}
if (failures.length !== 0) {
  console.error(JSON.stringify(failures, null, 2));
  process.exit(1);
}
console.log(`W02 conformance: ${corpus.cases.length} cases, 0 failures`);
NODE
