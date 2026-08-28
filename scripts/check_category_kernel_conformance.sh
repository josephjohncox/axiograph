#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cargo build --quiet --manifest-path "$ROOT/rust/Cargo.toml" \
	-p axiograph-kernel --example emit_category_kernel_certificate --locked
(
	cd "$ROOT/lean"
	lake build axiograph_category_kernel_formation axiograph_verify
)

ROOT="$ROOT" node <<'NODE'
const cp = require('child_process');
const fs = require('fs');
const path = require('path');

const root = process.env.ROOT;
const corpus = JSON.parse(fs.readFileSync(
  path.join(root, 'fixtures/canonical/category_kernel/corpus.json'),
  'utf8'
));
const outDir = path.join(root, 'build/category-kernel-conformance');
fs.mkdirSync(outDir, { recursive: true });
const emitter = path.join(
  root,
  'rust/target/debug/examples/emit_category_kernel_certificate'
);
const formation = path.join(
  root,
  'lean/.lake/build/bin/axiograph_category_kernel_formation'
);
const verifier = path.join(root, 'lean/.lake/build/bin/axiograph_verify');

function run(binary, args, options = {}) {
  return cp.spawnSync(binary, args, {
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    ...options,
  });
}

const failures = [];
for (const testCase of corpus.cases) {
  const axi = path.join(root, testCase.path);
  const emitted = run(emitter, [axi, testCase.schema]);
  const formed = run(formation, [axi, testCase.schema]);
  const rustAccepted = emitted.status === 0;
  const leanAccepted = formed.status === 0;
  let verified = false;
  let retiredProtocolRejected = true;

  if (rustAccepted) {
    const stem = path.basename(testCase.path, '.axi');
    const certificate = path.join(outDir, `${stem}.json`);
    fs.writeFileSync(certificate, emitted.stdout);
    const checked = run(verifier, [axi, certificate]);
    verified = checked.status === 0 && checked.stdout.includes('ok: category_kernel_v3');
    if (!verified) {
      failures.push({
        path: testCase.path,
        stage: 'certificate verification',
        stdout: checked.stdout,
        stderr: checked.stderr,
      });
    }

    const legacyEnvelope = JSON.parse(emitted.stdout);
    legacyEnvelope.kind = 'finite_theory_v2';
    legacyEnvelope.proof = {
      schema_name: legacyEnvelope.proof.schema_name,
      certificate: legacyEnvelope.proof.certificate,
    };
    const legacyCertificate = path.join(outDir, `${stem}-retired-finite-theory-v2.json`);
    fs.writeFileSync(legacyCertificate, JSON.stringify(legacyEnvelope));
    const legacyCheck = run(verifier, [axi, legacyCertificate]);
    retiredProtocolRejected = legacyCheck.status !== 0;
    if (!retiredProtocolRejected) {
      failures.push({
        path: testCase.path,
        stage: 'retired finite_theory_v2 rejection',
        stdout: legacyCheck.stdout,
        stderr: legacyCheck.stderr,
      });
    }
  }

  const passed =
    rustAccepted === testCase.compile &&
    leanAccepted === testCase.compile &&
    retiredProtocolRejected &&
    (!testCase.compile || verified);
  console.log(
    `${passed ? 'PASS' : 'FAIL'} ${testCase.path} ` +
    `rust=${rustAccepted} lean=${leanAccepted} verified=${verified} ` +
    `retired=${retiredProtocolRejected}`
  );
  if (!passed) {
    failures.push({
      path: testCase.path,
      expected: testCase.compile,
      rustAccepted,
      leanAccepted,
      rustError: emitted.stderr.trim(),
      leanError: formed.stderr.trim(),
    });
  }
}

if (failures.length !== 0) {
  console.error(JSON.stringify(failures, null, 2));
  process.exit(1);
}
console.log(`Category-kernel conformance: ${corpus.cases.length} cases, 0 failures`);
NODE
