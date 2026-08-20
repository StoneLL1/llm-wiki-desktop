use base64::Engine;
use llm_wiki_desktop_lib::errors::BackendError;
use llm_wiki_desktop_lib::models::update::{
    StaticUpdateManifest, UpdateCheckCandidate, UpdatePhase,
};
use llm_wiki_desktop_lib::services::{verify_signed_update_artifact, UpdateService};

const NOW: i64 = 1_775_000_000;
const TEST_PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
const TEST_SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";

fn encoded(value: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(value)
}

fn candidate(version: &str) -> UpdateCheckCandidate {
    UpdateCheckCandidate {
        version: version.into(),
        target: "windows".into(),
        arch: "x86_64".into(),
        download_url: "https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.0/LLM.Wiki.Desktop.nsis.zip".into(),
        signature: "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZQpSV1QwZXN0U2lnbmF0dXJl".into(),
        notes: Some("Safe update".into()),
        published_at: Some("2026-08-20T00:00:00Z".into()),
    }
}

#[test]
fn bad_signature_is_rejected_before_an_offer_is_registered() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();
    let mut remote = candidate("0.2.0");
    remote.signature.clear();

    let error = service
        .complete_check(generation, "0.1.0", Some(remote), NOW)
        .unwrap_err();

    assert_eq!(error.code, "UPDATE_SIGNATURE_INVALID");
    assert_eq!(service.state().phase, UpdatePhase::Error);
}

#[test]
fn install_time_signature_verification_accepts_only_the_exact_artifact() {
    let public_key = encoded(TEST_PUBLIC_KEY);
    let signature = encoded(TEST_SIGNATURE);
    verify_signed_update_artifact(&public_key, &signature, b"test")
        .expect("fixture signature should verify");

    assert_eq!(
        verify_signed_update_artifact(&public_key, &signature, b"tampered")
            .unwrap_err()
            .code,
        "UPDATE_SIGNATURE_INVALID"
    );
    assert_eq!(
        verify_signed_update_artifact(&public_key, "not-base64", b"test")
            .unwrap_err()
            .code,
        "UPDATE_SIGNATURE_INVALID"
    );
}

#[test]
fn bad_manifest_is_rejected_with_a_bounded_typed_error() {
    let bytes = br#"{"version":"0.2.0","platforms":{"windows-x86_64":{"url":"http://example.invalid/update.zip","signature":"sig"}}}"#;

    let error = StaticUpdateManifest::parse_bounded(bytes, 64 * 1024).unwrap_err();

    assert_eq!(error.code, "UPDATE_MANIFEST_INVALID");
}

#[test]
fn expired_offer_cannot_be_reused_for_download_or_install() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();
    let offer = service
        .complete_check(generation, "0.1.0", Some(candidate("0.2.0")), NOW)
        .unwrap()
        .unwrap();

    assert_eq!(
        service
            .begin_download(&offer.offer_id, offer.expires_at_unix_seconds + 1)
            .unwrap_err()
            .code,
        "UPDATE_OFFER_EXPIRED"
    );
    assert_eq!(
        service
            .begin_install(
                &offer.offer_id,
                "untrusted-identity",
                offer.expires_at_unix_seconds + 1,
            )
            .unwrap_err()
            .code,
        "UPDATE_OFFER_EXPIRED"
    );
}

#[test]
fn timeout_releases_check_single_flight_for_retry() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();
    assert_eq!(
        service.begin_check(NOW).unwrap_err().code,
        "UPDATE_CHECK_IN_PROGRESS"
    );

    service.fail_check(generation, "UPDATE_CHECK_TIMEOUT", NOW);

    assert!(service.begin_check(NOW + 1).is_ok());
}

#[test]
fn cancel_download_is_terminal_and_invalidates_the_permit() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();
    let offer = service
        .complete_check(generation, "0.1.0", Some(candidate("0.2.0")), NOW)
        .unwrap()
        .unwrap();
    let permit = service.begin_download(&offer.offer_id, NOW).unwrap();

    service.cancel_download(&offer.offer_id).unwrap();

    assert_eq!(service.state().phase, UpdatePhase::Cancelled);
    assert!(service.is_download_cancelled(&permit));
    assert_eq!(
        service
            .begin_download(&offer.offer_id, NOW)
            .unwrap_err()
            .code,
        "UPDATE_OFFER_EXPIRED"
    );
    assert_eq!(
        service
            .record_download_progress(&permit, 10, Some(100))
            .unwrap_err()
            .code,
        "UPDATE_DOWNLOAD_CANCELLED"
    );
}

#[test]
fn runtime_registration_failure_never_publishes_a_partial_offer() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();

    let error = service
        .complete_check_with_registration(
            generation,
            "0.1.0",
            Some(candidate("0.2.0")),
            NOW,
            |_, _| {
                Err(BackendError::new(
                    "UPDATE_RUNTIME_UNAVAILABLE",
                    "The update runtime is unavailable.",
                    true,
                    false,
                ))
            },
        )
        .unwrap_err();

    assert_eq!(error.code, "UPDATE_RUNTIME_UNAVAILABLE");
    assert_eq!(service.state().phase, UpdatePhase::Error);
    assert!(service.state().offer.is_none());
    assert!(service.begin_check(NOW + 1).is_ok());
}

#[test]
fn live_offer_identity_mismatch_is_rejected_before_install() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();
    let offer = service
        .complete_check(generation, "0.1.0", Some(candidate("0.2.0")), NOW)
        .unwrap()
        .unwrap();
    let permit = service.begin_download(&offer.offer_id, NOW).unwrap();
    service.finish_download(&permit).unwrap();

    assert_eq!(
        service
            .begin_install(&offer.offer_id, "wrong-live-identity", NOW)
            .unwrap_err()
            .code,
        "UPDATE_OFFER_EXPIRED"
    );
    assert_eq!(service.state().phase, UpdatePhase::Downloaded);
}

#[test]
fn installing_offer_cannot_be_dismissed_concurrently() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();
    let offer = service
        .complete_check(generation, "0.1.0", Some(candidate("0.2.0")), NOW)
        .unwrap()
        .unwrap();
    let identity = service.offer_identity(&offer.offer_id, NOW).unwrap();
    let permit = service.begin_download(&offer.offer_id, NOW).unwrap();
    service.finish_download(&permit).unwrap();
    service
        .begin_install(&offer.offer_id, &identity, NOW)
        .unwrap();

    assert_eq!(
        service.dismiss(&offer.offer_id).unwrap_err().code,
        "UPDATE_BUSY"
    );
    assert_eq!(service.state().phase, UpdatePhase::Installing);
}

#[test]
fn same_version_does_not_create_an_offer() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();

    let offer = service
        .complete_check(generation, "0.2.0", Some(candidate("0.2.0")), NOW)
        .unwrap();

    assert!(offer.is_none());
    assert_eq!(service.state().phase, UpdatePhase::Idle);
}

#[test]
fn downgrade_does_not_create_an_offer() {
    let service = UpdateService::default();
    let generation = service.begin_check(NOW).unwrap();

    let offer = service
        .complete_check(generation, "0.3.0", Some(candidate("0.2.0")), NOW)
        .unwrap();

    assert!(offer.is_none());
    assert_eq!(service.state().phase, UpdatePhase::Idle);
}

#[test]
fn updater_commands_keep_the_endpoint_and_signature_out_of_ipc_inputs() {
    let source = include_str!("../src/commands/update_commands.rs");
    let registration = include_str!("../src/lib.rs");

    for command in [
        "get_update_state",
        "check_app_update",
        "download_app_update",
        "cancel_app_update_download",
        "dismiss_app_update",
    ] {
        assert!(source.contains(&format!("fn {command}")));
        assert!(registration.contains(&format!("update_commands::{command}")));
    }
    assert!(source.contains("fn install_app_update"));
    assert!(!registration.contains("update_commands::install_app_update"));
    assert!(registration.contains("Batch 4B registers install_app_update"));
    assert!(!source.contains("endpoint:"));
    assert!(!source.contains("download_url:"));
    assert!(!source.contains("signature:"));
    assert!(!source.contains("ProjectContext"));
    assert!(!source.contains("project_root_path"));

    let main_capability = include_str!("../capabilities/main.json");
    assert!(!main_capability.contains("updater:"));
}

#[test]
fn updater_configuration_uses_the_frozen_public_trust_anchor() {
    let tauri: serde_json::Value =
        serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
    let release: serde_json::Value =
        serde_json::from_str(include_str!("../../release/release-contract.json")).unwrap();
    let updater = &tauri["plugins"]["updater"];

    assert_eq!(tauri["bundle"]["createUpdaterArtifacts"], true);
    assert_eq!(
        updater["pubkey"],
        release["signing"]["updater"]["publicKey"]
    );
    assert_eq!(
        updater["endpoints"][0],
        release["endpoints"]["stableUpdaterManifest"]
    );
    let command_source = include_str!("../src/commands/update_commands.rs");
    let pinned_updater_source =
        include_str!("../vendor/tauri-plugin-updater/src/updater.rs");
    assert!(command_source.contains(updater["pubkey"].as_str().unwrap()));
    assert!(command_source.contains(updater["endpoints"][0].as_str().unwrap()));
    assert!(command_source.contains(".max_manifest_bytes(MAX_UPDATE_MANIFEST_BYTES)"));
    assert!(pinned_updater_source.contains("pub fn max_manifest_bytes"));
    assert!(pinned_updater_source.contains("while let Some(chunk) = stream.next().await"));
    assert!(pinned_updater_source.contains("Error::ManifestTooLarge(max_manifest_bytes)"));
    assert_eq!(updater["windows"]["installMode"], "passive");
    assert_ne!(updater["windows"]["installMode"], "quiet");
}
