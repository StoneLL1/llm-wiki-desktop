use crate::errors::BackendError;

pub const APP_CAPABILITY_CONFINEMENT_UNAVAILABLE: &str = "APP_CAPABILITY_CONFINEMENT_UNAVAILABLE";

/// Batch 5 stopped at its mandatory four-target feasibility gate. Keep every
/// capability-install mutation fail closed until a supported confinement
/// design has real packaged evidence on all release targets.
pub fn capability_installation_mutations_enabled() -> bool {
    false
}

pub fn require_capability_installation_confinement() -> Result<(), BackendError> {
    if capability_installation_mutations_enabled() {
        return Ok(());
    }
    Err(BackendError::new(
        APP_CAPABILITY_CONFINEMENT_UNAVAILABLE,
        "Capability installation is disabled because runner confinement is not verified for every release target.",
        false,
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_installation_stays_fail_closed_after_the_batch_5_feasibility_stop() {
        assert!(!capability_installation_mutations_enabled());
        let error = require_capability_installation_confinement().unwrap_err();
        assert_eq!(error.code, APP_CAPABILITY_CONFINEMENT_UNAVAILABLE);
        assert!(!error.recoverable);
        assert!(!error.user_action_required);
    }
}
