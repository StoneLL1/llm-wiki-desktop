#!/usr/bin/env python3
"""JSON-RPC runner bundled into the immutable document-standard pack.

Dependencies are resolved only while building the release archive. This runner
never invokes pip and writes only beneath the item staging directory.
"""
import json
import shutil
import sys
from pathlib import Path


def fail(request_id, code, message):
    return {"jsonrpc": "2.0", "id": request_id, "result": None,
            "error": {"code": code, "message": message, "data": None}}


def contained(root, path):
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def handle(request):
    request_id = str(request.get("id", ""))
    if request.get("jsonrpc") != "2.0" or request.get("method") != "import.execute":
        return fail(request_id, -32600, "invalid request")
    params = request.get("params", {})
    if params.get("protocolVersion") != "2":
        return fail(request_id, -32602, "unsupported protocol version")
    project = Path(params.get("projectRoot", ""))
    staging = Path(params.get("stagingRoot", ""))
    if not staging.is_absolute():
        staging = project / staging
    chained = params.get("chainedInput")
    source = Path(chained) if chained else Path(params.get("input", {}).get("locator", ""))
    if chained and not source.is_absolute():
        source = staging / source
    if not source.is_absolute():
        source = project / source
    allowed_source = contained(staging, source) if chained else contained(project, source)
    if not source.is_file() or not contained(project, staging) or not allowed_source:
        return fail(request_id, -32602, "unauthorized source or staging path")
    try:
        from markitdown import MarkItDown
        staging.mkdir(parents=True, exist_ok=True)
        converted = MarkItDown(enable_plugins=False).convert(str(source))
        markdown = converted.text_content
        shutil.copyfile(source, staging / "source.bin")
        (staging / "document.md").write_text(markdown, encoding="utf-8", newline="\n")
        metadata = {"engineId": "pack.document-standard", "engineVersion": "0.1.0",
                    "route": "pack.markitdown", "contract": "markitdown-fallback"}
        (staging / "metadata.json").write_text(json.dumps(metadata, ensure_ascii=False), encoding="utf-8")
        result = {"sourceSnapshotPath": "source.bin", "markdownPath": "document.md",
                  "assetPaths": [], "metadataPath": "metadata.json", "title": source.stem,
                  "textCoverage": None, "tableCellAccuracy": None,
                  "warnings": ["MARKITDOWN_FALLBACK_REQUIRES_QUALITY_REVIEW"]}
        return {"jsonrpc": "2.0", "id": request_id, "result": result, "error": None}
    except Exception as exc:
        return fail(request_id, -32010, "conversion failed: " + type(exc).__name__)


def main():
    line = sys.stdin.readline()
    try:
        response = handle(json.loads(line))
    except (ValueError, TypeError, json.JSONDecodeError):
        response = fail("", -32700, "parse error")
    sys.stdout.write(json.dumps(response, separators=(",", ":")) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
