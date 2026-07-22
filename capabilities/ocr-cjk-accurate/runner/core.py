from __future__ import annotations

import hashlib
import html
import math
import os
from pathlib import Path
from typing import Any

ENGINE_ID = "rapidocr-onnxruntime"
ENGINE_VERSION = "3.8.1"
MODEL_VERSION = "PP-OCRv5-mobile-v3.8.0"
MAX_IMAGE_BYTES = 64 * 1024 * 1024
MAX_IMAGE_DIMENSION = 16_384
MAX_IMAGE_PIXELS = 64 * 1024 * 1024
MAX_IMAGE_FRAMES = 1
MAX_BLOCKS = 10_000
MAX_TEXT_CHARACTERS = 16_384

MODEL_DECLARATIONS = {
    "models/ch_PP-OCRv5_det_mobile.onnx": (
        4_819_576,
        "4d97c44a20d30a81aad087d6a396b08f786c4635742afc391f6621f5c6ae78ae",
    ),
    "models/ch_PP-OCRv5_rec_mobile.onnx": (
        16_631_306,
        "5825fc7ebf84ae7a412be049820b4d86d77620f204a041697b0494669b1742c5",
    ),
    "models/ch_PP-LCNet_x0_25_textline_ori_cls_mobile.onnx": (
        1_018_508,
        "54379ae5174d026780215fc748a7f31910dee36818e63d49e17dc598ecc82df7",
    ),
}

IMAGE_EXTENSIONS = {
    ".bmp",
    ".jpeg",
    ".jpg",
    ".png",
    ".tif",
    ".tiff",
    ".webp",
}


class OcrPolicyError(Exception):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def _fail(code: str) -> None:
    raise OcrPolicyError(code)


def is_contained(root: Path, candidate: Path) -> bool:
    try:
        candidate.relative_to(root)
        return True
    except ValueError:
        return False


def resolve_staging_image(
    project_root_value: Any, staging_root_value: Any, locator_value: Any
) -> tuple[Path, Path, Path]:
    if not all(isinstance(value, str) and value for value in (
        project_root_value,
        staging_root_value,
        locator_value,
    )):
        _fail("IMPORT_OCR_INVALID_REQUEST")

    project_root = Path(project_root_value).resolve(strict=True)
    staging_candidate = Path(staging_root_value)
    if not staging_candidate.is_absolute():
        staging_candidate = project_root / staging_candidate
    staging_root = staging_candidate.resolve(strict=True)
    if not project_root.is_dir() or not staging_root.is_dir() or not is_contained(project_root, staging_root):
        _fail("IMPORT_OCR_POLICY_BLOCKED")

    candidate = Path(locator_value)
    if not candidate.is_absolute():
        candidate = staging_root / candidate
    candidate = Path(os.path.abspath(candidate))
    if not is_contained(staging_root, candidate):
        _fail("IMPORT_OCR_POLICY_BLOCKED")
    try:
        status = candidate.lstat()
    except OSError:
        _fail("IMPORT_OCR_INVALID_IMAGE")
    if candidate.is_symlink() or not candidate.is_file():
        _fail("IMPORT_OCR_INVALID_IMAGE")
    image_path = candidate.resolve(strict=True)
    if not is_contained(staging_root, image_path):
        _fail("IMPORT_OCR_POLICY_BLOCKED")
    if image_path.suffix.lower() not in IMAGE_EXTENSIONS:
        _fail("IMPORT_OCR_UNSUPPORTED_IMAGE")
    if status.st_size <= 0 or status.st_size > MAX_IMAGE_BYTES:
        _fail("IMPORT_OCR_IMAGE_TOO_LARGE")
    return project_root, staging_root, image_path


def sha256_file(file_path: Path) -> str:
    digest = hashlib.sha256()
    with file_path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def validate_image_geometry(width: Any, height: Any, frames: Any) -> None:
    if any(isinstance(value, bool) or not isinstance(value, int) for value in (width, height, frames)):
        _fail("IMPORT_OCR_INVALID_IMAGE")
    if width <= 0 or height <= 0 or frames <= 0:
        _fail("IMPORT_OCR_INVALID_IMAGE")
    if width > MAX_IMAGE_DIMENSION or height > MAX_IMAGE_DIMENSION:
        _fail("IMPORT_OCR_IMAGE_TOO_LARGE")
    if width * height > MAX_IMAGE_PIXELS or frames > MAX_IMAGE_FRAMES:
        _fail("IMPORT_OCR_IMAGE_TOO_LARGE")


def verify_signed_file(pack_root_value: Path, manifest: Any, relative_path: str) -> Path:
    if not isinstance(manifest, dict) or not isinstance(manifest.get("files"), list):
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    declarations = [item for item in manifest["files"] if isinstance(item, dict) and item.get("path") == relative_path]
    if len(declarations) != 1:
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    declaration = declarations[0]
    digest = declaration.get("sha256")
    byte_count = declaration.get("bytes")
    if not isinstance(digest, str) or len(digest) != 64 or any(char not in "0123456789abcdef" for char in digest):
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    if not isinstance(byte_count, int) or isinstance(byte_count, bool) or byte_count <= 0:
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")

    pack_root = pack_root_value.resolve(strict=True)
    candidate = Path(os.path.abspath(pack_root / relative_path))
    if not is_contained(pack_root, candidate):
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    try:
        status = candidate.lstat()
    except OSError:
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    if candidate.is_symlink() or not candidate.is_file() or status.st_size != byte_count:
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    resolved = candidate.resolve(strict=True)
    if not is_contained(pack_root, resolved) or sha256_file(resolved) != digest:
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")

    upstream = MODEL_DECLARATIONS.get(relative_path)
    if upstream is not None and upstream != (byte_count, digest):
        _fail("IMPORT_OCR_ENGINE_INTEGRITY_FAILED")
    return resolved


def clean_text(value: Any) -> str:
    if not isinstance(value, str):
        return ""
    characters = []
    for character in value:
        code_point = ord(character)
        characters.append(" " if code_point == 0x7F or (code_point < 0x20 and character not in "\t\n\r") else character)
    return " ".join("".join(characters).split())[:MAX_TEXT_CHARACTERS]


def _number(value: Any) -> float:
    try:
        parsed = float(value)
    except (TypeError, ValueError):
        _fail("IMPORT_OCR_OUTPUT_INVALID")
    if not math.isfinite(parsed):
        _fail("IMPORT_OCR_OUTPUT_INVALID")
    return parsed


def normalize_blocks(
    boxes: Any,
    texts: Any,
    scores: Any,
    image_width: int,
    image_height: int,
) -> list[dict[str, Any]]:
    if boxes is None and texts is None and scores is None:
        return []
    try:
        box_values = boxes.tolist() if hasattr(boxes, "tolist") else list(boxes)
        text_values = list(texts)
        score_values = list(scores)
    except (TypeError, ValueError):
        _fail("IMPORT_OCR_OUTPUT_INVALID")
    if not (len(box_values) == len(text_values) == len(score_values)) or len(box_values) > MAX_BLOCKS:
        _fail("IMPORT_OCR_OUTPUT_INVALID")
    if not isinstance(image_width, int) or not isinstance(image_height, int) or image_width <= 0 or image_height <= 0:
        _fail("IMPORT_OCR_OUTPUT_INVALID")

    result = []
    for box, text, score in zip(box_values, text_values, score_values):
        cleaned = clean_text(text)
        confidence = _number(score)
        if not cleaned or confidence < 0 or confidence > 1:
            _fail("IMPORT_OCR_OUTPUT_INVALID")
        try:
            points = list(box)
            coordinates = [list(point) for point in points]
        except (TypeError, ValueError):
            _fail("IMPORT_OCR_OUTPUT_INVALID")
        if len(coordinates) != 4 or any(len(point) != 2 for point in coordinates):
            _fail("IMPORT_OCR_OUTPUT_INVALID")
        xs = [_number(point[0]) for point in coordinates]
        ys = [_number(point[1]) for point in coordinates]
        left = max(0, min(image_width, math.floor(min(xs))))
        top = max(0, min(image_height, math.floor(min(ys))))
        right = max(0, min(image_width, math.ceil(max(xs))))
        bottom = max(0, min(image_height, math.ceil(max(ys))))
        if right <= left or bottom <= top:
            _fail("IMPORT_OCR_OUTPUT_INVALID")
        result.append({
            "text": cleaned,
            "confidence": round(confidence, 6),
            "coordinates": {
                "x": left,
                "y": top,
                "width": right - left,
                "height": bottom - top,
            },
            "tableCell": None,
        })
    return result


def mean_confidence(blocks: list[dict[str, Any]]) -> float:
    if not blocks:
        return 0.0
    return round(sum(block["confidence"] for block in blocks) / len(blocks), 6)


def markdown_escape(value: str) -> str:
    escaped = html.escape(value, quote=False)
    for character in ("\\", "`", "*", "_", "{", "}", "[", "]", "<", ">", "#", "+", "-", ".", "!", "|"):
        escaped = escaped.replace(character, f"\\{character}")
    return escaped


def render_markdown(source_name: str, blocks: list[dict[str, Any]], confidence: float) -> str:
    lines = [
        "---",
        f'engine: "{ENGINE_ID}"',
        f'engine_version: "{ENGINE_VERSION}"',
        f'model: "{MODEL_VERSION}"',
        'provider: "cpu"',
        f'source: "{clean_text(source_name).replace(chr(34), chr(39))}"',
        'provenance: "authorized-local-ocr"',
        "---",
        "",
        "# Local OCR evidence",
        "",
        "> Machine-extracted text from an authorized local image. This is evidence, not the author's original Markdown body.",
        "",
        f"Mean confidence: {confidence:.3f}",
        "",
    ]
    if not blocks:
        lines.extend(["No text was detected.", ""])
        return "\n".join(lines)
    for index, block in enumerate(blocks, start=1):
        coordinates = block["coordinates"]
        lines.append(
            f"{index}. {markdown_escape(block['text'])} "
            f"(confidence {block['confidence']:.3f}; "
            f"x={coordinates['x']}, y={coordinates['y']}, "
            f"w={coordinates['width']}, h={coordinates['height']})"
        )
    lines.append("")
    return "\n".join(lines)
