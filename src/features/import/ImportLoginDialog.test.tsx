import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import type { ConnectorSessionRef } from "../../types/importV2Presentation";
import { ImportLoginDialog } from "./ImportLoginDialog";

const session: ConnectorSessionRef = {
  sessionId: "connector-1",
  platform: "wechat",
  state: "waiting_login",
  accountSummary: "Aletta · @aletta",
  lastVerifiedAt: "2026-07-27T00:00:00Z",
};

beforeEach(async () => {
  await i18next.changeLanguage("en");
});

describe("ImportLoginDialog", () => {
  it("explains the dedicated profile and captcha boundary without password or cookie inputs", () => {
    render(
      <ImportLoginDialog
        open
        platform="wechat"
        publicDomain="mp.weixin.qq.com"
        authState="captcha_required"
        connectorSession={session}
        onBeginLogin={vi.fn()}
        onCheckAgain={vi.fn()}
        onRevoke={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText(/mp.weixin.qq.com/i)).toBeInTheDocument();
    expect(screen.getByText(/Aletta · @aletta/i)).toBeInTheDocument();
    expect(screen.queryByText("connector-1")).not.toBeInTheDocument();
    expect(screen.getByText(/dedicated.*profile/i)).toBeInTheDocument();
    expect(screen.getByText(/complete the captcha yourself/i)).toBeInTheDocument();
    expect(screen.queryByLabelText(/password|cookie/i)).not.toBeInTheDocument();
  });

  it("begins, checks, and revokes the one connector session through explicit buttons", async () => {
    const onBeginLogin = vi.fn().mockResolvedValue(session);
    const onCheckAgain = vi.fn().mockResolvedValue({ ...session, state: "authenticated" });
    const onRevoke = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportLoginDialog
        open
        platform="zhihu"
        publicDomain="www.zhihu.com"
        authState="waiting_login"
        connectorSession={null}
        onBeginLogin={onBeginLogin}
        onCheckAgain={onCheckAgain}
        onRevoke={onRevoke}
        onCancel={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /begin login/i }));
    await waitFor(() => expect(onBeginLogin).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: /check again/i }));
    await waitFor(() => expect(onCheckAgain).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: /revoke/i }));
    await waitFor(() => {
      expect(onRevoke).toHaveBeenCalledWith("connector-1");
    });
  });

  it("closes without consuming or completing the waiting login", () => {
    const onCancel = vi.fn();
    const onCheckAgain = vi.fn();
    render(
      <ImportLoginDialog
        open
        platform="wechat"
        publicDomain="mp.weixin.qq.com"
        authState="waiting_login"
        connectorSession={session}
        onBeginLogin={vi.fn()}
        onCheckAgain={onCheckAgain}
        onRevoke={vi.fn()}
        onCancel={onCancel}
      />,
    );

    fireEvent.click(screen.getByText("Cancel"));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onCheckAgain).not.toHaveBeenCalled();
  });
});
