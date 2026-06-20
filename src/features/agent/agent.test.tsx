import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import "../../i18n";
import { AgentView } from "./AgentView";
import type { AgentInfo } from "../../types/agent";

describe("AgentView", () => {
  it("shows installed, missing, failed and default states with install guidance as text", () => {
    const agents: AgentInfo[] = [
      { kind: "claude", command: "claude", state: "installed", version: "2.1.133", executablePath: "C:/claude.exe", isDefault: true, installGuidance: "npm install claude", error: null },
      { kind: "codex", command: "codex", state: "missing", version: null, executablePath: null, isDefault: false, installGuidance: "npm install codex", error: null },
      { kind: "openclaw", command: "openclaw", state: "failed", version: null, executablePath: "C:/openclaw.exe", isDefault: false, installGuidance: "Read docs", error: "timed out" },
    ];
    render(<AgentView agents={agents} providerCount={1} onDetect={() => undefined} onCompile={() => undefined} />);
    expect(screen.getByText("2.1.133")).toBeInTheDocument();
    expect(screen.getByText("npm install codex")).toBeInTheDocument();
    expect(screen.getByText("timed out")).toBeInTheDocument();
    expect(screen.getByText(/default/i)).toBeInTheDocument();
  });
});
