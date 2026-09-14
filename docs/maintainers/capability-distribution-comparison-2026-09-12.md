# 能力分发与校验：同类实现对照

日期：2026-09-12。范围：公开官方文档、指定源码与本项目实现。
本文保留研究依据；本次确认的实现合同见 [ADR 0003](../architecture/decisions/0003-on-demand-capability-resources.md)。

## 同类实现的事实

| 应用 / 对象 | 查到的实现 | 对本项目的启示 |
| --- | --- | --- |
| Vibe / 本地转写模型 | 支持下载链接和模型目录。下载先写 `.part`，检查模型格式和最小大小；目录提供 size / SHA-256 时再验证这些字段。用户自定义 URL 的这两个字段可以缺省；该下载路径没有自定义包签名步骤。 | 模型不是必须绑定应用版本、CPU 平台及签名身份的可执行包；仍需识别下载到的 HTML 错误页和残缺文件。 |
| Buzz / Whisper 模型 | 官方 Whisper 路径使用预期 SHA-256，自定义 whisper.cpp URL 路径可以不提供该值；支持 Hugging Face 下载。FAQ 明确支持把另一台机器下载的模型搬到离线机器。 | 官方推荐下载与用户本地模型可以采用不同检查条件，无需只接受内置目录中的唯一 ZIP。 |
| Ollama / 本地模型 | 官方文档支持从本地 GGUF 和受支持架构的 Safetensors 文件导入。 | 模型的格式和引擎兼容性有意义；是否出现在本应用的 GitHub Release 中没有意义。 |
| Obsidian / 社区插件 | 官方目录安装确实从版本对应的 GitHub Release 取得 `main.js`、`manifest.json`、可选样式；插件 manifest 有自己的版本和最低应用版本。 | 使用 Release 是一个实际存在的分发选择，但不是普遍技术要求，也不代表插件必须与桌面版本同步发版。 |
| VS Code / 可执行扩展 | VSIX 可以独立打包并直接分发；`engines.vscode` 可声明兼容范围；平台包用于原生依赖，也支持通用包。Marketplace 分发的扩展有安装时签名验证。 | 不能推断“同类应用都不验签”；应按分发内容和来源选择机制，兼容范围与固定发行批次不是一回事。 |

主要来源：

- [Vibe 下载源码，固定提交 4d1db65](https://github.com/thewh1teagle/vibe/blob/4d1db65fd07d7927d7b4ef3a3d949539010147dc/desktop/src-tauri/src/cmd/download.rs)
- [Vibe 模型管理源码，同一提交](https://github.com/thewh1teagle/vibe/blob/4d1db65fd07d7927d7b4ef3a3d949539010147dc/desktop/src/lib/model.ts)
- [Buzz 下载源码，固定提交 2216597](https://github.com/chidiwilliams/buzz/blob/221659720db809da441d62e2cd031bbd5a174f42/buzz/model_loader.py)、[离线使用 FAQ](https://chidiwilliams.github.io/buzz/docs/faq)
- [Ollama 本地模型导入](https://docs.ollama.com/import)
- [Obsidian 插件发布](https://docs.obsidian.md/plugins/releasing/submit-plugin)、[Manifest](https://docs.obsidian.md/Reference/Manifest)
- [VS Code 打包、兼容范围与平台包](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)、[扩展运行安全](https://code.visualstudio.com/docs/configure/extensions/extension-runtime-security)

以上源码结论只覆盖读取的下载/模型管理路径。未运行这些应用做安装验收；未把文档未提及某个校验当作整款应用不存在该校验的证据。Vibe/Buzz 的模型下载不能直接等同于下载 Python、Node 或原生可执行程序。

## 本项目的问题在哪里

[目录校验脚本](../../scripts/verify-capability-catalog.mjs)要求下载 URL 使用固定 GitHub 仓库和 Release 语法，并将下载地址中的发布标签绑定桌面发布标签。[产物复用脚本](../../scripts/reuse-capability-release.mjs)还要求同一标签的指定历史工作流、具体任务及未过期产物，比较一组源码输入。

这里的绑定并不是“能力包版本号必须等于 App 版本号”，而是能力包的**分发地址和来源记录必须属于指定桌面发布批次**。这会让本来兼容的运行时、普通安装器代码变更和过期的 CI 产物相互牵连。

运行时、脚本和大型模型被封装为同一类按平台发布的包后，模型也被动承担了程序包的版本、平台、完整文件清单和签名合同。上一轮减少了重复哈希和等效日志改写，但没有解除这些耦合。

## 建议的轻量实现

1. **分发地址独立。** 配置实际可用的下载源；国内源、内网文件、官方 ZIP 都是传输方式。GitHub 是可选托管位置，不能成为唯一合法位置。校验下载文件本身，不校验 GitHub Release 身份作为运行前提。
2. **程序和模型分开。** 所有可选程序和运行环境均按需下载，不随 App 捆绑；可共享的资源复用。模型作为数据管理，可以下载或从本地导入。普通通用权重不按操作系统拆包；CoreML 等专用格式保留实际平台要求。
3. **版本按兼容性判断。** 保留包自身版本和必要的接口/模型格式版本，解除桌面发布批次绑定。不要让用户选择精确底层依赖版本；兼容时复用现有文件。
4. **官方程序下载一次校验。** 可先采用随 App 交付的可信清单 + HTTPS 下载 + 单个归档 SHA-256，再安全解压并做一次启动自检。此方案下无需同时把自定义 Ed25519 包签名、逐文件清单、每次调用完整重哈希全部设为前提。若未来从远端独立更新可信清单，再为那份清单确定来源认证方式；不能把同一不可信位置提供的文件和哈希当作独立证明。
5. **模型检查以可加载为目标。** 官方推荐模型有已知摘要则检查一次；用户导入的模型检查受支持格式、结构和实际加载结果，不因“没有本项目签名”拒绝。该通道只导入模型数据，不隐式执行附带脚本或安装依赖。
6. **就绪流程保持简单。** 下载/选择文件 → 校验并放到临时位置 → 自检 → 替换为可用文件。保留取消、具体错误和失败后的旧版本；完整路线回归主要由构建/测试承担，用户安装时不重复整套验收。

平台判断、损坏文件识别、解压路径限制和失败替换恢复都有直接的功能价值。可删除的是多层重复证明、发布平台硬绑定及不必要的同步发版要求。系统对 App 的代码签名与本项目额外的能力包签名是两件事。

国内可达性仍需要实际可维护的下载位置。把 URL 改成另一个海外服务，或去掉哈希/签名，都不能解决国内连接失败。本文没有配置或发布任何镜像。
