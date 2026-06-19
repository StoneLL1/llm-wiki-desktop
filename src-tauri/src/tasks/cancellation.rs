use crate::tasks::task_model::CancellationToken;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Default)]
pub struct CancellationRegistry {
    tokens: RwLock<HashMap<String, CancellationToken>>,
}

impl CancellationRegistry {
    pub fn new() -> Self {
        Self {
            tokens: RwLock::new(HashMap::new()),
        }
    }

    pub fn register(&self, task_id: &str) -> CancellationToken {
        let token = CancellationToken::new();
        let mut tokens = self.tokens.write().expect("lock poisoned");
        tokens.insert(task_id.to_string(), token.clone());
        token
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let tokens = self.tokens.read().expect("lock poisoned");
        if let Some(token) = tokens.get(task_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    pub fn is_cancelled(&self, task_id: &str) -> bool {
        let tokens = self.tokens.read().expect("lock poisoned");
        tokens
            .get(task_id)
            .map(|t| t.is_cancelled())
            .unwrap_or(false)
    }

    pub fn remove(&self, task_id: &str) {
        let mut tokens = self.tokens.write().expect("lock poisoned");
        tokens.remove(task_id);
    }

    pub fn get(&self, task_id: &str) -> Option<CancellationToken> {
        let tokens = self.tokens.read().expect("lock poisoned");
        tokens.get(task_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_cancel() {
        let registry = CancellationRegistry::new();
        let token = registry.register("task-1");
        assert!(!token.is_cancelled());
        assert!(!registry.is_cancelled("task-1"));

        let cancelled = registry.cancel("task-1");
        assert!(cancelled);
        assert!(token.is_cancelled());
        assert!(registry.is_cancelled("task-1"));
    }

    #[test]
    fn test_cancel_nonexistent() {
        let registry = CancellationRegistry::new();
        assert!(!registry.cancel("nonexistent"));
        assert!(!registry.is_cancelled("nonexistent"));
    }

    #[test]
    fn test_remove_token() {
        let registry = CancellationRegistry::new();
        registry.register("task-1");
        registry.remove("task-1");
        assert!(!registry.is_cancelled("task-1"));
        assert!(registry.get("task-1").is_none());
    }

    #[test]
    fn test_cancellation_token_is_cloneable_and_shared() {
        let registry = CancellationRegistry::new();
        let token1 = registry.register("task-shared");
        let token2 = registry.get("task-shared").unwrap();

        token1.cancel();
        assert!(token2.is_cancelled());
    }
}
