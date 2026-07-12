use crate::{errors::BackendError, models::import_v2::{ImportInput,ImportInputKind}, services::import_v2::engine::{EngineDescriptor,EngineRequest,EngineResult,ImportEngine}, tasks::task_model::CancellationToken};

pub struct GenericWebEngine;
impl ImportEngine for GenericWebEngine {
 fn descriptor(&self)->EngineDescriptor{EngineDescriptor{engine_id:"browser-runtime-lite".into(),engine_version:"0.1.0".into(),route:"web.generic.readability".into()}}
 fn supports(&self,input:&ImportInput)->bool{input.kind==ImportInputKind::Url}
 fn execute(&self,_:&EngineRequest,_:&CancellationToken)->Result<EngineResult,BackendError>{Err(BackendError::new(crate::errors::IMPORT_V2_ENGINE_UNAVAILABLE,"The signed browser-runtime-lite capability is required.",true,true))}
}
