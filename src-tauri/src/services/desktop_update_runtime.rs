use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri_plugin_updater::Update;

use crate::errors::BackendError;
use crate::models::update::update_error;

pub struct PendingDesktopUpdate {
    pub offer_id: String,
    pub bound_identity: String,
    pub update: Update,
    pub bytes: Option<Vec<u8>>,
    pub cancellation: Arc<AtomicBool>,
}

pub struct PendingDesktopDownload {
    pub bound_identity: String,
    pub update: Update,
    pub cancellation: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct DesktopUpdateRuntime {
    pending: Mutex<Option<PendingDesktopUpdate>>,
}

impl DesktopUpdateRuntime {
    pub fn register(
        &self,
        offer_id: String,
        bound_identity: String,
        update: Update,
    ) -> Result<(), BackendError> {
        let mut pending = self.lock()?;
        if let Some(previous) = pending.take() {
            previous.cancellation.store(true, Ordering::SeqCst);
        }
        *pending = Some(PendingDesktopUpdate {
            offer_id,
            bound_identity,
            update,
            bytes: None,
            cancellation: Arc::new(AtomicBool::new(false)),
        });
        Ok(())
    }

    pub fn begin_download(&self, offer_id: &str) -> Result<PendingDesktopDownload, BackendError> {
        let pending = self.lock()?;
        let pending = pending
            .as_ref()
            .filter(|pending| pending.offer_id == offer_id)
            .ok_or_else(runtime_offer_expired)?;
        pending.cancellation.store(false, Ordering::SeqCst);
        Ok(PendingDesktopDownload {
            bound_identity: pending.bound_identity.clone(),
            update: pending.update.clone(),
            cancellation: Arc::clone(&pending.cancellation),
        })
    }

    pub fn store_download(
        &self,
        offer_id: &str,
        bound_identity: &str,
        bytes: Vec<u8>,
    ) -> Result<(), BackendError> {
        let mut pending = self.lock()?;
        let pending = pending
            .as_mut()
            .filter(|pending| {
                pending.offer_id == offer_id && pending.bound_identity == bound_identity
            })
            .ok_or_else(runtime_offer_expired)?;
        if pending.cancellation.load(Ordering::SeqCst) {
            return Err(update_error(
                "UPDATE_DOWNLOAD_CANCELLED",
                "The update download was cancelled.",
                true,
            ));
        }
        pending.bytes = Some(bytes);
        Ok(())
    }

    pub fn cancel(&self, offer_id: &str) -> Result<(), BackendError> {
        let pending = self.lock()?;
        let pending = pending
            .as_ref()
            .filter(|pending| pending.offer_id == offer_id)
            .ok_or_else(runtime_offer_expired)?;
        pending.cancellation.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn take_for_install(
        &self,
        offer_id: &str,
        bound_identity: &str,
    ) -> Result<(Update, Vec<u8>), BackendError> {
        let mut pending = self.lock()?;
        let pending = pending
            .as_mut()
            .filter(|pending| {
                pending.offer_id == offer_id && pending.bound_identity == bound_identity
            })
            .ok_or_else(runtime_offer_expired)?;
        let bytes = pending.bytes.take().ok_or_else(|| {
            update_error(
                "UPDATE_DOWNLOAD_REQUIRED",
                "Download and verify the checked update before installing it.",
                true,
            )
        })?;
        Ok((pending.update.clone(), bytes))
    }

    pub fn restore_download(&self, offer_id: &str, bound_identity: &str, bytes: Vec<u8>) {
        if let Ok(mut pending) = self.pending.lock() {
            if let Some(pending) = pending.as_mut().filter(|pending| {
                pending.offer_id == offer_id && pending.bound_identity == bound_identity
            }) {
                pending.bytes = Some(bytes);
            }
        }
    }

    pub fn clear(&self, offer_id: &str) {
        if let Ok(mut pending) = self.pending.lock() {
            if pending
                .as_ref()
                .is_some_and(|pending| pending.offer_id == offer_id)
            {
                if let Some(previous) = pending.take() {
                    previous.cancellation.store(true, Ordering::SeqCst);
                }
            }
        }
    }

    fn lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<PendingDesktopUpdate>>, BackendError> {
        self.pending.lock().map_err(|_| {
            update_error(
                "UPDATE_STATE_UNAVAILABLE",
                "The update state is temporarily unavailable.",
                true,
            )
        })
    }
}

fn runtime_offer_expired() -> BackendError {
    update_error(
        "UPDATE_OFFER_EXPIRED",
        "The checked update offer expired or changed. Check for updates again.",
        true,
    )
}
