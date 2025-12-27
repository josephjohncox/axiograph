#!/usr/bin/env python3
"""
axiograph_llm_plugin_mock.py

This is a tiny reference implementation of the Axiograph REPL plugin protocol.

Supported:
- `axiograph_llm_plugin_v2` (query translation + answer)
- `axiograph_llm_plugin_v3` (tool-loop / agent step)

It is **not** a real LLM: it implements a few deterministic templates so the REPL
can demonstrate the full "LLM proposes → Axiograph executes" flow without any
model downloads.

Usage (from repo root):
  axiograph> llm use command python3 scripts/axiograph_llm_plugin_mock.py
  axiograph> llm ask find Node named b

Protocol:
  stdin:  JSON request
  stdout: JSON response
"""

from __future__ import annotations

import json
import sys
from typing import Any, Dict, List


def main() -> int:
    req = json.load(sys.stdin)
    protocol = req.get("protocol")
    if protocol not in ("axiograph_llm_plugin_v2", "axiograph_llm_plugin_v3"):
        return respond_error("unsupported protocol")

    task = req.get("task") or {}
    kind = task.get("kind")
    if kind == "to_query":
        question = (task.get("question") or "").strip()
        return respond_to_query(question)
    if kind == "answer":
        return respond_answer(task)
    if kind == "augment_proposals":
        return respond_augment_proposals(task)
    if kind == "tool_loop_step":
        return respond_tool_loop_step(task)

    return respond_error(f"unknown task kind: {kind!r}")


def respond_to_query(question: str) -> int:
    tokens = question.split()
    lower = [t.lower() for t in tokens]

    # Very small set of templates; enough to show the REPL flow.
    # Prefer path-following.
    if "follow" in lower or (lower[:1] == ["from"] and len(tokens) >= 2):
        start = "0"
        if lower[:1] == ["from"] and len(tokens) >= 2 and tokens[1].isdigit():
            start = tokens[1]
        # Default: follow rel_0/rel_1 if nothing else.
        rels: List[str] = [t for t in tokens if t.startswith("rel_")]
        path = "/".join(rels) if rels else "rel_0/rel_1"
        axql = f"select ?y where {start} -{path}-> ?y limit 20"
        query_ir_v1 = {
            "version": 1,
            "select": ["?y"],
            "where": [{"kind": "edge", "left": int(start), "path": path, "right": "?y"}],
            "limit": 20,
        }
        return respond_ok({"query_ir_v1": query_ir_v1, "axql": axql})

    if lower[:1] == ["find"] and len(tokens) >= 2:
        # `find Node named b`
        type_name = tokens[1]
        name = None
        if "named" in lower:
            i = lower.index("named")
            if i + 1 < len(tokens):
                name = tokens[i + 1]
        atoms: List[str] = []
        ir_atoms: List[Dict[str, Any]] = []
        if type_name.lower() not in ("thing", "things", "entity", "entities"):
            atoms.append(f"?x is {type_name}")
            ir_atoms.append({"kind": "type", "term": "?x", "type": type_name})
        if name is not None:
            atoms.append(f"?x.name = \"{name}\"")
            ir_atoms.append({"kind": "attr_eq", "term": "?x", "key": "name", "value": name})
        where = ", ".join(atoms) if atoms else "?x is Node"
        axql = f"select ?x where {where} limit 20"
        if not ir_atoms:
            ir_atoms = [{"kind": "type", "term": "?x", "type": "Node"}]
        query_ir_v1 = {"version": 1, "select": ["?x"], "where": ir_atoms, "limit": 20}
        return respond_ok({"query_ir_v1": query_ir_v1, "axql": axql})

    # Fallback: a harmless AxQL query.
    query_ir_v1 = {
        "version": 1,
        "select": ["?x"],
        "where": [{"kind": "type", "term": "?x", "type": "Node"}],
        "limit": 20,
    }
    return respond_ok({"query_ir_v1": query_ir_v1, "axql": "select ?x where ?x is Node limit 20"})


def respond_answer(task: Dict[str, Any]) -> int:
    results = task.get("results") or {}
    rows = results.get("rows") or []
    msg = f"Found {len(rows)} result rows."
    if rows:
        # Try to show the first binding.
        first: Dict[str, Any] = rows[0]
        parts = []
        for var, v in first.items():
            if isinstance(v, dict) and "id" in v:
                name = v.get("name")
                et = v.get("entity_type")
                if name:
                    parts.append(f"{var}={name} ({et}, id={v['id']})")
                else:
                    parts.append(f"{var}={et} (id={v['id']})")
        if parts:
            msg += " First row: " + ", ".join(parts)
    return respond_ok({"answer": msg})


def respond_tool_loop_step(task: Dict[str, Any]) -> int:
    """
    Deterministic tool-loop behavior:

    - Step 0: return a tool call to `axql_run` using the same templates as `to_query`.
    - Step 1: if we see an `axql_run` result, return a grounded `final_answer`.

    This exists to exercise the REPL's tool-loop wiring without requiring a real model.
    """
    question = (task.get("question") or "").strip()
    transcript = task.get("transcript") or []

    if not transcript:
        q_payload = build_query_payload(question)
        tool_call = {
            "name": "axql_run",
            "args": {
                "query_ir_v1": q_payload["query_ir_v1"],
                "limit": 25,
            },
        }
        return respond_ok({"tool_call": tool_call})

    last = transcript[-1] if transcript else {}
    if isinstance(last, dict) and last.get("tool") == "axql_run":
        result = last.get("result") or {}
        if isinstance(result, dict):
            query_text = result.get("query") or ""
            results = result.get("results") or {}
            rows = results.get("rows") or []
            msg = f"Found {len(rows)} result rows."
            if rows and isinstance(rows[0], dict):
                # Mirror the `answer` task: show the first row's bindings.
                first = rows[0]
                parts = []
                for var, v in first.items():
                    if isinstance(v, dict) and "id" in v:
                        name = v.get("name")
                        et = v.get("entity_type")
                        if name:
                            parts.append(f"{var}={name} ({et}, id={v['id']})")
                        else:
                            parts.append(f"{var}={et} (id={v['id']})")
                if parts:
                    msg += " First row: " + ", ".join(parts)

            final_answer = {
                "answer": msg,
                "citations": [],
                "queries": [query_text] if query_text else [],
                "notes": ["backend=mock_plugin (deterministic)"],
            }
            return respond_ok({"final_answer": final_answer})

    return respond_ok(
        {
            "final_answer": {
                "answer": "Done.",
                "citations": [],
                "queries": [],
                "notes": ["backend=mock_plugin (deterministic)"],
            }
        }
    )


def build_query_payload(question: str) -> Dict[str, Any]:
    """
    Like `respond_to_query`, but returns the payload as a dict instead of printing.
    """
    tokens = question.split()
    lower = [t.lower() for t in tokens]

    if "follow" in lower or (lower[:1] == ["from"] and len(tokens) >= 2):
        start = "0"
        if lower[:1] == ["from"] and len(tokens) >= 2 and tokens[1].isdigit():
            start = tokens[1]
        rels: List[str] = [t for t in tokens if t.startswith("rel_")]
        path = "/".join(rels) if rels else "rel_0/rel_1"
        axql = f"select ?y where {start} -{path}-> ?y limit 20"
        query_ir_v1 = {
            "version": 1,
            "select": ["?y"],
            "where": [{"kind": "edge", "left": int(start), "path": path, "right": "?y"}],
            "limit": 20,
        }
        return {"query_ir_v1": query_ir_v1, "axql": axql}

    if lower[:1] == ["find"] and len(tokens) >= 2:
        type_name = tokens[1]
        name = None
        if "named" in lower:
            i = lower.index("named")
            if i + 1 < len(tokens):
                name = tokens[i + 1]
        atoms: List[str] = []
        ir_atoms: List[Dict[str, Any]] = []
        if type_name.lower() not in ("thing", "things", "entity", "entities"):
            atoms.append(f"?x is {type_name}")
            ir_atoms.append({"kind": "type", "term": "?x", "type": type_name})
        if name is not None:
            atoms.append(f"?x.name = \"{name}\"")
            ir_atoms.append({"kind": "attr_eq", "term": "?x", "key": "name", "value": name})
        where = ", ".join(atoms) if atoms else "?x is Node"
        axql = f"select ?x where {where} limit 20"
        if not ir_atoms:
            ir_atoms = [{"kind": "type", "term": "?x", "type": "Node"}]
        query_ir_v1 = {"version": 1, "select": ["?x"], "where": ir_atoms, "limit": 20}
        return {"query_ir_v1": query_ir_v1, "axql": axql}

    query_ir_v1 = {
        "version": 1,
        "select": ["?x"],
        "where": [{"kind": "type", "term": "?x", "type": "Node"}],
        "limit": 20,
    }
    return {"query_ir_v1": query_ir_v1, "axql": "select ?x where ?x is Node limit 20"}


def respond_augment_proposals(task: Dict[str, Any]) -> int:
    """
    Deterministic “augmentation” for demos:

    - Set schema_hint to `machinist_learning` when we see a domain=machining hint.
    - Add a single Concept entity to demonstrate the shape of `added_proposals`.
    """
    proposals_file = task.get("proposals") or {}
    proposals = proposals_file.get("proposals") or []

    schema_hint_updates: List[Dict[str, Any]] = []
    for p in proposals:
        if not isinstance(p, dict):
            continue
        pid = p.get("proposal_id") or ""
        meta_domain = None
        meta = p  # flattened meta
        md = meta.get("metadata") or {}
        if isinstance(md, dict):
            meta_domain = md.get("domain")
        attrs = p.get("attributes") or {}
        attr_domain = attrs.get("domain") if isinstance(attrs, dict) else None
        domain = (meta_domain or attr_domain or "").strip().lower()
        if domain == "machining" and pid:
            schema_hint_updates.append(
                {
                    "proposal_id": pid,
                    "schema_hint": "machinist_learning",
                    "public_rationale": "mock plugin: domain=machining → machinist_learning",
                }
            )

    added_proposals: List[Dict[str, Any]] = [
        {
            "kind": "Entity",
            "proposal_id": "concept::mock::0",
            "confidence": 0.6,
            "evidence": [],
            "public_rationale": "mock plugin: added a sample Concept entity",
            "metadata": {"derived_from": "axiograph_llm_plugin_mock"},
            "schema_hint": None,
            "entity_id": "concept::mock::0",
            "entity_type": "Concept",
            "name": "MockConcept",
            "attributes": {"note": "deterministic mock"},
            "description": "A placeholder concept added by the mock plugin.",
        }
    ]

    return respond_ok(
        {
            "schema_hint_updates": schema_hint_updates,
            "added_proposals": added_proposals,
            "notes": [
                "mock plugin: this is deterministic; replace with a real local model runner for semantic labeling",
            ],
        }
    )


def respond_ok(payload: Dict[str, Any]) -> int:
    sys.stdout.write(json.dumps(payload))
    sys.stdout.write("\n")
    return 0


def respond_error(message: str) -> int:
    sys.stdout.write(json.dumps({"error": message}))
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
