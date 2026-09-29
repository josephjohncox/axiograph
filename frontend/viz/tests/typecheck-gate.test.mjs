import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  cpSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));

function productionTypeScriptFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return productionTypeScriptFiles(path);
    return entry.isFile() && entry.name.endsWith(".ts") ? [path] : [];
  });
}

function runScript(cwd, script) {
  const result = spawnSync("npm", ["run", script], {
    cwd,
    encoding: "utf8",
    timeout: 60_000,
    maxBuffer: 1024 * 1024,
  });
  assert.ifError(result.error);
  assert.equal(result.signal, null);
  return { status: result.status, output: result.stdout + result.stderr };
}

test("whole production frontend has one strict unsuppressed TypeScript boundary", () => {
  const config = JSON.parse(readFileSync(join(root, "tsconfig.json"), "utf8"));
  assert.equal(config.compilerOptions.strict, true);
  assert.deepEqual(config.include, ["src"]);

  const files = productionTypeScriptFiles(join(root, "src"));
  assert.ok(files.length > 0);
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    assert.doesNotMatch(source, /@ts-(?:nocheck|ignore|expect-error)/, file);
    assert.doesNotMatch(
      source,
      /(?:\:\s*any\b|<any>|\bas\s+any\b)/,
      `${file} contains an explicit any boundary`,
    );
    assert.doesNotMatch(
      source,
      /new\s+(?:Map|Set)\s*\(\s*\)/,
      `${file} contains an implicit-any collection boundary`,
    );
  }
});

test("actual typecheck and both build entrypoints reject invalid TypeScript before bundling", () => {
  // Run the real package scripts/config/source in isolation; never inject into
  // the working tree and never duplicate the gate command in the test.
  const temp = mkdtempSync(join(tmpdir(), "axiograph-viz-typecheck-"));
  try {
    for (const file of [
      "package.json",
      "tsconfig.json",
      "tsconfig.context.json",
      "vite.config.ts",
      "index.html",
      "src",
    ]) {
      cpSync(join(root, file), join(temp, file), { recursive: true });
    }
    symlinkSync(join(root, "node_modules"), join(temp, "node_modules"), "dir");
    const positive = runScript(temp, "typecheck");
    assert.equal(positive.status, 0, positive.output);

    mkdirSync(join(temp, "dist"));
    const sentinel = join(temp, "dist", "not-published.txt");
    writeFileSync(sentinel, "unchanged");
    writeFileSync(
      join(temp, "src", "invalid-context-gate.ts"),
      `
import { contextFilterOptions } from "./core/context";
import { selectDraft } from "./core/draft-selection";
import { initStatus } from "./core/status";
import { element } from "./render/dom";
import type { GraphEdge, GraphNode, VizUiState } from "./types";
const selection = selectDraft(null, new Set<string>());
if (selection) selection.chunks.push({ chunk_id: "mutation" });
initStatus({}).setReviewStatus({ innerHTML: "not a DOM node" });
element("div", {}, { innerHTML: "not text" });
const invalidNumber: number = "not a number";
const invalidNode: GraphNode = { id: "not numeric" };
const invalidEdge: GraphEdge = { source: "not numeric", target: 1 };
const invalidUi: VizUiState = { pathStart: "not numeric" };
contextFilterOptions({
  factContexts: new Map([[1, new Set(["2"])]]),
  contextNameById: new Map<number, string>(),
});
`,
    );
    for (const script of ["typecheck", "build", "build:debug"]) {
      const result = runScript(temp, script);
      assert.notEqual(
        result.status,
        0,
        `${script} accepted invalid TypeScript`,
      );
      assert.match(result.output, /invalid-context-gate\.ts/);
      assert.match(result.output, /TS2322/);
      assert.match(result.output, /Set<string>/);
      assert.ok(
        (result.output.match(/Type 'string' is not assignable to type 'number'/g) ?? [])
          .length >= 4,
        `${script} did not reject graph node, edge, UI and context string IDs`,
      );
      assert.match(result.output, /Property 'push' does not exist/);
      assert.match(result.output, /innerHTML/);
      assert.equal(
        readFileSync(sentinel, "utf8"),
        "unchanged",
        `${script} touched the bundle before typechecking`,
      );
    }
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});
