import { describe, expect, it } from "vitest";
import type { WebRecoveryAction, WebRouteKind } from "./importV2Web";

describe("Import V2 web contracts", () => {
  it("freezes route and recovery action wire names", () => {
    const routes: WebRouteKind[] = ["generic_http", "generic_browser", "wechat", "zhihu", "bilibili", "xiaohongshu", "x"];
    const actions: WebRecoveryAction[] = ["retry_route", "switch_route", "begin_login", "authorize_private_target", "install_browser_capability", "install_media_capability", "invoke_agent", "skip", "view_log"];
    expect(routes).toHaveLength(7);
    expect(actions).toHaveLength(9);
  });
});
