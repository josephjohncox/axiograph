import assert from "node:assert/strict";
import test from "node:test";
import {
  contextFilterOptions,
  initContextFilter,
  selectedContextFilter,
  currentContextNameFromFilter,
  updateContextBadge,
} from "../src/core/context.ts";

function data(entries = [], names = []) {
  return { factContexts: new Map(entries), contextNameById: new Map(names) };
}

// DOM write spies, not a browser emulator: fail if rendering ever uses HTML.
function textSink() {
  return {
    textContent: "",
    set innerHTML(_) {
      assert.fail("context labels must not enter an HTML sink");
    },
  };
}

function view(model = data()) {
  const contextFilterEl = {
    set innerHTML(_) {
      assert.fail("context select must use DOM construction");
    },
    value: "",
    disabled: false,
    children: [],
    ownerDocument: {
      createElement(tag) {
        assert.equal(tag, "option");
        return textSink();
      },
    },
    replaceChildren() {
      this.children = [];
      this.value = "";
    },
    appendChild(option) {
      if (this.children.length === 0) this.value = option.value;
      this.children.push(option);
    },
  };
  const contextBadgeEl = Object.assign(textSink(), {
    style: { display: "none" },
  });
  return { ...model, contextFilterEl, contextBadgeEl };
}

test("contextFilterOptions sorts numeric IDs and deduplicates across facts", () => {
  const model = data(
    [
      [1, new Set([10, 2, 0])],
      [2, new Set([2, 100, 10])],
    ],
    [[2, "two"]],
  );
  assert.deepEqual(contextFilterOptions(model), [
    { value: "*", text: "(all)" },
    { value: "__none__", text: "(no context)" },
    { value: "0", text: "Context#0" },
    { value: "2", text: "two" },
    { value: "10", text: "Context#10" },
    { value: "100", text: "Context#100" },
  ]);
  assert.deepEqual([...model.factContexts.get(1)], [10, 2, 0]);
});

test("empty maps and facts with empty context sets disable selection and clear stale badges", () => {
  for (const model of [data(), data([[1, new Set()]])]) {
    const ctx = view(model);
    ctx.contextBadgeEl.textContent = "stale";
    ctx.contextBadgeEl.style.display = "inline-flex";
    initContextFilter(ctx);
    assert.equal(ctx.contextFilterEl.disabled, true);
    assert.deepEqual(
      ctx.contextFilterEl.children.map((o) => [o.value, o.textContent]),
      [["*", "(no contexts)"]],
    );
    assert.equal(selectedContextFilter(ctx), "*");
    assert.equal(currentContextNameFromFilter(ctx), null);
    assert.equal(ctx.contextBadgeEl.textContent, "");
    assert.equal(ctx.contextBadgeEl.style.display, "none");
  }
});

test("initialization re-enables a previously empty filter and replaces old options", () => {
  const ctx = view();
  initContextFilter(ctx);
  ctx.factContexts = new Map([[1, new Set([2])]]);
  initContextFilter(ctx);
  assert.equal(ctx.contextFilterEl.disabled, false);
  assert.deepEqual(
    ctx.contextFilterEl.children.map((o) => o.value),
    ["*", "__none__", "2"],
  );
  initContextFilter(ctx);
  assert.equal(ctx.contextFilterEl.children.length, 3);
});

test("selection, current name, and badge agree for all/none/known/unknown/malformed values", () => {
  const ctx = view(data([[1, new Set([2])]], [[2, "shipment"]]));
  for (const [value, selection, name, label] of [
    ["", "*", null, ""],
    ["*", "*", null, ""],
    ["__none__", "__none__", null, "context: none"],
    ["2", "2", "shipment", "context: shipment"],
    ["10", "10", null, "context: Context#10"],
    ["NaN", "NaN", null, "context: NaN"],
    ["Infinity", "Infinity", null, "context: Infinity"],
  ]) {
    ctx.contextFilterEl.value = value;
    assert.equal(selectedContextFilter(ctx), selection);
    assert.equal(currentContextNameFromFilter(ctx), name);
    updateContextBadge(ctx);
    assert.equal(ctx.contextBadgeEl.textContent, label);
    assert.equal(
      ctx.contextBadgeEl.style.display,
      label ? "inline-flex" : "none",
    );
  }
  ctx.contextFilterEl.value = "2";
  ctx.contextFilterEl.disabled = true;
  assert.equal(selectedContextFilter(ctx), "*");
  assert.equal(currentContextNameFromFilter(ctx), null);
  updateContextBadge(ctx);
  assert.equal(ctx.contextBadgeEl.textContent, "");
});

test("absent controls are safe and do not leave a misleading badge", () => {
  const ctx = data();
  initContextFilter(ctx);
  updateContextBadge(ctx);
  assert.equal(selectedContextFilter(ctx), "*");
  assert.equal(currentContextNameFromFilter(ctx), null);
  ctx.contextBadgeEl = Object.assign(textSink(), {
    style: {},
    textContent: "stale",
  });
  updateContextBadge(ctx);
  assert.equal(ctx.contextBadgeEl.textContent, "");
  assert.equal(ctx.contextBadgeEl.style.display, "none");
});

test("hostile and unusual context labels remain literal option and badge text", () => {
  for (const label of [
    '<img src=x onerror="globalThis.contextAttack=true">',
    "</option><script>globalThis.contextAttack=true</script>",
    "&quot; <svg onload=alert(1)>",
    "\u0000\nContext 日本語 🧭",
  ]) {
    const ctx = view(data([[1, new Set([2])]], [[2, label]]));
    initContextFilter(ctx);
    assert.equal(ctx.contextFilterEl.children[2].textContent, label);
    ctx.contextFilterEl.value = "2";
    assert.equal(currentContextNameFromFilter(ctx), label);
    updateContextBadge(ctx);
    assert.equal(ctx.contextBadgeEl.textContent, `context: ${label}`);
    // Even a malformed selected value is written as text, never HTML.
    ctx.contextFilterEl.value = label;
    updateContextBadge(ctx);
    assert.equal(ctx.contextBadgeEl.textContent, `context: ${label}`);
  }
});
