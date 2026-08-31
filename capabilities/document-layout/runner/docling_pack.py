#!/usr/bin/env python3
"""Pinned, offline JSON-RPC Docling adapter; never installs at runtime."""
import json
import shutil
import sys
from pathlib import Path

PACK_ROOT = Path(__file__).resolve().parent.parent
SITE_PACKAGES = PACK_ROOT / "runtime" / "site-packages"
MODEL_ROOT = PACK_ROOT / "models"
if SITE_PACKAGES.is_dir():
    sys.path.insert(0, str(SITE_PACKAGES))


def fail(request_id, code, message):
    return {"jsonrpc": "2.0", "id": request_id, "result": None,
            "error": {"code": code, "message": message, "data": None}}


def contained(root, candidate):
    try:
        candidate.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def dependencies():
    from docling.datamodel.base_models import InputFormat
    from docling.datamodel.pipeline_options import PdfPipelineOptions
    from docling.document_converter import DocumentConverter, PdfFormatOption
    return InputFormat, PdfPipelineOptions, DocumentConverter, PdfFormatOption


def handle(request):
    request_id = str(request.get("id", ""))
    params = request.get("params", {})
    if request.get("jsonrpc") == "2.0" and request.get("method") == "capability.health":
        if (params.get("protocolVersion") != "2"
                or params.get("capabilityId") != "document-layout"
                or params.get("route") != "pdf.layout"):
            return fail(request_id, -32602, "invalid health request")
        try:
            dependencies()
        except ImportError:
            return fail(request_id, -32020, "document-layout runtime is incomplete")
        if not MODEL_ROOT.is_dir() or not any(MODEL_ROOT.iterdir()):
            return fail(request_id, -32020, "document-layout model payload is incomplete")
        return {"jsonrpc": "2.0", "id": request_id, "result": {
            "healthy": True, "protocolVersion": "2",
            "capabilityId": "document-layout", "route": "pdf.layout"}, "error": None}
    if request.get("jsonrpc") != "2.0" or request.get("method") != "import.execute":
        return fail(request_id, -32600, "invalid request")
    if params.get("protocolVersion") != "2":
        return fail(request_id, -32602, "unsupported protocol version")
    project = Path(params.get("projectRoot", "")).resolve()
    staging = Path(params.get("stagingRoot", ""))
    source = Path(params.get("chainedInput") or params.get("input", {}).get("locator", ""))
    if not staging.is_absolute():
        staging = project / staging
    if not source.is_absolute():
        source = (staging if params.get("chainedInput") else project) / source
    if (source.suffix.lower() != ".pdf" or not source.is_file()
            or not contained(project, staging)
            or not contained(project, source)):
        return fail(request_id, -32602, "unauthorized or unsupported source")
    try:
        InputFormat, PdfPipelineOptions, DocumentConverter, PdfFormatOption = dependencies()
        options = PdfPipelineOptions()
        options.artifacts_path = MODEL_ROOT
        options.enable_remote_services = False
        options.allow_external_plugins = False
        converter = DocumentConverter(format_options={
            InputFormat.PDF: PdfFormatOption(pipeline_options=options),
        })
        result = converter.convert(source)
        markdown = result.document.export_to_markdown()
        if not markdown.strip():
            raise ValueError("empty conversion")
        staging.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, staging / "source.pdf")
        (staging / "document.md").write_text(markdown, encoding="utf-8", newline="\n")
        metadata = {"engineId": "pack.document-layout", "engineVersion": "2.48.0",
                    "route": "pdf.layout", "offline": True}
        (staging / "metadata.json").write_text(json.dumps(metadata), encoding="utf-8")
        return {"jsonrpc": "2.0", "id": request_id, "result": {
            "sourceSnapshotPath": "source.pdf", "markdownPath": "document.md",
            "assetPaths": [], "metadataPath": "metadata.json", "title": source.stem,
            "textCoverage": None, "tableCellAccuracy": None, "warnings": []}, "error": None}
    except (ImportError, OSError, RuntimeError, ValueError) as error:
        return fail(request_id, -32010, "conversion failed: " + type(error).__name__)


def main():
    try:
        response = handle(json.loads(sys.stdin.readline()))
    except (ValueError, TypeError, json.JSONDecodeError):
        response = fail("", -32700, "parse error")
    sys.stdout.write(json.dumps(response, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
