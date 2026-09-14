# 桌面发布失败审查与简化（2026-09-14）

## 日志确认的问题

检查了 GitHub 上五次失败运行。失败包含发布脚本错误、重复工作和对命令文本的过度约束，未发现这些运行因能力引擎资格测试失败而被阻止的证据。

| 运行 | 实际失败 | 处理 |
| --- | --- | --- |
| [34467572971](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34467572971) | 创建能力包 tag 时引用历史 workflow 提交，触发额外 workflows 权限要求；macOS `lipo` 参数顺序错误 | 桌面发布不再创建能力包 tag；保留正确的实际架构检查 |
| [34469550297](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34469550297) | 草稿查询 404；修正 `lipo` 后，检查器仍匹配旧命令而失败 | 删除对 shell 命令拼写的正则门禁，改为工作流结构和产物行为检查 |
| [34473082811](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34473082811) | 草稿查询 HTTP 404 | 区分尚未创建、权限错误与网络错误 |
| [34476195296](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34476195296) | 创建后无法解析能力包草稿 | 桌面与能力资源解耦；共用可恢复的桌面发布函数 |
| [34490570839](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34490570839) | `gh release view` 的 `release not found` 被直接当作致命错误 | 不存在则创建；草稿补传；已公开且字节相同则直接成功 |

最后一次失败中，源码完整检查耗时 17 分 46 秒并通过，四平台实际构建也通过。能力发布步骤在约 8 分钟准备和旧包下载后，仅执行 13 秒就因草稿查询失败；整个运行耗时约 22 分钟。这些成本没有帮助发现真正的发布错误。

只读检查两个 GitHub environment 均只有分支/tag 范围规则，没有 required reviewers；此次未修改远程权限、保护规则或 environment。

## 实现结果

本次修改前的本地稳定版与 RC workflow 共 668 行，合并后为一个 296 行的 [Desktop release](../../.github/workflows/desktop-release.yml)，只有预检、四平台构建、发布三个 job。

- 源码完整测试留在 CI，发布不重复执行，也不通过额外 API 轮询 CI 状态。维护者在源码 CI 通过后打 tag。
- 直接使用提交内完整能力目录，取消 catalog artifact 中转、历史构建 run ID、能力包 Release/tag 绑定及每次发版重复下载大模型。
- 构建前匿名检查资源前 4 KiB、状态、长度与文件类型，尽早发现缺失文件和 HTML 登录页。资源首次上传或修改后仍完整核验 SHA；用户安装仍核验完整归档。
- 正式版与 RC 共用四平台构建、产物汇总与发布逻辑。RC 自动覆盖 Tauri 版本，界面版本读取同一个运行时版本。
- 汇总产物声明中的真实文件，不用固定附件总数或临时“不要求 OS 签名”的证明文件驱动流程。
- 发布可以恢复草稿、跳过相同文件、补传缺失文件。GitHub 缺少 digest 时只下载对应文件核验，额外说明附件不阻止发布。
- 已公开版本不被重写。仅在最终发布时判断最新稳定版，旧版本和 RC 不覆盖稳定更新入口；公开后的 CDN 传播检查只提示问题。

保留 tag/构建提交对应关系、版本与 App 身份、updater 签名、四平台产物完整性、安装启动检查，以及能力目录实际嵌入检查。删除的是重复证明和实现文本约束。

## 验证与边界

- 发布器 16 项、资源可达性 18 项测试通过；覆盖草稿恢复、缺失 digest、公开版本重试、latest 防倒退、Range/重定向/HTML/模型镜像回退。
- 四平台临时产物实际汇总测试覆盖稳定版清单、RC 签名附件、混合提交、缺失平台和残留输出拒绝；不只检查 YAML 文本。
- 真实 Node 官方 ZIP 与 ModelScope ONNX 模型的轻量 HTTP 检查通过，两个资源约 1.2 秒。这不代表已部署本应用资源，也不代表所有国内网络可达。
- 完整 `npm run check` 通过，耗时 13 分 22 秒：1507 项前端测试、175 项工具测试、74 项能力工具测试、1403 项 Rust 主单元测试及默认集成测试通过；同时通过 lint、构建、Rust GUI 编译与 bundle 等检查。本机使用命令范围内的现有 macOS 15.4 SDK/Python 3.12，未安装软件或修改系统配置。
- 尚未推送修改、运行四平台新 Actions 或发布版本。本地验证不等于四平台安装包已经构建验收。
- `capabilities/install-catalog.json` 当前仍为空。真实发布前需要上传能力资源并提交完整真实地址；不能通过取消检查来让不存在的资源可用。

操作方式与重试说明见 [发布流程](../release/release-runbook.md)。

## v0.2.2 发布验收补充：Linux runner 关闭的代码根因

实际推送候选版本后，[首次 Linux 检查](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34823703946/job/103910844200) 与[补充提交的 Linux 检查](https://github.com/StoneLL1/llm-wiki-desktop/actions/runs/34824442390/job/103913422656) 都在 connector 子进程停止测试边界收到 SIGTERM。不能将这种固定位置的失败简单视为 runner 随机故障。

`pack_engine::terminate_tree` 曾调用外部 `kill -TERM -<pid>`。在 [procps 4.0.4 实现](https://gitlab.com/procps-ng/procps/-/raw/v4.0.4/src/kill.c) 中，未终止选项解析的负数可按首位处理；以 1 开头的 PID 因而可能变成向所有可发信号进程发送 TERM。这同时影响运行时取消路径，不只是 CI。

已改为直接调用 `libc::kill`，保持 TERM → 100ms → KILL 时序；两个 connector 测试也建立独立进程组。新增测试确认目标后代退出、管道结束且独立哨兵仍存活，4 项定向测试通过。删除基于错误基础设施假设的自动重试 job，三平台源码检查名称和要求保持不变。完整检查和 GitHub CI 的最终结果以新版发布验收为准。
