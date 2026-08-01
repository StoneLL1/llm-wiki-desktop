pub mod coordinator;
pub mod fingerprint;
pub mod persistence;
pub mod stage_sink;

pub use coordinator::{EnqueueWorkflow, WorkflowCoordinator};
pub use fingerprint::{canonical_json, workflow_fingerprint};
pub(crate) use persistence::recover_workflow;
pub use persistence::{project_identity, ProjectWorkflowIdentity};
pub use stage_sink::WorkflowStageSink;

#[derive(Default)]
pub struct WorkflowService {
    pub coordinator: WorkflowCoordinator,
}
