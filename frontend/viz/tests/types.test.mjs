import assert from "node:assert/strict";
import test from "node:test";
import { isDraftOverlay, isGraphPayload } from "../src/types.ts";

test("graph boundary keeps numeric node and edge identities", () => {
  assert.equal(
    isGraphPayload({
      nodes: [{ id: 7, entity_type: "Context", name: "<literal>" }],
      edges: [{ source: 7, target: 7, label: "self" }],
      summary: { focus_ids: [7] },
    }),
    true,
  );
  for (const value of [
    { nodes: [{ id: "7" }], edges: [] },
    { nodes: [{ id: -1 }], edges: [] },
    { nodes: [{ id: 1 }], edges: [{ source: "1", target: 1 }] },
    { nodes: [{ id: 1 }], edges: [{ source: 1, target: 1.5 }] },
    { nodes: [{ id: 1, name: 7 }], edges: [] },
    { nodes: [{ id: 1, attrs: [] }], edges: [] },
    { nodes: [{ id: 1 }], edges: [], summary: { focus_ids: ["1"] } },
    { nodes: [{ id: 1 }], edges: [], contexts: [{ id: "1" }] },
    { nodes: [{ id: 1 }], edges: [], tuple_contexts: { 1: ["2"] } },
    { nodes: {}, edges: [] },
  ]) {
    assert.equal(isGraphPayload(value), false);
  }
});

test("draft state boundary enforces every loaded-state selection invariant", () => {
  assert.equal(
    isDraftOverlay({
      proposals_json: {
        proposals: [
          {
            proposal_id: "p1",
            evidence: [{ chunk_id: "c1", locator: "page 1" }],
          },
        ],
      },
      chunks: [{ chunk_id: "c1", text: "evidence" }],
    }),
    true,
  );
  assert.equal(
    isDraftOverlay({ proposals_json: { proposals: [] } }),
    true,
  );
  for (const value of [
    { proposals_json: { proposals: "p1" } },
    { proposals_json: { proposals: [null] } },
    { proposals_json: { proposals: [{ evidence: [] }] } },
    { proposals_json: { proposals: [{ proposal_id: "", evidence: [] }] } },
    { proposals_json: { proposals: [{ proposal_id: "  ", evidence: [] }] } },
    {
      proposals_json: {
        proposals: [
          { proposal_id: "p1", evidence: [] },
          { proposal_id: "p1", evidence: [] },
        ],
      },
    },
    { proposals_json: { proposals: [{ proposal_id: "p1" }] } },
    { proposals_json: { proposals: [{ proposal_id: "p1", evidence: null }] } },
    {
      proposals_json: {
        proposals: [{ proposal_id: "p1", evidence: [null] }],
      },
    },
    {
      proposals_json: {
        proposals: [{ proposal_id: "p1", evidence: [{}] }],
      },
    },
    {
      proposals_json: {
        proposals: [{ proposal_id: "p1", evidence: [{ chunk_id: "" }] }],
      },
    },
    {
      proposals_json: {
        proposals: [{ proposal_id: "p1", evidence: [{ chunk_id: "  " }] }],
      },
    },
    {
      proposals_json: {
        proposals: [{ proposal_id: "p1", evidence: [], confidence: "high" }],
      },
    },
    { proposals_json: { proposals: [] }, chunks: null },
    { proposals_json: { proposals: [] }, chunks: [null] },
    { proposals_json: { proposals: [] }, chunks: [{}] },
    { proposals_json: { proposals: [] }, chunks: [{ chunk_id: "" }] },
    {
      proposals_json: { proposals: [] },
      validation: { ok: "yes" },
    },
    null,
  ]) {
    assert.equal(isDraftOverlay(value), false);
  }
});
