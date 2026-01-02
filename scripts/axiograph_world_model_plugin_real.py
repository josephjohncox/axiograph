#!/usr/bin/env python3
import json
import os
import sys
import time
import urllib.request
from typing import Any, Dict, Tuple


def _read_stdin_json() -> Dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        raise RuntimeError("expected JSON request on stdin")
    return json.loads(raw)


def _http_post_json(url: str, headers: Dict[str, str], payload: Dict[str, Any]) -> Dict[str, Any]:
    data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, method="POST")
    for k, v in headers.items():
        req.add_header(k, v)
    with urllib.request.urlopen(req, timeout=120) as resp:
        body = resp.read().decode("utf-8")
    return json.loads(body)


def _extract_json(text: str) -> Dict[str, Any]:
    s = text.strip()
    if s.startswith("```"):
        # Strip markdown fences if present.
        s = s.strip("`")
    # Find first JSON object by brace matching.
    start = s.find("{")
    if start == -1:
        raise RuntimeError("model output did not include JSON object")
    depth = 0
    end = None
    for i in range(start, len(s)):
        if s[i] == "{":
            depth += 1
        elif s[i] == "}":
            depth -= 1
            if depth == 0:
                end = i + 1
                break
    if end is None:
        raise RuntimeError("model output JSON object was not balanced")
    return json.loads(s[start:end])


def _summarize_request(req: Dict[str, Any]) -> Tuple[str, Dict[str, Any]]:
    trace_id = req.get("trace_id", "wm::unknown")
    opts = req.get("options", {}) or {}
    input_obj = req.get("input", {}) or {}
    export = input_obj.get("export")
    export_summary = None
    if isinstance(export, dict):
        items = export.get("items", []) or []
        sample = []
        for it in items[:3]:
            sample.append({
                "schema": it.get("schema"),
                "instance": it.get("instance"),
                "relation": it.get("relation"),
                "fields": it.get("fields", [])[:4],
                "mask_fields": it.get("mask_fields", [])[:4],
            })
        export_summary = {
            "module_name": export.get("module_name"),
            "axi_digest_v1": export.get("axi_digest_v1"),
            "items": len(items),
            "sample": sample,
        }
    summary = {
        "trace_id": trace_id,
        "generated_at": req.get("generated_at_unix_secs"),
        "goals": opts.get("goals", []),
        "objectives": opts.get("objectives", []),
        "task_costs": opts.get("task_costs", []),
        "max_new_proposals": opts.get("max_new_proposals", 0),
        "notes": opts.get("notes", []),
        "axi_digest_v1": input_obj.get("axi_digest_v1"),
        "export_summary": export_summary,
    }
    prompt = (
        "You are a world-model assistant for Axiograph.\n"
        "Return ONLY JSON (no markdown) that conforms to:\n"
        "ProposalsFileV1 = {\n"
        '  "version": 1,\n'
        '  "generated_at": "<unix-secs as string>",\n'
        '  "source": {"source_type": "world_model", "locator": "<trace_id>"},\n'
        '  "schema_hint": null,\n'
        '  "proposals": [\n'
        "    ProposalV1 (entity or relation)\n"
        "  ]\n"
        "}\n"
        "ProposalV1 entity:\n"
        '{ "kind":"Entity", "proposal_id":"...", "confidence":0.0-1.0, "evidence":[], "public_rationale":"...", "metadata":{}, "schema_hint":null,\n'
        '  "entity_id":"...", "entity_type":"...", "name":"...", "attributes":{}, "description":null }\n'
        "ProposalV1 relation:\n"
        '{ "kind":"Relation", "proposal_id":"...", "confidence":0.0-1.0, "evidence":[], "public_rationale":"...", "metadata":{}, "schema_hint":null,\n'
        '  "relation_id":"...", "rel_type":"...", "source":"...", "target":"...", "attributes":{} }\n'
        "Rules:\n"
        "- Propose at most max_new_proposals items.\n"
        "- Use stable ids (e.g. wm::<trace_id>::n).\n"
        "- Keep confidence between 0.55 and 0.9.\n"
        "- Use only info grounded in export_summary + goals.\n"
    )
    return prompt, summary


def _normalize_proposals(trace_id: str, data: Dict[str, Any]) -> Dict[str, Any]:
    out = data if isinstance(data, dict) else {}
    out.setdefault("version", 1)
    out.setdefault("generated_at", str(int(time.time())))
    out.setdefault("source", {"source_type": "world_model", "locator": trace_id})
    out.setdefault("schema_hint", None)
    proposals = out.get("proposals", [])
    if not isinstance(proposals, list):
        proposals = []
    fixed = []
    for idx, p in enumerate(proposals):
        if not isinstance(p, dict):
            continue
        kind = p.get("kind")
        kind = "Entity" if str(kind).lower() == "entity" else "Relation"
        base_id = f"wm::{trace_id}::{idx}"
        meta = {
            "proposal_id": p.get("proposal_id") or base_id,
            "confidence": float(p.get("confidence", 0.7)),
            "evidence": p.get("evidence") if isinstance(p.get("evidence"), list) else [],
            "public_rationale": p.get("public_rationale") or "world model proposal",
            "metadata": p.get("metadata") if isinstance(p.get("metadata"), dict) else {},
            "schema_hint": p.get("schema_hint"),
        }
        if kind == "Entity":
            fixed.append({
                **meta,
                "kind": "Entity",
                "entity_id": p.get("entity_id") or f"{base_id}:entity",
                "entity_type": p.get("entity_type") or "Entity",
                "name": p.get("name") or p.get("entity_id") or f"Entity {idx}",
                "attributes": p.get("attributes") if isinstance(p.get("attributes"), dict) else {},
                "description": p.get("description"),
            })
        else:
            fixed.append({
                **meta,
                "kind": "Relation",
                "relation_id": p.get("relation_id") or f"{base_id}:rel",
                "rel_type": p.get("rel_type") or "related_to",
                "source": p.get("source") or "",
                "target": p.get("target") or "",
                "attributes": p.get("attributes") if isinstance(p.get("attributes"), dict) else {},
            })
    out["proposals"] = fixed
    return out


def _call_openai(prompt: str, summary: Dict[str, Any]) -> str:
    api_key = os.environ.get("OPENAI_API_KEY", "").strip()
    if not api_key:
        raise RuntimeError("OPENAI_API_KEY is required for openai backend")
    base_url = os.environ.get("OPENAI_BASE_URL", "https://api.openai.com").rstrip("/")
    model = os.environ.get("WORLD_MODEL_MODEL") or os.environ.get("OPENAI_MODEL") or "gpt-4o-mini"
    payload = {
        "model": model,
        "temperature": 0,
        "max_tokens": 1200,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": json.dumps(summary, indent=2)},
        ],
    }
    resp = _http_post_json(
        f"{base_url}/v1/chat/completions",
        {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}",
        },
        payload,
    )
    return resp["choices"][0]["message"]["content"]


def _call_anthropic(prompt: str, summary: Dict[str, Any]) -> str:
    api_key = os.environ.get("ANTHROPIC_API_KEY", "").strip()
    if not api_key:
        raise RuntimeError("ANTHROPIC_API_KEY is required for anthropic backend")
    model = os.environ.get("WORLD_MODEL_MODEL") or os.environ.get("ANTHROPIC_MODEL") or "claude-3-5-sonnet-20240620"
    payload = {
        "model": model,
        "max_tokens": 1200,
        "temperature": 0,
        "system": prompt,
        "messages": [
            {"role": "user", "content": json.dumps(summary, indent=2)},
        ],
    }
    resp = _http_post_json(
        "https://api.anthropic.com/v1/messages",
        {
            "Content-Type": "application/json",
            "x-api-key": api_key,
            "anthropic-version": "2023-06-01",
        },
        payload,
    )
    content = resp.get("content", [])
    if not content:
        raise RuntimeError("anthropic response had no content")
    return content[0].get("text", "")


def _call_ollama(prompt: str, summary: Dict[str, Any]) -> str:
    host = os.environ.get("OLLAMA_HOST", "").strip() or "http://127.0.0.1:11434"
    model = os.environ.get("WORLD_MODEL_MODEL") or os.environ.get("OLLAMA_MODEL") or "llama3.1"
    payload = {
        "model": model,
        "stream": False,
        "messages": [
            {"role": "system", "content": prompt},
            {"role": "user", "content": json.dumps(summary, indent=2)},
        ],
        "options": {"temperature": 0},
    }
    resp = _http_post_json(
        f"{host.rstrip('/')}/api/chat",
        {"Content-Type": "application/json"},
        payload,
    )
    msg = resp.get("message", {})
    return msg.get("content", "")


def main() -> None:
    req = _read_stdin_json()
    prompt, summary = _summarize_request(req)
    trace_id = req.get("trace_id", "wm::unknown")

    backend = os.environ.get("WORLD_MODEL_BACKEND", "").strip().lower()
    if not backend:
        if os.environ.get("OPENAI_API_KEY"):
            backend = "openai"
        elif os.environ.get("ANTHROPIC_API_KEY"):
            backend = "anthropic"
        elif os.environ.get("OLLAMA_HOST") or os.environ.get("OLLAMA_MODEL"):
            backend = "ollama"
        else:
            raise RuntimeError(
                "WORLD_MODEL_BACKEND not set and no API keys found. "
                "Set WORLD_MODEL_BACKEND=openai|anthropic|ollama and configure keys."
            )

    if backend == "openai":
        raw = _call_openai(prompt, summary)
    elif backend == "anthropic":
        raw = _call_anthropic(prompt, summary)
    elif backend == "ollama":
        raw = _call_ollama(prompt, summary)
    else:
        raise RuntimeError(f"unsupported WORLD_MODEL_BACKEND={backend}")

    parsed = _extract_json(raw)
    proposals = _normalize_proposals(trace_id, parsed)
    out = {
        "protocol": req.get("protocol", "axiograph_world_model_v1"),
        "trace_id": trace_id,
        "generated_at_unix_secs": int(time.time()),
        "proposals": proposals,
        "notes": [f"backend={backend}"],
        "error": None,
    }
    sys.stdout.write(json.dumps(out))


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        err = {
            "protocol": "axiograph_world_model_v1",
            "trace_id": "wm::error",
            "generated_at_unix_secs": int(time.time()),
            "proposals": {
                "version": 1,
                "generated_at": str(int(time.time())),
                "source": {"source_type": "world_model", "locator": "error"},
                "schema_hint": None,
                "proposals": [],
            },
            "notes": [],
            "error": str(exc),
        }
        sys.stdout.write(json.dumps(err))
        sys.exit(2)
