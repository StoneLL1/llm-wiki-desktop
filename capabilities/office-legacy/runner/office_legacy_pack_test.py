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


def legacy_bytes(payload=b"legacy-original"):
    return RUNNER.OLE_COMPOUND_FILE_MAGIC + payload


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


def chained_request_for(project, staging, source):
    request = request_for(project, staging, source)
    request["params"]["input"]["locator"] = source
    request["params"]["chainedInput"] = source
    return request


def write_docx(_executable, source, output_dir, _profile, _timeout_seconds):
    converted = output_dir / (source.stem + ".docx")
    with zipfile.ZipFile(converted, "w") as archive:
        archive.writestr("[Content_Types].xml", "<Types/>")
        archive.writestr("word/document.xml", "<w:document/>")
    return b"", b""


class OfficeLegacyPackRetryTests(unittest.TestCase):
    def test_chained_input_is_resolved_relative_to_staging(self):
        with tempfile.TemporaryDirectory(prefix="office-legacy-chained-") as temporary:
            project = Path(temporary)
            staging = project / "staging"
            staging.mkdir()
            source = staging / "legacy.doc"
            executable = project / "soffice"
            source.write_bytes(legacy_bytes())
            executable.write_bytes(b"fixture")
            native = tempfile.TemporaryDirectory(
                prefix="native-office-", dir=temporary
            )

            with (
                patch.dict(os.environ, {"LLM_WIKI_LIBREOFFICE": str(executable)}),
                patch.object(RUNNER, "execute_libreoffice", side_effect=write_docx),
                patch.object(
                    RUNNER,
                    "short_native_temporary_directory",
                    return_value=native,
                ),
            ):
                response = RUNNER.handle(
                    chained_request_for(project, staging, "legacy.doc")
                )

            self.assertIsNone(response["error"])
            self.assertTrue((staging / "source.bin").is_file())

    def test_chained_input_cannot_escape_staging(self):
        with tempfile.TemporaryDirectory(prefix="office-legacy-chained-policy-") as temporary:
            project = Path(temporary)
            staging = project / "staging"
            staging.mkdir()
            source = project / "outside.doc"
            source.write_bytes(legacy_bytes())

            response = RUNNER.handle(
                chained_request_for(project, staging, source)
            )

            self.assertIsNotNone(response["error"])
            self.assertEqual(response["error"]["code"], -32602)

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
            original = legacy_bytes()
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
            source.write_bytes(legacy_bytes())
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

    def test_corrupt_legacy_input_is_rejected_before_libreoffice(self):
        with tempfile.TemporaryDirectory(prefix="office-legacy-corrupt-") as temporary:
            project = Path(temporary)
            staging = project / "staging"
            source = project / "corrupt.doc"
            executable = project / "soffice"
            source.write_bytes(b"\x00\xff\x00\xff")
            executable.write_bytes(b"fixture")

            with (
                patch.dict(os.environ, {"LLM_WIKI_LIBREOFFICE": str(executable)}),
                patch.object(RUNNER, "execute_libreoffice") as execute,
            ):
                response = RUNNER.handle(request_for(project, staging, source))

            self.assertEqual(response["error"]["code"], -32010)
            execute.assert_not_called()


if __name__ == "__main__":
    unittest.main()
