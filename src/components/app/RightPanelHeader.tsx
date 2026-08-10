import { PanelRightClose } from "lucide-react";
import { createContext, useContext } from "react";
import { useTranslation } from "react-i18next";

import { useNavigationStore } from "../../stores/navigationStore";

interface RightPanelHeaderProps {
  title: string;
  panelId?: string;
}

export const RightPanelModalContext = createContext(false);

export function RightPanelHeader({
  title,
  panelId = "right-context-panel",
}: RightPanelHeaderProps) {
  const { t } = useTranslation();
  const modal = useContext(RightPanelModalContext);
  const setRightPanelOpen = useNavigationStore((state) => state.setRightPanelOpen);
  const closeLabel = t(modal ? "shell.contextPanel.close" : "shell.contextPanel.collapse");

  return (
    <div className="right-panel__header">
      <span className="right-panel__title" id={`${panelId}-title`}>{title}</span>
      <button
        aria-controls={panelId}
        aria-expanded="true"
        aria-label={closeLabel}
        className="icon-button shrink-0"
        onClick={() => setRightPanelOpen(false)}
        title={closeLabel}
        type="button"
      >
        <PanelRightClose aria-hidden="true" size={16} />
      </button>
    </div>
  );
}
