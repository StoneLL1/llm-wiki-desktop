import { PanelRightClose } from "lucide-react";
import { useTranslation } from "react-i18next";

import { useNavigationStore } from "../../stores/navigationStore";

interface RightPanelHeaderProps {
  title: string;
  panelId?: string;
}

export function RightPanelHeader({
  title,
  panelId = "right-context-panel",
}: RightPanelHeaderProps) {
  const { t } = useTranslation();
  const setRightPanelOpen = useNavigationStore((state) => state.setRightPanelOpen);

  return (
    <div className="right-panel__header">
      <span className="right-panel__title">{title}</span>
      <button
        aria-controls={panelId}
        aria-expanded="true"
        aria-label={t("shell.contextPanel.collapse")}
        className="icon-button shrink-0"
        onClick={() => setRightPanelOpen(false)}
        title={t("shell.contextPanel.collapse")}
        type="button"
      >
        <PanelRightClose aria-hidden="true" size={16} />
      </button>
    </div>
  );
}
