use super::*;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateWikiHistoryState {
    pub available: bool,
    pub undone: bool,
    pub recovery: bool,
    pub undo_in_progress: bool,
    pub checkpoint_hash: Option<String>,
    pub final_commit: Option<String>,
}

pub fn get_update_wiki_history_state(
    context: &ProjectContext,
    run: &WorkflowRun,
) -> Result<UpdateWikiHistoryState, BackendError> {
    use crate::models::workflow::WorkflowDisplayStatus;
    let recovery = matches!(
        run.display_status,
        WorkflowDisplayStatus::Failed | WorkflowDisplayStatus::Interrupted
    );
    let mut state = UpdateWikiHistoryState {
        available: false,
        undone: false,
        recovery,
        undo_in_progress: false,
        checkpoint_hash: None,
        final_commit: None,
    };
    if run.kind != WorkflowKind::UpdateWiki
        || (!recovery && run.display_status != WorkflowDisplayStatus::Completed)
    {
        return Ok(state);
    }
    // The planned snapshot is durable before the first project write. It is
    // an expected value, never evidence of successful publication.
    state.checkpoint_hash = GitService.history_snapshot(context, &run.task_id, "before")?;
    state.final_commit = GitService.history_snapshot(context, &run.task_id, "after")?;
    if recovery && state.final_commit.is_none() {
        state.final_commit = GitService.history_snapshot(context, &run.task_id, "planned")?;
    }
    state.available = state.checkpoint_hash.is_some() && state.final_commit.is_some();
    if state.available {
        state.undone = GitService
            .history_snapshot(context, &run.task_id, "undo")?
            .is_some();
        state.undo_in_progress = !state.undone
            && GitService
                .history_snapshot(context, &run.task_id, "undo-started")?
                .is_some();
    }
    Ok(state)
}

/// Caller holds current project write authority. Only immutable operation
/// snapshots supply paths/bytes; arbitrary frontend files cannot be restored.
pub fn undo_update_wiki_history(
    context: &ProjectContext,
    run: &WorkflowRun,
    file_store: &FileStore,
    bookmark_service: &BookmarkService,
    search_service: &SearchService,
    tasks: &TaskService,
) -> Result<UpdateWikiHistoryState, BackendError> {
    let state = get_update_wiki_history_state(context, run)?;
    if !state.available {
        return Err(BackendError::new(
            "WORKFLOW_UNDO_UNAVAILABLE",
            "This update has no application-owned recovery history.",
            true,
            true,
        ));
    }
    if state.undone {
        return Ok(state);
    }
    let before_commit = state
        .checkpoint_hash
        .as_deref()
        .expect("available history has a checkpoint");
    let after_commit = state
        .final_commit
        .as_deref()
        .expect("available history has a result or plan");
    let record_path = format!(".app/compile/{}.json", run.task_id);
    let record_files =
        GitService.read_history_files(context, after_commit, std::slice::from_ref(&record_path))?;
    let record: CompileConsumptionRecord = record_files
        .get(&record_path)
        .and_then(Option::as_ref)
        .and_then(|bytes| serde_json::from_slice(bytes).ok())
        .ok_or_else(|| {
            BackendError::new(
                "WORKFLOW_UNDO_UNAVAILABLE",
                "The update history receipt is unavailable.",
                true,
                true,
            )
        })?;
    if record.compile_task_id != run.task_id {
        return Err(BackendError::new(
            "WORKFLOW_UNDO_UNAVAILABLE",
            "The update history receipt does not match this task.",
            true,
            true,
        ));
    }
    let mut paths = record.affected_paths;
    if paths
        .iter()
        .any(|path| !crate::services::compile_service::is_safe_wiki_markdown(path))
    {
        return Err(BackendError::new(
            "WORKFLOW_UNDO_UNAVAILABLE",
            "History contains a path outside Wiki outputs.",
            false,
            true,
        ));
    }
    paths.push(record_path);
    paths.extend(
        record
            .source_versions
            .iter()
            .filter(|source| !source.source_id.starts_with("legacy-"))
            .map(|source| format!(".app/sources/{}.json", source.source_id)),
    );
    paths.sort();
    paths.dedup();
    let before = GitService.read_history_files(context, before_commit, &paths)?;
    let after = GitService.read_history_files(context, after_commit, &paths)?;
    let restore = CompileService::prepare_history_restore(context, &before, &after)?;
    // Persist intent first: a crash/late edit must remain visible as an
    // incomplete restore, and publishers cannot interleave with it.
    GitService.create_history_snapshot(
        context,
        &run.task_id,
        "undo-started",
        "Restore Wiki update",
        Some(after_commit),
        &before,
    )?;
    CompileService::restore_prepared_history_outputs(context, &restore)?;
    GitService.create_history_snapshot(
        context,
        &run.task_id,
        "undo",
        "Restore Wiki update completed",
        Some(after_commit),
        &before,
    )?;
    refresh_workflow_wiki_indexes(
        context,
        &run.task_id,
        file_store,
        bookmark_service,
        search_service,
        tasks,
    )?;
    Ok(UpdateWikiHistoryState {
        undone: true,
        undo_in_progress: false,
        ..state
    })
}
