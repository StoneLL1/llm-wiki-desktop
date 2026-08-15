import { describe, expect, it, vi } from "vitest";

import { activateLocale, createLocaleResourceLoader } from ".";

describe("locale loading", () => {
  it("loads only the requested locale and reuses the resolved bundle", async () => {
    const loadEnglish = vi.fn(async () => ({ default: { language: "English" } }));
    const loadChinese = vi.fn(async () => ({ default: { language: "中文" } }));
    const addResourceBundle = vi.fn();
    const loader = createLocaleResourceLoader(
      { en: loadEnglish, "zh-CN": loadChinese },
      { addResourceBundle },
    );

    await loader("zh-CN");
    await loader("zh-CN");

    expect(loadChinese).toHaveBeenCalledOnce();
    expect(loadEnglish).not.toHaveBeenCalled();
    expect(addResourceBundle).toHaveBeenCalledWith(
      "zh-CN",
      "translation",
      { language: "中文" },
      true,
      true,
    );
  });

  it("keeps the previous language when the target chunk fails and allows retry", async () => {
    const loadLocale = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("locale chunk missing"))
      .mockResolvedValueOnce(undefined);
    const changeLanguage = vi.fn(async () => undefined);

    await expect(
      activateLocale("zh-CN", loadLocale, { changeLanguage }),
    ).rejects.toThrow("locale chunk missing");
    expect(changeLanguage).not.toHaveBeenCalled();

    await activateLocale("zh-CN", loadLocale, { changeLanguage });
    expect(changeLanguage).toHaveBeenCalledWith("zh-CN");
  });

  it("lets the latest language request win when locale chunks resolve out of order", async () => {
    let resolveEnglish!: () => void;
    let resolveChinese!: () => void;
    const english = new Promise<void>((resolve) => {
      resolveEnglish = resolve;
    });
    const chinese = new Promise<void>((resolve) => {
      resolveChinese = resolve;
    });
    const loadLocale = vi.fn((language: "en" | "zh-CN") =>
      language === "en" ? english : chinese,
    );
    const changeLanguage = vi.fn(async () => undefined);

    const staleRequest = activateLocale("zh-CN", loadLocale, { changeLanguage });
    const latestRequest = activateLocale("en", loadLocale, { changeLanguage });
    resolveEnglish();
    await expect(latestRequest).resolves.toBe(true);
    resolveChinese();
    await expect(staleRequest).resolves.toBe(false);

    expect(changeLanguage).toHaveBeenCalledOnce();
    expect(changeLanguage).toHaveBeenCalledWith("en");
  });

  it("does not activate a loaded locale when its project scope is stale", async () => {
    const changeLanguage = vi.fn(async () => undefined);

    await expect(
      activateLocale("zh-CN", async () => undefined, { changeLanguage }, () => false),
    ).resolves.toBe(false);

    expect(changeLanguage).not.toHaveBeenCalled();
  });
});
