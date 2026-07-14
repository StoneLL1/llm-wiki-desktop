#!/usr/bin/env python3
"""Isolated JSON-RPC adapter for a release-bundled LibreOffice executable.

The release pack supplies LibreOffice; this runner never downloads or installs it.
The legacy original remains source.bin and converted OOXML is only a cache asset.
"""
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path
from urllib.parse import quote

PACK_VERSION = "24.2.7"
TARGETS = {".doc": ("docx", "word/document.xml"),
           ".xls": ("xlsx", "xl/workbook.xml"),
           ".ppt": ("pptx", "ppt/presentation.xml")}
WARNING_BY_EXTENSION = {
    ".doc": ["LEGACY_OFFICE_OLE_MAY_BE_LOST", "LEGACY_OFFICE_ACTIVEX_REMOVED",
             "LEGACY_OFFICE_UNIT_COUNT_REQUIRES_MODERN_ROUTE_COMPARISON"],
    ".xls": ["LEGACY_OFFICE_OLE_MAY_BE_LOST", "LEGACY_OFFICE_ACTIVEX_REMOVED",
             "LEGACY_OFFICE_UNIT_COUNT_REQUIRES_MODERN_ROUTE_COMPARISON"],
    ".ppt": ["LEGACY_OFFICE_OLE_MAY_BE_LOST", "LEGACY_OFFICE_ACTIVEX_REMOVED",
             "LEGACY_OFFICE_ANIMATION_MAY_BE_LOST",
             "LEGACY_OFFICE_UNIT_COUNT_REQUIRES_MODERN_ROUTE_COMPARISON"],
}


def contained(root, path):
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def fail(request_id, code, message):
    return {"jsonrpc": "2.0", "id": request_id, "result": None,
            "error": {"code": code, "message": message, "data": None}}


def profile_uri(profile):
    # LibreOffice requires an absolute RFC 8089 file URL, including quoted CJK paths.
    return "file://" + quote(profile.resolve().as_posix(), safe="/:~")


def write_locked_profile(profile):
    user = profile / "user"
    user.mkdir(parents=True)
    # The disposable profile cannot inherit the user's extensions or preferences.
    registry = """<?xml version="1.0" encoding="UTF-8"?>
<oor:items xmlns:oor="http://openoffice.org/2001/registry">
 <item oor:path="/org.openoffice.Office.Common/Security/Scripting/MacroSecurityLevel"><prop oor:name="Value"><value>3</value></prop></item>
 <item oor:path="/org.openoffice.Office.ExtensionManager"><prop oor:name="DisablePlugins"><value>true</value></prop></item>
 <item oor:path="/org.openoffice.Office.Jobs/Jobs/UpdateCheck"><prop oor:name="UpdateCheck"><value>false</value></prop></item>
</oor:items>"""
    (user / "registrymodifications.xcu").write_text(registry, encoding="utf-8")


def kill_process_tree(process):
    if process.poll() is not None:
        return
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"],
                       shell=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                       timeout=5, check=False)
    else:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    try:
        process.kill()
        process.wait(timeout=5)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        pass


def validate_ooxml(path, expected_part):
    if path.read_bytes()[:4] != b"PK\x03\x04":
        raise ValueError("converted output is not OOXML ZIP magic")
    # Reopen validation catches output that only survived one central-directory read.
    for _ in range(2):
        with zipfile.ZipFile(path, "r") as archive:
            bad = archive.testzip()
            names = set(archive.namelist())
            if bad or "[Content_Types].xml" not in names or expected_part not in names:
                raise ValueError("converted OOXML failed reopen/content-type validation")
            types = archive.read("[Content_Types].xml")
            if b"macroEnabled" in types or b"vbaProject" in types:
                raise ValueError("converted OOXML contains active macro content")
    with zipfile.ZipFile(path, "r") as archive:
        names = archive.namelist()
        if expected_part.startswith("word/"):
            count = 1
        elif expected_part.startswith("xl/"):
            count = sum(n.startswith("xl/worksheets/sheet") and n.endswith(".xml") for n in names)
        else:
            count = sum(n.startswith("ppt/slides/slide") and n.endswith(".xml") for n in names)
    if count < 1:
        raise ValueError("converted OOXML has no pages, sheets, or slides")
    return count


def execute_libreoffice(executable, source, output_dir, profile, timeout_seconds):
    extension, _ = TARGETS[source.suffix.lower()]
    args = [str(executable), "--headless", "--invisible", "--nologo", "--nodefault",
            "--nolockcheck", "--norestore", "--convert-to", extension,
            "--outdir", str(output_dir),
            "-env:UserInstallation=file://" + profile_uri(profile).removeprefix("file://"),
            str(source)]
    env = {"PATH": os.environ.get("PATH", ""), "HOME": str(profile),
           "USERPROFILE": str(profile), "TMPDIR": str(profile / "tmp"),
           "HTTP_PROXY": "", "HTTPS_PROXY": "", "ALL_PROXY": "", "NO_PROXY": "*"}
    process = subprocess.Popen(args, shell=False, stdin=subprocess.DEVNULL,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                               env=env, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired:
        kill_process_tree(process)
        raise TimeoutError("LibreOffice conversion timed out")
    if process.returncode != 0:
        raise RuntimeError("LibreOffice conversion failed")
    return stdout, stderr


def handle(request):
    request_id = str(request.get("id", ""))
    if request.get("jsonrpc") != "2.0" or request.get("method") != "import.execute":
        return fail(request_id, -32600, "invalid request")
    params = request.get("params", {})
    if params.get("protocolVersion") != "2":
        return fail(request_id, -32602, "unsupported protocol version")
    project = Path(params.get("projectRoot", "")).resolve()
    staging = Path(params.get("stagingRoot", ""))
    source = Path(params.get("input", {}).get("locator", ""))
    if not staging.is_absolute():
        staging = project / staging
    if not source.is_absolute():
        source = project / source
    suffix = source.suffix.lower()
    if not source.is_file() or not contained(project, staging) or suffix not in TARGETS:
        return fail(request_id, -32602, "unauthorized or unsupported source")
    executable = os.environ.get("LLM_WIKI_LIBREOFFICE")
    if not executable or not Path(executable).is_file():
        return fail(request_id, -32020, "office-legacy capability is not installed")
    try:
        staging.mkdir(parents=True, exist_ok=True)
        converted_dir = staging / "converted"
        converted_dir.mkdir()
        with tempfile.TemporaryDirectory(prefix="llm-wiki-lo-profile-") as temporary:
            profile = Path(temporary)
            write_locked_profile(profile)
            execute_libreoffice(Path(executable), source, converted_dir, profile, 120)
        target_extension, expected_part = TARGETS[suffix]
        converted = converted_dir / (source.stem + "." + target_extension)
        unit_count = validate_ooxml(converted, expected_part)
        shutil.copyfile(source, staging / "source.bin")
        (staging / "candidate.md").write_text(
            "# " + source.stem + "\n\nLegacy Office conversion completed; modern Office extraction is pending.\n",
            encoding="utf-8", newline="\n")
        metadata = {"engineId": "pack.office-legacy", "engineVersion": PACK_VERSION,
                    "route": "pack.office-legacy", "convertedCacheArtifact": "converted/" + converted.name,
                    "convertedFormat": target_extension, "unitCount": unit_count,
                    "sourceSnapshot": "source.bin", "activeContentExecuted": False}
        (staging / "metadata.json").write_text(json.dumps(metadata, ensure_ascii=False), encoding="utf-8")
        result = {"sourceSnapshotPath": "source.bin", "markdownPath": "candidate.md",
                  "assetPaths": ["converted/" + converted.name], "metadataPath": "metadata.json",
                  "title": source.stem, "textCoverage": None, "tableCellAccuracy": None,
                  "warnings": WARNING_BY_EXTENSION[suffix]}
        return {"jsonrpc": "2.0", "id": request_id, "result": result, "error": None}
    except (OSError, RuntimeError, TimeoutError, ValueError, zipfile.BadZipFile) as error:
        return fail(request_id, -32010, "conversion failed: " + type(error).__name__)


def main():
    try:
        response = handle(json.loads(sys.stdin.readline()))
    except (ValueError, TypeError, json.JSONDecodeError):
        response = fail("", -32700, "parse error")
    sys.stdout.write(json.dumps(response, separators=(",", ":")) + "\n")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
