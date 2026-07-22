from __future__ import annotations

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

PACK_ROOT = Path(__file__).resolve().parent.parent


def declaration(relative: str) -> dict:
    file_path = PACK_ROOT / relative
    digest = hashlib.sha256(file_path.read_bytes()).hexdigest()
    return {"path": relative, "bytes": file_path.stat().st_size, "sha256": digest}


required = [
    "models/ch_PP-OCRv5_det_mobile.onnx",
    "models/ch_PP-OCRv5_rec_mobile.onnx",
    "models/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx",
    "models/ppocrv5_dict.txt",
]
manifest_path = PACK_ROOT / "manifest.json"
root = Path(tempfile.mkdtemp(prefix="llm-wiki-rapidocr-qualification-"))
try:
    manifest_path.write_text(json.dumps({
        "schemaVersion": 2,
        "packId": "ocr-cjk-accurate",
        "version": "3.8.1+ppocrv5",
        "protocolVersion": "2",
        "files": [declaration(value) for value in required],
    }), encoding="utf-8")
    staging = root / "staging"
    workspace = staging / "runtime-temp" / "fixture"
    workspace.mkdir(parents=True)
    image = workspace / "ch_en_num.jpg"
    shutil.copyfile(PACK_ROOT / "qualification" / "ch_en_num.jpg", image)
    request = {
        "jsonrpc": "2.0",
        "id": 1,
        "params": {
            "operation": "extract",
            "projectRoot": str(root),
            "stagingRoot": str(workspace),
            "input": {"kind": "file", "locator": str(image)},
        },
    }
    completed = subprocess.run(
        [sys.executable, str(PACK_ROOT / "runner" / "index.py")],
        input=json.dumps(request, ensure_ascii=False),
        text=True,
        encoding="utf-8",
        capture_output=True,
        timeout=180,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"runner exited {completed.returncode}: {completed.stderr[:500]}")
    response = json.loads(completed.stdout.strip())
    if response.get("error") is not None:
        raise RuntimeError(f"runner returned an error: {response['error']}")
    result = response["result"]
    metadata = json.loads((workspace / result["metadataPath"]).read_text(encoding="utf-8"))
    markdown = (workspace / result["markdownPath"]).read_text(encoding="utf-8")
    text = "\n".join(block["text"] for block in metadata["blocks"])
    if "大桶装" not in text or "强力去污" not in text:
        raise RuntimeError(f"qualification text was not recognized: {text[:500]}")
    if metadata.get("provider") != "cpu" or metadata.get("provenance") != "authorized-local-ocr":
        raise RuntimeError("qualification metadata did not prove offline CPU OCR")
    if not metadata["blocks"] or any(block["coordinates"]["width"] <= 0 for block in metadata["blocks"]):
        raise RuntimeError("qualification did not produce bounded OCR blocks")
    if "Machine-extracted text" not in markdown:
        raise RuntimeError("qualification Markdown lost its evidence label")
    print(json.dumps({"qualified": True, "provider": "cpu", "blocks": len(metadata["blocks"])}, ensure_ascii=False))
finally:
    manifest_path.unlink(missing_ok=True)
    shutil.rmtree(root, ignore_errors=True)
