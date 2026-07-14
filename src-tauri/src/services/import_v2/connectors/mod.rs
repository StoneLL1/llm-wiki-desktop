pub mod wechat;
pub mod zhihu;
pub mod bilibili;
pub mod xiaohongshu;
pub mod x;
use serde::Serialize;
#[derive(Clone,Serialize)]#[serde(rename_all="camelCase")]pub struct ConnectorDocument{pub title:String,pub author:Option<String>,pub published_at:Option<String>,pub body_html:String,pub public_url:String,#[serde(skip)]pub image_requests:Vec<ImageRequest>}
impl std::fmt::Debug for ConnectorDocument{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.debug_struct("ConnectorDocument").field("title",&self.title).field("public_url",&self.public_url).field("image_request_count",&self.image_requests.len()).finish()}}
#[derive(Clone)]pub struct ImageRequest{pub request_url:String,pub public_url:String}
#[derive(Debug,Clone,PartialEq,Eq)]pub enum ConnectorFailure{Challenge,Captcha,LoginRequired,Removed,EmptyBody,StructureChanged}
pub(crate) fn between<'a>(s:&'a str,start:&str,end:&str)->Option<&'a str>{let i=s.find(start)?+start.len();let tail=&s[i..];Some(&tail[..tail.find(end)?])}
