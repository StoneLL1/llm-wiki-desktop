# LLM Wiki Desktop v2.0.3

导入完整性、网页登录恢复和文档/媒体解析可靠性更新。可选能力资源同步更新，继续按需下载。

## 更新内容

- 保留原始来源身份与附带资源，改进重复导入、合并及 Agent 候选提交的一致性。
- 改进 Office、PDF、表格与结构化文档解析，减少正文与格式信息丢失。
- 完善网页登录后恢复、文章正文提取和页面资源保留。
- 修复长音频转写和字幕提取，支持视频关键帧 OCR 与后续内容整合。
- 更新浏览器、文档、OCR、语音和媒体能力资源；失败或取消保留已有可用内容。

## Highlights

- More reliable source identity, attachment retention, and import commits.
- Improved document fidelity, authenticated web recovery, and article extraction.
- Long-form transcription, subtitle extraction, and video frame OCR improvements.

## Download and install

| Platform | Installer |
| --- | --- |
| macOS · Apple Silicon | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.3/darwin-aarch64-LLM.Wiki.Desktop_2.0.3_aarch64.dmg) |
| macOS · Intel | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.3/darwin-x86_64-LLM.Wiki.Desktop_2.0.3_x64.dmg) |
| Windows · x64 | [Setup EXE](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.3/windows-x86_64-LLM.Wiki.Desktop_2.0.3_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.3/linux-x86_64-LLM.Wiki.Desktop_2.0.3_amd64.AppImage) |

The `.app.tar.gz` files and `latest.json` support automatic updates. `CHECKSUMS.sha256` covers the public desktop downloads. Optional engines install on demand from the download locations embedded in this version. Offline installation requires both the program ZIP and its companion model files. Network access depends on the actual resource host; a GitHub download is not a guarantee of domestic network availability.

## 无法直接下载时的离线安装

从[独立能力资源页面](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-2026-09-20)取得对应系统的程序 ZIP。有模型的能力还需下载 `models.zip`，解压后将 `models/` 文件夹与程序 ZIP 放在同一目录，再在 App 中选择本地 ZIP 安装。文件可以通过可用网络或其他设备转交，安装和后续本地解析无需连接 GitHub。网页内容获取仍需能访问对应网站。

当前公开资源托管在 GitHub，未配置国内镜像，不能保证所有国内网络直连。

## Updating from v0.2.2

Use **Check for updates**, or quit the old app and install v2.0.3 over it. Keep your knowledge-base folder in place. The application identity and update-signing key are unchanged.

macOS binaries are not Apple-notarized, and Windows installers do not carry an Authenticode identity. First-launch OS warnings may appear. See [installation notes and limitations](https://github.com/StoneLL1/llm-wiki-desktop/blob/app-v2.0.3/docs/release/known-limitations.md).

Source CI, resource download verification, updater-signature verification, and installation and launch checks on all four supported targets precede desktop publication. Hosted checks do not constitute a completed interactive upgrade and rollback test on every supported operating system.

[Full changelog](https://github.com/StoneLL1/llm-wiki-desktop/compare/app-v0.2.2...app-v2.0.3) · [Quick start](https://github.com/StoneLL1/llm-wiki-desktop#get-started)
