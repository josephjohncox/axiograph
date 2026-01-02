#!/usr/bin/env python3
import json
import os
import sys
import time
from typing import Any, Dict, List


def _read_stdin_json() -> Dict[str, Any]:
    raw = sys.stdin.read()
    if not raw.strip():
        raise RuntimeError("expected JSON request on stdin")
    return json.loads(raw)


def _load_onnx(model_path: str):
    try:
        import onnxruntime as ort  # type: ignore
    except Exception as exc:  # pragma: no cover
        raise RuntimeError(
            "onnxruntime is required for the ONNX world model plugin. "
            "Install with: pip install onnxruntime"
        ) from exc
    providers = ["CPUExecutionProvider"]
    return ort.InferenceSession(model_path, providers=providers)


def _stable_hash(text: str) -> int:
    h = 2166136261
    for b in text.encode("utf-8"):
        h ^= b
        h = (h * 16777619) & 0xFFFFFFFF
    return h


def _normalize_proposals(trace_id: str, proposals: List[Dict[str, Any]]) -> Dict[str, Any]:
    out = {
        "version": 1,
        "generated_at": str(int(time.time())),
        "source": {"source_type": "world_model", "locator": trace_id},
        "schema_hint": None,
        "proposals": [],
    }
    for idx, p in enumerate(proposals):
        kind = p.get("kind", "Relation")
        kind = "Entity" if str(kind).lower() == "entity" else "Relation"
        base_id = f"wm::{trace_id}::{idx}"
        meta = {
            "proposal_id": p.get("proposal_id") or base_id,
            "confidence": float(p.get("confidence", 0.7)),
            "evidence": p.get("evidence") if isinstance(p.get("evidence"), list) else [],
            "public_rationale": p.get("public_rationale") or "onnx world model proposal",
            "metadata": p.get("metadata") if isinstance(p.get("metadata"), dict) else {},
            "schema_hint": p.get("schema_hint"),
        }
        if kind == "Entity":
            out["proposals"].append({
                **meta,
                "kind": "Entity",
                "entity_id": p.get("entity_id") or f"{base_id}:entity",
                "entity_type": p.get("entity_type") or "Entity",
                "name": p.get("name") or p.get("entity_id") or f"Entity {idx}",
                "attributes": p.get("attributes") if isinstance(p.get("attributes"), dict) else {},
                "description": p.get("description"),
            })
        else:
            out["proposals"].append({
                **meta,
                "kind": "Relation",
                "relation_id": p.get("relation_id") or f"{base_id}:rel",
                "rel_type": p.get("rel_type") or "related_to",
                "source": p.get("source") or "",
                "target": p.get("target") or "",
                "attributes": p.get("attributes") if isinstance(p.get("attributes"), dict) else {},
            })
    return out


def main() -> None:
    req = _read_stdin_json()
    trace_id = req.get("trace_id", "wm::onnx")
    export = (req.get("input") or {}).get("export") or {}
    items = export.get("items", []) or []

    # Prefer env var for deterministic local model.
    model_path = (
        (req.get("options", {}) or {}).get("model_path")
        if isinstance((req.get("options", {}) or {}).get("model_path"), str)
        else None
    )
    model_path = model_path or ""
    if not model_path:
        model_path = (
            os.environ.get("WORLD_MODEL_MODEL_PATH", "").strip()
            or "models/world_model_small.onnx"
        )

    session = _load_onnx(model_path)

    proposals: List[Dict[str, Any]] = []
    max_new = int((req.get("options") or {}).get("max_new_proposals", 50))
    for idx, it in enumerate(items[:max_new]):
        rel = it.get("relation") or "related_to"
        fields = it.get("fields", []) or []
        field_map = {k: v for (k, v) in fields} if isinstance(fields, list) else {}
        mask = it.get("mask_fields", []) or []
        # Deterministic pseudo-input to ONNX model.
        text = json.dumps({"relation": rel, "fields": fields, "mask": mask}, sort_keys=True)
        seed = _stable_hash(text)
        # Example inference: pass seed as int32; model should map to a score/confidence.
        input_name = session.get_inputs()[0].name
        out_name = session.get_outputs()[0].name
        result = session.run([out_name], {input_name: [seed]})[0]
        score = float(result[0]) if result is not None else 0.7
        conf = max(0.55, min(0.95, score))
        proposals.append({
            "kind": "Relation",
            "proposal_id": f"rel::{rel}::{idx}",
            "confidence": conf,
            "evidence": [],
            "public_rationale": "onnx world model prediction",
            "metadata": {"model_path": model_path},
            "schema_hint": it.get("schema"),
            "relation_id": f"rel::{rel}::{idx}",
            "rel_type": rel,
            "source": fields[0][1] if fields else "",
            "target": fields[1][1] if len(fields) > 1 else "",
            "attributes": field_map,
        })

    proposals_file = _normalize_proposals(trace_id, proposals)
    out = {
        "protocol": req.get("protocol", "axiograph_world_model_v1"),
        "trace_id": trace_id,
        "generated_at_unix_secs": int(time.time()),
        "proposals": proposals_file,
        "notes": [f"backend=onnx model={model_path}"],
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
