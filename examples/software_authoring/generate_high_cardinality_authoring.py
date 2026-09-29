#!/usr/bin/env python3
"""Generate a deterministic, realistic high-cardinality authoring package.

The output is measurement input, not accepted ontology state or capacity proof.
Run it in a temporary/build directory, then pass the generated request to the
production `axiograph authoring workspace` command.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--relations", type=int, default=180)
    args = parser.parse_args()
    if not 2 <= args.relations <= 512:
        parser.error("--relations must be between 2 and 512")
    if args.out_dir.is_absolute() or ".." in args.out_dir.parts:
        parser.error("--out-dir must be workspace-relative and cannot contain `..`")
    out = args.out_dir
    out.mkdir(parents=True, exist_ok=True)
    count = args.relations

    axi = ["module HighCardinality", "", "schema Logistics:", "  object Facility", "  object Shipment"]
    axi.extend(
        f"  relation ShipmentEvent{index:03}(shipment: Shipment, facility: Facility)"
        for index in range(count)
    )
    axi.extend(["", "theory LogisticsRules on Logistics:"])
    axi.extend(
        f"  constraint key ShipmentEvent{index:03}(shipment, facility)"
        for index in range(count)
    )
    axi.extend(
        [
            "",
            "instance LogisticsSeed of Logistics:",
            "  Facility = {" + ", ".join(f"Facility_{index:03}" for index in range(count)) + "}",
            "  Shipment = {" + ", ".join(f"Shipment_{index:03}" for index in range(count)) + "}",
        ]
    )
    axi.extend(
        f"  ShipmentEvent{index:03} = {{event_{index:03}: (shipment=Shipment_{index:03}, facility=Facility_{index:03})}}"
        for index in range(count)
    )
    axi_path = out / "HighCardinality.axi"
    axi_path.write_text("\n".join(axi) + "\n", encoding="utf-8")

    cq = ["version competency_question_bundle_v1"]
    for index in range(count):
        cq.extend(
            [
                f"question shipment_event_{index:03}:",
                f"  ask: does shipment {index:03} have an event?",
                f"  expect: exists Logistics.ShipmentEvent{index:03}(shipment=Shipment_{index:03}, facility=?facility)",
            ]
        )
    cq_path = out / "high_cardinality.cq"
    cq_path.write_text("\n".join(cq) + "\n", encoding="utf-8")

    request = {
        "version": "authoring_workspace_request_v1",
        "presentation": {"detail": "full"},
        "operation": "promotion_review",
        "axi_path": str(axi_path),
        "cq_path": str(cq_path),
        "schema": "Logistics",
        "query_ir_v1": {
            "version": 1,
            "select_vars": ["shipment"],
            "where_atoms": [{"kind": "type", "term": "?shipment", "type": "Shipment"}],
            "limit": 100,
        },
        "focus_variable": "shipment",
    }
    request_path = out / "high_cardinality_request.json"
    request_path.write_text(json.dumps(request, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"relations": count, "axi": str(axi_path), "cq": str(cq_path), "request": str(request_path)}))


if __name__ == "__main__":
    main()
