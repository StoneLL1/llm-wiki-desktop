import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ImportItem, ImportRecoveryAction } from "../../types/importV2";
import {
  buildImportActionGroups,
  ImportActionGroups,
  type ImportActionGroupKind,
} from "./ImportActionGroups";

function blocked(
  itemId: string,
  status: ImportItem["status"],
  recoveryAction?: ImportRecoveryAction,
  locator = `C:\\sources\\${itemId}.md`,
): ImportItem {
  return {
    itemId,
    input: {
      kind: locator.startsWith("http") ? "url" : "file",
      displayName: `${itemId}.md`,
      locator,
      normalizedLocator: null,
    },
    status,
    selected: false,
    taskId: null,
    progress: null,
    attempts: [],
    preview: null,
    issue: recoveryAction ? {
      code: `BLOCKED_${itemId}`,
      message: "blocked",
      stage: "extract",
      retryable: true,
      userActionRequired: true,
      recoveryActions: [recoveryAction],
      availableActions: [],
    } : null,
  };
}

const items = [
  blocked("login-a", "waiting_login", "begin_login"),
  blocked("login-b", "waiting_login", "begin_login"),
  blocked("ocr-a", "waiting_authorization", "enable_ocr"),
  blocked("asr-a", "waiting_authorization", "authorize_local_asr"),
  blocked("capability-a", "waiting_capability", "install_capability"),
  blocked("paused-a", "paused"),
];

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportActionGroups", () => {
  it("groups item blockers by login, OCR, ASR, capability, and resume", () => {
    const groups = buildImportActionGroups(items);
    const byGroup = Object.fromEntries(groups.map((group) => [group.groupKey, group.itemIds]));

    expect(byGroup).toEqual({
      "login:connector": ["login-a", "login-b"],
      "ocr:all": ["ocr-a"],
      "asr:all": ["asr-a"],
      "capability:document-standard": ["capability-a"],
      "resume:all": ["paused-a"],
    });
    expect(groups.map((group) => group.kind)).toEqual(
      expect.arrayContaining<ImportActionGroupKind>(["login", "ocr", "asr", "capability", "resume"]),
    );
  });

  it("keeps unrelated login platforms and capability packs in separate groups", () => {
    const groups = buildImportActionGroups([
      blocked("wechat", "waiting_login", "begin_login", "https://mp.weixin.qq.com/s/article"),
      blocked("zhihu", "waiting_login", "begin_login", "https://www.zhihu.com/question/1"),
      blocked("browser", "waiting_capability", "install_browser_capability"),
      blocked("ocr", "waiting_capability", "install_ocr_capability"),
    ]);
    const byGroup = Object.fromEntries(groups.map((group) => [group.groupKey, group.itemIds]));

    expect(byGroup).toEqual({
      "login:wechat": ["wechat"],
      "login:zhihu": ["zhihu"],
      "capability:browser-runtime": ["browser"],
      "capability:ocr-cjk-accurate": ["ocr"],
    });
  });

  it("runs one action for an entire group instead of opening one modal per item", () => {
    const onRun = vi.fn();
    render(<ImportActionGroups items={items} onRun={onRun} />);

    fireEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(onRun).toHaveBeenCalledWith(expect.objectContaining({
      kind: "login",
      itemIds: ["login-a", "login-b"],
    }));
    expect(screen.getAllByRole("button")).toHaveLength(5);
  });
});
