use crate::{
    errors::BackendError,
    services::import_v2::{
        capability_pack::ResolvedCapabilityPack,
        pack_engine::{
            attach_platform_job, terminate_tree, validate_entrypoint_unchanged, PlatformJob,
        },
        url_policy::PrivateTargetGrant,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorSessionRef {
    pub session_id: String,
    pub platform: String,
    pub profile_ref: String,
    pub state: String,
}
struct ManagedChild {
    child: Child,
    _job: Option<PlatformJob>,
}
struct SessionEntry {
    reference: ConnectorSessionRef,
    path: PathBuf,
    child: Option<Arc<Mutex<ManagedChild>>>,
}
#[derive(Default)]
pub struct ConnectorSessionService {
    sessions: Arc<Mutex<HashMap<String, SessionEntry>>>,
    grants: Mutex<HashMap<String, PrivateTargetGrant>>,
}
impl Drop for ConnectorSessionService {
    fn drop(&mut self) {
        let entries = self.sessions.lock().map(|mut sessions| sessions.drain().map(|(_, entry)| entry).collect::<Vec<_>>()).unwrap_or_default();
        for entry in entries {
            if let Some(child) = entry.child {
                if let Ok(mut child) = child.lock() {
                    terminate_tree(&mut child.child);
                    child._job.take();
                }
            }
            let _ = std::fs::remove_dir_all(entry.path);
        }
    }
}
impl ConnectorSessionService {
    pub fn create(
        &self,
        platform: &str,
        profiles_root: &Path,
    ) -> Result<ConnectorSessionRef, BackendError> {
        if !matches!(
            platform,
            "wechat" | "zhihu" | "bilibili" | "xiaohongshu" | "x"
        ) {
            return Err(e("Unsupported connector platform."));
        }
        reject_daily_profile(profiles_root)?;
        self.recover_orphans(profiles_root)?;
        let id = uuid::Uuid::new_v4().to_string();
        let path = profiles_root.join(&id);
        std::fs::create_dir_all(&path)
            .map_err(|_| e("Dedicated browser profile could not be created."))?;
        let r = ConnectorSessionRef {
            session_id: id.clone(),
            platform: platform.into(),
            profile_ref: format!("connector-profile:{id}"),
            state: "waiting_login".into(),
        };
        self.sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .insert(
                id,
                SessionEntry {
                    reference: r.clone(),
                    path,
                    child: None,
                },
            );
        Ok(r)
    }
    pub fn begin_login(
        &self,
        platform: &str,
        profiles_root: &Path,
        pack: &ResolvedCapabilityPack,
        url: &str,
    ) -> Result<ConnectorSessionRef, BackendError> {
        validate_entrypoint_unchanged(pack)?;
        let reference = self.create(platform, profiles_root)?;
        let profile = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .get(&reference.session_id)
            .map(|entry| entry.path.clone())
            .ok_or_else(|| e("Connector session was not found."))?;
        let mut command = Command::new(&pack.entrypoint);
        let runtime_temp = profile.join("runtime-temp");
        std::fs::create_dir_all(&runtime_temp)
            .map_err(|_| e("Connector runtime temp could not be created."))?;
        command
            .current_dir(&pack.root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear();
        for key in ["SystemRoot", "WINDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command.env("TEMP", &runtime_temp).env("TMP", &runtime_temp);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            unsafe {
                command.pre_exec(|| {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0000_0200);
        }
        let mut child = command
            .spawn()
            .map_err(|_| e("The browser login capability could not be started."))?;
        let job = match attach_platform_job(&child) {
            Ok(job) => job,
            Err(error) => {
                terminate_tree(&mut child);
                return Err(error);
            }
        };
        let rpc = serde_json::json!({"jsonrpc":"2.0","id":uuid::Uuid::new_v4().to_string(),"method":"browser.login","params":{"platform":platform,"profilePath":profile,"url":url,"timeoutMs":600000}});
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| e("Browser login stdin is unavailable."))?;
        serde_json::to_writer(&mut stdin, &rpc)
            .map_err(|_| e("Browser login request could not be encoded."))?;
        stdin
            .write_all(b"\n")
            .map_err(|_| e("Browser login request could not be sent."))?;
        drop(stdin);
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| e("Browser login stdout is unavailable."))?;
        let child = Arc::new(Mutex::new(ManagedChild { child, _job: job }));
        self.sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .get_mut(&reference.session_id)
            .ok_or_else(|| e("Connector session was not found."))?
            .child = Some(child.clone());
        let sessions = self.sessions.clone();
        let id = reference.session_id.clone();
        std::thread::spawn(move || {
            let mut output = String::new();
            let _ = stdout.take(1024 * 1024).read_to_string(&mut output);
            let authenticated =
                serde_json::from_str::<serde_json::Value>(output.lines().last().unwrap_or(""))
                    .ok()
                    .and_then(|v| {
                        v.get("result")
                            .and_then(|r| r.get("authenticated"))
                            .and_then(|v| v.as_bool())
                    })
                    .unwrap_or(false);
            if let Ok(mut child) = child.lock() {
                let _ = child.child.wait();
            }
            if let Ok(mut entries) = sessions.lock() {
                if let Some(entry) = entries.get_mut(&id) {
                    entry.reference.state = if authenticated {
                        "authenticated".into()
                    } else {
                        "failed".into()
                    };
                    entry.child = None;
                }
            }
        });
        Ok(reference)
    }
    pub fn resume(&self, id: &str) -> Result<ConnectorSessionRef, BackendError> {
        let s = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?;
        let entry = s
            .get(id)
            .ok_or_else(|| e("Connector session was not found."))?;
        if entry.reference.state != "authenticated" {
            return Err(e("The browser has not proven an authenticated session."));
        }
        Ok(entry.reference.clone())
    }
    pub fn authenticated_profile(&self, id: &str) -> Result<PathBuf, BackendError> {
        let sessions = self.sessions.lock().map_err(|_| e("Connector sessions are unavailable."))?;
        let entry = sessions.get(id).ok_or_else(|| e("Connector session was not found."))?;
        if entry.reference.state != "authenticated" || !entry.path.is_dir() {
            return Err(e("The browser has not proven an authenticated session."));
        }
        Ok(entry.path.clone())
    }
    pub fn revoke(&self, id: &str) -> Result<(), BackendError> {
        if let Some(entry) = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .remove(id)
        {
            if let Some(child) = entry.child {
                if let Ok(mut child) = child.lock() {
                    terminate_tree(&mut child.child);
                    child._job.take();
                }
            }
            std::fs::remove_dir_all(entry.path)
                .map_err(|_| e("Connector profile could not be removed."))?;
        }
        Ok(())
    }
    pub fn recover_orphans(&self, profiles_root: &Path) -> Result<(), BackendError> {
        if !profiles_root.exists() {
            return Ok(());
        }
        let active = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .values()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        for entry in std::fs::read_dir(profiles_root)
            .map_err(|_| e("Connector profiles could not be inspected."))?
        {
            let path = entry
                .map_err(|_| e("Connector profile entry is invalid."))?
                .path();
            if active.contains(&path) {
                continue;
            }
            let stale = path
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| SystemTime::now().duration_since(t).ok())
                .is_none_or(|age| age >= Duration::from_secs(24 * 60 * 60));
            if stale {
                std::fs::remove_dir_all(path)
                    .map_err(|_| e("Orphan connector profile could not be removed."))?;
            }
        }
        Ok(())
    }
    pub fn authorize_private(&self, grant: PrivateTargetGrant) -> Result<String, BackendError> {
        let id = format!("private-grant:{}", uuid::Uuid::new_v4());
        self.grants
            .lock()
            .map_err(|_| e("Private grants are unavailable."))?
            .insert(id.clone(), grant);
        Ok(id)
    }
    pub fn take_private(&self, id: &str) -> Result<Option<PrivateTargetGrant>, BackendError> {
        Ok(self
            .grants
            .lock()
            .map_err(|_| e("Private grants are unavailable."))?
            .remove(id))
    }
}
fn reject_daily_profile(path: &Path) -> Result<(), BackendError> {
    let p = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    if [
        "google/chrome",
        "microsoft/edge",
        "mozilla/firefox",
        "user data",
    ]
    .iter()
    .any(|x| p.contains(x))
    {
        return Err(e("Daily browser profiles are forbidden."));
    }
    Ok(())
}
fn e(m: &str) -> BackendError {
    BackendError::new("IMPORT_V2_BROWSER_SESSION_FAILED", m, true, true)
}
