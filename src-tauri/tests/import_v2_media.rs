use std::fs;

use llm_wiki_desktop_lib::services::import_v2::media_router::{
    recover_media_temp_root, render_timestamped_markdown, AsrModelCatalog, MediaArtifactPlan,
    MediaInput, MediaKind, MediaRouteStatus, MediaRouter, SubtitleCandidate, SubtitleKind,
    TemporaryMediaWorkspace, TranscriptSegment,
};

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
    assert_eq!(plan.subtitle.unwrap().path, "human.srt");
    assert!(!plan.requires_asr);
    assert_eq!(
        plan.artifacts,
        MediaArtifactPlan::subtitle_markdown_cover_metadata()
    );
}

#[test]
fn subtitle_priority_is_human_then_automatic_then_embedded_then_asr() {
    let router = MediaRouter::default();
    for (subtitles, expected) in [
        (
            vec![
                SubtitleCandidate::new(SubtitleKind::Automatic, "a.vtt"),
                SubtitleCandidate::new(SubtitleKind::Embedded, "e.vtt"),
            ],
            Some("a.vtt"),
        ),
        (
            vec![SubtitleCandidate::new(SubtitleKind::Embedded, "e.vtt")],
            Some("e.vtt"),
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
