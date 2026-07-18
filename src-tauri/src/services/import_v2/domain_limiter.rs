use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

#[derive(Default)]
pub struct DomainLimiter {
    limits: Mutex<HashMap<String, Arc<Semaphore>>>,
}
impl DomainLimiter {
    pub async fn acquire(&self, host: &str, sensitive: bool) -> Result<OwnedSemaphorePermit, ()> {
        let limit = if sensitive { 1 } else { 2 };
        let semaphore = self
            .limits
            .lock()
            .map_err(|_| ())?
            .entry(host.to_ascii_lowercase())
            .or_insert_with(|| Arc::new(Semaphore::new(limit)))
            .clone();
        semaphore.acquire_owned().await.map_err(|_| ())
    }
}
