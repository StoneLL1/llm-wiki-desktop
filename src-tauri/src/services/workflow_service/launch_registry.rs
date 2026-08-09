use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

use crate::errors::BackendError;

#[derive(Default)]
pub struct WorkflowLaunchRegistry {
    shared: Arc<LaunchShared>,
}

#[derive(Default)]
struct LaunchShared {
    owners: Mutex<HashMap<String, OwnerLaunchState>>,
    changed: Condvar,
}

#[derive(Default)]
struct OwnerLaunchState {
    authority_revision: String,
    generation: u64,
    closed: bool,
    publishing: usize,
}

pub struct WorkflowExternalLaunchPermit {
    shared: Arc<LaunchShared>,
    owner: String,
    task_id: String,
    authority_revision: String,
    generation: u64,
}

pub struct WorkflowLaunchPublication {
    shared: Arc<LaunchShared>,
    owner: String,
    finished: bool,
}

pub struct WorkflowLaunchCloseBarrier {
    shared: Arc<LaunchShared>,
    owner: String,
}

impl WorkflowLaunchRegistry {
    pub fn issue(
        &self,
        owner: &str,
        task_id: &str,
        authority_revision: &str,
    ) -> Result<WorkflowExternalLaunchPermit, BackendError> {
        let mut owners = self.shared.owners.lock().map_err(|_| launch_locked())?;
        let state = owners.entry(owner.to_string()).or_default();
        if state.authority_revision != authority_revision {
            state.authority_revision = authority_revision.to_string();
            state.generation = state.generation.wrapping_add(1);
            state.closed = false;
        }
        if state.closed {
            return Err(launch_revoked());
        }
        Ok(WorkflowExternalLaunchPermit {
            shared: Arc::clone(&self.shared),
            owner: owner.to_string(),
            task_id: task_id.to_string(),
            authority_revision: authority_revision.to_string(),
            generation: state.generation,
        })
    }

    pub fn close_owner(&self, owner: &str) -> WorkflowLaunchCloseBarrier {
        if let Ok(mut owners) = self.shared.owners.lock() {
            let state = owners.entry(owner.to_string()).or_default();
            state.closed = true;
            state.generation = state.generation.wrapping_add(1);
        }
        WorkflowLaunchCloseBarrier {
            shared: Arc::clone(&self.shared),
            owner: owner.to_string(),
        }
    }
}

impl WorkflowExternalLaunchPermit {
    pub(crate) fn prevalidated(run: &crate::models::workflow::WorkflowRun) -> Self {
        WorkflowLaunchRegistry::default()
            .issue(
                &run.canonical_identity_key,
                &run.task_id,
                "prevalidated-caller",
            )
            .expect("a private prevalidated launch registry must be open")
    }

    pub fn begin(self) -> Result<WorkflowLaunchPublication, BackendError> {
        let mut owners = self.shared.owners.lock().map_err(|_| launch_locked())?;
        let state = owners.get_mut(&self.owner).ok_or_else(launch_revoked)?;
        if state.closed
            || state.generation != self.generation
            || state.authority_revision != self.authority_revision
        {
            return Err(launch_revoked());
        }
        state.publishing += 1;
        drop(owners);
        Ok(WorkflowLaunchPublication {
            shared: self.shared,
            owner: self.owner,
            finished: false,
        })
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    pub fn authority_revision(&self) -> &str {
        &self.authority_revision
    }
}

impl WorkflowLaunchPublication {
    /// Marks the external dispatch call as having returned, which proves that
    /// its physical Agent/HTTP publication point was entered. The current
    /// service APIs do not expose a handle immediately after publication, so
    /// callers conservatively keep this window open for the whole cancellable
    /// call. Revocation waits here outside all project/coordinator/task locks.
    pub fn started(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        if self.finished {
            return;
        }
        if let Ok(mut owners) = self.shared.owners.lock() {
            if let Some(state) = owners.get_mut(&self.owner) {
                state.publishing = state.publishing.saturating_sub(1);
            }
            self.shared.changed.notify_all();
        }
        self.finished = true;
    }
}

impl Drop for WorkflowLaunchPublication {
    fn drop(&mut self) {
        self.finish();
    }
}

impl WorkflowLaunchCloseBarrier {
    pub fn wait(self) -> Result<(), BackendError> {
        let mut owners = self.shared.owners.lock().map_err(|_| launch_locked())?;
        while owners
            .get(&self.owner)
            .is_some_and(|state| state.publishing > 0)
        {
            owners = self
                .shared
                .changed
                .wait(owners)
                .map_err(|_| launch_locked())?;
        }
        Ok(())
    }
}

fn launch_locked() -> BackendError {
    BackendError::new(
        "WORKFLOW_LAUNCH_REGISTRY_LOCKED",
        "Workflow launch authority is temporarily unavailable.",
        true,
        true,
    )
}

fn launch_revoked() -> BackendError {
    BackendError::new(
        "WORKFLOW_EXTERNAL_LAUNCH_REVOKED",
        "Workflow authority was revoked before the external invocation was published.",
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::WorkflowLaunchRegistry;

    #[test]
    fn issued_permit_cannot_publish_after_epoch_close() {
        let registry = WorkflowLaunchRegistry::default();
        let permit = registry.issue("owner", "task", "rev-1").unwrap();
        let barrier = registry.close_owner("owner");

        let error = match permit.begin() {
            Err(error) => error,
            Ok(_) => panic!("a closed epoch must reject an issued permit"),
        };
        assert_eq!(error.code, "WORKFLOW_EXTERNAL_LAUNCH_REVOKED");
        barrier.wait().unwrap();
    }

    #[test]
    fn close_waits_for_a_publication_window_to_finish() {
        let registry = WorkflowLaunchRegistry::default();
        let publication = registry
            .issue("owner", "task", "rev-1")
            .unwrap()
            .begin()
            .unwrap();
        let barrier = registry.close_owner("owner");
        let (finished_tx, finished_rx) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            barrier.wait().unwrap();
            finished_tx.send(()).unwrap();
        });

        assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
        publication.started();
        finished_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        waiter.join().unwrap();
    }

    #[test]
    fn a_rotated_authority_opens_a_new_epoch_without_reviving_old_permits() {
        let registry = WorkflowLaunchRegistry::default();
        let stale = registry.issue("owner", "old", "rev-1").unwrap();
        registry.close_owner("owner").wait().unwrap();
        let current = registry.issue("owner", "new", "rev-2").unwrap();

        assert!(stale.begin().is_err());
        current.begin().unwrap().started();
    }
}
