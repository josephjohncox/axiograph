#!/usr/bin/env python3
"""
Baseline world model plugin (axiograph_world_model_v1).

This is a deterministic, dependency-free example that:
- reads a JEPA export,
- emits relation proposals for each tuple,
- ignores learning (acts as a placeholder for MLP-style baselines).
"""

import argparse
import json
import sys
import time
from typing import Dict, List, Tuple


def infer_endpoints(field_names: List[str]) -> Tuple[str, str]:
    if "from" in field_names and "to" in field_names:
        return ("from", "to")
    if "source" in field_names and "target" in field_names:
        return ("source", "target")
    if "lhs" in field_names and "rhs" in field_names:
        return ("lhs", "rhs")
    if "child" in field_names and "parent" in field_names:
        return ("child", "parent")
    if len(field_names) >= 2:
        return (field_names[0], field_names[1])
    return ("", "")


def load_export(req: Dict) -> Dict:
    export = req.get("input", {}).get("export")
    if export:
        return export
    export_path = req.get("input", {}).get("export_path")
    if export_path:
        with open(export_path, "r", encoding="utf-8") as f:
            return json.load(f)
    return {}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--strategy", default="oracle", choices=["oracle", "random"])
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()

    raw = sys.stdin.read()
    req = json.loads(raw)

    if req.get("protocol") != "axiograph_world_model_v1":
        raise SystemExit("unsupported protocol")

    export = load_export(req)
    items = export.get("items", [])

    proposals = []
    for idx, item in enumerate(items):
        fields = item.get("fields", [])
        field_map = {k: v for (k, v) in fields}
        field_names = list(field_map.keys())
        src_field, dst_field = infer_endpoints(field_names)
        if not src_field or not dst_field:
            continue
        src = field_map.get(src_field, "")
        dst = field_map.get(dst_field, "")
        if not src or not dst:
            continue

        rel = item.get("relation", "Rel")
        proposal_id = f"rel::{rel}::{src}::{dst}::{idx}"
        proposals.append(
            {
                "kind": "Relation",
                "proposal_id": proposal_id,
                "confidence": 0.9 if args.strategy == "oracle" else 0.5,
                "evidence": [],
                "public_rationale": f"baseline::{args.strategy}",
                "metadata": {"baseline": args.strategy},
                "schema_hint": item.get("schema"),
                "relation_id": proposal_id,
                "rel_type": rel,
                "source": src,
                "target": dst,
                "attributes": {k: v for (k, v) in fields},
            }
        )

    response = {
        "protocol": "axiograph_world_model_v1",
        "trace_id": req.get("trace_id", "wm::baseline"),
        "generated_at_unix_secs": int(time.time()),
        "proposals": {
            "version": 1,
            "generated_at": str(int(time.time())),
            "source": {"source_type": "world_model", "locator": req.get("trace_id", "")},
            "schema_hint": None,
            "proposals": proposals,
        },
        "notes": [f"baseline strategy={args.strategy} proposals={len(proposals)}"],
        "error": None,
    }

    sys.stdout.write(json.dumps(response))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
