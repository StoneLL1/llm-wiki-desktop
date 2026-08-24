use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct CapturedProcessOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

#[derive(Debug)]
pub(crate) enum BoundedProcessError {
    Spawn(io::Error),
    Isolation(io::Error),
    Stdin(io::Error),
    Read(io::Error),
    Wait(io::Error),
    Cancelled,
    Timeout,
    OutputTooLarge,
}

enum CaptureEvent {
    Stdin(Result<(), BoundedProcessError>),
    Stdout(Result<Vec<u8>, BoundedProcessError>),
    Stderr(Result<Vec<u8>, BoundedProcessError>),
}

/// Run a capture-only child with a hard deadline, cancellation callback,
/// raw-byte bounds and complete process-tree ownership.
pub(crate) fn run_bounded_process(
    command: &mut Command,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    max_stream_bytes: usize,
    cancelled: impl Fn() -> bool,
) -> Result<CapturedProcessOutput, BoundedProcessError> {
    command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_isolated_process(command);
    let mut child = command.spawn().map_err(BoundedProcessError::Spawn)?;
    let lifetime =
        ProcessLifetimeGuard::attach(&mut child).map_err(BoundedProcessError::Isolation)?;
    let stdin_pending = stdin.is_some();
    let (sender, receiver) = mpsc::channel();
    let mut stdin_complete = !stdin_pending;
    stdin.and_then(|input| {
        child.stdin.take().map(|mut writer| {
            let sender = sender.clone();
            thread::spawn(move || {
                let result = writer.write_all(&input).map_err(BoundedProcessError::Stdin);
                let _ = sender.send(CaptureEvent::Stdin(result));
            })
        })
    });
    if let Some(stdout) = child.stdout.take() {
        let sender = sender.clone();
        thread::spawn(move || {
            let _ = sender.send(CaptureEvent::Stdout(read_capture(stdout, max_stream_bytes)));
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let sender = sender.clone();
        thread::spawn(move || {
            let _ = sender.send(CaptureEvent::Stderr(read_capture(stderr, max_stream_bytes)));
        });
    }
    drop(sender);

    let deadline = Instant::now() + timeout;
    let mut stdout = None;
    let mut stderr = None;
    let status = loop {
        while let Ok(event) = receiver.try_recv() {
            store_capture(event, &mut stdin_complete, &mut stdout, &mut stderr).map_err(
                |error| {
                    lifetime.terminate(&mut child);
                    error
                },
            )?;
        }
        if cancelled() {
            lifetime.terminate(&mut child);
            return Err(BoundedProcessError::Cancelled);
        }
        if Instant::now() >= deadline {
            lifetime.terminate(&mut child);
            return Err(BoundedProcessError::Timeout);
        }
        let observed = child.try_wait().map_err(|error| {
            lifetime.terminate(&mut child);
            BoundedProcessError::Wait(error)
        })?;
        match observed {
            Some(status) => break status,
            None => thread::sleep(Duration::from_millis(10)),
        }
    };

    while !stdin_complete || stdout.is_none() || stderr.is_none() {
        if cancelled() {
            lifetime.terminate(&mut child);
            return Err(BoundedProcessError::Cancelled);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            lifetime.terminate(&mut child);
            return Err(BoundedProcessError::Timeout);
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(event) => store_capture(event, &mut stdin_complete, &mut stdout, &mut stderr)?,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    if !stdin_complete || stdout.is_none() || stderr.is_none() {
        lifetime.terminate(&mut child);
        return Err(BoundedProcessError::Wait(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "process capture channel closed before all streams completed",
        )));
    }
    Ok(CapturedProcessOutput {
        status,
        stdout: stdout.unwrap_or_default(),
        stderr: stderr.unwrap_or_default(),
    })
}

fn read_capture(
    reader: impl Read,
    max_stream_bytes: usize,
) -> Result<Vec<u8>, BoundedProcessError> {
    let mut bytes = Vec::new();
    reader
        .take(max_stream_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(BoundedProcessError::Read)?;
    if bytes.len() > max_stream_bytes {
        Err(BoundedProcessError::OutputTooLarge)
    } else {
        Ok(bytes)
    }
}

fn store_capture(
    event: CaptureEvent,
    stdin_complete: &mut bool,
    stdout: &mut Option<Vec<u8>>,
    stderr: &mut Option<Vec<u8>>,
) -> Result<(), BoundedProcessError> {
    match event {
        CaptureEvent::Stdin(Ok(())) => *stdin_complete = true,
        CaptureEvent::Stdin(Err(BoundedProcessError::Stdin(error)))
            if error.kind() == io::ErrorKind::BrokenPipe =>
        {
            *stdin_complete = true;
        }
        CaptureEvent::Stdin(Err(error)) => return Err(error),
        CaptureEvent::Stdout(result) => *stdout = Some(result?),
        CaptureEvent::Stderr(result) => *stderr = Some(result?),
    }
    Ok(())
}

/// Configure a child so the application can account for and terminate the
/// complete process tree. Call this before `spawn`, then attach a
/// [`ProcessLifetimeGuard`] immediately after spawning.
pub(crate) fn configure_isolated_process(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let expected_parent = unsafe { libc::getpid() };
        command.process_group(0);
        unsafe {
            command.pre_exec(move || {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                if libc::getppid() != expected_parent {
                    return Err(io::Error::from_raw_os_error(libc::ECHILD));
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::CREATE_SUSPENDED;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
    }
}

/// Clean up a child that could not be attached to the platform process-tree
/// guard. On Windows the child is still suspended, so a direct kill is enough;
/// on Unix the process group was established before exec and is also reaped.
pub(crate) fn terminate_process_tree(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
pub(crate) struct ProcessLifetimeGuard {
    watchdog_write: Option<std::fs::File>,
    watchdog: Child,
}

#[cfg(unix)]
impl ProcessLifetimeGuard {
    pub(crate) fn attach(child: &mut Child) -> io::Result<Self> {
        use std::os::fd::FromRawFd;

        let mut pipe = [-1; 2];
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let pipe_result = unsafe { libc::pipe2(pipe.as_mut_ptr(), libc::O_CLOEXEC) };
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let pipe_result = unsafe { libc::pipe(pipe.as_mut_ptr()) };
        if pipe_result != 0 {
            terminate_process_tree(child);
            return Err(io::Error::last_os_error());
        }

        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        for fd in pipe {
            if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
                let error = io::Error::last_os_error();
                unsafe {
                    libc::close(pipe[0]);
                    libc::close(pipe[1]);
                }
                terminate_process_tree(child);
                return Err(error);
            }
        }
        let process_group = child.id() as libc::pid_t;
        // SAFETY: both descriptors are uniquely owned after a successful
        // `pipe`/`pipe2` call and are transferred into their File values once.
        let watchdog_read = unsafe { std::fs::File::from_raw_fd(pipe[0]) };
        let watchdog_write = unsafe { std::fs::File::from_raw_fd(pipe[1]) };
        // Use exec rather than fork-only watchdog logic. A fork-only child
        // inherits every other in-flight guard pipe in this multithreaded
        // process, so later watchdogs can keep earlier guards alive and turn
        // independent short Git calls into a deadline-sized wait chain. Exec
        // closes those unrelated CLOEXEC descriptors while stdin retains only
        // this guard's read end.
        let watchdog = Command::new("/bin/sh")
            .args([
                "-c",
                "while IFS= read -r _; do :; done; kill -KILL \"-$1\" 2>/dev/null || :",
                "llm-wiki-process-watchdog",
                &process_group.to_string(),
            ])
            .stdin(Stdio::from(watchdog_read))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let watchdog = match watchdog {
            Ok(watchdog) => watchdog,
            Err(error) => {
                drop(watchdog_write);
                terminate_process_tree(child);
                return Err(error);
            }
        };
        Ok(Self {
            watchdog_write: Some(watchdog_write),
            watchdog,
        })
    }

    pub(crate) fn attach_capability(child: &mut Child) -> io::Result<Self> {
        Self::attach(child)
    }

    pub(crate) fn terminate(&self, child: &mut Child) {
        terminate_process_tree(child);
    }
}

#[cfg(unix)]
impl Drop for ProcessLifetimeGuard {
    fn drop(&mut self) {
        // Closing the owner pipe is terminal for this process tree even when
        // the direct child exited successfully: it may have left descendants
        // with redirected stdio behind. The exec'd watchdog owns the final
        // group cleanup without retaining unrelated guards' CLOEXEC pipes.
        drop(self.watchdog_write.take());
        let _ = self.watchdog.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_process_rejects_raw_output_over_limit() {
        #[cfg(windows)]
        let mut command = {
            let mut command =
                Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
            command.args([
                "-NoProfile",
                "-Command",
                "[Console]::Out.Write(('x' * 4096))",
            ]);
            command
        };
        #[cfg(unix)]
        let mut command = {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", "head -c 4096 /dev/zero"]);
            command
        };

        let error = run_bounded_process(&mut command, None, Duration::from_secs(5), 128, || false)
            .expect_err("raw output beyond the configured limit must fail");
        assert!(matches!(error, BoundedProcessError::OutputTooLarge));
    }

    #[cfg(windows)]
    #[test]
    fn configured_windows_child_runs_after_lifetime_attachment() {
        let mut command =
            Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        command.args([
            "-NoProfile",
            "-Command",
            "[Console]::Out.Write('lifetime-ready')",
        ]);

        let output =
            run_bounded_process(&mut command, None, Duration::from_secs(5), 1024, || false)
                .expect("the Job Object attachment must resume the suspended child");

        assert!(output.status.success());
        assert_eq!(output.stdout, b"lifetime-ready");
        assert!(output.stderr.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn capability_jobs_disable_unhandled_exception_dialogs() {
        use windows_sys::Win32::System::JobObjects::{
            JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        let default_flags = windows_job_limit_flags(false);
        let capability_flags = windows_job_limit_flags(true);

        assert_eq!(
            default_flags & JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
            0
        );
        assert_ne!(
            capability_flags & JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
            0
        );
        assert_ne!(capability_flags & JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, 0);
    }

    #[cfg(unix)]
    #[test]
    fn timeout_reaps_the_direct_child_and_its_process_group() {
        let root =
            std::env::temp_dir().join(format!("llm-wiki-process-tree-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let child_pid_path = root.join("child.pid");
        let script = format!(
            "sh -c 'echo $$ > \"{}\"; while :; do sleep 1; done' & wait",
            child_pid_path.display()
        );
        let mut command = Command::new("/bin/sh");
        command.args(["-c", &script]);

        let error =
            run_bounded_process(&mut command, None, Duration::from_millis(300), 1024, || {
                false
            })
            .expect_err("the process tree should hit the deadline");
        assert!(matches!(error, BoundedProcessError::Timeout));

        let child_pid = std::fs::read_to_string(&child_pid_path)
            .unwrap()
            .trim()
            .parse::<libc::pid_t>()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let alive = unsafe { libc::kill(-child_pid, 0) } == 0;
            if !alive {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "process-group child survived timeout: {child_pid}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn watchdog_reaps_process_group_after_abrupt_owner_exit() {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-process-watchdog-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let child_pid_path = root.join("child.pid");
        let status = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "utils::process_lifetime::tests::unix_watchdog_abrupt_exit_helper",
                "--nocapture",
            ])
            .env("LLM_WIKI_WATCHDOG_HELPER_PID_PATH", &child_pid_path)
            .status()
            .unwrap();
        assert!(status.success());

        let child_pid = std::fs::read_to_string(&child_pid_path)
            .unwrap()
            .trim()
            .parse::<libc::pid_t>()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let alive = unsafe { libc::kill(child_pid, 0) } == 0;
            if !alive {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "watchdog-owned process group survived owner exit: {child_pid}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_successful_processes_do_not_retain_each_others_watchdogs() {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-process-watchdog-overlap-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let ready_a = root.join("ready-a");
        let ready_b = root.join("ready-b");
        let release_a = root.join("release-a");
        let release_b = root.join("release-b");

        let spawn_guarded = |ready: std::path::PathBuf, release: std::path::PathBuf| {
            let (result_tx, result_rx) = mpsc::channel();
            let worker = thread::spawn(move || {
                let script = format!(
                    "touch \"{}\"; while [ ! -e \"{}\" ]; do sleep 0.01; done",
                    ready.display(),
                    release.display()
                );
                let mut command = Command::new("/bin/sh");
                command.args(["-c", &script]);
                // Keep the guarded child deadline well beyond the assertion
                // window. If watchdog descriptors leak across exec, process A
                // must still be blocked while B is live instead of coinciding
                // with B's own timeout.
                let result =
                    run_bounded_process(&mut command, None, Duration::from_secs(15), 1024, || {
                        false
                    });
                let _ = result_tx.send(result);
            });
            (worker, result_rx)
        };

        let (worker_a, result_a) = spawn_guarded(ready_a.clone(), release_a.clone());
        wait_for_path(&ready_a);
        let (worker_b, result_b) = spawn_guarded(ready_b.clone(), release_b.clone());
        wait_for_path(&ready_b);

        std::fs::write(&release_a, b"release").unwrap();
        let a_finished_while_b_was_live = result_a.recv_timeout(Duration::from_secs(5));
        std::fs::write(&release_b, b"release").unwrap();
        let b_result = result_b.recv_timeout(Duration::from_secs(5)).unwrap();
        worker_a.join().unwrap();
        worker_b.join().unwrap();

        assert!(a_finished_while_b_was_live
            .expect("process A retained process B's watchdog descriptors")
            .is_ok());
        assert!(b_result.is_ok());
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    #[test]
    fn successful_leader_cannot_leave_a_background_descendant_running() {
        let root = std::env::temp_dir().join(format!(
            "llm-wiki-process-watchdog-background-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let pid_path = root.join("background.pid");
        let script = format!(
            "(/bin/sh -c 'exec </dev/null >/dev/null 2>&1; while :; do sleep 30; done') & echo $! > \"{}\"; exit 0",
            pid_path.display()
        );
        let mut command = Command::new("/bin/sh");
        command.args(["-c", &script]);

        let result =
            run_bounded_process(&mut command, None, Duration::from_secs(2), 1024, || false)
                .unwrap();
        assert!(result.status.success());
        let background_pid = std::fs::read_to_string(&pid_path)
            .unwrap()
            .trim()
            .parse::<libc::pid_t>()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let alive = unsafe { libc::kill(background_pid, 0) } == 0;
            if !alive {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "successful leader left background descendant alive: {background_pid}"
            );
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[cfg(unix)]
    fn wait_for_path(path: &std::path::Path) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !path.exists() {
            assert!(
                Instant::now() < deadline,
                "guarded child did not publish readiness: {}",
                path.display()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_watchdog_abrupt_exit_helper() {
        let Some(child_pid_path) = std::env::var_os("LLM_WIKI_WATCHDOG_HELPER_PID_PATH") else {
            return;
        };
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "sleep 30 & wait"]);
        configure_isolated_process(&mut command);
        let mut child = command.spawn().unwrap();
        let _lifetime = ProcessLifetimeGuard::attach(&mut child).unwrap();
        std::fs::write(child_pid_path, child.id().to_string()).unwrap();
        thread::sleep(Duration::from_millis(50));

        // Simulate an abrupt application exit: destructors do not run, but the
        // OS closes the guard pipe and the watchdog must kill the process group.
        std::process::exit(0);
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) struct ProcessLifetimeGuard;

#[cfg(not(any(unix, windows)))]
impl ProcessLifetimeGuard {
    pub(crate) fn attach(_child: &mut Child) -> io::Result<Self> {
        Ok(Self)
    }

    pub(crate) fn attach_capability(child: &mut Child) -> io::Result<Self> {
        Self::attach(child)
    }

    pub(crate) fn terminate(&self, child: &mut Child) {
        terminate_process_tree(child);
    }
}

#[cfg(windows)]
pub(crate) struct ProcessLifetimeGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
// SAFETY: the guard owns a Job Object handle. Windows handles may be moved
// between threads, and all access remains synchronized by the owning value.
unsafe impl Send for ProcessLifetimeGuard {}

#[cfg(windows)]
impl ProcessLifetimeGuard {
    pub(crate) fn attach(child: &mut Child) -> io::Result<Self> {
        Self::attach_with_unhandled_exception_policy(child, false)
    }

    pub(crate) fn attach_capability(child: &mut Child) -> io::Result<Self> {
        Self::attach_with_unhandled_exception_policy(child, true)
    }

    fn attach_with_unhandled_exception_policy(
        child: &mut Child,
        terminate_on_unhandled_exception: bool,
    ) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            },
        };
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            terminate_process_tree(child);
            return Err(io::Error::last_os_error());
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        information.BasicLimitInformation.LimitFlags =
            windows_job_limit_flags(terminate_on_unhandled_exception);
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &information as *const _ as *const std::ffi::c_void,
                std::mem::size_of_val(&information) as u32,
            )
        } != 0;
        let assigned = configured
            && unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) } != 0;
        if !assigned {
            unsafe { CloseHandle(job) };
            terminate_process_tree(child);
            return Err(io::Error::last_os_error());
        }
        if let Err(error) = resume_suspended_child(child) {
            unsafe { CloseHandle(job) };
            terminate_process_tree(child);
            return Err(error);
        }
        Ok(Self(job))
    }

    pub(crate) fn terminate(&self, child: &mut Child) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1);
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[cfg(windows)]
fn windows_job_limit_flags(terminate_on_unhandled_exception: bool) -> u32 {
    use windows_sys::Win32::System::JobObjects::{
        JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    let mut flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if terminate_on_unhandled_exception {
        flags |= JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    }
    flags
}

#[cfg(windows)]
fn resume_suspended_child(child: &Child) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::HANDLE;

    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtResumeProcess(process_handle: HANDLE) -> i32;
    }

    // std::process does not expose the primary thread handle returned by
    // CreateProcessW. Resuming the already-owned process handle avoids a
    // system-wide thread snapshot for every Git/Agent launch while preserving
    // the required order: CREATE_SUSPENDED -> assign Job Object -> resume.
    let status = unsafe { NtResumeProcess(child.as_raw_handle() as HANDLE) };
    if status >= 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "suspended child process could not be resumed (NTSTATUS 0x{:08x})",
            status as u32
        )))
    }
}

#[cfg(windows)]
impl Drop for ProcessLifetimeGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
