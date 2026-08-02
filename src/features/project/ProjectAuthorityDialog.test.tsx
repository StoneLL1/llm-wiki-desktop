import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { defaultProject, useProjectStore } from "../../stores/projectStore";
import type { ProjectOpenAssessment } from "../../types/project";
import { ProjectAuthorityDialog } from "./ProjectAuthorityDialog";

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

const baseAssessment: ProjectOpenAssessment = {
  assessmentId: "assessment-a",
  canonicalRootPath: "D:/知识库/project-a",
  canonicalIdentityKey: "identity-a",
  identityRevision: "revision-a",
  format: "obsidian_vault",
  trust: "untrusted",
  filesystemAccess: "read_only",
  health: "healthy",
  layout: { markdownRoots: [{ path: ".", role: "mixed" }] },
  confidence: "high",
  markers: [],
  capabilities: ["read_markdown", "enable_compatible_features"],
  warnings: [],
  layoutWarnings: [],
  git: { isRepository: false, branch: null, head: null, hasChanges: false },
};

function queueAssessment(assessment: ProjectOpenAssessment, operationId: string): void {
  invokeMock
    .mockResolvedValueOnce({ assessmentOperationId: operationId })
    .mockResolvedValueOnce({ assessmentOperationId: operationId, status: "completed", assessment });
}

beforeEach(async () => {
  invokeMock.mockReset();
  Object.defineProperty(window, "__TAURI_INTERNALS__", { value: {}, configurable: true });
  await i18next.changeLanguage("en");
  useProjectStore.setState({
    currentProject: {
      ...defaultProject,
      projectId: "project-a",
      name: "Project A",
      rootPath: "D:/知识库/project-a",
    },
    pendingAction: undefined,
    assessmentOperationId: null,
    assessment: null,
    assessing: false,
    assessmentError: null,
  });
  useProjectStore.getState().setPendingAction(undefined);
});

describe("ProjectAuthorityDialog", () => {
  it("closes stale workflow authority context without assessing or re-preparing another project", async () => {
    useProjectStore.setState({
      currentProject: {
        ...useProjectStore.getState().currentProject,
        projectId: "project-b",
        rootPath: "D:/知识库/project-b",
      },
    });
    const onClose = vi.fn();
    const prepareAgain = vi.fn();

    render(
      <ProjectAuthorityDialog
        action="trust_project"
        project={{ projectId: "project-a", rootPath: "D:/知识库/project-a" }}
        onClose={onClose}
        onSatisfied={prepareAgain}
      />,
    );

    await waitFor(() => expect(onClose).toHaveBeenCalledTimes(1));
    expect(prepareAgain).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("re-prepares and closes when a fresh assessment already satisfies the prerequisite", async () => {
    queueAssessment(
      { ...baseAssessment, trust: "trusted", filesystemAccess: "writable", git: { isRepository: true, branch: "main", head: "abc", hasChanges: false } },
      "operation-a",
    );
    const onSatisfied = vi.fn();
    const onClose = vi.fn();

    render(
      <ProjectAuthorityDialog
        action="configure_git"
        project={useProjectStore.getState().currentProject}
        onClose={onClose}
        onSatisfied={onSatisfied}
      />,
    );

    await waitFor(() => expect(onSatisfied).toHaveBeenCalledTimes(1));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("offers explicit Git completion for an existing repository without HEAD", async () => {
    queueAssessment(
      {
        ...baseAssessment,
        trust: "trusted",
        filesystemAccess: "writable",
        git: { isRepository: true, branch: null, head: null, hasChanges: true },
      },
      "operation-unborn",
    );
    invokeMock.mockResolvedValueOnce({
      id: "initialize-unborn",
      actionType: "initialize_git_repository",
      title: "Initialize Git",
      message: "Initialize Git",
      riskLevel: "high",
      affectedPaths: [".git", "note.md"],
      preview: null,
      expiresAt: null,
    });

    render(
      <ProjectAuthorityDialog
        action="configure_git"
        project={useProjectStore.getState().currentProject}
        onClose={vi.fn()}
        onSatisfied={vi.fn()}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Initialize Git history" }));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.find(([command]) => command === "initialize_git_repository")?.[1],
      ).toMatchObject({
        request: {
          assessmentId: "assessment-a",
          projectId: "project-a",
        },
      }),
    );
  });

  it("keeps the manage surface open after a confirmed trust change", async () => {
    queueAssessment(baseAssessment, "operation-a");
    invokeMock.mockResolvedValueOnce({
      id: "action-a",
      actionType: "trust_compatible_project",
      title: "Trust",
      message: "Trust",
      riskLevel: "high",
      affectedPaths: [],
      preview: null,
      expiresAt: null,
    });
    const onClose = vi.fn();
    render(
      <ProjectAuthorityDialog
        action="manage"
        project={useProjectStore.getState().currentProject}
        onClose={onClose}
        onSatisfied={vi.fn()}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Trust this knowledge base" }));
    await waitFor(() => expect(useProjectStore.getState().pendingAction?.id).toBe("action-a"));

    invokeMock.mockResolvedValueOnce(undefined);
    queueAssessment({ ...baseAssessment, trust: "trusted", filesystemAccess: "writable" }, "operation-b");
    useProjectStore.setState({ pendingAction: undefined });

    expect(await screen.findByText("Trusted")).toBeInTheDocument();
    expect(screen.queryByText(/did not change/i)).not.toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });
});
