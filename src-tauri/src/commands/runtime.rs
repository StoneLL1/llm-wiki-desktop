use tauri::{AppHandle, Manager};

use crate::app_state::AppState;
use crate::errors::BackendError;
use crate::services::BlockingWorkClass;

pub async fn run_blocking<R, F>(
    app: AppHandle,
    class: BlockingWorkClass,
    operation: F,
) -> Result<R, BackendError>
where
    R: Send + 'static,
    F: FnOnce(AppHandle) -> Result<R, BackendError> + Send + 'static,
{
    let coordinator = app.state::<AppState>().blocking_work.clone();
    coordinator.run(class, move || operation(app)).await
}
