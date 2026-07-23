use crate::{
    errors::BackendError,
    services::import_v2::{
        capability_pack::ResolvedCapabilityPack,
        media_router::TemporaryMediaWorkspace,
        pack_engine::{
            attach_platform_job, terminate_tree, validate_entrypoint_unchanged, PlatformJob,
        },
        url_policy::PrivateTargetGrant,
    },
    services::SecretService,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    _runtime_temp: TemporaryMediaWorkspace,
}
struct SessionEntry {
    reference: ConnectorSessionRef,
    path: PathBuf,
    child: Option<Arc<Mutex<ManagedChild>>>,
    binding: Option<ConnectorSessionBinding>,
}
#[derive(Clone, PartialEq, Eq)]
struct ConnectorSessionBinding {
    project_id: String,
    import_session_id: String,
    item_id: String,
    target_sha256: String,
}
#[derive(Default)]
pub struct ConnectorSessionService {
    sessions: Arc<Mutex<HashMap<String, SessionEntry>>>,
    grants: Mutex<HashMap<String, PrivateTargetGrant>>,
    secrets: SecretService,
}
impl Drop for ConnectorSessionService {
    fn drop(&mut self) {
        let entries = self
            .sessions
            .lock()
            .map(|mut sessions| sessions.drain().map(|(_, entry)| entry).collect::<Vec<_>>())
            .unwrap_or_default();
        for entry in entries {
            if let Some(child) = entry.child {
                if let Ok(mut child) = child.lock() {
                    terminate_tree(&mut child.child);
                    child._job.take();
                }
            }
            // Connector profiles are intentionally persistent across app
            // restarts. Explicit revoke is the only operation that removes
            // one; dropping the service merely terminates the helper process.
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
            "wechat" | "zhihu" | "bilibili" | "xiaohongshu" | "douyin" | "x"
        ) {
            return Err(e("Unsupported connector platform."));
        }
        reject_daily_profile(profiles_root)?;
        let profiles_root = prepare_profiles_root(profiles_root)?;
        self.recover_orphans(&profiles_root)?;
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?;
        if sessions.values().any(|entry| {
            entry.reference.platform == platform
                && matches!(
                    entry.reference.state.as_str(),
                    "waiting_login" | "authenticated"
                )
        }) {
            return Err(e(
                "A connector login is already active for this platform. Revoke it before starting another.",
            ));
        }
        sessions.retain(|_, entry| {
            entry.reference.platform != platform || entry.reference.state != "failed"
        });
        let id = uuid::Uuid::new_v4().to_string();
        // One isolated persistent Chromium profile per platform. The session
        // id remains transient and only binds the login result to one import
        // item; the profile itself is reused by the next import.
        let path = prepare_platform_profile(&profiles_root, platform)?;
        let r = ConnectorSessionRef {
            session_id: id.clone(),
            platform: platform.into(),
            profile_ref: format!("connector-profile:{platform}"),
            state: "waiting_login".into(),
        };
        sessions.insert(
            id,
            SessionEntry {
                reference: r.clone(),
                path,
                child: None,
                binding: None,
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
        project_id: &str,
        import_session_id: &str,
        item_id: &str,
    ) -> Result<ConnectorSessionRef, BackendError> {
        validate_entrypoint_unchanged(pack)?;
        let reference = self.create(platform, profiles_root)?;
        let binding = ConnectorSessionBinding {
            project_id: project_id.to_string(),
            import_session_id: import_session_id.to_string(),
            item_id: item_id.to_string(),
            target_sha256: format!("{:x}", Sha256::digest(url.as_bytes())),
        };
        let profile = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .get_mut(&reference.session_id)
            .map(|entry| {
                entry.binding = Some(binding);
                entry.path.clone()
            })
            .ok_or_else(|| e("Connector session was not found."))?;
        let cookie_account = format!("connector-cookie:{platform}");
        let cookie_backup = self
            .secrets
            .get_account(&cookie_account)?
            .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
            .filter(|value| value.is_array());
        let secrets = self.secrets.clone();
        let platform_name = platform.to_string();
        let mut command = Command::new(&pack.entrypoint);
        let runtime_temp = TemporaryMediaWorkspace::create_unique(&profile, ".login-runtime")?;
        command
            .args(&pack.manifest.entrypoint_args)
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
        // Interactive connector login is headed. Preserve only the display/session
        // variables Chromium needs after env_clear; never forward arbitrary env vars.
        #[cfg(unix)]
        for key in [
            "DISPLAY",
            "WAYLAND_DISPLAY",
            "XAUTHORITY",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("TEMP", runtime_temp.path())
            .env("TMP", runtime_temp.path());
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
        let rpc = serde_json::json!({"jsonrpc":"2.0","id":uuid::Uuid::new_v4().to_string(),"method":"browser.login","params":{"platform":platform,"profilePath":profile,"url":url,"timeoutMs":600000,"cookieBackup":cookie_backup}});
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
        let child = Arc::new(Mutex::new(ManagedChild {
            child,
            _job: job,
            _runtime_temp: runtime_temp,
        }));
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
            let login_result =
                serde_json::from_str::<serde_json::Value>(output.lines().last().unwrap_or(""))
                    .ok()
                    .and_then(|v| v.get("result").cloned());
            let authenticated = login_result
                .as_ref()
                .and_then(|r| r.get("authenticated"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if authenticated {
                if let Some(cookies) = login_result.and_then(|r| r.get("cookies").cloned()) {
                    if cookies.is_array()
                        && serde_json::to_vec(&cookies).is_ok_and(|v| v.len() <= 64 * 1024)
                    {
                        let _ = secrets.set_account(
                            &format!("connector-cookie:{platform_name}"),
                            &cookies.to_string(),
                        );
                    }
                }
            }
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
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?;
        let entry = sessions
            .get(id)
            .ok_or_else(|| e("Connector session was not found."))?;
        if entry.reference.state != "authenticated" || !profile_is_unchanged(&entry.path) {
            return Err(e("The browser has not proven an authenticated session."));
        }
        Ok(entry.path.clone())
    }
    pub fn take_authenticated_profile_bound(
        &self,
        id: &str,
        project_id: &str,
        import_session_id: &str,
        item_id: &str,
        target_url: &str,
    ) -> Result<(ConnectorSessionRef, PathBuf), BackendError> {
        let expected = ConnectorSessionBinding {
            project_id: project_id.to_string(),
            import_session_id: import_session_id.to_string(),
            item_id: item_id.to_string(),
            target_sha256: format!("{:x}", Sha256::digest(target_url.as_bytes())),
        };
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?;
        let entry = sessions
            .get(id)
            .ok_or_else(|| e("Connector session was not found."))?;
        if entry.reference.state != "authenticated"
            || entry.binding.as_ref() != Some(&expected)
            || !profile_is_unchanged(&entry.path)
        {
            return Err(e(
                "The authenticated connector is not bound to this import item.",
            ));
        }
        let entry = sessions
            .remove(id)
            .ok_or_else(|| e("Connector session was not found."))?;
        Ok((entry.reference, entry.path))
    }
    pub fn revoke(&self, id: &str) -> Result<(), BackendError> {
        if let Some(entry) = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .remove(id)
        {
            let platform = entry.reference.platform.clone();
            if let Some(child) = entry.child {
                if let Ok(mut child) = child.lock() {
                    terminate_tree(&mut child.child);
                    child._job.take();
                }
            }
            let profile = validate_profile_directory(&entry.path)?;
            std::fs::remove_dir_all(profile)
                .map_err(|_| e("Connector profile could not be removed."))?;
            self.secrets
                .delete_account(&format!("connector-cookie:{platform}"))?;
        }
        Ok(())
    }
    pub fn revoke_platform(
        &self,
        platform: &str,
        profiles_root: &Path,
    ) -> Result<(), BackendError> {
        if !matches!(
            platform,
            "wechat" | "zhihu" | "bilibili" | "xiaohongshu" | "douyin" | "x"
        ) {
            return Err(e("Unsupported connector platform."));
        }
        let entries = {
            let mut sessions = self
                .sessions
                .lock()
                .map_err(|_| e("Connector sessions are unavailable."))?;
            let ids = sessions
                .iter()
                .filter(|(_, entry)| entry.reference.platform == platform)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|id| sessions.remove(&id))
                .collect::<Vec<_>>()
        };
        for entry in entries {
            if let Some(child) = entry.child {
                if let Ok(mut child) = child.lock() {
                    terminate_tree(&mut child.child);
                    child._job.take();
                }
            }
        }
        if profiles_root.exists() {
            let root = validate_profile_directory(profiles_root)?;
            let profile = root.join(platform);
            if profile.exists() {
                let profile = validate_profile_directory(&profile)?;
                if profile.parent() != Some(root.as_path()) {
                    return Err(e("Dedicated browser profile escaped its profile root."));
                }
                std::fs::remove_dir_all(profile)
                    .map_err(|_| e("Connector profile could not be removed."))?;
            }
        }
        self.secrets
            .delete_account(&format!("connector-cookie:{platform}"))
    }
    pub fn recover_orphans(&self, profiles_root: &Path) -> Result<(), BackendError> {
        if !profiles_root.exists() {
            return Ok(());
        }
        let profiles_root = validate_profile_directory(profiles_root)?;
        let active = self
            .sessions
            .lock()
            .map_err(|_| e("Connector sessions are unavailable."))?
            .values()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        for entry in std::fs::read_dir(&profiles_root)
            .map_err(|_| e("Connector profiles could not be inspected."))?
        {
            let path = entry
                .map_err(|_| e("Connector profile entry is invalid."))?
                .path();
            if active.contains(&path) {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| e("Connector profile entry is invalid."))?;
            if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
                continue;
            }
            let platform = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if matches!(
                platform,
                "wechat" | "zhihu" | "bilibili" | "xiaohongshu" | "douyin" | "x"
            ) {
                continue;
            }
            let stale = metadata
                .modified()
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

fn prepare_profiles_root(path: &Path) -> Result<PathBuf, BackendError> {
    if path.exists() {
        let path = validate_profile_directory(path)?;
        harden_private_directory(&path)?;
        return Ok(path);
    }
    std::fs::create_dir_all(path).map_err(|_| e("Connector profile root could not be created."))?;
    let path = validate_profile_directory(path)?;
    harden_private_directory(&path)?;
    Ok(path)
}

fn prepare_platform_profile(root: &Path, platform: &str) -> Result<PathBuf, BackendError> {
    let path = root.join(platform);
    if path.exists() {
        let canonical = validate_profile_directory(&path)?;
        if canonical.parent() != Some(root) {
            return Err(e("Dedicated browser profile escaped its profile root."));
        }
        harden_private_directory(&canonical)?;
        return Ok(canonical);
    }
    std::fs::create_dir(&path).map_err(|_| e("Dedicated browser profile could not be created."))?;
    let canonical = validate_profile_directory(&path)?;
    if canonical.parent() != Some(root) {
        return Err(e("Dedicated browser profile escaped its profile root."));
    }
    harden_private_directory(&canonical)?;
    Ok(canonical)
}

#[cfg(unix)]
fn harden_private_directory(path: &Path) -> Result<(), BackendError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|_| e("Connector profile permissions could not be restricted."))?;
    let mode = std::fs::metadata(path)
        .map_err(|_| e("Connector profile permissions could not be verified."))?
        .permissions()
        .mode();
    if mode & 0o077 != 0 {
        return Err(e("Connector profile permissions are too broad."));
    }
    Ok(())
}

#[cfg(not(unix))]
fn harden_private_directory(_path: &Path) -> Result<(), BackendError> {
    Ok(())
}

fn validate_profile_directory(path: &Path) -> Result<PathBuf, BackendError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| e("Connector profile directory is unavailable."))?;
    if metadata.file_type().is_symlink() || is_reparse(&metadata) || !metadata.is_dir() {
        return Err(e("Connector profile directory is invalid."));
    }
    path.canonicalize()
        .map_err(|_| e("Connector profile directory cannot be resolved."))
}

fn profile_is_unchanged(path: &Path) -> bool {
    validate_profile_directory(path).is_ok_and(|canonical| canonical == path)
}

#[cfg(windows)]
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn is_reparse(_: &std::fs::Metadata) -> bool {
    false
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

#[cfg(test)]
mod binding_tests {
    use super::*;

    #[test]
    fn authenticated_profile_is_exactly_bound_and_single_use() {
        let service = ConnectorSessionService::default();
        let root = tempfile::tempdir().unwrap();
        let profile = root.path().join("profile");
        std::fs::create_dir(&profile).unwrap();
        let profile = profile.canonicalize().unwrap();
        let id = "connector-a".to_string();
        let target = "https://www.bilibili.com/video/BV1exact?token=secret";
        service.sessions.lock().unwrap().insert(
            id.clone(),
            SessionEntry {
                reference: ConnectorSessionRef {
                    session_id: id.clone(),
                    platform: "bilibili".into(),
                    profile_ref: "connector-profile:connector-a".into(),
                    state: "authenticated".into(),
                },
                path: profile.clone(),
                child: None,
                binding: Some(ConnectorSessionBinding {
                    project_id: "project-a".into(),
                    import_session_id: "session-a".into(),
                    item_id: "item-a".into(),
                    target_sha256: format!("{:x}", Sha256::digest(target.as_bytes())),
                }),
            },
        );
        assert!(service
            .take_authenticated_profile_bound(&id, "project-b", "session-a", "item-a", target)
            .is_err());
        assert!(service.resume(&id).is_ok());
        let (_, taken) = service
            .take_authenticated_profile_bound(&id, "project-a", "session-a", "item-a", target)
            .unwrap();
        assert_eq!(taken, profile);
        assert!(service
            .take_authenticated_profile_bound(&id, "project-a", "session-a", "item-a", target)
            .is_err());
    }

    #[test]
    fn media_connector_uses_one_persistent_profile_per_platform() {
        let service = ConnectorSessionService::default();
        let root = tempfile::tempdir().unwrap();
        let first = service.create("douyin", root.path()).unwrap();
        let first_path = service
            .sessions
            .lock()
            .unwrap()
            .get(&first.session_id)
            .unwrap()
            .path
            .clone();
        assert!(service.create("douyin", root.path()).is_err());
        service.revoke(first.session_id.as_str()).unwrap();
        let second = service.create("douyin", root.path()).unwrap();
        let second_path = service
            .sessions
            .lock()
            .unwrap()
            .get(&second.session_id)
            .unwrap()
            .path
            .clone();

        assert_eq!(
            first_path,
            root.path().canonicalize().unwrap().join("douyin")
        );
        assert_eq!(first_path, second_path);
        assert!(first_path.is_dir());

        assert_eq!(first_path, second_path);
        service.revoke(second.session_id.as_str()).unwrap();
        assert!(!first_path.exists());
    }

    #[test]
    fn platform_revoke_removes_a_profile_without_a_live_session() {
        let service = ConnectorSessionService::default();
        let root = tempfile::tempdir().unwrap();
        let profiles = root.path().join("profiles");
        std::fs::create_dir(&profiles).unwrap();
        std::fs::create_dir(profiles.join("bilibili")).unwrap();
        service.revoke_platform("bilibili", &profiles).unwrap();
        assert!(!profiles.join("bilibili").exists());
    }

    #[cfg(unix)]
    #[test]
    fn platform_profiles_are_restricted_to_the_current_user() {
        use std::os::unix::fs::PermissionsExt;
        let service = ConnectorSessionService::default();
        let root = tempfile::tempdir().unwrap();
        let reference = service.create("douyin", root.path()).unwrap();
        let path = service.sessions.lock().unwrap()[&reference.session_id]
            .path
            .clone();
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o077,
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn platform_profile_symlinks_are_rejected_without_touching_the_target() {
        use std::os::unix::fs::symlink;

        let service = ConnectorSessionService::default();
        let root = tempfile::tempdir().unwrap();
        let profiles = root.path().join("profiles");
        let outside = root.path().join("outside");
        std::fs::create_dir(&profiles).unwrap();
        std::fs::create_dir(&outside).unwrap();
        symlink(&outside, profiles.join("douyin")).unwrap();

        assert!(service.create("douyin", &profiles).is_err());
        assert!(outside.is_dir());
    }
}
