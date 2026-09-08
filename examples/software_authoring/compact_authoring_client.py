#!/usr/bin/env python3
"""Read-only HTTP example: compact review, then source-bound stable-reference pages.

Run from the repository root with `axiograph authoring serve --workspace .`.
This is an executable wire-contract example, not an embedding SDK or proof client.
"""
import argparse
import http.client
import json
from urllib.parse import urlsplit

MAX_RESPONSE_BYTES = 16 * 1024 * 1024


def post(endpoint, payload):
    data = json.dumps(payload).encode("utf-8")
    if len(data) > 1024 * 1024:
        raise ValueError("request exceeds authoring HTTP limit")
    url = urlsplit(endpoint)
    if url.scheme not in ("http", "https") or not url.hostname or url.username or url.password or url.fragment:
        raise ValueError("endpoint must be an HTTP(S) URL without credentials or fragment")
    connection_type = http.client.HTTPSConnection if url.scheme == "https" else http.client.HTTPConnection
    connection = connection_type(url.hostname, url.port, timeout=30)
    try:
        path = (url.path or "/") + ("?" + url.query if url.query else "")
        connection.request("POST", path, data, {"Content-Type": "application/json"})
        response = connection.getresponse()
        data = response.read(MAX_RESPONSE_BYTES + 1)
        # Deliberately do not redirect request source buffers to another endpoint.
        if response.status != 200:
            raise ValueError(f"authoring HTTP {response.status}: {data[:1000]!r}")
    finally:
        connection.close()
    if len(data) > MAX_RESPONSE_BYTES:
        raise ValueError("response exceeds authoring HTTP limit")
    result = json.loads(data)
    if "error" in result:
        raise ValueError(result["error"])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--endpoint", default="http://127.0.0.1:8787/authoring")
    parser.add_argument("--request", required=True)
    parser.add_argument("--check-full", action="store_true", help="Explicitly fetch full and verify page union parity")
    args = parser.parse_args()
    with open(args.request, encoding="utf-8") as source:
        request = json.load(source)
    request["presentation"] = {"detail": "summary"}
    summary = post(args.endpoint, request)
    if summary["version"] != "authoring_workspace_response_v1":
        raise ValueError("unexpected response contract")
    print(summary["summary"])
    print("Promotion blockers:", "; ".join(summary["promotion"]["blockers"]))
    section = next(p for p in summary["sections"] if p["section"] == "stable_runtime_refs")
    items = []
    cursor = section["next_cursor"]
    while cursor is not None:
        request["presentation"] = {
            "detail": "summary", "sections": ["stable_runtime_refs"], "limit": 25, "cursor": cursor
        }
        page = post(args.endpoint, request)
        if page["input_identity"] != summary["input_identity"]:
            raise ValueError("source/request changed during pagination")
        selected = next(p for p in page["sections"] if p["selected"])
        if selected["offset"] != len(items) or selected["total"] != section["total"]:
            raise ValueError("inconsistent page offsets/totals")
        if not selected["items"]:
            raise ValueError("non-progressing page")
        items.extend(selected["items"])
        cursor = selected["next_cursor"]
    if len(items) != section["total"]:
        raise ValueError("follow-up unavailable: inspect compilation diagnostics with a fresh request")
    print(f"Retrieved {len(items)} stable runtime references in canonical order.")
    if args.check_full:
        request["presentation"] = {"detail": "full"}
        full = post(args.endpoint, request)
        if full["stable_runtime_refs"] != items:
            raise ValueError("page union differs from explicit full report")
        for field in ("source", "trust", "promotion", "ok"):
            if full.get(field) != summary[field]:
                raise ValueError(f"full/summary mismatch: {field}")
        print("Explicit full artifact parity passed; no acceptance or certification claimed.")


if __name__ == "__main__":
    main()
