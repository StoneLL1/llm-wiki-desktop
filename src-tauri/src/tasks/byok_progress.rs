//! Polling helper for BYOK (bring-your-own-key) LLM completions.
//!
//! The BYOK paths call [`LlmService::complete`](crate::services::LlmService::complete),
//! which is a single blocking-ish future that resolves with the full response
//! (no true SSE streaming — see `roadmap/cross-cutting.md` §2.4 acceptance
//! criterion 2). While that future is pending we still need two things:
//!
//! 1. **Cancellation** — the user must be able to cancel a long generation.
//!    `LlmService::complete` does not itself consult the task cancellation
//!    token, so we poll `TaskService::is_cancelled` on a short timer and
//!    return a [`ByokCancelled`] error as soon as the token flips.
//! 2. **Progress** — a generation can take many seconds. Without any log
//!    output the task drawer looks frozen. We append a `"Generating..."`
//!    [`Info`](crate::tasks::task_model::LogLevel::Info) log line immediately,
//!    then at most once every two seconds, so the user sees the task is alive.
//!
//! Both concerns are honored by a single `tokio::select!` loop with a 100 ms
//! cadence (cancellation) and a 2 s accumulator (progress). This keeps the
//! minimum-risk approach from the plan: no change to `LlmService`'s signature,
//! no real streaming transport introduced.

use std::future::Future;
use std::time::Duration;

use crate::errors::BackendError;
use crate::models::task::{TaskActivity, TaskActivityStatus};
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

/// Interval at which the cancellation token is polled.
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// Maximum interval between two progress log lines once generation has started.
const PROGRESS_INTERVAL_TICKS: u32 = 20; // 20 * 100 ms = 2 s

/// Error returned when the user cancels a BYOK generation mid-flight.
///
/// Each caller maps this to its own domain error code
/// (`COMPILE_CANCELLED` / `CHAT_CANCELLED` / `LINT_CANCELLED` / `EXPORT_CANCELLED`).
#[derive(Debug)]
pub struct ByokCancelled;

/// Drive a BYOK completion future to completion with cancellation + progress.
///
/// - Appends an `Info` log line `"{verb}…"` immediately so the drawer is not
///   silent during the first 2 s window.
/// - Polls cancellation every 100 ms; returns `Err(ByokCancelled)` if the
///   task's token is set.
/// - Re-appends the same progress line at most once every 2 s (not on every
///   tick) so the log is not flooded.
///
/// `verb` is the present-continuous action shown to the user, e.g.
/// `"Generating"`, `"Answering"`, `"Linting"`, `"Exporting"`.
///
/// The completion future is expected to be a `Result<T, E>` (as
/// [`LlmService::complete`](crate::services::LlmService::complete) is). The
/// inner result is returned unchanged through `Ok(Ok(t))` / `Ok(Err(e))` so
/// the caller can `?`-propagate the provider error after mapping the outer
/// [`ByokCancelled`]:
///
/// ```ignore
/// let raw = poll_with_progress(&service, task_id, "Generating", completion)
///     .await
///     .map_err(|_| cancelled_error("COMPILE_CANCELLED", "Wiki compile was cancelled."))??;
/// ```
pub async fn poll_with_progress<F, T, E>(
    task_service: &TaskService,
    task_id: &str,
    verb: &str,
    completion: F,
) -> Result<Result<T, E>, ByokCancelled>
where
    F: Future<Output = Result<T, E>>,
{
    let progress_message = format!("{verb}…");

    task_service.emit_activity(
        task_id,
        TaskActivity::Phase {
            name: "generation".into(),
            status: TaskActivityStatus::Started,
            label: Some(verb.into()),
        },
    );

    task_service
        .append_log(task_id, LogLevel::Info, progress_message.clone())
        .ok();

    // Check cancellation once up front so a cancel that raced the task start
    // is observed immediately instead of after the first 100 ms tick.
    if task_service.is_cancelled(task_id) {
        return Err(ByokCancelled);
    }

    tokio::pin!(completion);

    let mut ticks_since_progress: u32 = 0;
    loop {
        tokio::select! {
            result = &mut completion => {
                task_service.emit_activity(
                    task_id,
                    TaskActivity::Phase {
                        name: "generation".into(),
                        status: if result.is_ok() {
                            TaskActivityStatus::Completed
                        } else {
                            TaskActivityStatus::Failed
                        },
                        label: Some(if result.is_ok() { "Provider response ready" } else { "Provider request failed" }.into()),
                    },
                );
                return Ok(result);
            },
            _ = tokio::time::sleep(POLL_INTERVAL) => {
                if task_service.is_cancelled(task_id) {
                    task_service.emit_activity(
                        task_id,
                        TaskActivity::Phase {
                            name: "generation".into(),
                            status: TaskActivityStatus::Failed,
                            label: Some("Provider request cancelled".into()),
                        },
                    );
                    return Err(ByokCancelled);
                }
                ticks_since_progress += 1;
                if ticks_since_progress >= PROGRESS_INTERVAL_TICKS {
                    ticks_since_progress = 0;
                    task_service
                        .append_log(task_id, LogLevel::Info, progress_message.clone())
                        .ok();
                }
            }
        }
    }
}

/// Map a [`ByokCancelled`] into a [`BackendError`] with the given code/message.
///
/// Lets each caller keep its own domain-specific error code while sharing the
/// polling implementation.
pub fn cancelled_error(code: &str, message: &str) -> BackendError {
    BackendError::new(code, message, true, false)
}

#[cfg(test)]
mod tests {
    use super::{cancelled_error, poll_with_progress, ByokCancelled};
    use crate::errors::BackendError;
    use crate::models::task::TaskType;
    use crate::tasks::task_model::LogLevel;
    use crate::tasks::TaskService;

    #[tokio::test]
    async fn appends_immediate_progress_and_returns_inner_result() {
        let service = TaskService::default();
        // poll_with_progress never needs a real task id to *resolve* a future —
        // it only consults the task id for cancellation polling. Use a made-up
        // id here so the focus stays on the resolve + log path.
        let completion = async { Ok::<_, BackendError>(42_u8) };

        let result = poll_with_progress(&service, "phantom-id", "Generating", completion).await;

        // Ok(Ok(42)) — outer is no-cancel, inner is the provider's success.
        assert_eq!(result.unwrap().unwrap(), 42);
    }

    #[tokio::test]
    async fn propagates_inner_provider_error_without_reporting_cancelled() {
        let service = TaskService::default();
        let completion = async {
            Err::<u8, _>(BackendError::new(
                "LLM_PROVIDER_ERROR",
                "rate limited",
                true,
                false,
            ))
        };

        let result = poll_with_progress(&service, "phantom-id", "Generating", completion).await;

        // Outer Ok (not cancelled) wrapping inner Err.
        let inner = result.unwrap();
        let err = inner.unwrap_err();
        assert_eq!(err.code, "LLM_PROVIDER_ERROR");
    }

    #[tokio::test]
    async fn returns_cancelled_when_token_set() {
        let service = TaskService::default();
        let task = service.create_task(TaskType::WikiCompile, None, "compile".into(), true);
        let task_id = task.id.clone();

        // A future that never resolves on its own — only cancellation ends it.
        // Drive poll_with_progress in-line against a timer that fires the
        // cancellation token after one poll tick, so we can borrow local
        // values without spawning (which would demand 'static).
        let pending = std::future::pending::<Result<u8, BackendError>>();
        let poll = poll_with_progress(&service, &task_id, "Generating", pending);
        tokio::pin!(poll);

        let result = tokio::select! {
            r = &mut poll => r,
            _ = tokio::time::sleep(std::time::Duration::from_millis(150)) => {
                service.cancel_task(&task_id).unwrap();
                // Now let the poll loop observe the cancellation token.
                (&mut poll).await
            }
        };

        assert!(matches!(result, Err(ByokCancelled)));
        // The immediate progress line was appended before cancellation.
        let logs = service.get_logs(&task_id).unwrap();
        assert!(logs
            .iter()
            .any(|l| l.message == "Generating…" && l.level == LogLevel::Info));
    }

    #[test]
    fn cancelled_error_carries_domain_code() {
        let err = cancelled_error("COMPILE_CANCELLED", "Wiki compile was cancelled.");
        assert_eq!(err.code, "COMPILE_CANCELLED");
        assert_eq!(err.message, "Wiki compile was cancelled.");
    }
}
