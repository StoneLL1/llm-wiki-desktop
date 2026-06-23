import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import "../../i18n";
import { AgentView } from "./AgentView";
import { AgentRightPanel } from "./AgentRightPanel";
import type { AgentInfo } from "../../types/agent";

describe("AgentView", () => {
  it("shows installed, missing, failed and default states with install guidance as text", () => {
    const agents: AgentInfo[] = [
      { kind: "claude", command: "claude", state: "installed", version: "2.1.133", executablePath: "C:/claude.exe", isDefault: true, installGuidance: "npm install claude", error: null },
      { kind: "codex", command: "codex", state: "missing", version: null, executablePath: null, isDefault: false, installGuidance: "npm install codex", error: null },
      { kind: "openclaw", command: "openclaw", state: "failed", version: null, executablePath: "C:/openclaw.exe", isDefault: false, installGuidance: "Read docs", error: "timed out" },
    ];
    render(
      <AgentView
        agents={agents}
        providers={[]}
        tasks={[]}
        onDetect={() => undefined}
        onRunAgent={() => undefined}
      />,
    );
    expect(screen.getByText(/2\.1\.133/)).toBeInTheDocument();
    expect(screen.getByText("npm install codex")).toBeInTheDocument();
    expect(screen.getByText("timed out")).toBeInTheDocument();
    expect(screen.getByText(/default/i)).toBeInTheDocument();
  });

  it("renders the four-grid core operations with Ingest as the primary card", () => {
    const onRunAgent = vi.fn();
    render(
      <AgentView
        agents={[]}
        providers={[]}
        tasks={[]}
        onDetect={() => undefined}
        onRunAgent={onRunAgent}
      />,
    );
    const ingestCard = screen.getByRole("button", { name: /ingest/i });
    expect(ingestCard.className).toContain("is-primary");
    ingestCard.click();
    expect(onRunAgent).toHaveBeenCalledWith("wiki-ingest");
  });

  it("navigates via non-primary cards instead of opening the run dialog", () => {
    const onRunAgent = vi.fn();
    const onNavigate = vi.fn();
    render(
      <AgentView
        agents={[]}
        providers={[]}
        tasks={[]}
        onDetect={() => undefined}
        onRunAgent={onRunAgent}
        onNavigate={onNavigate}
      />,
    );
    screen.getByRole("button", { name: /lint/i }).click();
    expect(onNavigate).toHaveBeenCalledWith("lint");
    expect(onRunAgent).not.toHaveBeenCalled();
  });

  it("does not present an installed fallback as the configured default agent", () => {
    const agents: AgentInfo[] = [
      {
        kind: "claude",
        command: "claude",
        state: "installed",
        version: "2.1.133",
        executablePath: "C:/claude.cmd",
        isDefault: false,
        installGuidance: "",
        error: null,
      },
    ];

    render(<AgentRightPanel agents={agents} />);

    expect(screen.getByText("not configured")).toBeInTheDocument();
    expect(screen.queryByText("claude · v2.1.133")).not.toBeInTheDocument();
  });

  it("keeps a failed configured agent visible as the default", () => {
    const agents: AgentInfo[] = [
      {
        kind: "openclaw",
        command: "openclaw",
        state: "failed",
        version: null,
        executablePath: "C:/openclaw.exe",
        isDefault: true,
        installGuidance: "",
        error: "protocol check failed",
      },
    ];

    render(<AgentRightPanel agents={agents} />);

    expect(screen.getByText("openclaw · v?")).toBeInTheDocument();
    expect(screen.queryByText("not configured")).not.toBeInTheDocument();
  });
});
