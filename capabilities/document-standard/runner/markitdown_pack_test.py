import hashlib
import importlib.util
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import sys

spec = importlib.util.spec_from_file_location("runner", Path(__file__).with_name("markitdown_pack.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)

class SnapshotTests(unittest.TestCase):
    def test_chained_conversion_preserves_original_and_rejects_missing_or_changed_snapshot(self):
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory)
            staging = project / "staging"
            staging.mkdir()
            original = b"legacy original bytes"
            (project / "原件.doc").write_bytes(original)
            (staging / "converted.docx").write_bytes(b"converted OOXML")
            request = {"jsonrpc": "2.0", "id": 1, "method": "import.execute", "params": {
                "protocolVersion": "2", "projectRoot": str(project), "stagingRoot": str(staging),
                "input": {"locator": str(project / "原件.doc"), "sourceIdentity": {
                    "sha256": hashlib.sha256(original).hexdigest(), "sizeBytes": len(original)}},
                "chainedInput": "converted.docx"}}
            fake = SimpleNamespace(MarkItDown=lambda **kw: SimpleNamespace(convert=lambda path: SimpleNamespace(text_content="# Converted\n\nFaithful text.")))
            with patch.dict(sys.modules, {"markitdown": fake}):
                for snapshot in [None, b"wrong original", original]:
                    if snapshot is not None:
                        (staging / "source.bin").write_bytes(snapshot)
                    result = runner.handle(request)
                    if snapshot != original:
                        self.assertIsNotNone(result["error"])
                    else:
                        self.assertIsNone(result["error"])
                        self.assertEqual((staging / "source.bin").read_bytes(), original)
                        self.assertIn("Faithful text", (staging / "document.md").read_text())

if __name__ == "__main__":
    unittest.main()
