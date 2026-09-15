# LLM Wiki Desktop v0.2.2

能力包安装与发布可靠性修复。所有可选程序继续按需下载，不增加 App 安装包体积。

## 更新内容

- 简化能力包安装与启用：归档校验、安全解压、准备模型、一次自检后启用；日常使用避免重复扫描整个程序和模型。
- 程序和模型分开分发与缓存，支持本地 ZIP 和配套模型目录离线安装；取消或失败保留此前可用版本。
- 能力资源采用独立版本和下载目录，修复旧版资源缺失与同版本不同内容的安装冲突。
- 修复 Windows 安装哈希栈溢出、文档解析器用户目录缺失，以及 Linux 取消任务时错误发送进程信号的问题。
- 统一正式版与 RC 发布流程，支持恢复草稿、补传文件，并保留安装启动和 updater 签名检查。

## Highlights

- On-demand capability programs with separately cached models and complete offline directory installation.
- One installation check and runtime self-test, followed by automatic activation; existing working versions survive cancellation or failure.
- Independent resource versions and resumable desktop publication. Optional programs are not bundled into the App.

## Download and install

| Platform | Installer |
| --- | --- |
| macOS · Apple Silicon | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.2/darwin-aarch64-LLM.Wiki.Desktop_0.2.2_aarch64.dmg) |
| macOS · Intel | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.2/darwin-x86_64-LLM.Wiki.Desktop_0.2.2_x64.dmg) |
| Windows · x64 | [Setup EXE](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.2/windows-x86_64-LLM.Wiki.Desktop_0.2.2_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.2/linux-x86_64-LLM.Wiki.Desktop_0.2.2_amd64.AppImage) |

The `.app.tar.gz` files and `latest.json` support automatic updates. `CHECKSUMS.sha256` covers the public desktop downloads. Optional engines install on demand from the download locations embedded in this version. Offline installation requires both the program ZIP and its companion model files. Network access depends on the actual resource host; a GitHub download is not a guarantee of domestic network availability.

## 无法直接下载时的离线安装

从[独立能力资源页面](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-2026-09-14)取得对应系统的程序 ZIP。有模型的能力还需下载 `models.zip`，解压后将 `models/` 文件夹与程序 ZIP 放在同一目录，再在 App 中选择本地 ZIP 安装。文件可以通过可用网络或其他设备转交，安装和后续本地解析无需连接 GitHub。网页内容获取仍需能访问对应网站。

当前公开资源托管在 GitHub，未配置国内镜像，不能保证所有国内网络直连。

## Updating from v0.2.1

Use **Check for updates**, or quit the old app and install v0.2.2 over it. Keep your knowledge-base folder in place. The application identity and update-signing key are unchanged.

macOS binaries are not Apple-notarized, and Windows installers do not carry an Authenticode identity. First-launch OS warnings may appear. See [installation notes and limitations](https://github.com/StoneLL1/llm-wiki-desktop/blob/app-v0.2.2/docs/release/known-limitations.md).

Source CI, resource download verification, updater-signature verification, and installation and launch checks on all four supported targets precede desktop publication. Hosted checks do not constitute a completed interactive upgrade and rollback test on every supported operating system.

[Full changelog](https://github.com/StoneLL1/llm-wiki-desktop/compare/app-v0.2.1...app-v0.2.2) · [Quick start](https://github.com/StoneLL1/llm-wiki-desktop#get-started)
