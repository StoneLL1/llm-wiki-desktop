use serde::{Deserialize, Serialize};

use super::{agent::AgentKind, llm::LlmProviderKind};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompileRoutePreference {
    Auto,
    Agent,
    Byok,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompileRoute {
    Agent,
    Byok,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompileConflictResolution {
    KeepCurrent,
    UseGenerated,
    ManualMerge,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompileFile {
    pub path: String,
    pub content: String,
}

impl CompileFile {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompileManifest {
    pub files: Vec<CompileFile>,
    #[serde(default)]
    pub deletions: Vec<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileRequest {
    pub project_id: String,
    pub project_root_path: String,
    #[serde(default = "default_route")]
    pub route: CompileRoutePreference,
    pub agent: Option<AgentKind>,
    pub provider: Option<LlmProviderKind>,
}

fn default_route() -> CompileRoutePreference {
    CompileRoutePreference::Auto
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    pub route: CompileRoute,
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<String>,
    pub checkpoint: Option<String>,
}
