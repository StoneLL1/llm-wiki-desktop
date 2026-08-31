use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::services::{BlockingWorkClass, BlockingWorkOperation};

pub async fn run_blocking<R, F>(
    app: AppHandle,
    class: BlockingWorkClass,
    operation: F,
) -> Result<R, BackendError>
where
    R: Send + 'static,
    F: FnOnce(AppHandle) -> Result<R, BackendError> + Send + 'static,
{
    run_blocking_named(app, class, BlockingWorkOperation::Unspecified, operation).await
}

pub async fn run_blocking_named<R, F>(
    app: AppHandle,
    class: BlockingWorkClass,
    operation_label: BlockingWorkOperation,
    operation: F,
) -> Result<R, BackendError>
where
    R: Send + 'static,
    F: FnOnce(AppHandle) -> Result<R, BackendError> + Send + 'static,
{
    let coordinator = app.state::<AppState>().blocking_work.clone();
    coordinator
        .run_named(class, operation_label, move || operation(app))
        .await
}
