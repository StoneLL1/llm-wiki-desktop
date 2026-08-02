import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { i18next } from "../../i18n";
import type { ProjectOpenAssessment } from "../../types/project";
import { ProjectAssessmentPanel } from "./ProjectAssessmentPanel";

const assessment: ProjectOpenAssessment = {
  assessmentId: "assessment-a",
  canonicalRootPath: "D:/知识库/兼容库",
  canonicalIdentityKey: "identity-a",
  identityRevision: "revision-a",
  format: "obsidian_vault",
  trust: "untrusted",
  filesystemAccess: "read_only",
  health: "recovery",
  layout: { markdownRoots: [{ path: ".", role: "mixed" }] },
  confidence: "high",
  markers: [{ kind: "obsidian", path: ".obsidian" }],
  capabilities: ["read_markdown", "local_search"],
  warnings: [{ code: "PROJECT_APP_STATE_RECOVERY", message: "fallback" }],
  layoutWarnings: [],
  git: { isRepository: true, branch: "main", head: "abc", hasChanges: true },
};

afterEach(() => cleanup());

describe("ProjectAssessmentPanel", () => {
  it("renders all six independent assessment dimensions in English", async () => {
    await i18next.changeLanguage("en");
    render(<ProjectAssessmentPanel assessment={assessment} onBack={() => undefined} />);

    for (const label of ["Type", "Trust", "Filesystem", "Health", "Markdown layout", "Capabilities", "Git"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("Obsidian Vault")).toBeInTheDocument();
    expect(screen.getByText("Not trusted")).toBeInTheDocument();
    expect(screen.getByText("Read-only")).toBeInTheDocument();
    expect(screen.getByText("Recovery mode")).toBeInTheDocument();
  });

  it("has the matching Chinese authority copy", async () => {
    await i18next.changeLanguage("zh-CN");
    render(<ProjectAssessmentPanel assessment={assessment} onBack={() => undefined} />);

    expect(screen.getByText("文件夹评估")).toBeInTheDocument();
    expect(screen.getByText("尚未信任")).toBeInTheDocument();
    expect(screen.getByText("只读")).toBeInTheDocument();
    expect(screen.getByText("恢复模式")).toBeInTheDocument();
  });
});
