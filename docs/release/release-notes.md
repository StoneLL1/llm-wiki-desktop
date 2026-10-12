# LLM Wiki Desktop v2.0.6

修复 Update Wiki 对嵌套来源路径的误判，统一生成、审阅、恢复和应用过程中的来源校验，并完善设置保存与桌面任务体验。

## 更新内容

- 修复历史来源位于 wiki/sources/local/ 或 web/ 子目录时触发 COMPILE_SOURCE_REF_INVALID 的问题。
- 生成前绑定来源实际路径、版本和内容；审阅、恢复及应用沿用相同校验，及时识别来源变化与无效引用。
- 统一来源消费记录、写入快照和撤销范围，修复延迟处理、完整重编译、YAML 引用及新旧来源注册表混用问题。
- 完善全局与项目设置隔离、Provider 草稿保存、异步切换保护，以及主题、语言、通知和窗口关闭时的任务处理。
- 补齐来源校验和 Update Wiki 回归测试，沿用已验证的可选能力资源。

## Highlights

- Fix false source-reference failures for historical sources in nested local/web folders.
- Bind source paths, versions and content before generation; share validation across review, recovery and apply.
- Align consumption records, snapshots and undo; fix deferred inputs, full recompilation, YAML references and mixed registries.
- Improve settings persistence, provider drafts, project-switch protection and desktop task behavior.

## Download and install

| Platform | Installer |
| --- | --- |
| macOS · Apple Silicon | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.6/darwin-aarch64-LLM.Wiki.Desktop_2.0.6_aarch64.dmg) |
| macOS · Intel | [DMG](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.6/darwin-x86_64-LLM.Wiki.Desktop_2.0.6_x64.dmg) |
| Windows · x64 | [Setup EXE](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.6/windows-x86_64-LLM.Wiki.Desktop_2.0.6_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v2.0.6/linux-x86_64-LLM.Wiki.Desktop_2.0.6_amd64.AppImage) |

The `.app.tar.gz` files and `latest.json` support automatic updates. `CHECKSUMS.sha256` covers the public desktop downloads. Optional engines install on demand from the download locations embedded in this version. Offline installation requires both the program ZIP and its companion model files. Network access depends on the actual resource host; a GitHub download is not a guarantee of domestic network availability.

## 无法直接下载时的离线安装

从[独立能力资源页面](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/capabilities-2026-09-20)取得对应系统的程序 ZIP。有模型的能力还需下载 `models.zip`，解压后将 `models/` 文件夹与程序 ZIP 放在同一目录，再在 App 中选择本地 ZIP 安装。文件可以通过可用网络或其他设备转交，安装和后续本地解析无需连接 GitHub。网页内容获取仍需能访问对应网站。

当前公开资源托管在 GitHub，未配置国内镜像，不能保证所有国内网络直连。

## Updating from v2.0.5

Use **Check for updates**, or quit the old app and install v2.0.6 over it. Keep your knowledge-base folder in place. The application identity and update-signing key are unchanged.

macOS binaries are not Apple-notarized, and Windows installers do not carry an Authenticode identity. First-launch OS warnings may appear. See [installation notes and limitations](https://github.com/StoneLL1/llm-wiki-desktop/blob/app-v2.0.6/docs/release/known-limitations.md).

Source CI, resource download verification, updater-signature verification, and installation and launch checks on all four supported targets precede desktop publication. Hosted checks do not constitute a completed interactive upgrade and rollback test on every supported operating system.

[Full changelog](https://github.com/StoneLL1/llm-wiki-desktop/compare/app-v2.0.5...app-v2.0.6) · [Quick start](https://github.com/StoneLL1/llm-wiki-desktop#get-started)
