use std::{collections::HashMap,path::{Path,PathBuf},sync::Mutex};
use serde::{Deserialize,Serialize};
use crate::{errors::BackendError,services::import_v2::url_policy::PrivateTargetGrant};
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]#[serde(rename_all="camelCase")]pub struct ConnectorSessionRef{pub session_id:String,pub platform:String,pub profile_ref:String,pub state:String}
#[derive(Default)]pub struct ConnectorSessionService{sessions:Mutex<HashMap<String,(ConnectorSessionRef,PathBuf)>>,grants:Mutex<HashMap<String,PrivateTargetGrant>>}
impl ConnectorSessionService{
 pub fn create(&self,platform:&str,profiles_root:&Path)->Result<ConnectorSessionRef,BackendError>{if !matches!(platform,"wechat"|"zhihu"|"bilibili"|"xiaohongshu"|"x"){return Err(e("Unsupported connector platform."));}reject_daily_profile(profiles_root)?;let id=uuid::Uuid::new_v4().to_string();let path=profiles_root.join(&id);std::fs::create_dir_all(&path).map_err(|_|e("Dedicated browser profile could not be created."))?;let r=ConnectorSessionRef{session_id:id.clone(),platform:platform.into(),profile_ref:format!("connector-profile:{id}"),state:"waiting_login".into()};self.sessions.lock().map_err(|_|e("Connector sessions are unavailable."))?.insert(id,(r.clone(),path));Ok(r)}
 pub fn resume(&self,id:&str)->Result<ConnectorSessionRef,BackendError>{let mut s=self.sessions.lock().map_err(|_|e("Connector sessions are unavailable."))?;let (r,_)=s.get_mut(id).ok_or_else(||e("Connector session was not found."))?;r.state="authenticated".into();Ok(r.clone())}
 pub fn revoke(&self,id:&str)->Result<(),BackendError>{if let Some((_,path))=self.sessions.lock().map_err(|_|e("Connector sessions are unavailable."))?.remove(id){std::fs::remove_dir_all(path).map_err(|_|e("Connector profile could not be removed."))?;}Ok(())}
 pub fn authorize_private(&self,grant:PrivateTargetGrant)->Result<String,BackendError>{let id=format!("private-grant:{}",uuid::Uuid::new_v4());self.grants.lock().map_err(|_|e("Private grants are unavailable."))?.insert(id.clone(),grant);Ok(id)}
 pub fn take_private(&self,id:&str)->Result<Option<PrivateTargetGrant>,BackendError>{Ok(self.grants.lock().map_err(|_|e("Private grants are unavailable."))?.remove(id))}
}
fn reject_daily_profile(path:&Path)->Result<(),BackendError>{let p=path.to_string_lossy().to_ascii_lowercase();if ["google/chrome","microsoft/edge","mozilla/firefox","user data"].iter().any(|x|p.contains(x)){return Err(e("Daily browser profiles are forbidden."));}Ok(())}
fn e(m:&str)->BackendError{BackendError::new("IMPORT_V2_BROWSER_SESSION_FAILED",m,true,true)}
