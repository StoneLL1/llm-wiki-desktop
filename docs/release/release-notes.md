# LLM Wiki Desktop v2.0.4

修复导入、AI 对话、Agent、Wiki 更新与工作流中的一致性和安全性问题。

## 更新内容

- 修复 Agent 工具调用和导入恢复，确保来源版本、原始文件与生成候选保持一致。
- 修复只读 Chat 会话和回答引用，避免对话过程意外修改来源内容。
- 修复 Update Wiki 的生成、链接处理、预览、确认与撤销流程，保护外部编辑和 Git 状态。
- 修复工作流排队、取消与结果展示，以及 Agent 辅助 Lint 的修复边界。
- 沿用 v2.0.3 已验证的可选能力资源，继续按需下载。

## Highlights

- Safer Agent-assisted imports and recoverable source updates.
- Read-only Chat behavior and more reliable answer references.
- Safer Wiki updates, workflow execution, review, and undo.

## Download and install

| Platform | Installer |
| --- | --- |
| macOS · Apple Silicon | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.4/darwin-aarch64-LLM.Wiki.Desktop_2.0.4_aarch64.dmg) |
| macOS · Intel | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.4/darwin-x86_64-LLM.Wiki.Desktop_2.0.4_x64.dmg) |
| Windows · x64 | [Setup EXE](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.4/windows-x86_64-LLM.Wiki.Desktop_2.0.4_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.4/linux-x86_64-LLM.Wiki.Desktop_2.0.4_amd64.AppImage) |

The `.app.tar.gz` files and `latest.json` support automatic updates. `CHECKSUMS.sha256` covers the public desktop downloads. Optional engines install on demand from the download locations embedded in this version. Offline installation requires both the program ZIP and its companion model files. Network access depends on the actual resource host; a GitHub download is not a guarantee of domestic network availability.

## 无法直接下载时的离线安装

从[独立能力资源页面](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-2026-09-20)取得对应系统的程序 ZIP。有模型的能力还需下载 `models.zip`，解压后将 `models/` 文件夹与程序 ZIP 放在同一目录，再在 App 中选择本地 ZIP 安装。文件可以通过可用网络或其他设备转交，安装和后续本地解析无需连接 GitHub。网页内容获取仍需能访问对应网站。

当前公开资源托管在 GitHub，未配置国内镜像，不能保证所有国内网络直连。

## Updating from v2.0.3

Use **Check for updates**, or quit the old app and install v2.0.4 over it. Keep your knowledge-base folder in place. The application identity and update-signing key are unchanged.

macOS binaries are not Apple-notarized, and Windows installers do not carry an Authenticode identity. First-launch OS warnings may appear. See [installation notes and limitations](https://github.com/StoneLL1/llm-wiki-desktop/blob/app-v2.0.4/docs/release/known-limitations.md).

Source CI, resource download verification, updater-signature verification, and installation and launch checks on all four supported targets precede desktop publication. Hosted checks do not constitute a completed interactive upgrade and rollback test on every supported operating system.

[Full changelog](https://github.com/StoneLL1/llm-wiki-desktop/compare/app-v2.0.3...app-v2.0.4) · [Quick start](https://github.com/StoneLL1/llm-wiki-desktop#get-started)
