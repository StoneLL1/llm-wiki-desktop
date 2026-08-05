export type RiskLevel = "low" | "medium" | "high" | "destructive";

export type PendingActionType =
  | "repair_project"
  | "delete_file"
  | "overwrite_file"
  | "batch_rewrite"
  | "merge_conflict"
  | "agent_auto_fix"
  | "install_agent"
  | "run_skill"
  | "enable_compatible_project"
  | "configure_compatible_layout"
  | "trust_compatible_project"
  | "initialize_git_repository"
  | "create_git_checkpoint";

export interface ActionPreview {
  summary: string;
  before: string | null;
  after: string | null;
  diff: string | null;
}

export interface PendingAction {
  id: string;
  actionType: PendingActionType;
  title: string;
  message: string;
  riskLevel: RiskLevel;
  affectedPaths: string[];
  preview: ActionPreview | null;
  expiresAt: string | null;
  checkpointHash?: string | null;
}

export type ConfirmationStatus = "confirmed" | "cancelled";

export interface ConfirmedAction {
  action: PendingAction;
  status: ConfirmationStatus;
  checkpointExists: boolean;
  projectSummary: ProjectSummary | null;
}
import type { ProjectSummary } from "./project";
