pub mod wechat;
pub mod zhihu;
pub mod bilibili;
use serde::Serialize;
#[derive(Debug,Clone,Serialize)]#[serde(rename_all="camelCase")]pub struct ConnectorDocument{pub title:String,pub author:Option<String>,pub published_at:Option<String>,pub body_html:String,pub public_url:String,#[serde(skip)]pub image_requests:Vec<ImageRequest>}
#[derive(Debug,Clone)]pub struct ImageRequest{pub request_url:String,pub public_url:String}
#[derive(Debug,Clone,PartialEq,Eq)]pub enum ConnectorFailure{Challenge,Captcha,LoginRequired,Removed,EmptyBody,StructureChanged}
pub(crate) fn between<'a>(s:&'a str,start:&str,end:&str)->Option<&'a str>{let i=s.find(start)?+start.len();let tail=&s[i..];Some(&tail[..tail.find(end)?])}
