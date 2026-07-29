from __future__ import annotations

from pathlib import Path
import unittest

from core import (
    OcrPolicyError,
    mean_confidence,
    native_tool_path,
    normalize_blocks,
    render_markdown,
    sha256_file,
    validate_image_geometry,
    verify_signed_file,
)


class OcrCoreTests(unittest.TestCase):
    def test_native_tool_paths_use_the_windows_extended_namespace(self):
        self.assertEqual(
            native_tool_path(Path(r"C:\deep\input.png"), "nt"),
            r"\\?\C:\deep\input.png",
        )
        self.assertEqual(
            native_tool_path(Path(r"\\server\share\input.png"), "nt"),
            r"\\?\UNC\server\share\input.png",
        )
        self.assertEqual(
            native_tool_path(Path("//server/share/input.png"), "nt"),
            r"\\?\UNC\server\share\input.png",
        )
        self.assertEqual(
            native_tool_path(Path(r"\\server\share\..\other\input.png"), "nt"),
            r"\\?\UNC\server\share\other\input.png",
        )
        self.assertEqual(
            native_tool_path(Path(r"\\.\pipe\runner"), "nt"),
            r"\\.\pipe\runner",
        )
        self.assertEqual(
            native_tool_path(Path(r"\root-relative.png"), "nt"),
            r"\root-relative.png",
        )
        self.assertEqual(
            native_tool_path(Path(r"C:drive-relative.png"), "nt"),
            r"C:drive-relative.png",
        )
        self.assertEqual(native_tool_path(Path("relative.png"), "nt"), "relative.png")
        self.assertEqual(
            native_tool_path(Path(r"C:\deep\input.png"), "posix"),
            r"C:\deep\input.png",
        )

    def test_rejects_oversized_or_multiframe_images_before_decode(self):
        validate_image_geometry(4096, 4096, 1)
        for geometry in ((16385, 1, 1), (9000, 9000, 1), (100, 100, 2)):
            with self.assertRaises(OcrPolicyError) as raised:
                validate_image_geometry(*geometry)
            self.assertEqual(raised.exception.code, "IMPORT_OCR_IMAGE_TOO_LARGE")

    def test_normalizes_quadrilaterals_and_confidence(self):
        blocks = normalize_blocks(
            [[[1.2, 2.7], [10.1, 2.0], [11.8, 8.4], [0.2, 9.9]]],
            [" 中文\x00 text "],
            [0.98765432],
            20,
            20,
        )
        self.assertEqual(blocks[0]["text"], "中文 text")
        self.assertEqual(blocks[0]["coordinates"], {"x": 0, "y": 2, "width": 12, "height": 8})
        self.assertEqual(blocks[0]["confidence"], 0.987654)
        self.assertEqual(mean_confidence(blocks), 0.987654)

    def test_rejects_invalid_or_misaligned_output(self):
        with self.assertRaisesRegex(OcrPolicyError, "IMPORT_OCR_OUTPUT_INVALID"):
            normalize_blocks([], ["text"], [0.5], 10, 10)
        with self.assertRaisesRegex(OcrPolicyError, "IMPORT_OCR_OUTPUT_INVALID"):
            normalize_blocks([[[0, 0], [1, 0], [1, 1]]], ["text"], [0.5], 10, 10)
        with self.assertRaisesRegex(OcrPolicyError, "IMPORT_OCR_OUTPUT_INVALID"):
            normalize_blocks([[[0, 0], [1, 0], [1, 1], [0, 1]]], ["text"], [1.1], 10, 10)

    def test_signed_inventory_is_required(self):
        root = Path(__file__).resolve().parent
        file_path = root / "core.py"
        manifest = {"files": [{
            "path": "core.py",
            "bytes": file_path.stat().st_size,
            "sha256": sha256_file(file_path),
        }]}
        self.assertEqual(verify_signed_file(root, manifest, "core.py"), file_path.resolve())
        manifest["files"][0]["sha256"] = "0" * 64
        with self.assertRaisesRegex(OcrPolicyError, "IMPORT_OCR_ENGINE_INTEGRITY_FAILED"):
            verify_signed_file(root, manifest, "core.py")

    def test_markdown_labels_machine_evidence_and_escapes_content(self):
        markdown = render_markdown("page.jpg", [{
            "text": "# heading | value",
            "confidence": 0.9,
            "coordinates": {"x": 1, "y": 2, "width": 3, "height": 4},
            "tableCell": None,
        }], 0.9)
        self.assertIn("Machine-extracted text", markdown)
        self.assertIn("\\# heading \\| value", markdown)
        self.assertNotIn("# heading | value", markdown)


if __name__ == "__main__":
    unittest.main()
