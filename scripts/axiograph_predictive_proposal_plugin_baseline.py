#!/usr/bin/env python3
"""
Baseline predictive proposal adapter plugin (axiograph_predictive_proposal_v1).

This is a deterministic, dependency-free example that:
- reads canonical `.axi` semantics plus optional derived training export layers,
- emits relation proposals for each tuple-like training/example item,
- ignores learning (acts as a placeholder for MLP-style baselines).
"""

import argparse
import json
import re
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


INSTANCE_RE = re.compile(r"^\s*instance\s+(\S+)\s+of\s+(\S+)\s*:\s*$")
ASSIGN_RE = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*=\s*\{(.*)$")


def strip_axi_comment(line: str) -> str:
    return line.split("--", 1)[0].rstrip()


def semantic_input(req: Dict) -> Dict:
    return req.get("input", {}).get("semantic_input") or {}


def semantic_layers(req: Dict) -> List[Dict]:
    layers = semantic_input(req).get("layers")
    if isinstance(layers, list):
        return layers
    return []


def load_training_export(req: Dict) -> Dict:
    for layer in semantic_layers(req):
        if layer.get("kind") == "training_export" and isinstance(layer.get("export"), dict):
            return layer["export"]
    return {}


def parse_tuple_fields(body: str) -> List[Tuple[str, str]]:
    fields: List[Tuple[str, str]] = []
    for raw_part in body.split(","):
        part = raw_part.strip()
        if not part or "=" not in part:
            continue
        key, value = part.split("=", 1)
        key = key.strip()
        value = value.strip()
        if key and value:
            fields.append((key, value))
    return fields


def extract_items_from_axi(axi_text: str) -> List[Dict]:
    items: List[Dict] = []
    current_instance = None
    current_schema = None
    assignment_name = None
    assignment_lines: List[str] = []
    brace_depth = 0

    def flush_assignment() -> None:
        nonlocal assignment_name, assignment_lines, brace_depth
        if not assignment_name:
            return
        block = "\n".join(assignment_lines)
        tuples = re.findall(r"\(([^()]*)\)", block)
        for tuple_body in tuples:
            fields = parse_tuple_fields(tuple_body)
            if not fields:
                continue
            items.append(
                {
                    "schema": current_schema,
                    "instance": current_instance,
                    "relation": assignment_name,
                    "fields": fields,
                    "mask_fields": [],
                }
            )
        assignment_name = None
        assignment_lines = []
        brace_depth = 0

    for raw_line in axi_text.splitlines():
        line = strip_axi_comment(raw_line)
        if not line.strip():
            continue

        if assignment_name is None:
            instance_match = INSTANCE_RE.match(line)
            if instance_match:
                current_instance, current_schema = instance_match.groups()
                continue

            assign_match = ASSIGN_RE.match(line)
            if current_instance and assign_match:
                assignment_name = assign_match.group(1)
                remainder = assign_match.group(2)
                assignment_lines = [remainder]
                brace_depth = 1 + remainder.count("{") - remainder.count("}")
                if brace_depth <= 0:
                    flush_assignment()
                continue
        else:
            assignment_lines.append(line)
            brace_depth += line.count("{") - line.count("}")
            if brace_depth <= 0:
                flush_assignment()

    flush_assignment()
    return items


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--strategy", default="oracle", choices=["oracle", "random"])
    parser.add_argument("--seed", type=int, default=1)
    args = parser.parse_args()

    raw = sys.stdin.read()
    req = json.loads(raw)

    if req.get("protocol") != "axiograph_predictive_proposal_v1":
        raise SystemExit("unsupported protocol")

    export = load_training_export(req)
    items = export.get("items", [])
    mode = "training_export"
    if not items:
        axi_text = req.get("input", {}).get("axi_module_text") or ""
        items = extract_items_from_axi(axi_text)
        mode = "canonical_axi"

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
        attributes = {k: v for (k, v) in fields}
        attributes["axi_source_field"] = src_field
        attributes["axi_target_field"] = dst_field
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
                "attributes": attributes,
            }
        )

    response = {
        "protocol": "axiograph_predictive_proposal_v1",
        "trace_id": req.get("trace_id", "proposal::baseline"),
        "generated_at_unix_secs": int(time.time()),
        "proposals": {
            "version": 1,
            "generated_at": str(int(time.time())),
            "source": {"source_type": "predictive_proposal_adapter", "locator": req.get("trace_id", "")},
            "schema_hint": None,
            "proposals": proposals,
        },
        "notes": [f"baseline strategy={args.strategy} mode={mode} proposals={len(proposals)}"],
        "error": None,
    }

    sys.stdout.write(json.dumps(response))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
