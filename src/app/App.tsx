import { AppShell } from "../components/app/AppShell";
import { useTaskEvents } from "../hooks/useTaskEvents";
import "../i18n";

export function App() {
  useTaskEvents();
  return <AppShell />;
}
