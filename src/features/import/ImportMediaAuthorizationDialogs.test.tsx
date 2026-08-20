import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { i18next } from "../../i18n";
import { ImportAsrDialog } from "./ImportAsrDialog";
import { ImportSubtitleDialog } from "./ImportSubtitleDialog";

const asrPlan = {
  recommendedProfile: "accurate" as const,
  availableMemoryBytes: 16 * 1024 ** 3,
  availableDiskBytes: 120 * 1024 ** 3,
  mediaDurationSeconds: 600,
  installLocation: "C:\\AppData\\installed-capabilities",
  localOnly: true,
  profiles: ([
    ["fast", true, false, 0.5],
    ["balanced", true, false, 0.75],
    ["accurate", false, true, 1.25],
  ] as const).map(([profile, available, installable, speed]) => ({
    profile,
    capabilityId: profile === "accurate" ? "asr-whisper" : "asr-sensevoice-small",
    engineName: profile === "accurate" ? "whisper.cpp" : "sherpa-onnx SenseVoiceSmall",
    modelName: profile === "accurate" ? "Whisper small" : "SenseVoiceSmall int8",
    available,
    installable,
    downloadBytes: available ? null : 512 * 1024 ** 2,
    installedBytes: profile === "accurate" ? 1024 * 1024 ** 2 : 256 * 1024 ** 2,
    modelBytes: profile === "accurate" ? 488 * 1024 ** 2 : 128 * 1024 ** 2,
    device: "cpu",
    estimatedSeconds: Math.ceil(600 * speed),
    unavailableReasonCode: available ? null : "not_installed",
    dependencies: [
      {
        kind: "media_runtime" as const,
        name: "FFmpeg local media runtime",
        available,
        bundledWithCapability: true,
        source: "https://ffmpeg.org/",
        license: "LGPL-2.1-or-later",
      },
      {
        kind: "engine" as const,
        name: profile === "accurate" ? "whisper.cpp" : "sherpa-onnx SenseVoiceSmall",
        available,
        bundledWithCapability: true,
        source: "https://example.com",
        license: "MIT",
      },
    ],
  })),
};

beforeEach(async () => {
  localStorage.clear();
  await i18next.changeLanguage("en");
});

describe("Import media authorization dialogs", () => {
  it("shows local ASR tradeoffs and submits the explicit profile and language", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportAsrDialog
        open
        plan={asrPlan}
        loading={false}
        onConfirm={onConfirm}
        onInstall={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(screen.getByText("High quality").closest("label")).toHaveTextContent("(recommended)");
    expect(screen.getByText("512 MB")).toBeInTheDocument();
    expect(screen.getByText("C:\\AppData\\installed-capabilities")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Dependency chain" })).toBeInTheDocument();
    expect(screen.getByText(/No cloud fallback/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: /Balanced/i }));
    fireEvent.change(screen.getByRole("combobox", { name: /Spoken language/i }), {
      target: { value: "zh" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: /Remember this profile/i }));
    fireEvent.click(screen.getByRole("button", { name: /Enable and continue/i }));

    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith({
      profile: "balanced",
      language: "zh",
    }));
    expect(localStorage.getItem("llm-wiki-desktop.import.asr-preference.v1")).toContain("\"language\":\"zh\"");
  });

  it("supports Chinese copy and Escape without granting authorization", async () => {
    await i18next.changeLanguage("zh-CN");
    const onCancel = vi.fn();
    const onConfirm = vi.fn();
    render(
      <ImportAsrDialog
        open
        plan={{ ...asrPlan, recommendedProfile: "balanced" }}
        loading={false}
        onConfirm={onConfirm}
        onInstall={vi.fn()}
        onCancel={onCancel}
      />,
    );
    expect(screen.getByRole("heading", { name: "设置本地语音识别" })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("keeps the ASR dialog open and presents an actionable install failure", async () => {
    const onInstall = vi.fn().mockRejectedValue({
      code: "IMPORT_V2_CAPABILITY_UNAVAILABLE",
      message: "The signed archive could not be downloaded.",
      recoverable: true,
    });
    render(
      <ImportAsrDialog
        open
        plan={asrPlan}
        loading={false}
        onConfirm={vi.fn()}
        onInstall={onInstall}
        onCancel={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Download and enable/i }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/required import capability is unavailable/i);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Retry/i })).toBeInTheDocument();
  });

  it("requires an explicit choice when multiple CJK companion subtitles match", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    render(
      <ImportSubtitleDialog
        open
        candidates={["访谈.en.srt", "访谈.zh-CN.srt"]}
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("radio", { name: "访谈.zh-CN.srt" }));
    fireEvent.click(screen.getByRole("button", { name: "Use this file" }));
    await waitFor(() => expect(onConfirm).toHaveBeenCalledWith("访谈.zh-CN.srt"));
  });
});
