import {
  Bot,
  BookOpenText,
  FileOutput,
  LayoutDashboard,
  MessageSquare,
  Network,
  ShieldCheck,
  Upload,
  type LucideIcon,
} from "lucide-react";
import type { AppView } from "../../stores/navigationStore";

export interface NavigationItem {
  view: AppView;
  labelKey: string;
  icon: LucideIcon;
}

export const mainViews: NavigationItem[] = [
  { view: "dashboard", labelKey: "nav.dashboard", icon: LayoutDashboard },
  { view: "wiki", labelKey: "nav.wiki", icon: BookOpenText },
  { view: "chat", labelKey: "nav.chat", icon: MessageSquare },
  { view: "graph", labelKey: "nav.graph", icon: Network },
];

export const workflowViews: NavigationItem[] = [
  { view: "agent", labelKey: "nav.agent", icon: Bot },
  { view: "import", labelKey: "nav.import", icon: Upload },
  { view: "lint", labelKey: "nav.lint", icon: ShieldCheck },
  { view: "exports", labelKey: "nav.exports", icon: FileOutput },
];
