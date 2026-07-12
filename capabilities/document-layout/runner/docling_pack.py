#!/usr/bin/env python3
"""Pinned JSON-RPC Docling adapter; never installs packages at runtime."""
import json
import sys


def main():
    request = json.loads(sys.stdin.readline())
    request_id = str(request.get("id", ""))
    try:
        import docling  # noqa: F401 -- availability probe for the signed wheelhouse
    except ImportError:
        response = {"jsonrpc": "2.0", "id": request_id, "result": None,
                    "error": {"code": -32020, "message": "waiting_capability: document-layout pack payload is unavailable", "data": {"packId": "document-layout"}}}
        sys.stdout.write(json.dumps(response, separators=(",", ":")) + "\n")
        return
    response = {"jsonrpc": "2.0", "id": request_id, "result": None,
                "error": {"code": -32021, "message": "document-layout runner contract is present but this development pack has no verified model payload", "data": {"packId": "document-layout"}}}
    sys.stdout.write(json.dumps(response, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
