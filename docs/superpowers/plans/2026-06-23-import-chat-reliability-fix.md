# Import And Chat Reliability Fix Implementation Plan

> Historical implementation plan. Current Source identity, deduplication, commit and repair behavior is defined by [`../specs/2026-07-24-import-source-media-flow-design.md`](../specs/2026-07-24-import-source-media-flow-design.md).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Markdown confirmation feed compile correctly and make persisted default-Agent selection match Chat behavior and visible UI state.

**Architecture:** Keep the existing import, settings, Agent, and task boundaries. Narrow source duplicate scanning to original-source roots; coordinate the two existing Agent configuration files through `SettingsService`; carry terminal task errors into the existing Chat error state.

**Tech Stack:** Rust/Tauri v2 services and commands, React 19, TypeScript, Zustand, Vitest.

---

### Task 1: Exclude extracted artifacts from source duplicate detection

**Files:**
- Modify: `src-tauri/src/services/import_service.rs`
- Test: `src-tauri/src/services/import_service.rs`

- [ ] **Step 1: Write the failing regression test**

Add a service test that creates an external `notes.md`, writes identical bytes to `raw/extracted/notes-<hash>.md`, calls `preview_import`, and asserts `archived_files == 1`, `duplicate_files == 0`, and `files[0].conflict.is_none()`.

- [ ] **Step 2: Run the targeted test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features import_service::tests::extracted_markdown_does_not_make_source_a_duplicate -- --nocapture`

Expected: FAIL because `scan_existing` currently includes `raw/extracted` and returns `ExactDuplicate`.

- [ ] **Step 3: Implement the narrow scan**

Change `scan_existing` to enumerate only these roots when they exist:

```rust
for relative in ["raw/sources", "raw/assets"] {
    let root = context.resolve_project_path(relative)?;
    self.collect_hashes(&root, &context.root, &mut map)?;
}
```

Keep `collect_hashes` and exact-duplicate resolution otherwise unchanged.

- [ ] **Step 4: Verify GREEN and retained-source duplicate behavior**

Run the new test, then run the existing exact-duplicate tests in `import_service`.

### Task 2: Make default-Agent persistence consistent and honest

**Files:**
- Modify: `src-tauri/src/services/settings_service.rs`
- Modify: `src-tauri/src/commands/agent_commands.rs`
- Modify: `src/components/app/AppShell.tsx`
- Modify: `src/components/app/BottomStatusBar.tsx`
- Modify: `src/components/app/LeftSidebar.tsx`
- Modify: `src/features/agent/AgentRightPanel.tsx`
- Test: `src-tauri/src/services/settings_service.rs`
- Test: `src/features/agent/agent.test.tsx`

- [ ] **Step 1: Write failing backend persistence tests**

Add tests that:

```rust
service.save_agent_default(&context, Some(AgentKind::Codex)).unwrap();
assert_eq!(service.read_settings(&context).unwrap().agent_default, Some(AgentKind::Codex));
assert_eq!(AgentService::load_config(&context).unwrap().default_agent, Some(AgentKind::Codex));
```

Also create disagreeing legacy files and assert `read_settings` takes the value from `.app/agent-config.json`.

- [ ] **Step 2: Run backend tests and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --no-default-features settings_service::tests::agent -- --nocapture`

Expected: FAIL because `save_agent_default` does not exist and reads do not always overlay the canonical file.

- [ ] **Step 3: Write the failing UI truthfulness test**

Render `AgentRightPanel` with one installed Agent whose `isDefault` is false. Assert the Default Agent field shows the localized unconfigured value and not the installed command/version.

- [ ] **Step 4: Run the UI test and verify RED**

Run: `npm run test -- src/features/agent/agent.test.tsx`

Expected: FAIL because the panel currently falls back to the first installed Agent.

- [ ] **Step 5: Implement coordinated persistence and remove pseudo-default fallbacks**

Add `SettingsService::save_agent_default`, delegate `set_default_agent` to it, reload `settingsStore` after AppShell saves a default, and replace every `isDefault ?? first installed` expression used as a default with an `isDefault && installed` lookup.

- [ ] **Step 6: Verify both backend and frontend tests GREEN**

Run both targeted commands from Steps 2 and 4.

### Task 3: Surface terminal Chat failures

**Files:**
- Modify: `src/stores/chatStore.ts`
- Modify: `src/features/chat/ChatView.tsx`
- Test: `src/features/chat/chatView.test.tsx`

- [ ] **Step 1: Write the failing Chat task test**

Seed `chatStore` with an active session and `sendTaskId`, seed `taskStore` with a failed `llm_request` task containing `error.message = "No usable Agent CLI is configured."`, render `ChatView`, and assert the message appears after the targeted session reload completes.

- [ ] **Step 2: Run the test and verify RED**

Run: `npm run test -- src/features/chat/chatView.test.tsx`

Expected: FAIL because terminal task errors are discarded by `clearSendTask()`.

- [ ] **Step 3: Preserve the backend error after reload**

Allow `clearSendTask` to accept a nullable error message. In the terminal-task effect, await `reloadActive`, then clear the send task with the failed task message. Preserve the existing successful reload and streaming cleanup.

- [ ] **Step 4: Run the test and verify GREEN**

Run the command from Step 2 and confirm the original empty-state test remains green.

### Task 4: Full verification and project records

**Files:**
- Modify: `SPEC/progress.txt`
- Modify: `SPEC/gotchas.txt`

- [ ] **Step 1: Run the full required checks independently**

Run:

```powershell
npm run test
npm run lint
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --no-default-features --offline --lib --tests -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --offline
```

Expected: all available checks exit 0. If the known Windows WebView2 loader failure occurs, capture the exact exit code and retain the successful compile/clippy evidence.

- [ ] **Step 2: Scan source for debug logs and validate scope**

Confirm no unintended `console.log`, `dbg!`, or `println!` was added. Confirm forbidden paths and unrelated dirty files are absent from this task's diff.

- [ ] **Step 3: Perform two manual review passes**

Pass A checks the approved data flow and integration consistency. Pass B starts from the diff and searches for blind spots, missing tests, stale-state races, and cross-project leakage. Fix valid findings and rerun the full checks from Step 1.

- [ ] **Step 4: Record the milestone and gotchas**

Prepend one `SPEC/progress.txt` entry and add concise `SPEC/gotchas.txt` entries for the extracted-artifact duplicate trap and pseudo-default Agent split-brain.
