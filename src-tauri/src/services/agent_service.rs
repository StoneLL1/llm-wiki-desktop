use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::errors::BackendError;
use crate::models::agent::AgentConfig;
use crate::models::agent::{AgentDetectionState, AgentInfo, AgentKind};
use crate::models::paths::ProjectContext;
use crate::services::FileStore;
use crate::tasks::task_model::LogLevel;
use crate::tasks::TaskService;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentInvocation {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub cwd: PathBuf,
}

pub trait ProcessRunner: Send + Sync {
    fn find_executable(&self, command: &str) -> Option<PathBuf>;
    fn run_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, BackendError>;
    fn run_capture(&self, invocation: &AgentInvocation) -> Result<(String, String), BackendError>;
    fn run_task_streaming(
        &self,
        invocation: &AgentInvocation,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<String, BackendError>;
    /// Same as [`run_task_streaming`](Self::run_task_streaming) but additionally
    /// invokes `on_delta` for each captured stdout line, so callers that render
    /// output live (chat) get an incremental feed. The default impl ignores the
    /// callback and delegates to `run_task_streaming`, which is correct for test
    /// fakes that only need the final captured text. Takes `&dyn Fn` (not a
    /// generic) so the trait stays dyn-compatible — `ProcessRunner` is held as
    /// `Arc<dyn ProcessRunner>`.
    fn run_task_streaming_with_delta(
        &self,
        invocation: &AgentInvocation,
        tasks: &TaskService,
        task_id: &str,
        on_delta: &(dyn Fn(&str) + Sync),
    ) -> Result<String, BackendError> {
        let _ = on_delta;
        self.run_task_streaming(invocation, tasks, task_id)
    }
}

#[derive(Default)]
pub struct SystemProcessRunner;

pub struct AgentService {
    runner: Arc<dyn ProcessRunner>,
}

impl Default for AgentService {
    fn default() -> Self {
        Self {
            runner: Arc::new(SystemProcessRunner),
        }
    }
}

impl AgentService {
    pub fn with_runner(runner: Arc<dyn ProcessRunner>) -> Self {
        Self { runner }
    }

    pub fn load_config(context: &ProjectContext) -> Result<AgentConfig, BackendError> {
        let path = ".app/agent-config.json";
        if !context.resolve_project_path(path)?.exists() {
            return Ok(AgentConfig::default());
        }
        FileStore.read_json(context, path)
    }

    pub fn save_config(context: &ProjectContext, config: &AgentConfig) -> Result<(), BackendError> {
        FileStore.write_json_atomic(context, ".app/agent-config.json", config)
    }

    pub fn detect_agents(&self, default_agent: Option<AgentKind>) -> Vec<AgentInfo> {
        AgentKind::ALL
            .into_iter()
            .map(|kind| self.detect(kind, default_agent == Some(kind)))
            .collect()
    }

    fn detect(&self, kind: AgentKind, is_default: bool) -> AgentInfo {
        let command = kind.command();
        let executable_path = self.runner.find_executable(command);
        if executable_path.is_none() {
            return AgentInfo {
                kind,
                command: command.into(),
                state: AgentDetectionState::Missing,
                version: None,
                executable_path: None,
                is_default,
                install_guidance: Self::install_guidance(kind).into(),
                error: None,
            };
        }

        match self
            .runner
            .run_with_timeout(command, &["--version"], Duration::from_secs(3))
        {
            Ok(output) if invocation_supported(self.runner.as_ref(), kind, command) => AgentInfo {
                kind,
                command: command.into(),
                state: AgentDetectionState::Installed,
                version: Some(first_non_empty_line(&output)),
                executable_path: executable_path.map(|path| path.to_string_lossy().replace('\\', "/")),
                is_default,
                install_guidance: Self::install_guidance(kind).into(),
                error: None,
            },
            Ok(_) => AgentInfo {
                kind,
                command: command.into(),
                state: AgentDetectionState::Failed,
                version: None,
                executable_path: executable_path.map(|path| path.to_string_lossy().replace('\\', "/")),
                is_default,
                install_guidance: Self::install_guidance(kind).into(),
                error: Some("Installed CLI does not expose the supported non-interactive invocation protocol.".into()),
            },
            Err(error) => AgentInfo {
                kind,
                command: command.into(),
                state: AgentDetectionState::Failed,
                version: None,
                executable_path: executable_path.map(|path| path.to_string_lossy().replace('\\', "/")),
                is_default,
                install_guidance: Self::install_guidance(kind).into(),
                error: Some(error.message),
            },
        }
    }

    pub fn invocation(
        kind: AgentKind,
        workspace: &Path,
        prompt: &str,
    ) -> Result<AgentInvocation, BackendError> {
        validate_candidate_workspace(workspace)?;
        let cwd = workspace.to_path_buf();
        let prompt_owned = prompt.to_string();
        let invocation = match kind {
            AgentKind::Claude => AgentInvocation {
                // --bare isolates the programmatic run from the user's
                // interactive-session state: it skips hooks, plugin sync, MCP
                // server init, auto-memory and CLAUDE.md auto-discovery. This
                // matters because the host claude may be configured (via
                // ~/.claude/) with MCP servers / SessionStart hooks that block
                // or fail when spawned from a GUI process context, which
                // otherwise hangs --print runs indefinitely. Auth falls back to
                // ANTHROPIC_API_KEY / --settings, matching BYOK-style usage.
                program: "claude".into(),
                args: vec![
                    "--bare".into(),
                    "--print".into(),
                    "--output-format".into(),
                    "stream-json".into(),
                    "--verbose".into(),
                    "--permission-mode".into(),
                    "dontAsk".into(),
                    // Pre-approve the file tools the wiki-ingest compiler
                    // needs. dontAsk alone denies anything not explicitly
                    // allowed, and the sandbox setting only auto-allows Bash
                    // (not Edit/Write), so without this allowlist the agent
                    // reads sources, plans pages, then silently fails to write
                    // any file — producing a stub wiki (only index/log/overview)
                    // on every platform. Bash is included because bulk page
                    // creation (heredocs) is how most compile models reliably
                    // emit many files; without it weaker models try Bash, get
                    // denied, and give up instead of falling back to Edit.
                    // Residual risk: a prompt-injected source in
                    // raw/extracted/*.md could run arbitrary shell, and the CLI
                    // sandbox that would jail Bash is unsupported on Windows —
                    // project-level safety is still enforced by the isolated
                    // temp workspace + validated manifest + Git checkpoint, but
                    // system-level commands are NOT contained. Accepted by the
                    // user (user-initiated compile on user-imported content).
                    // The `=` binding is required because --allowedTools is
                    // variadic and would otherwise consume the prompt arg.
                    "--allowedTools=Edit Write Read Bash".into(),
                    "--settings".into(),
                    r#"{"sandbox":{"enabled":true,"autoAllowBashIfSandboxed":true}}"#.into(),
                    prompt_owned,
                ],
                stdin: None,
                cwd,
            },
            AgentKind::Codex => AgentInvocation {
                program: "codex".into(),
                args: vec![
                    "exec".into(),
                    "--json".into(),
                    "--ephemeral".into(),
                    "--sandbox".into(),
                    "workspace-write".into(),
                    "--skip-git-repo-check".into(),
                    "-C".into(),
                    workspace.to_string_lossy().into_owned(),
                    "-".into(),
                ],
                stdin: Some(prompt_owned),
                cwd,
            },
            AgentKind::Openclaw => AgentInvocation {
                program: "openclaw".into(),
                args: vec![
                    "agent".into(),
                    "--local".into(),
                    "--sandbox".into(),
                    "workspace".into(),
                    "--message".into(),
                    prompt_owned,
                    "--json".into(),
                ],
                stdin: None,
                cwd,
            },
            AgentKind::Hermes => AgentInvocation {
                program: "hermes".into(),
                args: vec![
                    "--workspace".into(),
                    workspace.to_string_lossy().into_owned(),
                    "--sandbox".into(),
                    "--prompt".into(),
                    prompt_owned,
                    "--json".into(),
                ],
                stdin: None,
                cwd,
            },
        };
        Ok(invocation)
    }

    /// Build a plain-text-oriented Agent invocation for chat Q&A. Unlike
    /// [`invocation`] (which uses stream-json so compile can diff a workspace),
    /// chat wants the captured stdout to be the answer text itself, so the
    /// Claude profile uses `--output-format text`. Other CLIs reuse best-effort
    /// non-interactive args; the BYOK route is the guaranteed path, so this is
    /// an enhancement when an Agent is installed.
    pub fn chat_invocation(
        kind: AgentKind,
        workspace: &Path,
        prompt: &str,
    ) -> Result<AgentInvocation, BackendError> {
        validate_chat_workspace(workspace)?;
        if !Self::supports_read_only_project_chat(kind) {
            return Err(unsupported_chat_agent(kind));
        }
        let cwd = workspace.to_path_buf();
        let prompt_owned = prompt.to_string();
        let invocation = match kind {
            AgentKind::Claude => AgentInvocation {
                program: "claude".into(),
                args: vec![
                    "--bare".into(),
                    "--print".into(),
                    "--output-format".into(),
                    "text".into(),
                    "--permission-mode".into(),
                    "dontAsk".into(),
                    "--allowedTools=Read Grep Glob".into(),
                ],
                stdin: Some(prompt_owned),
                cwd,
            },
            AgentKind::Codex => AgentInvocation {
                program: "codex".into(),
                args: vec![
                    "exec".into(),
                    "--ephemeral".into(),
                    "--ignore-rules".into(),
                    "--sandbox".into(),
                    "read-only".into(),
                    "--skip-git-repo-check".into(),
                    "-C".into(),
                    workspace.to_string_lossy().into_owned(),
                    "-".into(),
                ],
                stdin: Some(prompt_owned),
                cwd,
            },
            AgentKind::Openclaw | AgentKind::Hermes => unreachable!("filtered above"),
        };
        Ok(invocation)
    }

    pub fn supports_read_only_project_chat(kind: AgentKind) -> bool {
        matches!(kind, AgentKind::Claude | AgentKind::Codex)
    }

    /// Build a plain-text Agent invocation for the `wiki-lint` deep-lint run.
    /// The captured stdout is the structured lint report (a fenced JSON block),
    /// so this reuses the chat text-output profile rather than the stream-json
    /// compile profile. The BYOK route remains the guaranteed fallback.
    pub fn lint_invocation(
        kind: AgentKind,
        workspace: &Path,
        prompt: &str,
    ) -> Result<AgentInvocation, BackendError> {
        validate_candidate_workspace(workspace)?;
        let cwd = workspace.to_path_buf();
        let prompt_owned = prompt.to_string();
        let invocation = match kind {
            AgentKind::Claude => AgentInvocation {
                program: "claude".into(),
                args: vec![
                    "--bare".into(),
                    "--print".into(),
                    "--output-format".into(),
                    "text".into(),
                    prompt_owned,
                ],
                stdin: None,
                cwd,
            },
            AgentKind::Codex => AgentInvocation {
                program: "codex".into(),
                args: vec!["exec".into(), "-".into()],
                stdin: Some(prompt_owned),
                cwd,
            },
            AgentKind::Openclaw => AgentInvocation {
                program: "openclaw".into(),
                args: vec!["agent".into(), "--message".into(), prompt_owned],
                stdin: None,
                cwd,
            },
            AgentKind::Hermes => AgentInvocation {
                program: "hermes".into(),
                args: vec!["--prompt".into(), prompt_owned],
                stdin: None,
                cwd,
            },
        };
        Ok(invocation)
    }

    /// Build a plain-text Agent invocation for the `html-*` export skills. The
    /// captured stdout is the standalone HTML document, so this reuses the
    /// chat/lint text-output profile rather than the stream-json compile
    /// profile. The BYOK route remains the guaranteed fallback.
    pub fn html_export_invocation(
        kind: AgentKind,
        workspace: &Path,
        prompt: &str,
    ) -> Result<AgentInvocation, BackendError> {
        validate_candidate_workspace(workspace)?;
        let cwd = workspace.to_path_buf();
        let prompt_owned = prompt.to_string();
        let invocation = match kind {
            AgentKind::Claude => AgentInvocation {
                program: "claude".into(),
                args: vec![
                    "--bare".into(),
                    "--print".into(),
                    "--output-format".into(),
                    "text".into(),
                    prompt_owned,
                ],
                stdin: None,
                cwd,
            },
            AgentKind::Codex => AgentInvocation {
                program: "codex".into(),
                args: vec!["exec".into(), "-".into()],
                stdin: Some(prompt_owned),
                cwd,
            },
            AgentKind::Openclaw => AgentInvocation {
                program: "openclaw".into(),
                args: vec!["agent".into(), "--message".into(), prompt_owned],
                stdin: None,
                cwd,
            },
            AgentKind::Hermes => AgentInvocation {
                program: "hermes".into(),
                args: vec!["--prompt".into(), prompt_owned],
                stdin: None,
                cwd,
            },
        };
        Ok(invocation)
    }

    pub fn install_guidance(kind: AgentKind) -> &'static str {
        match kind {
            AgentKind::Claude => "npm install -g @anthropic-ai/claude-code",
            AgentKind::Codex => "npm install -g @openai/codex",
            AgentKind::Openclaw => "See https://docs.openclaw.ai for installation instructions.",
            AgentKind::Hermes => {
                "See https://github.com/NousResearch/hermes-agent for installation instructions."
            }
        }
    }

    pub fn run_capture(
        &self,
        invocation: &AgentInvocation,
    ) -> Result<(String, String), BackendError> {
        self.runner.run_capture(invocation)
    }

    pub fn run_task_streaming(
        &self,
        invocation: &AgentInvocation,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<String, BackendError> {
        self.runner.run_task_streaming(invocation, tasks, task_id)
    }

    /// Streaming variant of [`run_task_streaming`](Self::run_task_streaming):
    /// additionally invokes `on_delta` for each captured stdout line so the
    /// chat route can render the agent answer incrementally, uniform with the
    /// BYOK streaming path.
    pub fn run_task_streaming_with_delta(
        &self,
        invocation: &AgentInvocation,
        tasks: &TaskService,
        task_id: &str,
        on_delta: &(dyn Fn(&str) + Sync),
    ) -> Result<String, BackendError> {
        self.runner
            .run_task_streaming_with_delta(invocation, tasks, task_id, on_delta)
    }
}

impl ProcessRunner for SystemProcessRunner {
    fn find_executable(&self, command: &str) -> Option<PathBuf> {
        find_executable(command)
    }

    fn run_with_timeout(
        &self,
        command: &str,
        args: &[&str],
        timeout: Duration,
    ) -> Result<String, BackendError> {
        run_with_timeout(command, args, timeout)
    }

    fn run_capture(&self, invocation: &AgentInvocation) -> Result<(String, String), BackendError> {
        let mut child = build_command(
            &invocation.program,
            &invocation.args,
            &invocation.cwd,
            invocation.stdin.is_some(),
        )
        .spawn()
        .map_err(|error| BackendError::new("AGENT_SPAWN_FAILED", error.to_string(), true, false))?;
        if let Some(input) = &invocation.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).map_err(|error| {
                    BackendError::new("AGENT_STDIN_FAILED", error.to_string(), true, false)
                })?;
            }
        }
        let output = child.wait_with_output().map_err(|error| {
            BackendError::new("AGENT_WAIT_FAILED", error.to_string(), true, false)
        })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            return Err(BackendError::new(
                "AGENT_EXIT_FAILED",
                if stderr.trim().is_empty() {
                    "Agent process failed."
                } else {
                    stderr.trim()
                },
                true,
                false,
            ));
        }
        Ok((stdout, stderr))
    }

    fn run_task_streaming(
        &self,
        invocation: &AgentInvocation,
        tasks: &TaskService,
        task_id: &str,
    ) -> Result<String, BackendError> {
        self.run_task_streaming_with_delta(invocation, tasks, task_id, &|_| {})
    }

    fn run_task_streaming_with_delta(
        &self,
        invocation: &AgentInvocation,
        tasks: &TaskService,
        task_id: &str,
        on_delta: &(dyn Fn(&str) + Sync),
    ) -> Result<String, BackendError> {
        let mut child = build_command(
            &invocation.program,
            &invocation.args,
            &invocation.cwd,
            invocation.stdin.is_some(),
        )
        .spawn()
        .map_err(|error| BackendError::new("AGENT_SPAWN_FAILED", error.to_string(), true, false))?;
        if let Some(input) = &invocation.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(input.as_bytes()).map_err(|error| {
                    BackendError::new("AGENT_STDIN_FAILED", error.to_string(), true, false)
                })?;
            }
        }
        let (sender, receiver) = std::sync::mpsc::channel::<(LogLevel, String)>();
        if let Some(stdout) = child.stdout.take() {
            let sender = sender.clone();
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = sender.send((LogLevel::Info, line));
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let sender = sender.clone();
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = sender.send((LogLevel::Warn, line));
                }
            });
        }
        drop(sender);
        // Stdout (Info) lines are captured as the answer payload for the chat
        // route, while still being streamed to the task drawer as logs. Compile
        // discards this value and reads workspace files instead.
        let mut stdout_lines: Vec<String> = Vec::new();
        loop {
            while let Ok((level, line)) = receiver.try_recv() {
                if level == LogLevel::Info {
                    stdout_lines.push(line.clone());
                    // Forward each captured stdout line as a live delta so chat
                    // can render the answer incrementally (uniform with the
                    // BYOK streaming path).
                    on_delta(&line);
                }
                let _ = tasks.append_log(task_id, level, line);
            }
            if tasks.is_cancelled(task_id) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(BackendError::new(
                    "AGENT_CANCELLED",
                    "Agent task was cancelled.",
                    true,
                    false,
                ));
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    for (level, line) in receiver.try_iter() {
                        if level == LogLevel::Info {
                            stdout_lines.push(line.clone());
                            on_delta(&line);
                        }
                        let _ = tasks.append_log(task_id, level, line);
                    }
                    if status.success() {
                        return Ok(stdout_lines.join("\n"));
                    }
                    return Err(BackendError::new(
                        "AGENT_EXIT_FAILED",
                        format!("Agent exited with {status}."),
                        true,
                        false,
                    ));
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(error) => {
                    return Err(BackendError::new(
                        "AGENT_WAIT_FAILED",
                        error.to_string(),
                        true,
                        false,
                    ))
                }
            }
        }
    }
}

fn invocation_supported(runner: &dyn ProcessRunner, kind: AgentKind, command: &str) -> bool {
    let args: &[&str] = match kind {
        AgentKind::Openclaw => &["agent", "--help"],
        _ => &["--help"],
    };
    let Ok(help) = runner.run_with_timeout(command, args, Duration::from_secs(3)) else {
        return false;
    };
    match kind {
        AgentKind::Claude => {
            help.contains("--print")
                && help.contains("--output-format")
                && help.contains("--settings")
                && help.contains("--bare")
        }
        AgentKind::Codex => help.contains("exec") && help.contains("non-interactively"),
        AgentKind::Openclaw => {
            help.contains("--message") && help.contains("--json") && help.contains("--sandbox")
        }
        AgentKind::Hermes => {
            help.contains("--prompt")
                && help.contains("--json")
                && help.contains("--workspace")
                && help.contains("--sandbox")
        }
    }
}

fn find_executable(command: &str) -> Option<PathBuf> {
    // 1. `where` / `which` against the App process's PATH.
    let lookup = if cfg!(windows) {
        ("where", vec![command])
    } else {
        ("which", vec![command])
    };
    if let Ok(output) = Command::new(lookup.0).args(lookup.1).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect();
            // On Windows `where claude` lists the extensionless bash shim
            // FIRST and `claude.cmd` later. CreateProcess cannot run the
            // extensionless shim, so prefer a `.cmd`/`.bat`/`.exe` line.
            #[cfg(windows)]
            if let Some(preferred) = lines.iter().find(|line| {
                Path::new(line)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "cmd" | "bat" | "exe"))
                    .unwrap_or(false)
            }) {
                return Some(PathBuf::from(preferred));
            }
            if let Some(first) = lines.first() {
                return Some(PathBuf::from(first));
            }
        }
    }

    // 2. Fallback: npm's global bin dir (`%APPDATA%\npm` on Windows). GUI
    //    launches and some IDE shells inherit a PATH that does not include
    //    the npm global dir even though the CLI is installed there, so the
    //    `where` lookup above fails inside the App while succeeding in a
    //    fresh terminal. Check the well-known location directly.
    #[cfg(windows)]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let base = PathBuf::from(appdata).join("npm");
            for ext in ["cmd", "bat", "exe"] {
                let candidate = base.join(format!("{command}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    None
}

/// A directly-spawnable target for an Agent CLI command.
///
/// On Windows, npm-installed CLIs ship as `.cmd` shims (e.g. `claude.cmd`).
/// Rust's `Command` refuses to spawn a `.cmd`/`.bat` directly with the error
/// `batch file arguments are invalid` whenever an argument contains
/// characters `cmd.exe` cannot safely represent (CVE-2024-24576) — and the
/// compile/chat prompts (arbitrary wiki content carrying `"`, `%`, newlines)
/// always do. Drilling through the shim to the executable it forwards to and
/// spawning *that* directly uses normal Windows argument escaping and passes
/// the prompt byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SpawnTarget {
    program: String,
    /// Arguments inserted before the caller's args. Populated when the shim
    /// forwards to `node <script>` (e.g. `codex`): `program` is the node
    /// executable and `leading_args` holds the script path.
    leading_args: Vec<String>,
}

fn resolve_spawn_target(program: &str) -> SpawnTarget {
    if cfg!(not(windows)) || program.contains('/') || program.contains('\\') {
        return SpawnTarget {
            program: program.to_string(),
            leading_args: Vec::new(),
        };
    }
    let Some(resolved) = find_executable(program) else {
        return SpawnTarget {
            program: program.to_string(),
            leading_args: Vec::new(),
        };
    };
    let is_batch = resolved
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "cmd" | "bat"))
        .unwrap_or(false);
    if !is_batch {
        return SpawnTarget {
            program: resolved.to_string_lossy().into_owned(),
            leading_args: Vec::new(),
        };
    }
    resolve_cmd_shim(&resolved).unwrap_or(SpawnTarget {
        program: resolved.to_string_lossy().into_owned(),
        leading_args: Vec::new(),
    })
}

/// Parse an npm-style `.cmd`/`.bat` shim into the executable it forwards to.
/// Such shims end with a line that quotes the real target and appends `%*`,
/// e.g. `"%dp0%\...\claude.exe" %*` or `"%_prog%" "%dp0%\...\codex.js" %*`.
/// Returns `None` when no quoted existing target is found, so callers fall
/// back to the shim itself — detection of `--version`/`--help` still works
/// there because those arguments are trivial and never trip the guard.
fn resolve_cmd_shim(cmd_path: &Path) -> Option<SpawnTarget> {
    let dp0 = cmd_path.parent()?;
    let text = std::fs::read_to_string(cmd_path).ok()?;
    let forward_line = text
        .lines()
        .map(str::trim)
        .rfind(|line| line.contains("%*"))?;
    let mut targets: Vec<PathBuf> = Vec::new();
    let mut forwards_to_node = false;
    for token in quoted_tokens(forward_line) {
        if token.contains("%_prog%") {
            forwards_to_node = true;
            continue;
        }
        if !token.contains("%dp0%") {
            continue;
        }
        let candidate = PathBuf::from(token.replace("%dp0%", &dp0.to_string_lossy()));
        // Reject anything that does not exist or that climbs out of the shim's
        // own directory after resolving `..`, so a corrupted/malicious shim
        // cannot redirect to an arbitrary executable outside the npm dir.
        if is_within_dir(&candidate, dp0) {
            targets.push(candidate);
        }
    }
    if let Some(exe) = targets.iter().find(|path| is_extension(path, "exe")) {
        return Some(SpawnTarget {
            program: exe.to_string_lossy().into_owned(),
            leading_args: Vec::new(),
        });
    }
    let script = targets.iter().find(|path| {
        is_extension(path, "js") || is_extension(path, "cjs") || is_extension(path, "mjs")
    })?;
    let node_program = node_executable(forwards_to_node)?;
    Some(SpawnTarget {
        program: node_program,
        leading_args: vec![script.to_string_lossy().into_owned()],
    })
}

fn is_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(expected))
        .unwrap_or(false)
}

/// `candidate` must be an existing file that, after canonicalizing `..`,
/// still lives under `root`. Guards the shim resolver against `..`-climbing
/// or poisoned targets that would otherwise spawn an unintended executable.
fn is_within_dir(candidate: &Path, root: &Path) -> bool {
    candidate.is_file()
        && candidate
            .canonicalize()
            .ok()
            .zip(root.canonicalize().ok())
            .map(|(candidate, root)| candidate.starts_with(root))
            .unwrap_or(false)
}

/// Resolve a real `node` executable for JS-forwarding shims. Only a native
/// `.exe` is accepted: returning anything else (a `.cmd`/extensionless shim)
/// would re-introduce the batch-spawn failure this module exists to avoid, so
/// a miss yields `None` and the caller falls back to the `.cmd` shim itself —
/// which is safe for `codex` because it pipes the prompt via stdin, never as
/// a command-line argument.
fn node_executable(forwards_to_node: bool) -> Option<String> {
    if !forwards_to_node {
        return None;
    }
    match find_executable("node") {
        Some(path) if is_extension(&path, "exe") => Some(path.to_string_lossy().into_owned()),
        _ => None,
    }
}

fn quoted_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                if in_quote {
                    tokens.push(std::mem::take(&mut current));
                }
                in_quote = !in_quote;
            }
            other if in_quote => current.push(other),
            _ => {}
        }
    }
    tokens
}

#[cfg(windows)]
fn no_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
}

#[cfg(not(windows))]
fn no_window(_command: &mut Command) {}

/// Build a `Command` for the resolved target of `program` + `args`, avoiding
/// the Windows batch-shim spawn failure. See [`resolve_spawn_target`].
fn build_command(program: &str, args: &[String], cwd: &Path, stdin_piped: bool) -> Command {
    let target = resolve_spawn_target(program);
    let mut command = Command::new(&target.program);
    for leading in &target.leading_args {
        command.arg(leading);
    }
    command
        .args(args)
        .current_dir(cwd)
        .stdin(if stdin_piped {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_window(&mut command);
    command
}

fn run_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, BackendError> {
    let target = resolve_spawn_target(command);
    let mut program = Command::new(&target.program);
    for leading in &target.leading_args {
        program.arg(leading);
    }
    program.args(args);
    no_window(&mut program);
    let mut child = program
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            BackendError::new("AGENT_DETECT_FAILED", error.to_string(), true, false)
        })?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                let output = child.wait_with_output().map_err(|error| {
                    BackendError::new("AGENT_DETECT_FAILED", error.to_string(), true, false)
                })?;
                if !output.status.success() {
                    return Err(BackendError::new(
                        "AGENT_DETECT_FAILED",
                        String::from_utf8_lossy(&output.stderr).trim(),
                        true,
                        false,
                    ));
                }
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                return Err(BackendError::new(
                    "AGENT_DETECT_TIMEOUT",
                    "Agent version detection timed out.",
                    true,
                    false,
                ));
            }
            Err(error) => {
                return Err(BackendError::new(
                    "AGENT_DETECT_FAILED",
                    error.to_string(),
                    true,
                    false,
                ));
            }
        }
    }
}

fn first_non_empty_line(value: &str) -> String {
    value
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown")
        .trim()
        .to_string()
}

fn validate_candidate_workspace(workspace: &Path) -> Result<(), BackendError> {
    let workspace = workspace.canonicalize().map_err(|error| {
        BackendError::new("AGENT_WORKSPACE_INVALID", error.to_string(), true, false)
    })?;
    let candidate_root = std::env::temp_dir().join("llm-wiki-desktop");
    let candidate_root = candidate_root.canonicalize().map_err(|error| {
        BackendError::new("AGENT_WORKSPACE_INVALID", error.to_string(), true, false)
    })?;
    if !workspace.starts_with(candidate_root) {
        return Err(BackendError::new(
            "AGENT_WORKSPACE_OUTSIDE_CANDIDATE",
            "Agent execution is restricted to a candidate workspace.",
            false,
            true,
        ));
    }
    Ok(())
}

fn validate_chat_workspace(workspace: &Path) -> Result<(), BackendError> {
    let workspace = workspace.canonicalize().map_err(|error| {
        BackendError::new("AGENT_WORKSPACE_INVALID", error.to_string(), true, false)
    })?;
    if !workspace.is_dir() {
        return Err(BackendError::new(
            "AGENT_WORKSPACE_INVALID",
            "Chat Agent workspace must be a project directory.",
            true,
            true,
        ));
    }
    if !workspace.join("wiki").is_dir() {
        return Err(BackendError::new(
            "AGENT_WORKSPACE_INVALID",
            "Chat Agent workspace must contain a wiki/ directory.",
            true,
            true,
        ));
    }
    Ok(())
}

fn unsupported_chat_agent(kind: AgentKind) -> BackendError {
    BackendError::new(
        "CHAT_AGENT_UNSUPPORTED",
        format!(
            "{} does not expose a verified read-only project chat profile. Use Claude, Codex, or BYOK for Chat.",
            kind.command()
        ),
        true,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invocation_profiles_are_non_interactive_and_workspace_scoped() {
        let workspace = std::env::temp_dir().join("llm-wiki-desktop/invocation-test");
        std::fs::create_dir_all(&workspace).unwrap();
        let claude = AgentService::invocation(AgentKind::Claude, &workspace, "compile").unwrap();
        assert_eq!(claude.program, "claude");
        assert!(claude.args.contains(&"--bare".to_string()));
        assert!(claude.args.contains(&"--print".to_string()));
        assert!(claude.args.contains(&"--output-format".to_string()));

        let codex = AgentService::invocation(AgentKind::Codex, &workspace, "compile").unwrap();
        assert_eq!(codex.program, "codex");
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "workspace-write"]));
        assert_eq!(codex.stdin.as_deref(), Some("compile"));
    }

    #[test]
    fn install_guidance_is_data_not_an_executable_action() {
        for kind in AgentKind::ALL {
            let guidance = AgentService::install_guidance(kind);
            assert!(!guidance.trim().is_empty());
        }
    }

    #[test]
    fn html_export_invocation_uses_text_profile() {
        let workspace = std::env::temp_dir().join("llm-wiki-desktop/export-invocation-test");
        std::fs::create_dir_all(&workspace).unwrap();
        let claude =
            AgentService::html_export_invocation(AgentKind::Claude, &workspace, "build html")
                .unwrap();
        assert_eq!(claude.program, "claude");
        assert!(claude.args.contains(&"--bare".to_string()));
        assert!(claude.args.contains(&"--output-format".to_string()));
        assert!(claude.args.contains(&"text".to_string()));
        assert!(!claude.args.contains(&"stream-json".to_string()));
        assert!(claude.stdin.is_none());

        let codex =
            AgentService::html_export_invocation(AgentKind::Codex, &workspace, "build html")
                .unwrap();
        assert_eq!(codex.stdin.as_deref(), Some("build html"));
    }

    #[test]
    fn chat_invocation_runs_from_project_root_read_only() {
        let workspace = std::env::temp_dir().join(format!(
            "llm-wiki-chat-project-root-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(workspace.join("wiki")).unwrap();

        let claude =
            AgentService::chat_invocation(AgentKind::Claude, &workspace, "answer").unwrap();
        assert_eq!(claude.cwd, workspace);
        assert!(claude.args.contains(&"--bare".to_string()));
        assert!(claude.args.contains(&"--permission-mode".to_string()));
        assert!(claude.args.contains(&"dontAsk".to_string()));
        assert_eq!(claude.stdin.as_deref(), Some("answer"));
        assert!(!claude.args.contains(&"answer".to_string()));
        assert!(
            claude
                .args
                .iter()
                .any(|arg| arg == "--allowedTools=Read Grep Glob"),
            "chat should allow only read/search tools, got {:?}",
            claude.args
        );
        assert!(
            !claude
                .args
                .iter()
                .any(|arg| arg.contains("Edit") || arg.contains("Write")),
            "chat invocation must not pre-authorize write tools: {:?}",
            claude.args
        );

        let _ = std::fs::remove_dir_all(&claude.cwd);
    }

    #[test]
    fn codex_chat_invocation_is_ephemeral_read_only_and_ignores_project_rules() {
        let workspace =
            std::env::temp_dir().join(format!("llm-wiki-chat-codex-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(workspace.join("wiki")).unwrap();

        let codex = AgentService::chat_invocation(AgentKind::Codex, &workspace, "answer").unwrap();

        assert_eq!(codex.stdin.as_deref(), Some("answer"));
        assert!(codex.args.contains(&"--ephemeral".to_string()));
        assert!(codex.args.contains(&"--ignore-rules".to_string()));
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair == ["--sandbox", "read-only"]));
        assert!(codex
            .args
            .windows(2)
            .any(|pair| pair[0] == "-C" && pair[1] == workspace.to_string_lossy()));

        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn chat_invocation_rejects_agents_without_verified_read_only_profile() {
        let workspace = std::env::temp_dir().join(format!(
            "llm-wiki-chat-unsupported-root-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(workspace.join("wiki")).unwrap();

        for kind in [AgentKind::Openclaw, AgentKind::Hermes] {
            let err = AgentService::chat_invocation(kind, &workspace, "answer")
                .expect_err("unsupported chat agents must be rejected");
            assert_eq!(err.code, "CHAT_AGENT_UNSUPPORTED");
        }

        let _ = std::fs::remove_dir_all(&workspace);
    }

    #[test]
    fn claude_invocations_isolate_from_user_session_state() {
        // Regression guard: every Claude invocation must pass --bare so
        // programmatic runs do not load the host's ~/.claude hooks, MCP
        // servers, plugin sync, or auto-memory. Without --bare, a Claude
        // configured with blocking MCP servers (e.g. claude-mem, zotero) hangs
        // indefinitely during init when spawned from the GUI process, because
        // --print never reaches the model while MCP/SessionStart hooks stall.
        let workspace = std::env::temp_dir().join("llm-wiki-desktop/bare-invariant-test");
        std::fs::create_dir_all(workspace.join("wiki")).unwrap();
        for invocation in [
            AgentService::invocation(AgentKind::Claude, &workspace, "compile").unwrap(),
            AgentService::chat_invocation(AgentKind::Claude, &workspace, "chat").unwrap(),
            AgentService::lint_invocation(AgentKind::Claude, &workspace, "lint").unwrap(),
            AgentService::html_export_invocation(AgentKind::Claude, &workspace, "html").unwrap(),
        ] {
            assert!(
                invocation.args.contains(&"--bare".to_string()),
                "Claude invocation missing --bare (isolation): {:?}",
                invocation.args
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn find_executable_falls_back_to_npm_global_dir_off_path() {
        // Simulate a process whose PATH lacks the npm global dir but whose
        // APPDATA points at a temp dir with a `claude.cmd` shim installed
        // there. `where` should not find it (nothing on PATH), so the
        // %APPDATA%\npm fallback must resolve the `.cmd`.
        let dir = std::env::temp_dir().join(format!(
            "llm-wiki-desktop/agent-fallback-{}",
            uuid::Uuid::new_v4()
        ));
        let npm = dir.join("npm");
        std::fs::create_dir_all(&npm).unwrap();
        std::fs::write(npm.join("claude.cmd"), "@echo off\n").unwrap();

        let prior = std::env::var("APPDATA").ok();
        std::env::set_var("APPDATA", &dir);
        let resolved = find_executable("claude");
        if let Some(p) = prior {
            std::env::set_var("APPDATA", p);
        } else {
            std::env::remove_var("APPDATA");
        }
        let _ = std::fs::remove_dir_all(&dir);

        let resolved = resolved.expect("fallback must find claude.cmd off-PATH via %APPDATA%\\npm");
        assert!(
            resolved.to_string_lossy().ends_with("claude.cmd"),
            "fallback must resolve the .cmd shim, got {resolved:?}"
        );
    }

    #[test]
    fn resolve_spawn_target_passes_through_paths_and_bare_names() {
        // Already-qualified paths are returned unchanged on every platform...
        let target = resolve_spawn_target("/usr/bin/claude");
        assert_eq!(target.program, "/usr/bin/claude");
        assert!(target.leading_args.is_empty());
        let target = resolve_spawn_target("./local/agent");
        assert_eq!(target.program, "./local/agent");
        // ...and bare unresolvable names pass through unchanged too, so the
        // caller still gets the original input rather than an empty string.
        let target = resolve_spawn_target("definitely-not-installed-cli-xyz");
        assert!(
            target.program == "definitely-not-installed-cli-xyz",
            "unresolvable bare name should pass through unchanged, got {:?}",
            target
        );
        assert!(target.leading_args.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn resolve_cmd_shim_drills_through_npm_shim_to_forwarded_executable() {
        // Regression guard for the "batch file arguments are invalid" failure:
        // spawning a `.cmd` directly trips Rust's CVE-2024-24576 guard once the
        // prompt argument carries characters cmd.exe can't safely represent.
        // The resolver must drill through the npm shim to the executable it
        // forwards to (claude.exe) or to node + script (codex.js).
        let dir = std::env::temp_dir().join(format!(
            "llm-wiki-desktop/shim-resolve-{}",
            uuid::Uuid::new_v4()
        ));
        let npm = dir.join("npm");

        // claude-style shim: forwards directly to a native .exe.
        let exe_pkg = npm.join("node_modules/@anthropic-ai/claude-code/bin");
        std::fs::create_dir_all(&exe_pkg).unwrap();
        std::fs::write(exe_pkg.join("claude.exe"), b"MZ").unwrap();
        std::fs::write(
            npm.join("claude.cmd"),
            "@ECHO off\nGOTO start\n:find_dp0\nSET dp0=%~dp0\nEXIT /b\n:start\nSETLOCAL\nCALL :find_dp0\n\"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\bin\\claude.exe\"   %*\n",
        )
        .unwrap();
        let expected_exe = exe_pkg.join("claude.exe");
        let resolved = resolve_cmd_shim(&npm.join("claude.cmd"))
            .expect("exe-forwarding shim must resolve to the forwarded .exe");
        assert_eq!(
            std::fs::canonicalize(&resolved.program).ok(),
            std::fs::canonicalize(&expected_exe).ok(),
            "resolved program must be exactly the forwarded .exe, got {:?}",
            resolved.program,
        );
        assert!(resolved.leading_args.is_empty());

        // codex-style shim: forwards to "%_prog%" "<script.js>". Only asserts
        // when node.exe is resolvable; otherwise the resolver returns None and
        // the caller falls back to the .cmd shim (safe — codex pipes its
        // prompt via stdin, never as a command-line argument).
        let js_pkg = npm.join("node_modules/@openai/codex/bin");
        std::fs::create_dir_all(&js_pkg).unwrap();
        let expected_script = js_pkg.join("codex.js");
        std::fs::write(&expected_script, "// entry").unwrap();
        std::fs::write(
            npm.join("codex.cmd"),
            "@ECHO off\n:start\nendLocal & title x & \"%_prog%\"  \"%dp0%\\node_modules\\@openai\\codex\\bin\\codex.js\" %*\n",
        )
        .unwrap();
        if let Some(resolved) = resolve_cmd_shim(&npm.join("codex.cmd")) {
            let script = resolved
                .leading_args
                .iter()
                .find(|arg| arg.ends_with("codex.js"))
                .expect("script path should be a leading arg when node is resolvable");
            assert_eq!(
                std::fs::canonicalize(script).ok(),
                std::fs::canonicalize(&expected_script).ok(),
                "leading script arg must be exactly the forwarded .js",
            );
        }

        // A shim that climbs out of its own directory must be rejected, so a
        // corrupted/malicious shim cannot redirect to an arbitrary executable.
        std::fs::write(dir.join("outside.exe"), b"MZ").unwrap();
        std::fs::write(
            npm.join("poison.cmd"),
            "@ECHO off\n:start\n\"%dp0%\\..\\outside.exe\" %*\n",
        )
        .unwrap();
        assert!(
            resolve_cmd_shim(&npm.join("poison.cmd")).is_none(),
            "a ..-climbing shim must not resolve to a target outside the npm dir"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
