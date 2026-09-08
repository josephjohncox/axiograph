import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import {
  cpSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const root = fileURLToPath(new URL("../", import.meta.url));

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
const selection = selectDraft(null, new Set<string>());
if (selection) selection.chunks.push({ chunk_id: "mutation" });
initStatus({}).setReviewStatus({ innerHTML: "not a DOM node" });
element("div", {}, { innerHTML: "not text" });
const invalidNumber: number = "not a number";
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
