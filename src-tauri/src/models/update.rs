use std::collections::BTreeMap;

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::BackendError;
use crate::models::settings::UpdateFrequency;

pub const MAX_UPDATE_MANIFEST_BYTES: usize = 512 * 1024;
pub const UPDATE_OFFER_TTL_SECONDS: i64 = 60 * 60;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Downloaded,
    Installing,
    Installed,
    Cancelled,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    pub offer_id: String,
    pub current_version: String,
    pub version: String,
    pub target: String,
    pub arch: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    pub created_at_unix_seconds: i64,
    pub expires_at_unix_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateState {
    pub phase: UpdatePhase,
    pub offer: Option<UpdateOffer>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: Option<BackendError>,
}

impl Default for AppUpdateState {
    fn default() -> Self {
        Self {
            phase: UpdatePhase::Idle,
            offer: None,
            downloaded_bytes: 0,
            total_bytes: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckCandidate {
    pub version: String,
    pub target: String,
    pub arch: String,
    pub download_url: String,
    pub signature: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
}

impl UpdateCheckCandidate {
    pub fn new(
        version: String,
        target: String,
        arch: String,
        artifact_location: String,
        release_proof: String,
        notes: Option<String>,
        published_at: Option<String>,
    ) -> Self {
        Self {
            version,
            target,
            arch,
            download_url: artifact_location,
            signature: release_proof,
            notes,
            published_at,
        }
    }

    pub fn validate(&self) -> Result<(), BackendError> {
        semver::Version::parse(self.version.trim_start_matches('v')).map_err(|_| {
            update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest contains an invalid version.",
                true,
            )
        })?;
        let url = url::Url::parse(&self.download_url).map_err(|_| {
            update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest contains an invalid download URL.",
                true,
            )
        })?;
        if url.scheme() != "https" || url.host_str().is_none() {
            return Err(update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest download URL must use HTTPS.",
                true,
            ));
        }
        if self.target.trim().is_empty() || self.arch.trim().is_empty() {
            return Err(update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest target is invalid.",
                true,
            ));
        }
        validate_signature(&self.signature)
    }

    pub fn identity(&self) -> String {
        let mut hasher = Sha256::new();
        for value in [
            self.version.as_str(),
            self.target.as_str(),
            self.arch.as_str(),
            self.download_url.as_str(),
            self.signature.as_str(),
        ] {
            hasher.update((value.len() as u64).to_le_bytes());
            hasher.update(value.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct StaticUpdateManifest {
    version: String,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    pub_date: Option<String>,
    platforms: BTreeMap<String, StaticUpdatePlatform>,
}

#[derive(Debug, Clone, Deserialize)]
struct StaticUpdatePlatform {
    url: String,
    signature: String,
}

impl StaticUpdateManifest {
    pub fn parse_bounded(bytes: &[u8], max_bytes: usize) -> Result<Self, BackendError> {
        if bytes.is_empty() || bytes.len() > max_bytes.min(MAX_UPDATE_MANIFEST_BYTES) {
            return Err(update_error(
                "UPDATE_MANIFEST_TOO_LARGE",
                "The update manifest is empty or exceeds the allowed size.",
                true,
            ));
        }
        let manifest: Self = serde_json::from_slice(bytes).map_err(|_| {
            update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest is not valid JSON.",
                true,
            )
        })?;
        semver::Version::parse(manifest.version.trim_start_matches('v')).map_err(|_| {
            update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest contains an invalid version.",
                true,
            )
        })?;
        if manifest.platforms.is_empty() {
            return Err(update_error(
                "UPDATE_MANIFEST_INVALID",
                "The update manifest has no platform entries.",
                true,
            ));
        }
        for platform in manifest.platforms.values() {
            let url = url::Url::parse(&platform.url).map_err(|_| {
                update_error(
                    "UPDATE_MANIFEST_INVALID",
                    "The update manifest contains an invalid platform URL.",
                    true,
                )
            })?;
            if url.scheme() != "https" || url.host_str().is_none() {
                return Err(update_error(
                    "UPDATE_MANIFEST_INVALID",
                    "Every update platform URL must use HTTPS.",
                    true,
                ));
            }
            validate_signature(&platform.signature)?;
        }
        Ok(manifest)
    }

    pub fn candidate_for(
        &self,
        target: &str,
        arch: &str,
    ) -> Result<UpdateCheckCandidate, BackendError> {
        let key = format!("{target}-{arch}");
        let platform = self.platforms.get(&key).ok_or_else(|| {
            update_error(
                "UPDATE_PLATFORM_UNAVAILABLE",
                "No signed update artifact is available for this platform.",
                true,
            )
        })?;
        let candidate = UpdateCheckCandidate::new(
            self.version.clone(),
            target.to_string(),
            arch.to_string(),
            platform.url.clone(),
            platform.signature.clone(),
            self.notes.clone(),
            self.pub_date.clone(),
        );
        candidate.validate()?;
        Ok(candidate)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOfferRequest {
    pub offer_id: String,
}

/// Frontend-only presentation facts are carried explicitly, while task and
/// workflow facts are revalidated from AppState by the install command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateInstallRequest {
    pub offer_id: String,
    pub restart_consent: bool,
    pub unsaved_editor: bool,
    pub import_commit_active: bool,
    pub pending_user_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateInstallReceiptPhase {
    HandoffReady,
    InstalledAwaitingRestart,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateInstallReceipt {
    pub offer_id: String,
    pub from_version: String,
    pub target_version: String,
    pub phase: UpdateInstallReceiptPhase,
    pub recorded_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Default)]
pub(crate) struct InstallGuardFacts {
    pub(crate) pending_task_confirmation: bool,
    pub(crate) critical_task_active: bool,
    pub(crate) workflow_apply_active: bool,
}

pub(crate) fn validate_update_install_guard(
    request: &UpdateInstallRequest,
    facts: InstallGuardFacts,
) -> Result<(), BackendError> {
    if !request.restart_consent {
        return Err(update_error(
            "UPDATE_RESTART_CONSENT_REQUIRED",
            "Installing an update requires explicit restart consent.",
            true,
        ));
    }
    if request.unsaved_editor {
        return Err(update_error(
            "UPDATE_UNSAVED_EDITOR",
            "Save or discard the current editor draft before installing the update.",
            true,
        ));
    }
    if request.import_commit_active {
        return Err(update_error(
            "UPDATE_IMPORT_COMMIT_ACTIVE",
            "Wait for the active Import commit to finish before installing the update.",
            true,
        ));
    }
    if request.pending_user_confirmation || facts.pending_task_confirmation {
        return Err(update_error(
            "UPDATE_CONFIRMATION_PENDING",
            "Resolve the pending confirmation before installing the update.",
            true,
        ));
    }
    if facts.critical_task_active {
        return Err(update_error(
            "UPDATE_CRITICAL_TASK_ACTIVE",
            "Wait for the non-interruptible task to finish before installing the update.",
            true,
        ));
    }
    if facts.workflow_apply_active {
        return Err(update_error(
            "UPDATE_WORKFLOW_APPLY_ACTIVE",
            "Wait for the workflow apply stage to finish before installing the update.",
            true,
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressEvent {
    pub phase: UpdatePhase,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlobalUpdatePreferences {
    pub check_updates: bool,
    pub update_frequency: UpdateFrequency,
    pub auto_download_updates: bool,
    pub prompt_changelog_before_install: bool,
    pub last_checked_at: Option<String>,
    pub dismissed_offer_id: Option<String>,
    pub dismissed_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveGlobalUpdatePreferences {
    pub check_updates: bool,
    pub update_frequency: UpdateFrequency,
    pub auto_download_updates: bool,
    pub prompt_changelog_before_install: bool,
}

pub(crate) fn update_error(
    code: &'static str,
    message: &'static str,
    recoverable: bool,
) -> BackendError {
    BackendError::new(code, message, recoverable, false)
}

fn validate_signature(signature: &str) -> Result<(), BackendError> {
    let encoded = signature.trim();
    if encoded.is_empty() || encoded.len() > 16 * 1024 {
        return Err(update_error(
            "UPDATE_SIGNATURE_INVALID",
            "The update signature is missing or invalid.",
            true,
        ));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| {
            update_error(
                "UPDATE_SIGNATURE_INVALID",
                "The update signature is missing or invalid.",
                true,
            )
        })?;
    if decoded.len() < 32 || decoded.len() > 12 * 1024 {
        return Err(update_error(
            "UPDATE_SIGNATURE_INVALID",
            "The update signature is missing or invalid.",
            true,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod install_guard_tests {
    use super::{validate_update_install_guard, InstallGuardFacts, UpdateInstallRequest};

    fn request() -> UpdateInstallRequest {
        UpdateInstallRequest {
            offer_id: "offer-1".into(),
            restart_consent: true,
            unsaved_editor: false,
            import_commit_active: false,
            pending_user_confirmation: false,
        }
    }

    #[test]
    fn explicit_restart_consent_is_required() {
        let mut request = request();
        request.restart_consent = false;
        assert_eq!(
            validate_update_install_guard(&request, InstallGuardFacts::default())
                .unwrap_err()
                .code,
            "UPDATE_RESTART_CONSENT_REQUIRED"
        );
    }

    #[test]
    fn unsaved_editor_and_import_commit_are_blocked() {
        let mut request = request();
        request.unsaved_editor = true;
        assert_eq!(
            validate_update_install_guard(&request, InstallGuardFacts::default())
                .unwrap_err()
                .code,
            "UPDATE_UNSAVED_EDITOR"
        );
        request.unsaved_editor = false;
        request.import_commit_active = true;
        assert_eq!(
            validate_update_install_guard(&request, InstallGuardFacts::default())
                .unwrap_err()
                .code,
            "UPDATE_IMPORT_COMMIT_ACTIVE"
        );
    }

    #[test]
    fn backend_critical_facts_are_revalidated() {
        for (facts, code) in [
            (
                InstallGuardFacts {
                    pending_task_confirmation: true,
                    ..InstallGuardFacts::default()
                },
                "UPDATE_CONFIRMATION_PENDING",
            ),
            (
                InstallGuardFacts {
                    critical_task_active: true,
                    ..InstallGuardFacts::default()
                },
                "UPDATE_CRITICAL_TASK_ACTIVE",
            ),
            (
                InstallGuardFacts {
                    workflow_apply_active: true,
                    ..InstallGuardFacts::default()
                },
                "UPDATE_WORKFLOW_APPLY_ACTIVE",
            ),
        ] {
            assert_eq!(
                validate_update_install_guard(&request(), facts)
                    .unwrap_err()
                    .code,
                code
            );
        }
    }

    #[test]
    fn safe_idle_state_allows_install() {
        validate_update_install_guard(&request(), InstallGuardFacts::default()).unwrap();
    }
}
