import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

// EQ-02-U03: inventory of every dynamic-rendering HTML sink under
// src/render, src/core, src/tabs, and src/dom.ts.
//
// The DOM-spy tests in rendering.test.mjs / context.test.mjs already fail
// closed on any innerHTML/outerHTML/insertAdjacentHTML use inside the
// bundled production modules they exercise (list.ts, detail.ts, status.ts,
// draft.ts, add.ts, llm.ts, draft-selection.ts, labels.ts). This test
// closes the remaining inventory: it enumerates every *.ts file in the
// covered directories and asserts that the only files still touching an
// HTML sink are the three reviewed call sites below, and that each of
// those call sites is one of:
//   - `<el>.innerHTML = ""` — clearing a container, not writing markup.
//   - a fully static, non-interpolated markup literal with no `${`
//     expression, so no production data can ever reach the sink.
// Any other file, or any interpolated/non-literal use, fails this test.
const root = fileURLToPath(new URL("../", import.meta.url));

function tsFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return tsFiles(path);
    return entry.isFile() && entry.name.endsWith(".ts") ? [path] : [];
  });
}

const sinkPattern = /\.(innerHTML|outerHTML)\s*=|insertAdjacentHTML\s*\(/;

// file -> exact reviewed disposition per occurrence, in source order.
const reviewed = new Map([
  [
    join(root, "src/render/graph.ts"),
    [
      { kind: "clear", re: /svg\.innerHTML\s*=\s*""\s*;/ },
      {
        kind: "static",
        re: /defs\.innerHTML\s*=\s*`\s*<marker id="arrow"[\s\S]*?`\s*;/,
      },
    ],
  ],
  [
    join(root, "src/core/run_filter.ts"),
    [{ kind: "clear", re: /runFilterEl\.innerHTML\s*=\s*""\s*;/ }],
  ],
  [
    join(root, "src/core/context_menu.ts"),
    [
      { kind: "clear", re: /menu\.innerHTML\s*=\s*""\s*;/ },
      { kind: "clear", re: /menu\.innerHTML\s*=\s*""\s*;/ },
    ],
  ],
]);

test("every dynamic-rendering HTML sink outside the reviewed inventory is absent", () => {
  const directories = ["src/render", "src/core", "src/tabs"].map((d) =>
    join(root, d),
  );
  const files = [...directories.flatMap(tsFiles), join(root, "src/dom.ts")];
  assert.ok(files.length > 0);

  const unexpected = [];
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    const matches = source.match(new RegExp(sinkPattern, "g")) || [];
    if (matches.length === 0) continue;
    if (!reviewed.has(file)) {
      unexpected.push(`${file}: unreviewed HTML sink use (${matches.join(", ")})`);
      continue;
    }
    if (matches.length !== reviewed.get(file).length) {
      unexpected.push(
        `${file}: expected ${reviewed.get(file).length} reviewed sink use(s), found ${matches.length}`,
      );
    }
  }
  assert.deepEqual(unexpected, []);
});

test("each reviewed HTML sink call site is either a literal clear or non-interpolated static markup", () => {
  for (const [file, occurrences] of reviewed) {
    const source = readFileSync(file, "utf8");
    for (const occurrence of occurrences) {
      const match = source.match(occurrence.re);
      assert.ok(
        match,
        `${file}: expected reviewed ${occurrence.kind} call site not found verbatim`,
      );
      // A "clear" call site must be exactly an empty-string literal
      // assignment (no template-literal interpolation possible).
      if (occurrence.kind === "clear") {
        assert.doesNotMatch(match[0], /\$\{/, `${file}: clear call site is not a bare literal`);
      }
      // A "static" call site must contain no `${...}` interpolation, so no
      // production data flow (node/edge labels, evidence text, locators,
      // etc.) can ever reach this sink.
      if (occurrence.kind === "static") {
        assert.doesNotMatch(
          match[0],
          /\$\{/,
          `${file}: static markup sink contains an interpolation and is no longer provably safe`,
        );
      }
    }
  }
});

test("no *.ts file elsewhere in src references an HTML-parsing sink", () => {
  // Full-tree sweep beyond render/core/tabs/dom.ts, so a new sink added in
  // e.g. src/util or src/types cannot silently bypass this inventory.
  const allFiles = tsFiles(join(root, "src"));
  const reviewedSet = new Set(reviewed.keys());
  const covered = new Set([
    ...tsFiles(join(root, "src/render")),
    ...tsFiles(join(root, "src/core")),
    ...tsFiles(join(root, "src/tabs")),
    join(root, "src/dom.ts"),
  ]);
  const outside = allFiles.filter((f) => !covered.has(f));
  const offenders = [];
  for (const file of outside) {
    const source = readFileSync(file, "utf8");
    if (sinkPattern.test(source)) offenders.push(file);
  }
  assert.deepEqual(offenders, []);
  // Sanity: the reviewed set is a subset of the covered inventory.
  for (const file of reviewedSet) assert.ok(covered.has(file), file);
});
