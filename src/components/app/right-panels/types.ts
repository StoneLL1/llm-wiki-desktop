import type { RightPanelMode } from "../../../stores/navigationStore";
import type { ProjectSummary } from "../../../types/project";

export interface RightPanelHostProps {
  currentProject: ProjectSummary;
  rightPanelMode: RightPanelMode;
}
