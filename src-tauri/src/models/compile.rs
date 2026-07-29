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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompilePlan {
    pub summary: String,
    pub items: Vec<CompilePlanItem>,
    #[serde(default)]
    pub global_risk_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompilePlanItem {
    pub action: CompileAction,
    pub target_path: String,
    pub page_type: CompilePageType,
    #[serde(default)]
    pub source_ids: Vec<String>,
    #[serde(default)]
    pub affected_existing_pages: Vec<String>,
    pub reason: String,
    #[serde(default)]
    pub risk_flags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompileAction {
    Create,
    Update,
    Merge,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CompilePageType {
    Entity,
    Concept,
    Synthesis,
    Comparison,
    Query,
    Overview,
    Index,
    Log,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceVersionRef {
    pub source_id: String,
    pub version_id: String,
    pub content_hash: String,
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
    /// Explicit, hash-bound Source versions selected by the user. Paths are
    /// deliberately absent and are resolved only from the trusted registry.
    pub source_versions: Vec<SourceVersionRef>,
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
    pub consumed_versions: Vec<SourceVersionRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileConsumptionRecord {
    pub schema_version: u32,
    pub compile_task_id: String,
    pub route: CompileRoute,
    pub consumed_at: String,
    pub source_versions: Vec<SourceVersionRef>,
    pub affected_paths: Vec<String>,
    pub checkpoint: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{CompileAction, CompilePageType, CompilePlan, CompilePlanItem};

    #[test]
    fn compile_plan_serde_uses_camel_case_and_snake_case_enums() {
        let plan = CompilePlan {
            summary: "Create one synthesis and update navigation.".into(),
            items: vec![CompilePlanItem {
                action: CompileAction::Create,
                target_path: "wiki/synthesis/context-engineering.md".into(),
                page_type: CompilePageType::Synthesis,
                source_ids: vec!["wiki/sources/source-a.md".into(), "source-b.md".into()],
                affected_existing_pages: vec!["wiki/index.md".into()],
                reason: "New cross-source theme requires a derived synthesis page.".into(),
                risk_flags: vec!["new_concept".into()],
            }],
            global_risk_flags: vec!["cascade_required".into()],
        };

        let json = serde_json::to_string(&plan).unwrap();

        assert!(json.contains(r#""targetPath":"wiki/synthesis/context-engineering.md""#));
        assert!(json.contains(r#""pageType":"synthesis""#));
        assert!(json.contains(r#""sourceIds":["wiki/sources/source-a.md","source-b.md"]"#));
        assert!(json.contains(r#""affectedExistingPages":["wiki/index.md"]"#));
        assert!(json.contains(r#""globalRiskFlags":["cascade_required"]"#));
        let round_trip: CompilePlan = serde_json::from_str(&json).unwrap();
        assert_eq!(round_trip, plan);
    }
}
