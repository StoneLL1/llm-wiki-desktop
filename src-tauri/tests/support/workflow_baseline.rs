#![allow(dead_code)]

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub const WORKFLOW_EVENT_COUNT: usize = 200;
pub const DRAWER_EVENT_COUNT: usize = 1_000;
pub const MARKDOWN_FILE_COUNT: usize = 1_000;
pub const PROGRESS_UPDATE_COUNT: usize = 500;
pub const SCOPE_OPTION_COUNT: usize = 10_000;
pub const HISTORY_ATTEMPT_COUNT: usize = 10_000;
pub const DIFF_FILE_COUNT: usize = 500;
pub const DIFF_BYTES: usize = 20 * 1_024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestAuthoritySnapshot {
    pub trusted: bool,
    pub writable: bool,
    pub runtime_project_id: String,
    pub canonical_identity_key: String,
    pub identity_revision: String,
}

#[derive(Clone)]
pub struct MutableTestAuthority {
    inner: Arc<Mutex<TestAuthoritySnapshot>>,
}

impl MutableTestAuthority {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(TestAuthoritySnapshot {
                trusted: true,
                writable: true,
                runtime_project_id: "runtime-a".into(),
                canonical_identity_key: "identity-a".into(),
                identity_revision: "revision-a".into(),
            })),
        }
    }

    pub fn snapshot(&self) -> TestAuthoritySnapshot {
        self.inner.lock().unwrap().clone()
    }

    pub fn replace(
        &self,
        trusted: bool,
        writable: bool,
        runtime_project_id: &str,
        canonical_identity_key: &str,
        identity_revision: &str,
    ) {
        *self.inner.lock().unwrap() = TestAuthoritySnapshot {
            trusted,
            writable,
            runtime_project_id: runtime_project_id.into(),
            canonical_identity_key: canonical_identity_key.into(),
            identity_revision: identity_revision.into(),
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RacePoint {
    Claimed,
    DispatchGuard,
    FirstStage,
    WorkerFinish,
}

pub struct RaceController {
    reached: mpsc::Receiver<RacePoint>,
    release: mpsc::Sender<()>,
}

pub struct RaceWorker {
    reached: mpsc::Sender<RacePoint>,
    release: mpsc::Receiver<()>,
}

pub fn controlled_race() -> (RaceController, RaceWorker) {
    let (reached_tx, reached_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    (
        RaceController {
            reached: reached_rx,
            release: release_tx,
        },
        RaceWorker {
            reached: reached_tx,
            release: release_rx,
        },
    )
}

impl RaceController {
    pub fn wait_for(&self, expected: RacePoint) {
        assert_eq!(
            self.reached
                .recv_timeout(Duration::from_secs(10))
                .expect("timed out waiting for controlled workflow race point"),
            expected,
        );
    }

    pub fn release(&self) {
        self.release.send(()).unwrap();
    }
}

impl RaceWorker {
    pub fn pause_at(&self, point: RacePoint) {
        self.reached.send(point).unwrap();
        self.release.recv().unwrap();
    }
}

pub fn markdown_paths() -> Vec<String> {
    (0..MARKDOWN_FILE_COUNT)
        .map(|index| format!("wiki/scale/页面-{index:04}.md"))
        .collect()
}

pub fn scope_options() -> Vec<(String, String)> {
    (0..SCOPE_OPTION_COUNT)
        .map(|index| (format!("source-{index:05}"), format!("version-{index:05}")))
        .collect()
}

pub fn history_attempts() -> Vec<String> {
    (0..HISTORY_ATTEMPT_COUNT)
        .map(|index| format!("workflow-{index:05}"))
        .collect()
}

pub fn fixed_diffs() -> Vec<(String, String)> {
    (0..DIFF_FILE_COUNT)
        .map(|index| {
            let path = format!("wiki/scale/diff-{index:04}.md");
            let prefix = format!("--- a/{path}\n+++ b/{path}\n");
            let mut diff = String::with_capacity(DIFF_BYTES);
            diff.push_str(&prefix);
            diff.extend(std::iter::repeat('x').take(DIFF_BYTES - prefix.len()));
            (path, diff)
        })
        .collect()
}

pub fn fixture_signature() -> String {
    let markdown = markdown_paths();
    let options = scope_options();
    let history = history_attempts();
    let diffs = fixed_diffs();
    let mut hash = 0xcbf29ce484222325_u64;
    let mut update = |value: &str| {
        for byte in value
            .as_bytes()
            .iter()
            .copied()
            .chain(std::iter::once(0xff))
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for value in [
        WORKFLOW_EVENT_COUNT,
        DRAWER_EVENT_COUNT,
        MARKDOWN_FILE_COUNT,
        PROGRESS_UPDATE_COUNT,
        SCOPE_OPTION_COUNT,
        HISTORY_ATTEMPT_COUNT,
        DIFF_FILE_COUNT,
        DIFF_BYTES,
    ] {
        update(&value.to_string());
    }
    for path in &markdown {
        update(path);
    }
    for (source, version) in &options {
        update(source);
        update(version);
    }
    for task_id in &history {
        update(task_id);
    }
    for (path, diff) in &diffs {
        update(path);
        update(diff);
    }
    format!("{hash:016x}")
}
