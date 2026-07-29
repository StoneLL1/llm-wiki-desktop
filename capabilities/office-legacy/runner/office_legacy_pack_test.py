import importlib.util
import os
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import MagicMock, patch


RUNNER_PATH = Path(__file__).with_name("office_legacy_pack.py")
SPEC = importlib.util.spec_from_file_location("office_legacy_pack", RUNNER_PATH)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


def request_for(project, staging, source):
    return {
        "jsonrpc": "2.0",
        "id": "retry-fixture",
        "method": "import.execute",
        "params": {
            "protocolVersion": "2",
            "projectRoot": str(project),
            "stagingRoot": str(staging),
            "input": {"locator": str(source)},
        },
    }


def write_docx(_executable, source, output_dir, _profile, _timeout_seconds):
    converted = output_dir / (source.stem + ".docx")
    with zipfile.ZipFile(converted, "w") as archive:
        archive.writestr("[Content_Types].xml", "<Types/>")
        archive.writestr("word/document.xml", "<w:document/>")
    return b"", b""


class OfficeLegacyPackRetryTests(unittest.TestCase):
    def test_libreoffice_profile_argument_uses_rfc8089_file_uri(self):
        profile = RUNNER_PATH.parents[3] / ".superpowers" / "配置 profile"
        process = MagicMock()
        process.communicate.return_value = (b"", b"")
        process.returncode = 0

        with patch.object(RUNNER.subprocess, "Popen", return_value=process) as popen:
            RUNNER.execute_libreoffice(
                Path("soffice"),
                Path("旧 文档.doc"),
                Path("converted"),
                profile,
                1,
            )

        arguments = popen.call_args.args[0]
        profile_argument = next(
            value for value in arguments if value.startswith("-env:UserInstallation=")
        )
        expected = "-env:UserInstallation=" + profile.resolve().as_uri()
        self.assertEqual(profile_argument, expected)
        self.assertTrue(profile_argument.startswith("-env:UserInstallation=file:///"))
        self.assertNotIn(" ", profile_argument)
        self.assertNotIn("配置", profile_argument)

    def test_retry_replaces_stale_conversion_and_preserves_original_bytes(self):
        with tempfile.TemporaryDirectory(prefix="office-legacy-retry-") as temporary:
            project = Path(temporary)
            source = project / "旧文档.doc"
            staging = project / ".app" / "import" / "item"
            executable = project / "soffice"
            original = b"\xd0\xcf\x11\xe0legacy-original"
            source.write_bytes(original)
            executable.write_bytes(b"fixture")
            request = request_for(project, staging, source)

            with (
                patch.dict(os.environ, {"LLM_WIKI_LIBREOFFICE": str(executable)}),
                patch.object(RUNNER, "execute_libreoffice", side_effect=write_docx),
            ):
                first = RUNNER.handle(request)
                converted = staging / "converted" / "旧文档.docx"
                converted.write_bytes(b"stale-invalid-cache")
                second = RUNNER.handle(request)

            self.assertIsNone(first["error"])
            self.assertIsNone(second["error"])
            self.assertEqual((staging / "source.bin").read_bytes(), original)
            self.assertEqual(RUNNER.validate_ooxml(converted, "word/document.xml"), 1)

    def test_libreoffice_receives_only_short_system_temp_work_paths(self):
        with tempfile.TemporaryDirectory(prefix="office-legacy-long-project-") as temporary:
            project = (
                Path(temporary)
                / ("p" * 96)
                / ("q" * 96)
                / ".app"
                / "import-sessions"
                / ("s" * 36)
                / "items"
                / ("i" * 36)
            )
            project.mkdir(parents=True)
            source = project / "旧文档.doc"
            staging = project / "staging"
            executable = project / "soffice"
            source.write_bytes(b"\xd0\xcf\x11\xe0legacy-original")
            self.assertGreater(len(str(source)), 260)
            executable.write_bytes(b"fixture")
            observed = {}

            def write_short_docx(_executable, native_source, output_dir, profile, _timeout_seconds):
                observed.update(source=native_source, output=output_dir, profile=profile)
                return write_docx(_executable, native_source, output_dir, profile, _timeout_seconds)

            with (
                patch.dict(os.environ, {"LLM_WIKI_LIBREOFFICE": str(executable)}),
                patch.object(RUNNER, "execute_libreoffice", side_effect=write_short_docx),
            ):
                response = RUNNER.handle(request_for(project, staging, source))

            self.assertIsNone(response["error"])
            self.assertEqual(observed["source"].name, "input.doc")
            self.assertEqual(observed["output"].name, "converted")
            self.assertEqual(observed["source"].parent, observed["profile"])
            self.assertFalse(observed["source"].is_relative_to(project))
            self.assertLessEqual(len(str(observed["source"].resolve())), 220)
            self.assertTrue((staging / "converted" / "旧文档.docx").is_file())


if __name__ == "__main__":
    unittest.main()
