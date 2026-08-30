from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import socket
import sys
import tempfile
import warnings

from core import (
    ENGINE_ID,
    ENGINE_VERSION,
    MAX_IMAGE_PIXELS,
    MODEL_DECLARATIONS,
    MODEL_VERSION,
    OcrPolicyError,
    mean_confidence,
    native_tool_path,
    normalize_blocks,
    render_markdown,
    resolve_staging_image,
    validate_image_geometry,
    verify_signed_file,
)

MAX_RPC_BYTES = 1024 * 1024
PACK_ROOT = Path(__file__).resolve().parent.parent
PACK_ROUTES = {"ocr-basic": "ocr.basic", "ocr-cjk-accurate": "ocr.cjk-accurate"}


def block_network() -> None:
    original_socket = socket.socket

    class OfflineSocket(original_socket):
        def connect(self, *_args, **_kwargs):
            raise OSError("IMPORT_OCR_NETWORK_BLOCKED")

        def connect_ex(self, *_args, **_kwargs):
            raise OSError("IMPORT_OCR_NETWORK_BLOCKED")

    def blocked(*_args, **_kwargs):
        raise OSError("IMPORT_OCR_NETWORK_BLOCKED")

    socket.socket = OfflineSocket
    socket.create_connection = blocked
    socket.getaddrinfo = blocked
    for name in (
        "ALL_PROXY", "HTTP_PROXY", "HTTPS_PROXY", "NO_PROXY",
        "all_proxy", "http_proxy", "https_proxy", "no_proxy",
    ):
        os.environ.pop(name, None)


def read_rpc() -> dict:
    payload = sys.stdin.buffer.read(MAX_RPC_BYTES + 1)
    if not payload or len(payload) > MAX_RPC_BYTES:
        raise OcrPolicyError("IMPORT_OCR_INVALID_REQUEST")
    try:
        value = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError):
        raise OcrPolicyError("IMPORT_OCR_INVALID_REQUEST") from None
    if not isinstance(value, dict):
        raise OcrPolicyError("IMPORT_OCR_INVALID_REQUEST")
    return value


def write_response(value: dict) -> None:
    sys.stdout.write(json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def failure(request_id, code: str) -> None:
    write_response({
        "jsonrpc": "2.0",
        "id": request_id,
        "result": None,
        "error": {
            "code": -32021,
            "message": "The authorized local RapidOCR helper could not complete the request.",
            "data": {"code": code},
        },
    })


def relative_to_staging(staging_root: Path, value: Path) -> str:
    return value.relative_to(staging_root).as_posix()


rpc = None
output_root = None
rendered_pdf_root = None
completed = False
try:
    rpc = read_rpc()
    params = rpc.get("params")
    if rpc.get("method") == "capability.health":
        route = params.get("route") if isinstance(params, dict) else None
        with (PACK_ROOT / "manifest.json").open("r", encoding="utf-8") as handle:
            manifest = json.load(handle)
        capability_id = manifest.get("packId")
        if (
            not isinstance(params, dict)
            or params.get("protocolVersion") != "2"
            or params.get("capabilityId") != capability_id
            or route != PACK_ROUTES.get(capability_id)
        ):
            raise OcrPolicyError("IMPORT_OCR_INVALID_REQUEST")
        if capability_id not in PACK_ROUTES or manifest.get("protocolVersion") != "2":
            raise OcrPolicyError("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
        for relative in MODEL_DECLARATIONS:
            verify_signed_file(PACK_ROOT, manifest, relative)
        verify_signed_file(PACK_ROOT, manifest, "models/ppocrv5_dict.txt")
        import rapidocr  # noqa: F401
        from PIL import Image  # noqa: F401
        import pillow_heif  # noqa: F401
        import pypdfium2  # noqa: F401
        write_response({
            "jsonrpc": "2.0",
            "id": rpc.get("id"),
            "result": {
                "healthy": True,
                "protocolVersion": "2",
                "capabilityId": capability_id,
                "route": route,
            },
            "error": None,
        })
        completed = True
        raise SystemExit(0)
    if (
        rpc.get("jsonrpc") != "2.0"
        or not isinstance(params, dict)
        or params.get("operation") != "extract"
        or not isinstance(params.get("input"), dict)
        or params["input"].get("kind") != "file"
    ):
        raise OcrPolicyError("IMPORT_OCR_INVALID_REQUEST")

    _, staging_root, image_path = resolve_staging_image(
        params.get("projectRoot"),
        params.get("stagingRoot"),
        params["input"].get("locator"),
    )
    with (PACK_ROOT / "manifest.json").open("r", encoding="utf-8") as handle:
        manifest = json.load(handle)
    if manifest.get("packId") not in PACK_ROUTES or manifest.get("protocolVersion") != "2":
        raise OcrPolicyError("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")

    model_paths = {
        relative: verify_signed_file(PACK_ROOT, manifest, relative)
        for relative in MODEL_DECLARATIONS
    }
    dictionary = verify_signed_file(PACK_ROOT, manifest, "models/ppocrv5_dict.txt")

    block_network()
    warnings.filterwarnings("ignore", category=UserWarning, module="omegaconf")
    os.environ.update({
        "OMP_NUM_THREADS": str(min(8, max(1, os.cpu_count() or 1))),
        "ORT_LOG_SEVERITY_LEVEL": "3",
        "NO_COLOR": "1",
        "PYTHONDONTWRITEBYTECODE": "1",
    })
    from rapidocr import OCRVersion, RapidOCR
    from PIL import Image, UnidentifiedImageError
    import pillow_heif
    import pypdfium2

    pillow_heif.register_heif_opener()

    Image.MAX_IMAGE_PIXELS = MAX_IMAGE_PIXELS
    images_for_ocr = [image_path]
    try:
        if image_path.suffix.lower() == ".pdf":
            document = pypdfium2.PdfDocument(native_tool_path(image_path))
            if len(document) < 1 or len(document) > 200:
                raise OcrPolicyError("IMPORT_OCR_IMAGE_TOO_LARGE")
            rendered_pdf_root = Path(tempfile.mkdtemp(prefix=".pdf-ocr-input-", dir=staging_root))
            images_for_ocr = []
            total_pixels = 0
            for page_index in range(len(document)):
                rendered = rendered_pdf_root / f"page-{page_index + 1}.png"
                document[page_index].render(scale=2).to_pil().save(rendered, format="PNG")
                images_for_ocr.append(rendered)
            document.close()
        for image_for_ocr in images_for_ocr:
            with Image.open(native_tool_path(image_for_ocr)) as image_probe:
                validate_image_geometry(
                    image_probe.width,
                    image_probe.height,
                    int(getattr(image_probe, "n_frames", 1)),
                )
                if image_path.suffix.lower() == ".pdf":
                    total_pixels += image_probe.width * image_probe.height
                    if total_pixels > MAX_IMAGE_PIXELS * 4:
                        raise OcrPolicyError("IMPORT_OCR_IMAGE_TOO_LARGE")
    except OcrPolicyError:
        raise
    except Image.DecompressionBombError:
        raise OcrPolicyError("IMPORT_OCR_IMAGE_TOO_LARGE") from None
    except (OSError, UnidentifiedImageError, ValueError):
        raise OcrPolicyError("IMPORT_OCR_INVALID_IMAGE") from None

    threads = min(8, max(1, os.cpu_count() or 1))
    engine = RapidOCR(params={
        "Global.log_level": "critical",
        "Global.model_root_dir": native_tool_path(PACK_ROOT / "models"),
        "Global.return_word_box": False,
        "EngineConfig.onnxruntime.intra_op_num_threads": threads,
        "EngineConfig.onnxruntime.inter_op_num_threads": 1,
        "EngineConfig.onnxruntime.enable_cpu_mem_arena": False,
        "EngineConfig.onnxruntime.use_cuda": False,
        "EngineConfig.onnxruntime.use_dml": False,
        "EngineConfig.onnxruntime.use_coreml": False,
        "Det.ocr_version": OCRVersion.PPOCRV5,
        "Det.model_path": native_tool_path(model_paths["models/ch_PP-OCRv5_det_mobile.onnx"]),
        "Cls.ocr_version": OCRVersion.PPOCRV5,
        "Cls.model_path": native_tool_path(model_paths["models/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx"]),
        "Rec.ocr_version": OCRVersion.PPOCRV5,
        "Rec.model_path": native_tool_path(model_paths["models/ch_PP-OCRv5_rec_mobile.onnx"]),
        "Rec.rec_keys_path": native_tool_path(dictionary),
    })
    pages = []
    blocks = []
    for page_number, image_for_ocr in enumerate(images_for_ocr, start=1):
        output = engine(native_tool_path(image_for_ocr))
        if output.img is None or getattr(output.img, "ndim", 0) < 2:
            raise OcrPolicyError("IMPORT_OCR_OUTPUT_INVALID")
        image_height, image_width = (int(output.img.shape[0]), int(output.img.shape[1]))
        page_blocks = normalize_blocks(output.boxes, output.txts, output.scores, image_width, image_height)
        for block in page_blocks:
            block["pageNumber"] = page_number
        blocks.extend(page_blocks)
        pages.append({
            "pageNumber": page_number,
            "image": {"width": image_width, "height": image_height},
            "confidence": mean_confidence(page_blocks),
            "blocks": page_blocks,
        })
        if len(blocks) > 10_000:
            raise OcrPolicyError("IMPORT_OCR_OUTPUT_INVALID")
    confidence = mean_confidence(blocks)

    output_root = Path(tempfile.mkdtemp(prefix=".rapidocr-output-", dir=staging_root))
    markdown_path = output_root / "candidate.md"
    source_path = output_root / "source.json"
    metadata_path = output_root / "metadata.json"
    metadata = {
        "engineId": ENGINE_ID,
        "engineVersion": ENGINE_VERSION,
        "modelVersion": MODEL_VERSION,
        "provider": "cpu",
        "sourceName": image_path.name,
        "pageCount": len(pages),
        "pages": pages,
        "confidence": confidence,
        "blocks": blocks,
        "models": [
            {"path": relative, "bytes": declaration[0], "sha256": declaration[1]}
            for relative, declaration in MODEL_DECLARATIONS.items()
        ],
        "provenance": "authorized-local-ocr",
    }
    markdown_path.write_text(
        "\n\n".join(
            render_markdown(
                image_path.name if len(pages) == 1 else f"{image_path.name} — page {page['pageNumber']}",
                page["blocks"],
                page["confidence"],
            )
            for page in pages
        ),
        encoding="utf-8",
        newline="\n",
    )
    serialized = json.dumps(metadata, ensure_ascii=False, separators=(",", ":"))
    source_path.write_text(serialized, encoding="utf-8", newline="\n")
    metadata_path.write_text(serialized, encoding="utf-8", newline="\n")
    warnings_out = [] if blocks else ["IMPORT_OCR_NO_TEXT"]
    write_response({
        "jsonrpc": "2.0",
        "id": rpc.get("id"),
        "result": {
            "sourceSnapshotPath": relative_to_staging(staging_root, source_path),
            "markdownPath": relative_to_staging(staging_root, markdown_path),
            "assetPaths": [],
            "metadataPath": relative_to_staging(staging_root, metadata_path),
            "title": f"Local OCR - {image_path.name}",
            "textCoverage": 1.0 if blocks else 0.0,
            "warnings": warnings_out,
        },
        "error": None,
    })
    completed = True
except OcrPolicyError as error:
    failure(rpc.get("id") if isinstance(rpc, dict) else None, error.code)
except Exception:
    failure(rpc.get("id") if isinstance(rpc, dict) else None, "IMPORT_OCR_ENGINE_FAILED")
finally:
    if output_root is not None and not completed:
        shutil.rmtree(output_root, ignore_errors=True)
    if rendered_pdf_root is not None:
        shutil.rmtree(rendered_pdf_root, ignore_errors=True)
