use std::cell::Cell;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once, OnceLock, Weak};
use std::time::{Duration, Instant};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::errors::BackendError;
use crate::tasks::task_model::CancellationToken;

const BLOCKING_WORK_CANCELLED: &str = "BLOCKING_WORK_CANCELLED";
const BLOCKING_WORK_JOIN_FAILED: &str = "BLOCKING_WORK_JOIN_FAILED";
const ADMISSION_POLL_INTERVAL: Duration = Duration::from_millis(10);
const BLOCKING_WORK_TRACE_PATH_ENV: &str = "LLM_WIKI_BLOCKING_TRACE_PATH";

thread_local! {
    static IN_BLOCKING_WORK_PANIC_BOUNDARY: Cell<bool> = const { Cell::new(false) };
}

struct BlockingPanicBoundary;

impl BlockingPanicBoundary {
    fn enter() -> Self {
        IN_BLOCKING_WORK_PANIC_BOUNDARY.set(true);
        Self
    }
}

impl Drop for BlockingPanicBoundary {
    fn drop(&mut self) {
        IN_BLOCKING_WORK_PANIC_BOUNDARY.set(false);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingWorkClass {
    MetadataIo,
    HeavyIo,
    ProcessProbe,
    ProjectGit,
}

impl BlockingWorkClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MetadataIo => "metadata_io",
            Self::HeavyIo => "heavy_io",
            Self::ProcessProbe => "process_probe",
            Self::ProjectGit => "project_git",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::MetadataIo => 0,
            Self::HeavyIo => 1,
            Self::ProcessProbe => 2,
            Self::ProjectGit => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingWorkOperation {
    Unspecified,
    ProjectFactsGitStatus,
    ProjectFactsAgentDetection,
    ProjectFactsProviderStatus,
}

impl BlockingWorkOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unspecified => "unspecified",
            Self::ProjectFactsGitStatus => "project_facts_git_status",
            Self::ProjectFactsAgentDetection => "project_facts_agent_detection",
            Self::ProjectFactsProviderStatus => "project_facts_provider_status",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockingWorkClassSnapshot {
    pub class: BlockingWorkClass,
    pub started: u64,
    pub completed: u64,
    pub cancelled_before_start: u64,
    pub failed: u64,
    pub max_queue_wait_nanos: u64,
    pub max_run_nanos: u64,
}

#[derive(Debug, Default)]
struct BlockingWorkClassStats {
    started: AtomicU64,
    completed: AtomicU64,
    cancelled_before_start: AtomicU64,
    failed: AtomicU64,
    max_queue_wait_nanos: AtomicU64,
    max_run_nanos: AtomicU64,
}

impl BlockingWorkClassStats {
    fn snapshot(&self, class: BlockingWorkClass) -> BlockingWorkClassSnapshot {
        BlockingWorkClassSnapshot {
            class,
            started: self.started.load(Ordering::Relaxed),
            completed: self.completed.load(Ordering::Relaxed),
            cancelled_before_start: self.cancelled_before_start.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            max_queue_wait_nanos: self.max_queue_wait_nanos.load(Ordering::Relaxed),
            max_run_nanos: self.max_run_nanos.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct BlockingWorkLimits {
    metadata_io: usize,
    heavy_io: usize,
    process_probe: usize,
}

impl Default for BlockingWorkLimits {
    fn default() -> Self {
        Self {
            metadata_io: 4,
            heavy_io: 2,
            process_probe: 2,
        }
    }
}

#[derive(Debug)]
struct BlockingWorkInner {
    metadata_io: Arc<Semaphore>,
    heavy_io: Arc<Semaphore>,
    process_probe: Arc<Semaphore>,
    project_git: Mutex<HashMap<String, Weak<Semaphore>>>,
    stats: [BlockingWorkClassStats; 4],
}

#[derive(Debug, Clone)]
pub struct BlockingWorkCoordinator {
    inner: Arc<BlockingWorkInner>,
}

impl Default for BlockingWorkCoordinator {
    fn default() -> Self {
        Self::with_limits(BlockingWorkLimits::default())
    }
}

impl BlockingWorkCoordinator {
    fn with_limits(limits: BlockingWorkLimits) -> Self {
        install_sanitized_worker_panic_hook();
        assert!(limits.metadata_io > 0);
        assert!(limits.heavy_io > 0);
        assert!(limits.process_probe > 0);
        Self {
            inner: Arc::new(BlockingWorkInner {
                metadata_io: Arc::new(Semaphore::new(limits.metadata_io)),
                heavy_io: Arc::new(Semaphore::new(limits.heavy_io)),
                process_probe: Arc::new(Semaphore::new(limits.process_probe)),
                project_git: Mutex::new(HashMap::new()),
                stats: std::array::from_fn(|_| BlockingWorkClassStats::default()),
            }),
        }
    }

    pub async fn run<R, F>(&self, class: BlockingWorkClass, operation: F) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_named(class, BlockingWorkOperation::Unspecified, operation)
            .await
    }

    pub async fn run_named<R, F>(
        &self,
        class: BlockingWorkClass,
        operation_label: BlockingWorkOperation,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_with_admission(
            class,
            operation_label,
            self.semaphore(class),
            None,
            operation,
        )
        .await
    }

    pub(crate) async fn run_project_facts_agent<R, F>(
        &self,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_named(
            BlockingWorkClass::ProcessProbe,
            BlockingWorkOperation::ProjectFactsAgentDetection,
            operation,
        )
        .await
    }

    pub(crate) async fn run_project_facts_provider<R, F>(
        &self,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_named(
            BlockingWorkClass::HeavyIo,
            BlockingWorkOperation::ProjectFactsProviderStatus,
            operation,
        )
        .await
    }

    /// Resolve project authority/identity under MetadataIo, release that
    /// permit, then wait for the canonical project Git lane. The token carries
    /// the authority revision into the Git worker for drift revalidation.
    pub(crate) async fn run_project_facts_git<T, R, M, G>(
        &self,
        metadata: M,
        git: G,
    ) -> Result<R, BackendError>
    where
        T: Send + 'static,
        R: Send + 'static,
        M: FnOnce() -> Result<(String, T), BackendError> + Send + 'static,
        G: FnOnce(T) -> Result<R, BackendError> + Send + 'static,
    {
        let (canonical_identity, authority_token) = self
            .run_named(
                BlockingWorkClass::MetadataIo,
                BlockingWorkOperation::ProjectFactsGitStatus,
                metadata,
            )
            .await?;
        self.run_project_git_named(
            canonical_identity,
            BlockingWorkOperation::ProjectFactsGitStatus,
            None,
            move || git(authority_token),
        )
        .await
    }

    pub async fn run_cancellable<R, F>(
        &self,
        class: BlockingWorkClass,
        cancellation: CancellationToken,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_cancellable_named(
            class,
            BlockingWorkOperation::Unspecified,
            cancellation,
            operation,
        )
        .await
    }

    pub async fn run_cancellable_named<R, F>(
        &self,
        class: BlockingWorkClass,
        operation_label: BlockingWorkOperation,
        cancellation: CancellationToken,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_with_admission(
            class,
            operation_label,
            self.semaphore(class),
            Some(cancellation),
            operation,
        )
        .await
    }

    pub async fn run_project_git<R, F>(
        &self,
        canonical_project_identity: String,
        cancellation: Option<CancellationToken>,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        self.run_project_git_named(
            canonical_project_identity,
            BlockingWorkOperation::Unspecified,
            cancellation,
            operation,
        )
        .await
    }

    pub async fn run_project_git_named<R, F>(
        &self,
        canonical_project_identity: String,
        operation_label: BlockingWorkOperation,
        cancellation: Option<CancellationToken>,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        let semaphore = self.project_git_semaphore(canonical_project_identity);
        self.run_with_admission(
            BlockingWorkClass::ProjectGit,
            operation_label,
            semaphore,
            cancellation,
            operation,
        )
        .await
    }

    /// Enters the canonical-project Git lane from an existing blocking worker.
    /// Callers must acquire project authority/write access before this method so
    /// the global lock order remains authority -> project Git -> domain locks.
    pub(crate) fn run_project_git_blocking<R, F>(
        &self,
        canonical_project_identity: String,
        cancellation: Option<&CancellationToken>,
        operation: F,
    ) -> Result<R, BackendError>
    where
        F: FnOnce() -> Result<R, BackendError>,
    {
        self.run_project_git_blocking_named(
            canonical_project_identity,
            BlockingWorkOperation::Unspecified,
            cancellation,
            operation,
        )
    }

    pub(crate) fn run_project_git_blocking_named<R, F>(
        &self,
        canonical_project_identity: String,
        operation_label: BlockingWorkOperation,
        cancellation: Option<&CancellationToken>,
        operation: F,
    ) -> Result<R, BackendError>
    where
        F: FnOnce() -> Result<R, BackendError>,
    {
        let class = BlockingWorkClass::ProjectGit;
        let queued_at = Instant::now();
        let permit = match acquire_permit_blocking(
            self.project_git_semaphore(canonical_project_identity),
            cancellation,
        ) {
            Ok(permit) => permit,
            Err(error) => {
                self.stats(class)
                    .cancelled_before_start
                    .fetch_add(1, Ordering::Relaxed);
                return Err(error);
            }
        };
        let queue_wait_nanos = elapsed_nanos(queued_at);
        update_max(&self.stats(class).max_queue_wait_nanos, queue_wait_nanos);
        self.stats(class).started.fetch_add(1, Ordering::Relaxed);
        let thread = format!("{:?}", std::thread::current().id());
        let started_at = Instant::now();
        let result = operation();
        let run_nanos = elapsed_nanos(started_at);
        drop(permit);
        update_max(&self.stats(class).max_run_nanos, run_nanos);
        match &result {
            Ok(_) => self.stats(class).completed.fetch_add(1, Ordering::Relaxed),
            Err(_) => self.stats(class).failed.fetch_add(1, Ordering::Relaxed),
        };
        write_perf_span(
            class,
            operation_label,
            &thread,
            &thread,
            queue_wait_nanos,
            run_nanos,
            result.as_ref().err().map(|error| error.code.as_str()),
        );
        result
    }

    pub fn snapshot(&self) -> [BlockingWorkClassSnapshot; 4] {
        [
            self.stats(BlockingWorkClass::MetadataIo)
                .snapshot(BlockingWorkClass::MetadataIo),
            self.stats(BlockingWorkClass::HeavyIo)
                .snapshot(BlockingWorkClass::HeavyIo),
            self.stats(BlockingWorkClass::ProcessProbe)
                .snapshot(BlockingWorkClass::ProcessProbe),
            self.stats(BlockingWorkClass::ProjectGit)
                .snapshot(BlockingWorkClass::ProjectGit),
        ]
    }

    fn semaphore(&self, class: BlockingWorkClass) -> Arc<Semaphore> {
        match class {
            BlockingWorkClass::MetadataIo => Arc::clone(&self.inner.metadata_io),
            BlockingWorkClass::HeavyIo => Arc::clone(&self.inner.heavy_io),
            BlockingWorkClass::ProcessProbe => Arc::clone(&self.inner.process_probe),
            BlockingWorkClass::ProjectGit => {
                unreachable!("project Git work requires a canonical project identity")
            }
        }
    }

    fn project_git_semaphore(&self, canonical_project_identity: String) -> Arc<Semaphore> {
        let mut registry = self
            .inner
            .project_git
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        registry.retain(|_, semaphore| semaphore.strong_count() > 0);
        if let Some(semaphore) = registry
            .get(&canonical_project_identity)
            .and_then(Weak::upgrade)
        {
            return semaphore;
        }
        let semaphore = Arc::new(Semaphore::new(1));
        registry.insert(canonical_project_identity, Arc::downgrade(&semaphore));
        semaphore
    }

    fn stats(&self, class: BlockingWorkClass) -> &BlockingWorkClassStats {
        &self.inner.stats[class.index()]
    }

    async fn run_with_admission<R, F>(
        &self,
        class: BlockingWorkClass,
        operation_label: BlockingWorkOperation,
        semaphore: Arc<Semaphore>,
        cancellation: Option<CancellationToken>,
        operation: F,
    ) -> Result<R, BackendError>
    where
        R: Send + 'static,
        F: FnOnce() -> Result<R, BackendError> + Send + 'static,
    {
        let queued_at = Instant::now();
        let permit = match acquire_permit(semaphore, cancellation.as_ref()).await {
            Ok(permit) => permit,
            Err(error) => {
                self.stats(class)
                    .cancelled_before_start
                    .fetch_add(1, Ordering::Relaxed);
                return Err(error);
            }
        };
        if cancellation
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            drop(permit);
            self.stats(class)
                .cancelled_before_start
                .fetch_add(1, Ordering::Relaxed);
            return Err(blocking_work_cancelled());
        }
        let queue_wait_nanos = elapsed_nanos(queued_at);
        update_max(&self.stats(class).max_queue_wait_nanos, queue_wait_nanos);
        let caller_thread = format!("{:?}", std::thread::current().id());
        let cancellation_for_worker = cancellation.clone();
        let stats_inner = Arc::clone(&self.inner);
        let joined = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            if cancellation_for_worker
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                stats_inner.stats[class.index()]
                    .cancelled_before_start
                    .fetch_add(1, Ordering::Relaxed);
                return (Err(blocking_work_cancelled()), 0, true);
            }
            stats_inner.stats[class.index()]
                .started
                .fetch_add(1, Ordering::Relaxed);
            let _panic_boundary = BlockingPanicBoundary::enter();
            let worker_started_at = Instant::now();
            let worker_thread = format!("{:?}", std::thread::current().id());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation));
            let run_nanos = elapsed_nanos(worker_started_at);
            match result {
                Ok(result) => {
                    write_perf_span(
                        class,
                        operation_label,
                        &caller_thread,
                        &worker_thread,
                        queue_wait_nanos,
                        run_nanos,
                        result.as_ref().err().map(|error| error.code.as_str()),
                    );
                    (result, run_nanos, false)
                }
                Err(payload) => {
                    write_perf_span(
                        class,
                        operation_label,
                        &caller_thread,
                        &worker_thread,
                        queue_wait_nanos,
                        run_nanos,
                        Some(BLOCKING_WORK_JOIN_FAILED),
                    );
                    std::panic::resume_unwind(payload)
                }
            }
        })
        .await;

        match joined {
            Ok((Ok(value), run_nanos, false)) => {
                update_max(&self.stats(class).max_run_nanos, run_nanos);
                self.stats(class).completed.fetch_add(1, Ordering::Relaxed);
                Ok(value)
            }
            Ok((Err(error), _, true)) => Err(error),
            Ok((Err(error), run_nanos, false)) => {
                update_max(&self.stats(class).max_run_nanos, run_nanos);
                self.stats(class).failed.fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
            Ok((Ok(_), _, true)) => unreachable!("cancelled workers cannot return success"),
            Err(_) => {
                self.stats(class).failed.fetch_add(1, Ordering::Relaxed);
                Err(BackendError::new(
                    BLOCKING_WORK_JOIN_FAILED,
                    format!("{} worker did not complete.", class.as_str()),
                    true,
                    false,
                ))
            }
        }
    }
}

fn install_sanitized_worker_panic_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if IN_BLOCKING_WORK_PANIC_BOUNDARY.get() {
                eprintln!("blocking worker panicked; payload suppressed");
            } else {
                previous(info);
            }
        }));
    });
}

fn blocking_work_cancelled() -> BackendError {
    BackendError::new(
        BLOCKING_WORK_CANCELLED,
        "Blocking work was cancelled before it started.",
        true,
        false,
    )
}

fn write_perf_span(
    class: BlockingWorkClass,
    operation: BlockingWorkOperation,
    caller_thread: &str,
    worker_thread: &str,
    queue_wait_nanos: u64,
    run_nanos: u64,
    error_code: Option<&str>,
) {
    let Some(path) = std::env::var_os(BLOCKING_WORK_TRACE_PATH_ENV) else {
        return;
    };
    append_perf_span(
        Path::new(&path),
        class,
        operation,
        caller_thread,
        worker_thread,
        queue_wait_nanos,
        run_nanos,
        error_code,
    );
}

#[allow(clippy::too_many_arguments)]
fn append_perf_span(
    path: &Path,
    class: BlockingWorkClass,
    operation: BlockingWorkOperation,
    caller_thread: &str,
    worker_thread: &str,
    queue_wait_nanos: u64,
    run_nanos: u64,
    error_code: Option<&str>,
) {
    static TRACE_WRITE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = TRACE_WRITE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let error_code = error_code.map(sanitize_error_code);
    let payload = serde_json::json!({
        "class": class.as_str(),
        "operation": operation.as_str(),
        "callerThread": caller_thread,
        "workerThread": worker_thread,
        "queueWaitNanos": queue_wait_nanos,
        "runNanos": run_nanos,
        "outcome": if error_code.is_some() { "error" } else { "success" },
        "errorCode": error_code,
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{payload}");
    }
}

fn sanitize_error_code(value: &str) -> &str {
    match value {
        "BLOCKING_WORK_CANCELLED" | "BLOCKING_WORK_JOIN_FAILED" | "BLOCKING_WORK_PANIC" => value,
        _ => "OTHER_ERROR",
    }
}

async fn acquire_permit(
    semaphore: Arc<Semaphore>,
    cancellation: Option<&CancellationToken>,
) -> Result<OwnedSemaphorePermit, BackendError> {
    loop {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(blocking_work_cancelled());
        }
        match tokio::time::timeout(
            ADMISSION_POLL_INTERVAL,
            Arc::clone(&semaphore).acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => return Ok(permit),
            Ok(Err(_)) => {
                return Err(BackendError::new(
                    BLOCKING_WORK_JOIN_FAILED,
                    "Blocking work admission closed unexpectedly.",
                    true,
                    false,
                ));
            }
            Err(_) => continue,
        }
    }
}

fn acquire_permit_blocking(
    semaphore: Arc<Semaphore>,
    cancellation: Option<&CancellationToken>,
) -> Result<OwnedSemaphorePermit, BackendError> {
    loop {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(blocking_work_cancelled());
        }
        match Arc::clone(&semaphore).try_acquire_owned() {
            Ok(permit) => {
                if cancellation.is_some_and(CancellationToken::is_cancelled) {
                    drop(permit);
                    return Err(blocking_work_cancelled());
                }
                return Ok(permit);
            }
            Err(tokio::sync::TryAcquireError::NoPermits) => {
                std::thread::sleep(ADMISSION_POLL_INTERVAL);
            }
            Err(tokio::sync::TryAcquireError::Closed) => {
                return Err(BackendError::new(
                    BLOCKING_WORK_JOIN_FAILED,
                    "Blocking work admission closed unexpectedly.",
                    true,
                    false,
                ));
            }
        }
    }
}

fn elapsed_nanos(started_at: Instant) -> u64 {
    started_at.elapsed().as_nanos().min(u64::MAX as u128) as u64
}

fn update_max(value: &AtomicU64, candidate: u64) {
    let mut current = value.load(Ordering::Relaxed);
    while candidate > current {
        match value.compare_exchange_weak(current, candidate, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{
        BlockingWorkClass, BlockingWorkCoordinator, BlockingWorkLimits, BlockingWorkOperation,
    };
    use crate::errors::BackendError;
    use crate::tasks::task_model::CancellationToken;

    fn track_concurrency(current: &AtomicUsize, maximum: &AtomicUsize) {
        let active = current.fetch_add(1, Ordering::SeqCst) + 1;
        maximum.fetch_max(active, Ordering::SeqCst);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn metadata_admission_never_exceeds_the_configured_limit() {
        let coordinator = BlockingWorkCoordinator::with_limits(BlockingWorkLimits {
            metadata_io: 2,
            heavy_io: 1,
            process_probe: 2,
        });
        let current = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();

        for _ in 0..6 {
            let coordinator = coordinator.clone();
            let current = Arc::clone(&current);
            let maximum = Arc::clone(&maximum);
            tasks.push(tokio::spawn(async move {
                coordinator
                    .run(BlockingWorkClass::MetadataIo, move || {
                        track_concurrency(&current, &maximum);
                        std::thread::sleep(Duration::from_millis(25));
                        current.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .await
            }));
        }
        for task in tasks {
            task.await.unwrap().unwrap();
        }

        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn process_probe_admission_is_bounded_and_does_not_starve_metadata() {
        let coordinator = BlockingWorkCoordinator::with_limits(BlockingWorkLimits {
            metadata_io: 1,
            heavy_io: 1,
            process_probe: 2,
        });
        let current = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut probes = Vec::new();
        for _ in 0..6 {
            let coordinator = coordinator.clone();
            let current = Arc::clone(&current);
            let maximum = Arc::clone(&maximum);
            probes.push(tokio::spawn(async move {
                coordinator
                    .run_project_facts_agent(move || {
                        track_concurrency(&current, &maximum);
                        std::thread::sleep(Duration::from_millis(30));
                        current.fetch_sub(1, Ordering::SeqCst);
                        Ok(())
                    })
                    .await
            }));
        }

        tokio::time::timeout(
            Duration::from_secs(2),
            coordinator.run(BlockingWorkClass::MetadataIo, || Ok(())),
        )
        .await
        .expect("queued process probes must not consume MetadataIo admission")
        .unwrap();

        for probe in probes {
            probe.await.unwrap().unwrap();
        }
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
        assert_eq!(coordinator.snapshot().len(), 4);
        assert_eq!(
            coordinator.snapshot()[2].class,
            BlockingWorkClass::ProcessProbe
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn queued_process_probe_does_not_delay_a_metadata_worker() {
        let coordinator = BlockingWorkCoordinator::with_limits(BlockingWorkLimits {
            metadata_io: 1,
            heavy_io: 1,
            process_probe: 1,
        });
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let holder = {
            let coordinator = coordinator.clone();
            tokio::spawn(async move {
                coordinator
                    .run_project_facts_agent(move || {
                        entered_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(())
                    })
                    .await
            })
        };
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();

        let queued = {
            let coordinator = coordinator.clone();
            tokio::spawn(async move { coordinator.run_project_facts_agent(|| Ok(())).await })
        };
        tokio::time::sleep(Duration::from_millis(15)).await;
        let metadata_worker = coordinator
            .run(BlockingWorkClass::MetadataIo, || {
                Ok(std::thread::current().id())
            })
            .await
            .unwrap();
        assert_ne!(metadata_worker, std::thread::current().id());

        release_tx.send(()).unwrap();
        holder.await.unwrap().unwrap();
        queued.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn worker_panic_is_typed_and_the_lane_remains_usable() {
        let coordinator = BlockingWorkCoordinator::default();
        let error = coordinator
            .run(
                BlockingWorkClass::HeavyIo,
                || -> Result<(), BackendError> { panic!("private panic payload") },
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, "BLOCKING_WORK_JOIN_FAILED");
        assert!(!error.message.contains("private panic payload"));

        coordinator
            .run(BlockingWorkClass::HeavyIo, || Ok(()))
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_facts_command_core_runs_every_slow_stage_on_named_workers() {
        let coordinator = BlockingWorkCoordinator::default();
        let caller = std::thread::current().id();
        let agent_worker = coordinator
            .run_project_facts_agent(|| {
                std::thread::sleep(Duration::from_millis(10));
                Ok(std::thread::current().id())
            })
            .await
            .unwrap();
        let provider_worker = coordinator
            .run_project_facts_provider(|| {
                std::thread::sleep(Duration::from_millis(10));
                Ok(std::thread::current().id())
            })
            .await
            .unwrap();
        let (metadata_worker, git_worker) = coordinator
            .run_project_facts_git(
                || {
                    std::thread::sleep(Duration::from_millis(10));
                    Ok(("project-a".into(), std::thread::current().id()))
                },
                |metadata_worker| {
                    std::thread::sleep(Duration::from_millis(10));
                    Ok((metadata_worker, std::thread::current().id()))
                },
            )
            .await
            .unwrap();

        assert_ne!(caller, agent_worker);
        assert_ne!(caller, provider_worker);
        assert_ne!(caller, metadata_worker);
        assert_ne!(caller, git_worker);
        let snapshot = coordinator.snapshot();
        assert_eq!(snapshot[0].started, 1);
        assert_eq!(snapshot[1].started, 1);
        assert_eq!(snapshot[2].started, 1);
        assert_eq!(snapshot[3].started, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn worker_panic_writes_a_named_anonymous_failure_span() {
        static TRACE_ENV_LOCK: Mutex<()> = Mutex::new(());
        let _env_guard = TRACE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let directory = tempfile::tempdir().unwrap();
        let trace_path = directory.path().join("panic-spans.jsonl");
        std::env::set_var(super::BLOCKING_WORK_TRACE_PATH_ENV, &trace_path);

        let coordinator = BlockingWorkCoordinator::default();
        let error = coordinator
            .run_project_facts_provider(|| -> Result<(), BackendError> {
                panic!("private provider binding payload")
            })
            .await
            .unwrap_err();
        std::env::remove_var(super::BLOCKING_WORK_TRACE_PATH_ENV);

        assert_eq!(error.code, super::BLOCKING_WORK_JOIN_FAILED);
        let trace = fs::read_to_string(trace_path).unwrap();
        let span = trace
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|span| span["operation"] == "project_facts_provider_status")
            .expect("the panicking named operation must emit a span");
        assert_eq!(span["class"], "heavy_io");
        assert_eq!(span["outcome"], "error");
        assert_eq!(span["errorCode"], super::BLOCKING_WORK_JOIN_FAILED);
        assert!(!trace.contains("private provider binding payload"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn project_git_lane_serializes_one_identity_and_allows_different_projects() {
        let coordinator = BlockingWorkCoordinator::default();
        let same_current = Arc::new(AtomicUsize::new(0));
        let same_maximum = Arc::new(AtomicUsize::new(0));
        let mut same_tasks = Vec::new();
        for _ in 0..2 {
            let coordinator = coordinator.clone();
            let current = Arc::clone(&same_current);
            let maximum = Arc::clone(&same_maximum);
            same_tasks.push(tokio::spawn(async move {
                coordinator
                    .run_project_git_named(
                        "project-a".into(),
                        BlockingWorkOperation::ProjectFactsGitStatus,
                        None,
                        move || {
                            track_concurrency(&current, &maximum);
                            std::thread::sleep(Duration::from_millis(30));
                            current.fetch_sub(1, Ordering::SeqCst);
                            Ok(())
                        },
                    )
                    .await
            }));
        }
        for task in same_tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(same_maximum.load(Ordering::SeqCst), 1);

        let different_current = Arc::new(AtomicUsize::new(0));
        let different_maximum = Arc::new(AtomicUsize::new(0));
        let mut different_tasks = Vec::new();
        for project in ["project-a", "project-b"] {
            let coordinator = coordinator.clone();
            let current = Arc::clone(&different_current);
            let maximum = Arc::clone(&different_maximum);
            different_tasks.push(tokio::spawn(async move {
                coordinator
                    .run_project_git_named(
                        project.into(),
                        BlockingWorkOperation::ProjectFactsGitStatus,
                        None,
                        move || {
                            track_concurrency(&current, &maximum);
                            std::thread::sleep(Duration::from_millis(30));
                            current.fetch_sub(1, Ordering::SeqCst);
                            Ok(())
                        },
                    )
                    .await
            }));
        }
        for task in different_tasks {
            task.await.unwrap().unwrap();
        }
        assert_eq!(different_maximum.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn queued_cancellation_never_starts_the_operation() {
        let coordinator = BlockingWorkCoordinator::with_limits(BlockingWorkLimits {
            metadata_io: 1,
            heavy_io: 1,
            process_probe: 1,
        });
        let holder = {
            let coordinator = coordinator.clone();
            tokio::spawn(async move {
                coordinator
                    .run(BlockingWorkClass::MetadataIo, || {
                        std::thread::sleep(Duration::from_millis(100));
                        Ok(())
                    })
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(15)).await;

        let cancellation = CancellationToken::new();
        let started = Arc::new(AtomicUsize::new(0));
        let waiter = {
            let coordinator = coordinator.clone();
            let cancellation_for_waiter = cancellation.clone();
            let started = Arc::clone(&started);
            tokio::spawn(async move {
                coordinator
                    .run_cancellable(
                        BlockingWorkClass::MetadataIo,
                        cancellation_for_waiter,
                        move || {
                            started.fetch_add(1, Ordering::SeqCst);
                            Ok(())
                        },
                    )
                    .await
            })
        };
        tokio::time::sleep(Duration::from_millis(15)).await;
        cancellation.cancel();

        let error = waiter.await.unwrap().unwrap_err();
        assert_eq!(error.code, "BLOCKING_WORK_CANCELLED");
        assert_eq!(started.load(Ordering::SeqCst), 0);
        holder.await.unwrap().unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn work_runs_on_a_blocking_thread_and_observations_are_anonymous() {
        let coordinator = BlockingWorkCoordinator::default();
        let caller = std::thread::current().id();
        let worker = coordinator
            .run(BlockingWorkClass::MetadataIo, || {
                let _private_payload = "C:\\private\\secret-source.md";
                Ok(std::thread::current().id())
            })
            .await
            .unwrap();
        assert_ne!(caller, worker);

        let snapshot = format!("{:?}", coordinator.snapshot());
        assert!(snapshot.contains("MetadataIo"));
        assert!(!snapshot.contains("secret-source"));
        assert!(!snapshot.contains("private"));
    }

    #[test]
    fn perf_span_contains_only_anonymous_execution_facts() {
        let directory = tempfile::tempdir().unwrap();
        let trace_path = directory.path().join("blocking-spans.jsonl");
        let private_payload = "C:\\private\\secret-source.md";

        super::append_perf_span(
            &trace_path,
            BlockingWorkClass::HeavyIo,
            BlockingWorkOperation::ProjectFactsProviderStatus,
            "ThreadId(1)",
            "ThreadId(2)",
            17,
            23,
            Some(private_payload),
        );

        let trace = fs::read_to_string(trace_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(trace.trim()).unwrap();
        assert_eq!(value["class"], "heavy_io");
        assert_eq!(value["operation"], "project_facts_provider_status");
        assert_eq!(value["callerThread"], "ThreadId(1)");
        assert_eq!(value["workerThread"], "ThreadId(2)");
        assert_eq!(value["errorCode"], "OTHER_ERROR");
        let keys = value
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                "callerThread",
                "class",
                "errorCode",
                "operation",
                "outcome",
                "queueWaitNanos",
                "runNanos",
                "workerThread",
            ]
        );
        assert!(!trace.contains("secret-source"));
        assert!(!trace.contains("private"));
        assert_eq!(super::sanitize_error_code("SK_LIVE_ABC123"), "OTHER_ERROR");
    }

    #[test]
    fn project_facts_operation_labels_are_closed_and_path_free() {
        let labels = [
            BlockingWorkOperation::ProjectFactsGitStatus.as_str(),
            BlockingWorkOperation::ProjectFactsAgentDetection.as_str(),
            BlockingWorkOperation::ProjectFactsProviderStatus.as_str(),
        ];

        assert_eq!(
            labels,
            [
                "project_facts_git_status",
                "project_facts_agent_detection",
                "project_facts_provider_status",
            ]
        );
        assert!(labels.iter().all(|label| !label.contains('\\')));
        assert!(labels.iter().all(|label| !label.contains('/')));
        assert!(labels.iter().all(|label| !label.contains("secret")));
    }
}
