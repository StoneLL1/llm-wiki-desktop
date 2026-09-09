//! Editable form defaults are metadata, not permission to execute a workflow.
use super::{
    preparation::{route_selection, wiki_pages_from_inventory},
    project_identity, WorkflowService,
};
use crate::errors::BackendError;
use crate::models::layout::ProjectMarkdownRootRole;
use crate::models::paths::ProjectContext;
use crate::models::workflow::{
    WorkflowKind, WorkflowPersistenceMode, WorkflowRouteSelection, WorkflowScope,
};
use crate::services::{AgentService, SettingsService};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowFormCatalog {
    pub kind: WorkflowKind,
    pub routes: Vec<WorkflowRouteSelection>,
    pub default_route: Option<WorkflowRouteSelection>,
    pub wiki_pages: Vec<String>,
    pub remembered_draft: Option<WorkflowRememberedDraft>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRememberedDraft {
    pub scope: WorkflowScope,
    pub route_selection: Option<WorkflowRouteSelection>,
}

impl WorkflowService {
    /// No AgentService instance or SecretService is accepted: catalog reads can
    /// neither attest an executable nor inspect credentials or issue a receipt.
    pub fn form_catalog(
        &self,
        context: &ProjectContext,
        settings: &SettingsService,
        kind: WorkflowKind,
    ) -> Result<WorkflowFormCatalog, BackendError> {
        if kind == WorkflowKind::UpdateWiki {
            return Err(BackendError::new(
                "WORKFLOW_FORM_KIND_INVALID",
                "Update Wiki uses its own options and paged Source catalog.",
                true,
                false,
            ));
        }
        let mut options = self.update_wiki_options(context, settings)?;
        if kind == WorkflowKind::HealthCheck {
            options.routes.retain(|route| match route {
                WorkflowRouteSelection::Agent { agent } => {
                    AgentService::lint_route_profile_revision(*agent).is_some()
                }
                WorkflowRouteSelection::Byok { .. } => true,
            });
            // Complete Health requires an explicit provider selection. A sole
            // configured provider must not turn local Health into remote work.
            options.default_route = options.default_route.filter(|route| {
                matches!(route, WorkflowRouteSelection::Agent { .. })
                    && options.routes.contains(route)
            });
        }
        let wiki_pages = if kind == WorkflowKind::GenerateContent {
            wiki_page_catalog(context)?
        } else {
            Vec::new()
        };
        let remembered_draft = project_identity(&context.root).ok().and_then(|identity| {
            let persistence = if context.layout.workflow_state_root.is_some() {
                WorkflowPersistenceMode::Persistent
            } else {
                WorkflowPersistenceMode::MemoryOnly
            };
            self.preferences
                .load(
                    context,
                    &identity.canonical_identity_key,
                    &identity.identity_revision,
                    &persistence,
                )
                .ok()?
                .into_iter()
                .find(|entry| entry.kind == kind)
                .map(|entry| {
                    let mut scope = entry.scope;
                    // Persisted preparations contain the resolved filename,
                    // including automatic destinations. A new operation must
                    // explicitly choose any overwrite target again.
                    if let WorkflowScope::GenerateContent { output_path, .. } = &mut scope {
                        *output_path = None;
                    }
                    WorkflowRememberedDraft {
                        route_selection: route_selection(&entry.route),
                        scope,
                    }
                })
        });
        Ok(WorkflowFormCatalog {
            kind,
            routes: options.routes,
            default_route: options.default_route,
            wiki_pages,
            remembered_draft,
        })
    }
}

fn wiki_page_catalog(context: &ProjectContext) -> Result<Vec<String>, BackendError> {
    // Native Source pages live below the Wiki root. Exclude those subtrees
    // before walking, so opening Generate never enumerates the Source archive.
    let mut layout = context.layout.clone();
    let source_roots = layout
        .markdown_roots
        .iter()
        .filter(|root| {
            root.role == ProjectMarkdownRootRole::Source
                && root.path != "."
                && root.exclude.as_ref().is_none_or(Vec::is_empty)
        })
        .map(|root| root.path.clone())
        .collect::<Vec<_>>();
    for root in &mut layout.markdown_roots {
        if matches!(
            root.role,
            ProjectMarkdownRootRole::Wiki | ProjectMarkdownRootRole::Mixed
        ) {
            root.exclude
                .get_or_insert_with(Vec::new)
                .extend(source_roots.iter().cloned());
        }
    }
    let pages = layout
        .list_markdown_files(
            &context.root,
            &[
                ProjectMarkdownRootRole::Wiki,
                ProjectMarkdownRootRole::Mixed,
            ],
        )?
        .into_iter()
        .map(|path| context.to_project_relative(&path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(wiki_pages_from_inventory(context, &pages))
}

#[cfg(test)]
mod tests;
