use std::sync::Mutex;

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use uuid::Uuid;

use crate::errors::BackendError;
use crate::models::update::{
    update_error, AppUpdateState, UpdateCheckCandidate, UpdateOffer, UpdatePhase,
    UPDATE_OFFER_TTL_SECONDS,
};

#[derive(Debug, Clone)]
pub struct UpdateDownloadPermit {
    offer_id: String,
    generation: u64,
    candidate_identity: String,
}

impl UpdateDownloadPermit {
    pub fn offer_id(&self) -> &str {
        &self.offer_id
    }

    pub fn candidate_identity(&self) -> &str {
        &self.candidate_identity
    }
}

#[derive(Default)]
struct CoordinatorState {
    presentation: AppUpdateState,
    check_generation: u64,
    active_check_generation: Option<u64>,
    download_generation: u64,
    active_download: Option<UpdateDownloadPermit>,
    candidate_identity: Option<String>,
    offer_check_generation: Option<u64>,
}

#[derive(Default)]
pub struct UpdateService {
    coordinator: Mutex<CoordinatorState>,
}

impl UpdateService {
    pub fn state(&self) -> AppUpdateState {
        self.coordinator
            .lock()
            .map(|state| state.presentation.clone())
            .unwrap_or_else(|_| AppUpdateState {
                phase: UpdatePhase::Error,
                error: Some(coordinator_locked()),
                ..AppUpdateState::default()
            })
    }

    pub fn begin_check(&self, _now: i64) -> Result<u64, BackendError> {
        let mut state = self.lock()?;
        if state.active_check_generation.is_some() {
            return Err(update_error(
                "UPDATE_CHECK_IN_PROGRESS",
                "An update check is already running.",
                true,
            ));
        }
        if matches!(
            state.presentation.phase,
            UpdatePhase::Downloading | UpdatePhase::Downloaded | UpdatePhase::Installing
        ) {
            return Err(update_error(
                "UPDATE_BUSY",
                "Finish or cancel the current update operation before checking again.",
                true,
            ));
        }
        state.check_generation = state.check_generation.saturating_add(1);
        let generation = state.check_generation;
        state.active_check_generation = Some(generation);
        state.active_download = None;
        state.candidate_identity = None;
        state.offer_check_generation = None;
        state.presentation = AppUpdateState {
            phase: UpdatePhase::Checking,
            ..AppUpdateState::default()
        };
        Ok(generation)
    }

    pub fn complete_check(
        &self,
        generation: u64,
        current_version: &str,
        candidate: Option<UpdateCheckCandidate>,
        now: i64,
    ) -> Result<Option<UpdateOffer>, BackendError> {
        self.complete_check_with_registration(
            generation,
            current_version,
            candidate,
            now,
            |_, _| Ok(()),
        )
    }

    pub fn complete_check_with_registration<F>(
        &self,
        generation: u64,
        current_version: &str,
        candidate: Option<UpdateCheckCandidate>,
        now: i64,
        register: F,
    ) -> Result<Option<UpdateOffer>, BackendError>
    where
        F: FnOnce(&UpdateOffer, &str) -> Result<(), BackendError>,
    {
        let mut state = self.lock()?;
        require_check_generation(&state, generation)?;
        let Some(candidate) = candidate else {
            state.active_check_generation = None;
            state.presentation = AppUpdateState::default();
            return Ok(None);
        };
        if let Err(error) = candidate.validate() {
            state.active_check_generation = None;
            state.presentation.phase = UpdatePhase::Error;
            state.presentation.error = Some(error.clone());
            return Err(error);
        }
        let current = match parse_version(current_version) {
            Ok(version) => version,
            Err(error) => {
                state.active_check_generation = None;
                state.presentation.phase = UpdatePhase::Error;
                state.presentation.error = Some(error.clone());
                return Err(error);
            }
        };
        let remote = match parse_version(&candidate.version) {
            Ok(version) => version,
            Err(error) => {
                state.active_check_generation = None;
                state.presentation.phase = UpdatePhase::Error;
                state.presentation.error = Some(error.clone());
                return Err(error);
            }
        };
        if remote <= current {
            state.active_check_generation = None;
            state.presentation = AppUpdateState::default();
            return Ok(None);
        }
        let identity = bind_offer_identity(generation, &candidate.identity());
        let offer = UpdateOffer {
            offer_id: Uuid::new_v4().to_string(),
            current_version: current.to_string(),
            version: remote.to_string(),
            target: candidate.target,
            arch: candidate.arch,
            notes: candidate.notes,
            published_at: candidate.published_at,
            created_at_unix_seconds: now,
            expires_at_unix_seconds: now.saturating_add(UPDATE_OFFER_TTL_SECONDS),
        };
        if let Err(error) = register(&offer, &identity) {
            state.active_check_generation = None;
            state.presentation.phase = UpdatePhase::Error;
            state.presentation.error = Some(error.clone());
            return Err(error);
        }
        state.active_check_generation = None;
        state.candidate_identity = Some(identity);
        state.offer_check_generation = Some(generation);
        state.presentation = AppUpdateState {
            phase: UpdatePhase::Available,
            offer: Some(offer.clone()),
            ..AppUpdateState::default()
        };
        Ok(Some(offer))
    }

    pub fn fail_check(&self, generation: u64, code: &'static str, _now: i64) {
        self.fail_check_with_error(
            generation,
            update_error(
                code,
                "The update check failed. Try again when the network is available.",
                true,
            ),
        );
    }

    pub fn fail_check_with_error(&self, generation: u64, error: BackendError) {
        let Ok(mut state) = self.coordinator.lock() else {
            return;
        };
        if state.active_check_generation != Some(generation) {
            return;
        }
        state.active_check_generation = None;
        state.offer_check_generation = None;
        state.presentation.phase = UpdatePhase::Error;
        state.presentation.error = Some(error);
    }

    pub fn begin_download(
        &self,
        offer_id: &str,
        now: i64,
    ) -> Result<UpdateDownloadPermit, BackendError> {
        let mut state = self.lock()?;
        if state.active_download.is_some() {
            return Err(update_error(
                "UPDATE_DOWNLOAD_IN_PROGRESS",
                "The checked update is already downloading.",
                true,
            ));
        }
        if state.presentation.phase != UpdatePhase::Available {
            return Err(offer_expired());
        }
        let offer = require_offer(&state, offer_id, now)?;
        let identity = state.candidate_identity.clone().ok_or_else(offer_expired)?;
        state.download_generation = state.download_generation.saturating_add(1);
        let permit = UpdateDownloadPermit {
            offer_id: offer.offer_id,
            generation: state.download_generation,
            candidate_identity: identity,
        };
        state.active_download = Some(permit.clone());
        state.presentation.phase = UpdatePhase::Downloading;
        state.presentation.downloaded_bytes = 0;
        state.presentation.total_bytes = None;
        state.presentation.error = None;
        Ok(permit)
    }

    pub fn record_download_progress(
        &self,
        permit: &UpdateDownloadPermit,
        downloaded_bytes: u64,
        total_bytes: Option<u64>,
    ) -> Result<AppUpdateState, BackendError> {
        let mut state = self.lock()?;
        require_download_permit(&state, permit)?;
        if state.presentation.phase == UpdatePhase::Cancelled {
            return Err(download_cancelled());
        }
        if let Some(total) = total_bytes {
            if downloaded_bytes > total {
                return Err(update_error(
                    "UPDATE_DOWNLOAD_INVALID",
                    "The update download reported invalid progress.",
                    true,
                ));
            }
        }
        state.presentation.phase = UpdatePhase::Downloading;
        state.presentation.downloaded_bytes = downloaded_bytes;
        state.presentation.total_bytes = total_bytes;
        Ok(state.presentation.clone())
    }

    pub fn finish_download(
        &self,
        permit: &UpdateDownloadPermit,
    ) -> Result<AppUpdateState, BackendError> {
        let mut state = self.lock()?;
        require_download_permit(&state, permit)?;
        if state.presentation.phase == UpdatePhase::Cancelled {
            return Err(download_cancelled());
        }
        state.presentation.phase = UpdatePhase::Downloaded;
        state.active_download = None;
        Ok(state.presentation.clone())
    }

    pub fn fail_download(
        &self,
        permit: &UpdateDownloadPermit,
        error: BackendError,
    ) -> Result<(), BackendError> {
        let mut state = self.lock()?;
        require_download_permit(&state, permit)?;
        state.active_download = None;
        state.presentation.phase = UpdatePhase::Error;
        state.presentation.error = Some(error);
        Ok(())
    }

    pub fn cancel_download(&self, offer_id: &str) -> Result<(), BackendError> {
        let mut state = self.lock()?;
        let active = state.active_download.as_ref().ok_or_else(|| {
            update_error(
                "UPDATE_DOWNLOAD_NOT_ACTIVE",
                "No update download is currently active.",
                true,
            )
        })?;
        if active.offer_id != offer_id {
            return Err(offer_expired());
        }
        state.presentation.phase = UpdatePhase::Cancelled;
        state.presentation.error = Some(download_cancelled());
        state.active_download = None;
        Ok(())
    }

    pub fn is_download_cancelled(&self, permit: &UpdateDownloadPermit) -> bool {
        self.coordinator.lock().map_or(true, |state| {
            state.presentation.phase == UpdatePhase::Cancelled
                || state
                    .active_download
                    .as_ref()
                    .is_none_or(|active| active.generation != permit.generation)
        })
    }

    pub fn begin_install(
        &self,
        offer_id: &str,
        candidate_identity: &str,
        now: i64,
    ) -> Result<(), BackendError> {
        let mut state = self.lock()?;
        require_offer(&state, offer_id, now)?;
        if state.presentation.phase != UpdatePhase::Downloaded
            || state.candidate_identity.as_deref() != Some(candidate_identity)
        {
            return Err(offer_expired());
        }
        state.presentation.phase = UpdatePhase::Installing;
        state.presentation.error = None;
        Ok(())
    }

    pub fn finish_install(&self, offer_id: &str) -> Result<AppUpdateState, BackendError> {
        let mut state = self.lock()?;
        if state
            .presentation
            .offer
            .as_ref()
            .map(|offer| offer.offer_id.as_str())
            != Some(offer_id)
            || state.presentation.phase != UpdatePhase::Installing
        {
            return Err(offer_expired());
        }
        state.presentation.phase = UpdatePhase::Installed;
        Ok(state.presentation.clone())
    }

    pub fn fail_install(&self, offer_id: &str, error: BackendError) -> Result<(), BackendError> {
        let mut state = self.lock()?;
        if state
            .presentation
            .offer
            .as_ref()
            .map(|offer| offer.offer_id.as_str())
            != Some(offer_id)
            || state.presentation.phase != UpdatePhase::Installing
        {
            return Err(offer_expired());
        }
        state.presentation.phase = UpdatePhase::Error;
        state.presentation.error = Some(error);
        Ok(())
    }

    pub fn fail_offer(&self, offer_id: &str, error: BackendError) -> Result<(), BackendError> {
        let mut state = self.lock()?;
        if state
            .presentation
            .offer
            .as_ref()
            .map(|offer| offer.offer_id.as_str())
            != Some(offer_id)
        {
            return Err(offer_expired());
        }
        state.presentation.phase = UpdatePhase::Error;
        state.presentation.error = Some(error);
        state.active_download = None;
        state.candidate_identity = None;
        state.offer_check_generation = None;
        Ok(())
    }

    pub fn offer_identity(&self, offer_id: &str, now: i64) -> Result<String, BackendError> {
        let state = self.lock()?;
        require_offer(&state, offer_id, now)?;
        let generation = state.offer_check_generation.ok_or_else(offer_expired)?;
        let identity = state.candidate_identity.clone().ok_or_else(offer_expired)?;
        if identity.starts_with(&format!("{generation}:")) {
            Ok(identity)
        } else {
            Err(offer_expired())
        }
    }

    pub fn dismiss(&self, offer_id: &str) -> Result<UpdateOffer, BackendError> {
        let mut state = self.lock()?;
        if matches!(
            state.presentation.phase,
            UpdatePhase::Downloading | UpdatePhase::Installing
        ) {
            return Err(update_error(
                "UPDATE_BUSY",
                "Finish or cancel the current update operation before dismissing it.",
                true,
            ));
        }
        let offer = state
            .presentation
            .offer
            .clone()
            .filter(|offer| offer.offer_id == offer_id)
            .ok_or_else(offer_expired)?;
        state.presentation = AppUpdateState::default();
        state.active_download = None;
        state.candidate_identity = None;
        state.offer_check_generation = None;
        Ok(offer)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, CoordinatorState>, BackendError> {
        self.coordinator.lock().map_err(|_| coordinator_locked())
    }
}

pub fn verify_signed_update_artifact(
    public_key_base64: &str,
    signature_base64: &str,
    bytes: &[u8],
) -> Result<(), BackendError> {
    let public_key = decode_base64_text(public_key_base64)
        .and_then(|key| PublicKey::decode(&key).map_err(|_| signature_invalid()))?;
    let proof = decode_base64_text(signature_base64)
        .and_then(|proof| Signature::decode(&proof).map_err(|_| signature_invalid()))?;
    public_key
        .verify(bytes, &proof, true)
        .map_err(|_| signature_invalid())
}

fn decode_base64_text(value: &str) -> Result<String, BackendError> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| signature_invalid())?;
    String::from_utf8(bytes).map_err(|_| signature_invalid())
}

fn bind_offer_identity(generation: u64, candidate_identity: &str) -> String {
    format!("{generation}:{candidate_identity}")
}

fn require_check_generation(state: &CoordinatorState, generation: u64) -> Result<(), BackendError> {
    if state.active_check_generation == Some(generation) {
        Ok(())
    } else {
        Err(update_error(
            "UPDATE_CHECK_STALE",
            "The update check was superseded or expired.",
            true,
        ))
    }
}

fn require_offer(
    state: &CoordinatorState,
    offer_id: &str,
    now: i64,
) -> Result<UpdateOffer, BackendError> {
    state
        .presentation
        .offer
        .clone()
        .filter(|offer| offer.offer_id == offer_id && now <= offer.expires_at_unix_seconds)
        .ok_or_else(offer_expired)
}

fn require_download_permit(
    state: &CoordinatorState,
    permit: &UpdateDownloadPermit,
) -> Result<(), BackendError> {
    let Some(active) = state.active_download.as_ref() else {
        return Err(download_cancelled());
    };
    if active.offer_id == permit.offer_id
        && active.generation == permit.generation
        && active.candidate_identity == permit.candidate_identity
    {
        Ok(())
    } else {
        Err(download_cancelled())
    }
}

fn parse_version(value: &str) -> Result<Version, BackendError> {
    Version::parse(value.trim_start_matches('v')).map_err(|_| {
        update_error(
            "UPDATE_VERSION_INVALID",
            "The application or update version is invalid.",
            true,
        )
    })
}

fn offer_expired() -> BackendError {
    update_error(
        "UPDATE_OFFER_EXPIRED",
        "The checked update offer expired or changed. Check for updates again.",
        true,
    )
}

fn download_cancelled() -> BackendError {
    update_error(
        "UPDATE_DOWNLOAD_CANCELLED",
        "The update download was cancelled.",
        true,
    )
}

fn signature_invalid() -> BackendError {
    update_error(
        "UPDATE_SIGNATURE_INVALID",
        "The update signature is invalid. The update was not installed.",
        true,
    )
}

fn coordinator_locked() -> BackendError {
    update_error(
        "UPDATE_STATE_UNAVAILABLE",
        "The update state is temporarily unavailable.",
        true,
    )
}
