use super::*;

fn fixture(root: &Path) -> (EngineRequest, EngineResult) {
    std::fs::create_dir_all(root.join(".ocr-input-test")).unwrap();
    std::fs::write(
        root.join(".ocr-input-test/frame-at-30000.png"),
        include_bytes!("../../../../tests/fixtures/import-v2/local/batch3/matrix/image.png"),
    )
    .unwrap();
    std::fs::write(root.join("source.json"), b"{}").unwrap();
    std::fs::write(root.join("candidate.md"), b"# Video").unwrap();
    let request = serde_json::from_value(serde_json::json!({
        "protocolVersion":"2", "requestId":"r", "sessionId":"s", "itemId":"i", "taskId":"t",
        "operation":"extract", "projectRoot":root, "stagingRoot":".", "localOcrAuthorized":true,
        "input":{"kind":"file", "displayName":"movie.mp4", "locator":"movie.mp4"}
    }))
    .unwrap();
    let result = serde_json::from_value(serde_json::json!({
        "sourceSnapshotPath":"source.json", "markdownPath":"candidate.md", "assetPaths":[],
        "title":"Video", "textCoverage":null, "tableCellAccuracy":null, "warnings":[]
    }))
    .unwrap();
    (request, result)
}
fn evidence(path: &str) -> VideoFrameEvidence {
    VideoFrameEvidence {
        frames: vec![VideoFrame {
            path: path.into(),
            timestamp_ms: 30000,
        }],
        duration_ms: 40000,
        sampled_frame_count: 180,
    }
}

#[test]
fn video_frame_adapter_preserves_images_timestamps_and_authority() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let (mut request, mut result) = fixture(root);
    request.local_ocr_authorized = false;
    assert!(adapt_video_frames(
        &request,
        &mut result,
        Some(evidence(".ocr-input-test/frame-at-30000.png")),
        "media-runtime",
        "media.keyframes"
    )
    .is_err());
    request.local_ocr_authorized = true;
    assert!(adapt_video_frames(
        &request,
        &mut result,
        Some(evidence(".ocr-input-test/frame-at-30000.png")),
        "browser-runtime",
        "media.keyframes"
    )
    .is_err());
    for bad in [
        "../frame-at-30000.png",
        ".ocr-input-test/nested/frame-at-30000.png",
        ".media-output-test/frame-at-30000.png",
        ".ocr-input-test\\escape/frame-at-30000.png",
    ] {
        assert!(adapt_video_frames(
            &request,
            &mut result,
            Some(evidence(bad)),
            "media-runtime",
            "media.keyframes"
        )
        .is_err());
    }
    adapt_video_frames(
        &request,
        &mut result,
        Some(evidence(".ocr-input-test/frame-at-30000.png")),
        "media-runtime",
        "media.keyframes",
    )
    .unwrap();
    assert_eq!(
        result.text_coverage,
        Some(0.0),
        "frames alone are not readable content"
    );
    assert!(matches!(
        result.continuation,
        Some(super::super::engine::EngineContinuation::LocalOcr { .. })
    ));
    let markdown = std::fs::read_to_string(root.join(&result.markdown_path)).unwrap();
    assert!(
        markdown.contains("[00:00:30.000]") && markdown.contains("OCR_IMAGE_001"),
        "{markdown}"
    );
    assert!(result
        .asset_paths
        .iter()
        .all(|path| root.join(path).is_file()));
}

#[cfg(unix)]
#[test]
fn video_frame_adapter_rejects_symlinked_workspaces_and_mismatched_time() {
    let temporary = tempfile::tempdir().unwrap();
    let (request, mut result) = fixture(temporary.path());
    std::os::unix::fs::symlink(
        temporary.path().join(".ocr-input-test"),
        temporary.path().join(".ocr-input-link"),
    )
    .unwrap();
    assert!(adapt_video_frames(
        &request,
        &mut result,
        Some(evidence(".ocr-input-link/frame-at-30000.png")),
        "media-runtime",
        "media.keyframes"
    )
    .is_err());
    let mut wrong = evidence(".ocr-input-test/frame-at-30000.png");
    wrong.frames[0].timestamp_ms = 50000;
    assert!(adapt_video_frames(
        &request,
        &mut result,
        Some(wrong),
        "media-runtime",
        "media.keyframes"
    )
    .is_err());
}

#[test]
fn video_evidence_rpc_is_bounded_and_ocr_required_is_a_stable_error() {
    let result = serde_json::json!({ "sourceSnapshotPath":"source.json", "markdownPath":"candidate.md", "assetPaths":[], "title":"Video", "warnings":[], "videoFrames":{"durationMs":40000,"sampledFrameCount":180,"frames":[{"path":".ocr-input-test/frame-at-30000.png","timestampMs":30000}]} });
    let bytes = format!(
        "{}\n",
        serde_json::json!({"jsonrpc":"2.0","id":"r","result":result,"error":null})
    );
    let parsed = read_response(std::io::Cursor::new(bytes)).unwrap();
    assert!(parsed.rpc.result.unwrap().continuation.is_none());
    assert_eq!(parsed.video_frames.unwrap().frames[0].timestamp_ms, 30000);
    assert_eq!(
        stable_capability_error_code(Some("IMPORT_VIDEO_FRAME_OCR_REQUIRED")),
        "IMPORT_VIDEO_FRAME_OCR_REQUIRED"
    );
}

#[test]
fn old_video_resources_require_update_only_for_authorized_frame_recognition() {
    let temporary = tempfile::tempdir().unwrap();
    let (mut request, _) = fixture(temporary.path());
    assert!(video_frame_resource_update_required(
        "asr-sensevoice-small",
        "1.13.4+2024.07.17.resources.2",
        "media.asr",
        &request
    ));
    assert!(!video_frame_resource_update_required(
        "asr-sensevoice-small",
        "1.13.4+2024.07.17.resources.3",
        "media.asr",
        &request
    ));
    assert!(video_frame_resource_update_required(
        "media-runtime",
        "8.1.2+resources.2",
        "media.keyframes",
        &request
    ));
    assert!(!video_frame_resource_update_required(
        "asr-whisper",
        "1.8.3+resources.2",
        "media.asr",
        &request
    ));
    for (pack, version, route) in [
        ("asr-sensevoice-small", "1.14.0", "media.asr"),
        ("asr-whisper", "1.9.0", "media.asr"),
        ("media-runtime", "8.2.0", "media.keyframes"),
    ] {
        assert!(!video_frame_resource_update_required(
            pack, version, route, &request
        ));
    }
    request.local_ocr_authorized = false;
    assert!(!video_frame_resource_update_required(
        "asr-sensevoice-small",
        "1.13.4+2024.07.17",
        "media.asr",
        &request
    ));
    request.local_ocr_authorized = true;
    request.asr_probe_only = true;
    assert!(!video_frame_resource_update_required(
        "asr-sensevoice-small",
        "1.13.4+2024.07.17",
        "media.asr",
        &request
    ));
}
