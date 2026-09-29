import assert from "node:assert/strict";
import test from "node:test";
import {
  assertJsonBudget,
  parseBoundedJson,
  readBoundedJsonResponse,
} from "../src/json-boundary.ts";

const budget = (overrides = {}) => ({
  maxBytes: 32,
  maxDepth: 3,
  maxValues: 4,
  maxContainerEntries: 2,
  maxStringBytes: 8,
  maxTotalStringBytes: 16,
  ...overrides,
});

test("production JSON byte boundary accepts N and rejects N+1", () => {
  const exact = `${" ".repeat(30)}{}`;
  assert.deepEqual(parseBoundedJson(exact, budget(), "fixture"), {});
  assert.throws(() => parseBoundedJson(`${exact} `, budget(), "fixture"), /32 bytes/);
});

test("production lexical and object depth boundaries accept N and reject N+1", () => {
  const depthBudget = budget({ maxBytes: 64, maxValues: 8, maxContainerEntries: 4 });
  assert.deepEqual(parseBoundedJson('[[["{}"]]]', depthBudget, "fixture"), [[['{}']]]);
  assert.throws(() => parseBoundedJson("[[[[0]]]]", depthBudget, "fixture"), /depth exceeds 3/);
  assertJsonBudget([[[0]]], depthBudget, "fixture");
  assert.throws(() => assertJsonBudget([[[[0]]]], depthBudget, "fixture"), /depth exceeds 3/);
});

test("production value, fanout and string boundaries accept N and reject N+1", () => {
  assertJsonBudget([0, 0, 0], budget({ maxContainerEntries: 3 }), "fixture");
  assert.throws(() => assertJsonBudget([0, 0, 0, 0], budget({ maxContainerEntries: 4 }), "fixture"), /value count exceeds 4/);
  assertJsonBudget({ a: 0, b: 0 }, budget(), "fixture");
  assert.throws(() => assertJsonBudget({ a: 0, b: 0, c: 0 }, budget({ maxValues: 8 }), "fixture"), /fanout exceeds 2/);
  assertJsonBudget("12345678", budget(), "fixture");
  assert.throws(() => assertJsonBudget("123456789", budget(), "fixture"), /string exceeds 8 bytes/);
});

test("production fanout rejection stops before traversing attacker-sized containers", () => {
  let arrayReads = 0;
  const hugeSparse = [];
  hugeSparse.length = 0xffffffff;
  Object.defineProperty(hugeSparse, 0, {
    enumerable: true,
    get() { arrayReads += 1; return 0; },
  });
  assert.throws(() => assertJsonBudget(hugeSparse, budget(), "fixture"), /fanout exceeds 2/);
  assert.equal(arrayReads, 0);

  let objectReads = 0;
  const wideObject = {};
  for (let index = 0; index < 1000; index += 1) {
    Object.defineProperty(wideObject, `key${index}`, {
      enumerable: true,
      get() { objectReads += 1; return 0; },
    });
  }
  assert.throws(() => assertJsonBudget(wideObject, budget({ maxValues: 8 }), "fixture"), /fanout exceeds 2/);
  assert.equal(objectReads, 2);
});

test("streamed response uses the same exact byte budget", async () => {
  const exact = `${" ".repeat(30)}{}`;
  const headers = { "content-type": "application/json" };
  assert.deepEqual(await readBoundedJsonResponse(new Response(exact, { headers }), budget(), "fixture"), {});
  await assert.rejects(readBoundedJsonResponse(new Response(`${exact} `, { headers }), budget(), "fixture"), /oversized response/);
});
