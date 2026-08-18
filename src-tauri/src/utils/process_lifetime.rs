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
        command.process_group(0);
        unsafe {
            command.pre_exec(|| {
                #[cfg(any(target_os = "linux", target_os = "android"))]
                {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(io::Error::last_os_error());
                    }
                }
                if libc::getppid() == 1 {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "parent exited before process launch completed",
                    ));
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
    watchdog_write: std::os::fd::RawFd,
    watchdog_pid: libc::pid_t,
}

#[cfg(unix)]
impl ProcessLifetimeGuard {
    pub(crate) fn attach(child: &mut Child) -> io::Result<Self> {
        let mut pipe = [-1; 2];
        if unsafe { libc::pipe(pipe.as_mut_ptr()) } != 0 {
            terminate_process_tree(child);
            return Err(io::Error::last_os_error());
        }
        for fd in pipe {
            unsafe {
                libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
            }
        }
        let process_group = child.id() as libc::pid_t;
        let watchdog_pid = unsafe { libc::fork() };
        if watchdog_pid == 0 {
            unsafe {
                libc::close(pipe[1]);
                let mut byte = 0_u8;
                loop {
                    let read = libc::read(pipe[0], (&mut byte as *mut u8).cast(), 1);
                    if read <= 0 {
                        break;
                    }
                }
                libc::kill(-process_group, libc::SIGKILL);
                libc::_exit(0);
            }
        }
        unsafe { libc::close(pipe[0]) };
        if watchdog_pid < 0 {
            unsafe { libc::close(pipe[1]) };
            terminate_process_tree(child);
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            watchdog_write: pipe[1],
            watchdog_pid,
        })
    }

    pub(crate) fn terminate(&self, child: &mut Child) {
        terminate_process_tree(child);
    }
}

#[cfg(unix)]
impl Drop for ProcessLifetimeGuard {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.watchdog_write);
            libc::waitpid(self.watchdog_pid, std::ptr::null_mut(), 0);
        }
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
            let alive = unsafe { libc::kill(child_pid, 0) } == 0;
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
}

#[cfg(not(any(unix, windows)))]
pub(crate) struct ProcessLifetimeGuard;

#[cfg(not(any(unix, windows)))]
impl ProcessLifetimeGuard {
    pub(crate) fn attach(_child: &mut Child) -> io::Result<Self> {
        Ok(Self)
    }

    pub(crate) fn terminate(&self, child: &mut Child) {
        terminate_process_tree(child);
    }
}

#[cfg(windows)]
pub(crate) struct ProcessLifetimeGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ProcessLifetimeGuard {
    pub(crate) fn attach(child: &mut Child) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{
            Foundation::{CloseHandle, HANDLE},
            System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
        };
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            terminate_process_tree(child);
            return Err(io::Error::last_os_error());
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
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
