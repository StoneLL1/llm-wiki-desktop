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
        let mut child = Command::new(resolve_program(&invocation.program))
            .args(&invocation.args)
            .current_dir(&invocation.cwd)
            .stdin(if invocation.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                BackendError::new("AGENT_SPAWN_FAILED", error.to_string(), true, false)
            })?;
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
        let mut child = Command::new(resolve_program(&invocation.program))
            .args(&invocation.args)
            .current_dir(&invocation.cwd)
            .stdin(if invocation.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                BackendError::new("AGENT_SPAWN_FAILED", error.to_string(), true, false)
            })?;
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

/// Resolve a bare program name to a spawnable path. On Windows, `CreateProcess`
/// (and therefore `Command::new`) will not find a `.cmd` shim via PATH in
/// processes whose PATH lacks the npm global dir; resolving to the full path
/// first lets `Command` run the `.cmd` (std internally routes `.cmd` through
/// `cmd.exe`). On non-Windows, or when the program is already a path, it is
/// returned unchanged.
fn resolve_program(program: &str) -> String {
    if cfg!(not(windows)) || program.contains('/') || program.contains('\\') {
        return program.to_string();
    }
    match find_executable(program) {
        Some(resolved) => resolved.to_string_lossy().into_owned(),
        None => program.to_string(),
    }
}

fn run_with_timeout(
    command: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, BackendError> {
    let mut child = Command::new(resolve_program(command))
        .args(args)
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
    fn claude_invocations_isolate_from_user_session_state() {
        // Regression guard: every Claude invocation must pass --bare so
        // programmatic runs do not load the host's ~/.claude hooks, MCP
        // servers, plugin sync, or auto-memory. Without --bare, a Claude
        // configured with blocking MCP servers (e.g. claude-mem, zotero) hangs
        // indefinitely during init when spawned from the GUI process, because
        // --print never reaches the model while MCP/SessionStart hooks stall.
        let workspace = std::env::temp_dir().join("llm-wiki-desktop/bare-invariant-test");
        std::fs::create_dir_all(&workspace).unwrap();
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
    fn resolve_program_passes_through_paths_and_non_windows_bare_names() {
        // Already-qualified paths are returned unchanged on every platform...
        assert_eq!(resolve_program("/usr/bin/claude"), "/usr/bin/claude");
        assert_eq!(resolve_program("./local/agent"), "./local/agent");
        // ...and bare names are left as-is when nothing is resolvable, so the
        // caller (e.g. `Command::new`) still gets the original input rather
        // than an empty string.
        let bare = resolve_program("definitely-not-installed-cli-xyz");
        assert!(
            bare == "definitely-not-installed-cli-xyz",
            "unresolvable bare name should pass through unchanged, got {bare:?}"
        );
    }
}
