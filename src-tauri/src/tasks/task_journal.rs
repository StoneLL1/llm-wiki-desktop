//! Append-only observational history; lifecycle and publication facts stay in task snapshots.
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::models::task::TaskActivity;
use crate::tasks::task_model::LogLine;
use crate::utils::safe_project_dir::BoundProjectMutationRoot;

#[derive(Default)]
pub(crate) struct JournalCursor {
    pub path: Option<PathBuf>,
    pub logs: usize,
    pub activities: usize,
    bytes: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum JournalRecord {
    Log(LogLine),
    Activity(TaskActivity),
}

impl JournalCursor {
    pub fn append(
        &mut self,
        project_root: &Path,
        path: &Path,
        logs: &[LogLine],
        activities: &[TaskActivity],
    ) -> Result<(), String> {
        let initialize = self.path.as_deref() != Some(path);
        if !initialize && logs.is_empty() && activities.is_empty() {
            return Ok(());
        }
        let binding = BoundProjectMutationRoot::bind(project_root, path)
            .map_err(|error| format!("Task journal directory is unsafe: {error}"))?;
        let mut bytes = Vec::new();
        for log in logs {
            serde_json::to_writer(&mut bytes, &JournalRecord::Log(log.clone()))
                .map_err(|error| error.to_string())?;
            bytes.push(b'\n');
        }
        for activity in activities {
            serde_json::to_writer(&mut bytes, &JournalRecord::Activity(activity.clone()))
                .map_err(|error| error.to_string())?;
            bytes.push(b'\n');
        }
        if initialize {
            // Migrate inline history once, before the small snapshot advertises a journal.
            binding
                .write_atomic_replace(path, &bytes)
                .map_err(|error| format!("Failed to initialize task journal: {error}"))?;
            self.path = Some(path.to_path_buf());
            self.logs = logs.len();
            self.activities = activities.len();
            self.bytes = bytes.len() as u64;
        } else {
            let mut file = binding
                .open_regular_mutate_or_create(path, false)
                .map_err(|error| format!("Failed to open task journal: {error}"))?;
            if file.metadata().map_err(|error| error.to_string())?.len() < self.bytes {
                return Err("Task journal was truncated outside its persistence lane".into());
            }
            // A failed prior append may have left a partial tail. The cursor advances
            // only after sync, so retry replaces exactly that uncommitted tail.
            file.set_len(self.bytes)
                .and_then(|_| file.seek(SeekFrom::Start(self.bytes)))
                .and_then(|_| file.write_all(&bytes))
                .and_then(|_| file.sync_data())
                .map_err(|error| format!("Failed to append task journal: {error}"))?;
            self.logs += logs.len();
            self.activities += activities.len();
            self.bytes += bytes.len() as u64;
        }
        Ok(())
    }
}

pub(crate) fn recover(
    project_root: &Path,
    path: &Path,
) -> Result<(Vec<LogLine>, Vec<TaskActivity>, JournalCursor), String> {
    let binding = BoundProjectMutationRoot::bind_read(project_root, path)
        .map_err(|error| format!("Task journal directory is unsafe: {error}"))?;
    let bytes = binding
        .read_regular(path)
        .map_err(|error| format!("Failed to read task journal: {error}"))?;
    let mut logs = Vec::new();
    let mut activities = Vec::new();
    let mut valid_bytes = 0_u64;
    // Only newline-terminated records were completely appended. Preserve all
    // durable predecessors after a crash in the middle of the final record.
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !line.ends_with(b"\n") {
            break;
        }
        let record: JournalRecord = serde_json::from_slice(line)
            .map_err(|error| format!("Invalid task journal record: {error}"))?;
        valid_bytes += line.len() as u64;
        match record {
            JournalRecord::Log(log) => logs.push(log),
            JournalRecord::Activity(activity) => activities.push(activity),
        }
    }
    let cursor = JournalCursor {
        path: Some(path.to_path_buf()),
        logs: logs.len(),
        activities: activities.len(),
        bytes: valid_bytes,
    };
    Ok((logs, activities, cursor))
}
