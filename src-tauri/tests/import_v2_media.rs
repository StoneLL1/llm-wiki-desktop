use std::{fs, fs::OpenOptions};

use llm_wiki_desktop_lib::models::import_v2::{
    ImportInput, ImportInputKind, MediaSaveMode, SourceIdentity,
};
use llm_wiki_desktop_lib::services::import_v2::engine::{
    EngineContinuation, EngineOperation, EngineRequest, ImportEngine,
};
use llm_wiki_desktop_lib::services::import_v2::file_discovery::FileDiscoveryService;
use llm_wiki_desktop_lib::services::import_v2::local_media_engine::NativeMediaCompanionEngine;
use llm_wiki_desktop_lib::services::import_v2::media_router::{
    recover_media_temp_root, render_timestamped_markdown, AsrModelCatalog, MediaArtifactPlan,
    MediaInput, MediaKind, MediaRouteStatus, MediaRouter, SubtitleCandidate, SubtitleKind,
    TemporaryMediaWorkspace, TranscriptSegment,
};
use llm_wiki_desktop_lib::tasks::task_model::CancellationToken;
use llm_wiki_desktop_lib::{models::import_v2_file::FileScanPolicy, models::paths::ProjectContext};
use sha2::{Digest, Sha256};

#[test]
fn subtitle_priority_avoids_asr_when_preferred_text_exists() {
    let router = MediaRouter::default();
    let input = MediaInput {
        kind: MediaKind::Video,
        subtitles: vec![
            SubtitleCandidate::new(SubtitleKind::Embedded, "embedded.vtt"),
            SubtitleCandidate::new(SubtitleKind::Automatic, "auto.vtt"),
            SubtitleCandidate::new(SubtitleKind::HumanLocal, "human.srt"),
        ],
        cover_path: Some("cover.jpg".into()),
    };

    let plan = router.plan(&input, true);
    assert_eq!(plan.subtitle.unwrap().path, "embedded.vtt");
    assert!(!plan.requires_asr);
    assert_eq!(
        plan.artifacts,
        MediaArtifactPlan::subtitle_markdown_cover_metadata()
    );
}

#[test]
fn subtitle_priority_is_platform_then_embedded_then_companion_then_asr() {
    let router = MediaRouter::default();
    for (subtitles, expected) in [
        (
            vec![
                SubtitleCandidate::new(SubtitleKind::Automatic, "a.vtt"),
                SubtitleCandidate::new(SubtitleKind::HumanLocal, "c.srt"),
                SubtitleCandidate::new(SubtitleKind::Embedded, "e.vtt"),
                SubtitleCandidate::new(SubtitleKind::HumanPlatform, "p.vtt"),
            ],
            Some("p.vtt"),
        ),
        (
            vec![
                SubtitleCandidate::new(SubtitleKind::Automatic, "a.vtt"),
                SubtitleCandidate::new(SubtitleKind::HumanLocal, "c.srt"),
                SubtitleCandidate::new(SubtitleKind::Embedded, "e.vtt"),
            ],
            Some("e.vtt"),
        ),
        (
            vec![
                SubtitleCandidate::new(SubtitleKind::Automatic, "a.vtt"),
                SubtitleCandidate::new(SubtitleKind::HumanLocal, "c.srt"),
            ],
            Some("c.srt"),
        ),
        (
            vec![SubtitleCandidate::new(SubtitleKind::Automatic, "a.vtt")],
            Some("a.vtt"),
        ),
        (vec![], None::<&str>),
    ] {
        let plan = router.plan(
            &MediaInput {
                kind: MediaKind::Audio,
                subtitles,
                cover_path: None,
            },
            true,
        );
        assert_eq!(
            plan.subtitle.as_ref().map(|value| value.path.as_str()),
            expected
        );
        assert_eq!(plan.requires_asr, expected.is_none());
    }
}

#[test]
fn model_catalog_requires_pinned_hash_and_signed_resumable_download() {
    let manifest = include_str!("../../capabilities/asr-whisper/manifest.json");
    let catalog = AsrModelCatalog::from_manifest_str(manifest).unwrap();
    let small = catalog.select("small").unwrap();
    assert_eq!(small.sha256.len(), 64);
    assert!(small.download.resumable);
    assert!(small.download.signature_required);
    assert!(!small.download.runtime_install_allowed);
}

#[test]
fn asr_runtime_requires_qualified_lgpl_m4a_decoding() {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../capabilities/asr-whisper/manifest.json")).unwrap();
    assert_eq!(
        value["audioDecoding"]["requiredBuildFeature"],
        "WHISPER_FFMPEG"
    );
    assert_eq!(
        value["audioDecoding"]["qualificationFixture"],
        "bilibili-m4a-to-transcript-v1"
    );
    assert!(value["licenseExpression"]
        .as_str()
        .unwrap()
        .contains("LGPL-2.1-or-later"));
    assert!(value["audioDecoding"]["componentInventory"]
        .as_array()
        .unwrap()
        .iter()
        .all(|component| component["gpl"] != true && component["nonfree"] != true));
}

#[test]
fn temporary_media_is_removed_on_success_failure_and_restart() {
    let root = tempfile::tempdir().unwrap();
    for outcome in [true, false] {
        let work = root
            .path()
            .join(if outcome { "success" } else { "failure" });
        {
            let workspace = TemporaryMediaWorkspace::create(&work).unwrap();
            fs::write(workspace.path().join("media.bin"), b"temporary").unwrap();
        }
        assert!(!work.exists());
    }
    let orphan = root.path().join("orphan");
    fs::create_dir_all(&orphan).unwrap();
    fs::write(orphan.join("media.bin"), b"temporary").unwrap();
    recover_media_temp_root(root.path()).unwrap();
    assert!(!orphan.exists());
}

#[test]
fn media_manifest_is_strict_lgpl_and_does_not_promise_installed_binaries() {
    let value: serde_json::Value = serde_json::from_str(include_str!(
        "../../capabilities/media-runtime/manifest.json"
    ))
    .unwrap();
    let flags = value["buildProvenance"]["configureFlags"]
        .as_array()
        .unwrap();
    assert!(flags.iter().any(|flag| flag == "--disable-gpl"));
    assert!(flags.iter().any(|flag| flag == "--disable-nonfree"));
    assert_eq!(value["targetTriples"].as_array().unwrap().len(), 0);
    assert!(
        value["buildProvenance"]["componentInventory"]
            .as_array()
            .unwrap()
            .len()
            > 0
    );
    assert!(value["buildProvenance"]["sourceRecipe"].is_string());
}

#[test]
fn raw_media_is_never_a_formal_artifact() {
    let plan = MediaRouter::default().plan(
        &MediaInput {
            kind: MediaKind::Video,
            subtitles: vec![],
            cover_path: None,
        },
        false,
    );
    assert!(!plan
        .artifacts
        .iter()
        .any(|artifact| artifact.contains("video")
            || artifact.contains("audio")
            || artifact.contains("raw/sources")));
}

#[test]
fn absent_whisper_is_reported_as_waiting_instead_of_success() {
    let plan = MediaRouter::default().plan(
        &MediaInput {
            kind: MediaKind::Audio,
            subtitles: vec![],
            cover_path: None,
        },
        false,
    );
    assert_eq!(plan.status, MediaRouteStatus::WaitingCapability);
    assert!(!plan.requires_asr);
}

#[test]
fn timestamped_markdown_records_provenance_and_language_confidence() {
    let markdown = render_timestamped_markdown(
        &[TranscriptSegment {
            start_ms: 1_250,
            end_ms: 3_500,
            text: " hello ".into(),
        }],
        "whisper.cpp-1.8.3",
        "ggml-small",
        "zh",
        0.925,
    );
    assert!(markdown.contains("engine: whisper.cpp-1.8.3"));
    assert!(markdown.contains("model: ggml-small"));
    assert!(markdown.contains("languageConfidence: 0.925"));
    assert!(markdown.contains("[00:00:01.250 --> 00:00:03.500] hello"));
}

#[test]
fn multiple_companion_subtitles_require_and_honor_an_explicit_selection() {
    let root = tempfile::tempdir().unwrap();
    let media = root.path().join("访谈.mp4");
    let media_bytes = b"\0\0\0\x18ftypisom\0\0\0\0isomiso2";
    fs::write(&media, media_bytes).unwrap();
    fs::write(
        root.path().join("访谈.en.srt"),
        b"1\n00:00:00,000 --> 00:00:02,000\nEnglish\n",
    )
    .unwrap();
    fs::write(
        root.path().join("访谈.zh-CN.srt"),
        "1\n00:00:00,000 --> 00:00:02,000\n中文内容\n".as_bytes(),
    )
    .unwrap();
    let mut request = EngineRequest {
        protocol_version: "2".into(),
        request_id: "subtitle-choice".into(),
        project_id: "project".into(),
        session_id: "session".into(),
        item_id: "item".into(),
        task_id: "task".into(),
        operation: EngineOperation::Extract,
        input: ImportInput {
            kind: ImportInputKind::File,
            display_name: "访谈.mp4".into(),
            locator: media.to_string_lossy().into_owned(),
            normalized_locator: None,
            source_identity: Some(SourceIdentity {
                canonical_path: media.canonicalize().unwrap().to_string_lossy().into_owned(),
                size_bytes: media_bytes.len() as u64,
                modified_nanos: None,
                file_id: None,
                sha256: format!("{:x}", Sha256::digest(media_bytes)),
                magic: format!("{:x}", Sha256::digest(media_bytes)),
            }),
            media_save_mode: MediaSaveMode::ExtractOnly,
        },
        project_root: root.path().to_string_lossy().into_owned(),
        staging_root: "staging".into(),
        chained_input: None,
        local_asr_authorized: false,
        asr_probe_only: false,
        asr_profile: None,
        recognition_language: None,
        selected_subtitle: None,
        local_ocr_authorized: false,
        media_save_mode: MediaSaveMode::ExtractOnly,
    };
    let token = CancellationToken::new();
    let engine = NativeMediaCompanionEngine;
    let ambiguous = engine.execute(&request, &token).unwrap_err();
    assert_eq!(ambiguous.code, "IMPORT_LOCAL_SUBTITLE_AMBIGUOUS");
    let details = ambiguous.details.unwrap();
    let candidates = details["subtitleCandidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    assert_eq!(candidates, vec!["访谈.en.srt", "访谈.zh-CN.srt"]);

    request.selected_subtitle = Some("访谈.zh-CN.srt".into());
    let result = engine.execute(&request, &token).unwrap();
    assert!(matches!(
        result.continuation,
        Some(EngineContinuation::LocalAsr { .. })
    ));
    let markdown = fs::read_to_string(
        root.path()
            .join("staging/transcripts/companion-fallback.md"),
    )
    .unwrap();
    assert!(markdown.contains("中文内容"));
    assert!(!markdown.contains("English"));
}

#[test]
fn media_larger_than_the_legacy_document_limit_streams_through_discovery_and_staging() {
    let project = tempfile::tempdir().unwrap();
    let sources = tempfile::tempdir().unwrap();
    let media = sources.path().join("long-interview.mp3");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&media)
        .unwrap();
    file.set_len(65 * 1024 * 1024).unwrap();
    drop(file);
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = OpenOptions::new().write(true).open(&media).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"ID3\x04\0\0\0\0\0\0").unwrap();
        file.sync_all().unwrap();
    }

    let context = ProjectContext::new("project", project.path().to_path_buf());
    let scan = FileDiscoveryService
        .scan(
            &context,
            std::slice::from_ref(&media),
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    assert_eq!(
        scan.files.len(),
        1,
        "large media must not inherit the 64 MiB document cap"
    );
    assert!(scan.skipped.is_empty());
    let discovered = scan.files.into_iter().next().unwrap();
    let request = EngineRequest {
        protocol_version: "2".into(),
        request_id: "large-media".into(),
        project_id: "project".into(),
        session_id: "session".into(),
        item_id: "item".into(),
        task_id: "task".into(),
        operation: EngineOperation::Extract,
        input: ImportInput {
            kind: ImportInputKind::File,
            display_name: discovered.display_name,
            locator: discovered.source_path,
            normalized_locator: None,
            source_identity: Some(discovered.source_identity),
            media_save_mode: MediaSaveMode::ExtractOnly,
        },
        project_root: project.path().to_string_lossy().into_owned(),
        staging_root: "staging".into(),
        chained_input: None,
        local_asr_authorized: false,
        asr_probe_only: false,
        asr_profile: None,
        recognition_language: None,
        selected_subtitle: None,
        local_ocr_authorized: false,
        media_save_mode: MediaSaveMode::ExtractOnly,
    };

    let result = NativeMediaCompanionEngine
        .execute(&request, &CancellationToken::new())
        .unwrap();
    assert!(matches!(
        result.continuation,
        Some(EngineContinuation::LocalAsr { .. })
    ));
    assert_eq!(
        fs::metadata(project.path().join("staging/source.bin"))
            .unwrap()
            .len(),
        65 * 1024 * 1024
    );
    fs::write(
        project.path().join("staging/.media-copy-crash.tmp"),
        b"interrupted",
    )
    .unwrap();
    NativeMediaCompanionEngine
        .execute(&request, &CancellationToken::new())
        .expect("a verified source.bin from a pre-activation crash must be reusable");
    assert!(!project
        .path()
        .join("staging/.media-copy-crash.tmp")
        .exists());
}

#[test]
fn video_larger_than_the_legacy_document_limit_is_discovered_by_bounded_header() {
    let project = tempfile::tempdir().unwrap();
    let sources = tempfile::tempdir().unwrap();
    let media = sources.path().join("long-recording.mp4");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&media)
        .unwrap();
    file.set_len(65 * 1024 * 1024).unwrap();
    drop(file);
    {
        use std::io::{Seek, SeekFrom, Write};
        let mut file = OpenOptions::new().write(true).open(&media).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"\0\0\0\x18ftypisom\0\0\0\0isomiso2")
            .unwrap();
        file.sync_all().unwrap();
    }
    let context = ProjectContext::new("project", project.path().to_path_buf());
    let scan = FileDiscoveryService
        .scan(
            &context,
            std::slice::from_ref(&media),
            FileScanPolicy::default(),
            |_| {},
            || false,
        )
        .unwrap();
    assert_eq!(scan.files.len(), 1);
    assert_eq!(
        scan.files[0].format,
        llm_wiki_desktop_lib::models::import_v2_file::FileFormat::Mp4
    );
    assert!(scan.files[0]
        .large_data
        .as_ref()
        .is_some_and(|estimate| estimate.requires_confirmation));
}
