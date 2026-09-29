# LLM Wiki Desktop v2.0.5

完善 Lint 健康状态与 Agent 修复工作流，改进修复结果查看，并修正 Wiki 更新后的关系图缓存和上下文文档路径展示。

## 更新内容

- 完成 Lint 健康检查、修复建议与 Agent 辅助修复的集成，改善任务状态和结果导航。
- Wiki 内容变更后继续复用有效的关系图缓存，减少不必要的重新计算。
- 在评估上下文中显示相关文档路径，方便核对来源。
- 沿用已验证的可选能力资源，继续按需下载。

## Highlights

- More complete Lint health reporting and Agent-assisted repair workflow.
- Preserve valid graph cache after Wiki mutations and improve repair-result navigation.
- Show assessed context document paths for easier source review.

## Download and install

| Platform | Installer |
| --- | --- |
| macOS · Apple Silicon | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.5/darwin-aarch64-LLM.Wiki.Desktop_2.0.5_aarch64.dmg) |
| macOS · Intel | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.5/darwin-x86_64-LLM.Wiki.Desktop_2.0.5_x64.dmg) |
| Windows · x64 | [Setup EXE](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.5/windows-x86_64-LLM.Wiki.Desktop_2.0.5_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.5/linux-x86_64-LLM.Wiki.Desktop_2.0.5_amd64.AppImage) |

The `.app.tar.gz` files and `latest.json` support automatic updates. `CHECKSUMS.sha256` covers the public desktop downloads. Optional engines install on demand from the download locations embedded in this version. Offline installation requires both the program ZIP and its companion model files. Network access depends on the actual resource host; a GitHub download is not a guarantee of domestic network availability.

## 无法直接下载时的离线安装

从[独立能力资源页面](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-2026-09-20)取得对应系统的程序 ZIP。有模型的能力还需下载 `models.zip`，解压后将 `models/` 文件夹与程序 ZIP 放在同一目录，再在 App 中选择本地 ZIP 安装。文件可以通过可用网络或其他设备转交，安装和后续本地解析无需连接 GitHub。网页内容获取仍需能访问对应网站。

当前公开资源托管在 GitHub，未配置国内镜像，不能保证所有国内网络直连。

## Updating from v2.0.4

Use **Check for updates**, or quit the old app and install v2.0.5 over it. Keep your knowledge-base folder in place. The application identity and update-signing key are unchanged.

macOS binaries are not Apple-notarized, and Windows installers do not carry an Authenticode identity. First-launch OS warnings may appear. See [installation notes and limitations](https://github.com/StoneLL1/llm-wiki-desktop/blob/app-v2.0.5/docs/release/known-limitations.md).

Source CI, resource download verification, updater-signature verification, and installation and launch checks on all four supported targets precede desktop publication. Hosted checks do not constitute a completed interactive upgrade and rollback test on every supported operating system.

[Full changelog](https://github.com/StoneLL1/llm-wiki-desktop/compare/app-v2.0.4...app-v2.0.5) · [Quick start](https://github.com/StoneLL1/llm-wiki-desktop#get-started)
