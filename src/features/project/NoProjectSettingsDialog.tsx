import { SettingsDialog } from "../settings/SettingsDialog";

const unavailable = () => Promise.reject(new Error("Open a knowledge base to configure AI."));
const noop = () => undefined;

/** The same application preferences remain available before a project opens. */
export function NoProjectSettingsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  return <SettingsDialog open={open} onClose={onClose} project={null} providers={[]} agents={[]} onRefreshCapabilities={noop} onSaveProvider={unavailable} onSaveSecret={unavailable} onDeleteSecret={unavailable} onTestProvider={unavailable} />;
}
