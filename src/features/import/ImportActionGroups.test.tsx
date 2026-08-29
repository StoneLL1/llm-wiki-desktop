import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportSessionActionGroup } from "../../types/importV2";
import {
  ImportActionGroups,
  type ImportActionGroupKind,
} from "./ImportActionGroups";

const groups: ImportSessionActionGroup[] = [
  { groupKey: "login:connector", kind: "login", subjectId: "connector", itemCount: 2, itemIds: ["login-a", "login-b"] },
  { groupKey: "ocr:all", kind: "ocr", subjectId: null, itemCount: 1, itemIds: ["ocr-a"] },
  { groupKey: "asr:all", kind: "asr", subjectId: null, itemCount: 1, itemIds: ["asr-a"] },
  { groupKey: "capability:document-standard", kind: "capability", subjectId: "document-standard", itemCount: 1, itemIds: ["capability-a"] },
  { groupKey: "resume:all", kind: "resume", subjectId: null, itemCount: 1, itemIds: ["paused-a"] },
];

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportActionGroups", () => {
  it("renders backend-aggregated login, OCR, ASR, capability, and resume facts", () => {
    expect(groups.map((group) => group.kind)).toEqual(
      expect.arrayContaining<ImportActionGroupKind>(["login", "ocr", "asr", "capability", "resume"]),
    );
  });

  it("runs one action for an entire group instead of opening one modal per item", () => {
    const onRun = vi.fn();
    render(<ImportActionGroups groups={groups} onRun={onRun} />);

    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(onRun).toHaveBeenCalledWith(expect.objectContaining({
      kind: "login",
      itemIds: ["login-a", "login-b"],
    }));
    expect(screen.getAllByRole("button")).toHaveLength(5);
  });
});
