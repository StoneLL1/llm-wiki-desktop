# Import 重构实施与验收记录

日期：2026-09-06。实施依据：[计划](../plans/2026-09-06-import-architecture-refactor-plan.md)、[原始审查](../audits/2026-09-06-import-architecture-review.md)、[产品设计](../superpowers/specs/2026-07-24-import-source-media-flow-design.md)。

本记录区分可重复测试、真实组件和桌面操作。实时平台测试的进程退出成功只表示探测完成，只有确实提交并重新读到正文才计为导入成功。所有写入使用临时知识库，未修改用户现有知识库。

## 实施结果与工程选择

- 生产队列、并发配额、任务收尾和临时工作区归入 `services/import_execution.rs`；命令保留 IPC 和权限入口。小文本有独立执行名额，不等待长媒体释放全部配额。路由策略归入 `import_v2/routing.rs`。
- 原件、网页二进制、资源与提取证据通过文件引用和有界流进入质量检查及写入事务。`artifact.rs` 在实际复制时绑定预览摘要和长度，拒绝候选准备后发生变化的文件。正文仍有合理大小限制；大媒体不再误用正文 64 MiB 限制。
- 保留短 PDF 文本、空白页和普通 URI 链接。混合 PDF 先提供文字稿，用户可以补充扫描页。OCR 使用完整单页 PDF 渲染，保留成功页和定位失败页，完全没有正文仍失败。
- OCR/ASR 文章组成保留文字和时间/页码定位；坐标、识别参数与原始报告留在证据中。ASR 分片缓存绑定语言、profile、引擎和版本；网页字幕只选择一条主轨道。
- 首次缺能力时一次“准备并继续”，同时持久化所需 OCR/ASR 意图。应用级准备任务共享下载；后端在完成后重验每个原项目与条目，独立接续。关闭弹框、切项目不影响准备；取消或已提交条目不会复活。
- 安装器保留已验证 Range 断点，暂时网络错误最多重试三次并遵守有界 Retry-After；进度事件限制到每秒十次。更新失败保留旧版本。
- 新 catalog 增加 ZIP 成员摘要分块。程序修订可复用相同模型/运行时压缩块，安装后通过内容寻址大文件共享节省磁盘；不支持硬链接时保留普通文件。继续读取旧 catalog，不引入第二套包格式、下载资产或签名权威。旧已发布资产不含分块，首次迁移仍可能完整下载。
- 已装组件按签名、协议、平台、路由、权限、许可证和完整文件清单检查，不强制依赖版本等于当前桌面 recipe。开发 catalog 使用官方受信发布，开发 ASR 使用相同 staging 合同及仅 debug 接受的本地签名。
- `npm run tauri -- dev` 显式启用开发组件目录；打包应用不自动访问构建仓库。macOS 原生库签名在能力清单签名前归一化，Python 不向签名包写入 bytecode。
- Bilibili 继续使用现有 API 与必要音轨获取；抖音、小红书保留现有结构化适配器。没有为了平台包装引入另一套下载器/凭据系统。这一选择已经证明 Bilibili 无字幕 ASR 能形成 Source，但其他平台的实时覆盖仍有缺口，见下表。
- 应用级准备任务保留在任务抽屉，使用已有全局安装任务的终态刷新支持范围，避免把能力管理模块提前引入首屏；组件缺失时不显示历史成功提示。
- 成功识别的路线标签只留在尝试/来源记录，不再作为质量警告；终态事件先刷新可保存条目的投影，避免首次保存仅刷新旧确认。ASR 首屏只保留选择、实际下载与时长，不展示未经测量的耗时估计。
- 普通网页优先抽取文章/main 区域，保留代码块；网页、微信及小红书以正文和结构证据判断登录/验证码，正文中的认证词不再直接触发阻断。Source 阅读默认折叠内部元数据。

## 删除或收敛的旧复杂度

- 删除 Import presentation command 内约 480 行旧安装 worker 与重复安装编排；当前安装统一使用应用级协调器。应用级安装任务即使带 batch ID，也不会被旧 Import 批次筛选隐藏。
- 从 Import command 移出约 850 行执行服务；从主 orchestrator 移出约 250 行路由策略。移动不计作删除功能。
- 删除“最大嵌入图片等于 PDF 页面”的 OCR 提取实现。
- 删除以当前 recipe dependency lock 精确相等判断已装组件兼容的分支和两份嵌入 recipe 常量。
- 删除每个远程资源临时创建 Tokio runtime 的 pack 获取路径，复用有界运行设施。
- 复用已安装且验证通过的同版本目录，不再次获取 ZIP；实时测试将下载 URL 改为不可达域名，仍以新增下载 0 完成识别和保存。
- 移除当前 Import 弹框的重复安装勾选确认和重复技术详情；独立能力管理保留其提前准备入口。内置解析器不再出现在可安装组件表中并错误标为应用不支持。
- 修复旧图片等待状态错误选择文档组件，并兼容已经保存的旧 session；无须丢弃队列重新添加。

## 真实样本与结果

环境：Apple Silicon macOS、Rust 1.98.1、Python 3.12、未优化 debug 构建。Rust 工具链按用户授权装入任务临时目录；检查使用该目录的 PATH 和已有 Python 3.12，未修改全局 PATH 或 shell 配置。Windows/WebView2、Linux 和 Intel macOS 没有本机实测条件。本机网络使用 Fake-IP/TUN；普通网页诊断只对该次临时任务明确授权其精确解析地址，没有改变用户网络设置或全局 URL 策略。

| 样本/路径 | 实际结果 | 证据边界 |
| --- | --- | --- |
| 中文路径 MD，正文含 `login required`、`challenge`、`安全验证` | 桌面选择文件夹 → 预览 → 保存 → Wiki 阅读；重启后 Source 仍在 | 真实打包应用、真实 Markdown/JSON/Git |
| 混合 PDF，文字页 + 扫描页 | 原文字可先保存；真实 CJK OCR 识别第二页 `SCANNED PAGE- OCR REQUIRED`，提交重读成功 | 官方 runner/模型；识别约 16.1 s，准备/完整性检查约 34.3 s |
| TTS 生成的真实英语 WAV/M4A | SenseVoice 本地识别出验收语句；预览 → 提交 → 销毁服务后重读 Source | 真实 sherpa-onnx/CoreML；一次 WAV 识别 31.3 s，含完整性检查全程 59.7 s；打包应用中也完成 WAV 识别、保存及 Wiki 阅读；不表示人工语音精度基准 |
| 中文路径 PNG，已有 OCR，下载域名不可达 | 新增下载 0，识别约 16.4 s；正文仅为识别文字，质量 pass，保存后重读成功 | 签名/health 准备约 46.8 s；原始坐标报告保留为证据，不混入正文 |
| 同一媒体有可靠 SRT | 无 ASR 准备；一条转录正文，原媒体作为证据提交 | 生产发现、提取、质量检查和事务 |
| 65 MiB、256 MiB、1 GiB 媒体 + SRT | 均形成候选、保存并重读；65 MiB 原实现不能形成候选 | 自建稀疏媒体验证 I/O 与提交，不冒充 1 GiB 真实语音识别 |
| MDN HTTP Authentication 页面 | 真实 Source 约 13.4 KiB，含正文与代码，候选约 3.36 s | 允许缺图并显示警告；完整闭环约 8.28 s |
| 微信公开技术文章 `FAWINkUQGaLSHLAWlgHtmA` | 真实正文约 8.8 KiB，候选 1.91 s，保存重读全程约 2.97 s | 已检查 RobustMQ/Kafka 正文及代码；另一个以信息图为主的样本不计作纯文字文章成功 |
| 知乎公开回答 `1915729795955789864/answer/2049792362042364821` | 提交并重读约 6.9 KiB 正文 | 仍含部分导航；3 个图片未本地化，不算完整排版验收 |
| Bilibili `BV1GJ411x7h7` | 无公开字幕 → 明确识别意图 → 真实本地 ASR → Source 约 2.2 KiB → 重读 | 视频获取/识别约 153 s；另一视频 `BV1Sb41127UZ` CDN 获取失败，被隔离，未拖住后续条目 |
| 小红书 `67ea1ed7000000001e00424e` | 架构验收时未生成 Source；后续查明该链接已不可访问 | 后续已修复实际请求与交互，并完成真实照片、图片文字及视频 Source；见[小红书专项验收](xiaohongshu-import-acceptance.md) |
| 抖音 `7541559388436319503` | 实时提取未获得可用视频/正文 | 真实平台成功仍未闭合，不将 metadata 视作成功 |
| X NASA `1546290906046816256` | 正确等待交互浏览器组件 | 本轮未验证账号登录后 Source 闭环 |

桌面 OCR 首次准备：135.4 MiB 官方旧资产，安装目录约 346 MiB。第一次下载在约 79 MiB 处退出应用，重启后显示可继续；一次继续从原断点恢复、校验、运行 health、完成激活。准备期间可保存其他文本/PDF。原 PDF 已提交，因此完成结果为 resumed 0 / deferred 1，原 Source 未被重写。随后在同一隔离验收应用暂存旧 OCR 安装目录，重新制造缺组件状态：对旧 session 中的扫描图片点击一次“准备并继续”，关闭弹框并切到 Wiki。135.4 MiB 下载、完整性检查、health 与激活共约 162 s，后端 resumed 1 / deferred 1 / failed 0；自动生成干净预览，无第二次识别确认。一次保存成功；最终打包应用重启后从 Wiki 文件树重新打开，Source 正文、来源版本与预览一致，元数据默认折叠，质量通过，原始报告独立保留。此前三篇 Source 及其证据共 20 个文件摘要不变。重启后再次添加同一 PNG，使用已安装 OCR，无新下载任务；识别后自动判定 exact duplicate，保留原 Source ID/版本，队列自行结束，未写第二篇来源。

## 性能实测

同一机器、同一生产 journey 测试，对比任务开始时 HEAD 与改后代码。峰值为 macOS `/usr/bin/time -l` 的 maximum resident set size，包含测试控制进程，不包含模型子进程；后台还运行编译/其他测试，耗时只作为方向性测量。

| 样本 | 改前候选/提交 | 改后候选/提交 | 峰值内存 |
| --- | --- | --- | --- |
| 32 MiB + SRT | 3586 / 6246 ms | 3362 / 3640 ms | 81.5 → 17.2 MiB，约降低 79% |
| 65 MiB + SRT | 质量阶段拒绝，无可用提交 | 成功完成候选、提交与重读 | 不把改前失败的内存当作成功基线 |
| 256 MiB + SRT | — | 20916 / 21394 ms | 17.1 MiB |
| 1 GiB + SRT | — | 78324 / 183564 ms | 16.1 MiB |

分块测试修改 runner 后复用了 2,097,534 字节未变模型压缩载荷；故障注入覆盖 HTTP 503 自动重试和离线命中缓存。实际 inode 测试确认两个组件共享同一大文件，移除其中一个版本不破坏另一个。尚未用全套重新发布的 Windows 包测量首次安装总量，也没有把这些局部数字写成应用整体包体节省。

最终首屏 JS 为 622,896 B（gzip 180,518 B），低于原有 625,000 B / 192,000 B 预算，未放宽门槛。

未测交互 p95、100 个小文本真实桌面吞吐和 10,000 条真实窗口操作；既有有界分页/窗口化和并发测试继续作为代码级证据。最终未优化应用二进制约 152.6 MiB（160,020,224 字节），不是发布包体基准。

## 验证入口与审查

新增 `import_article_journey`、显式 opt-in 的 `import_real_capability_journey` / `import_live_url_journey`；扩展 PDF、缓存、流式事务、组件载荷、能力控制平面、Source 阅读和首次准备的回归验证。实时测试默认 ignored，不让普通 CI 下载模型或依赖第三方平台在线。

```sh
npm run prepare:import
npm run prepare:asr
npm run check

# 模型必须是受信目录；ROOT 必须为临时验收目录。
IMPORT_ACCEPTANCE_ROOT=/tmp/import-acceptance \
IMPORT_ACCEPTANCE_INPUT=/tmp/speech.wav \
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features \
  --test import_real_capability_journey -- --ignored --nocapture

IMPORT_LIVE_URLS='["https://developer.mozilla.org/en-US/docs/Web/HTTP/Guides/Authentication"]' \
cargo test --locked --manifest-path src-tauri/Cargo.toml --no-default-features \
  --test import_live_url_journey -- --ignored --nocapture
```

已完成一次初始双视角审查，处理有效问题。按用户后续要求不再重复调用 review 子代理；后续用本地检查、回归测试和实际打包应用修正发现的问题。

最终 `npm run check` 从头完整通过，耗时 **16m 57.8s**：前端 **147 个文件 / 1327 个测试**；Node/Python 工具验证、lint、生产前端构建、首屏包体、Tailwind、console 与命令边界检查通过；Rust GUI 编译、**1287 个单元测试（4 个按设计 ignored）**及全部常规集成测试通过；文档测试阶段完成（1 项按设计 ignored）。集成测试包括全格式发现到 Source 提交、10,000 条目队列、取消/恢复、项目权限、来源外部编辑、事务崩溃恢复及工作流。真实联网/模型验收单独执行，不计作普通 CI 的默认通过项目。

最终 debug `.app` 打包通过。运行应用已验证中文/英文阅读、缺 OCR 一次准备与后台自动接续、已有组件复用、重复来源保护、保存和重启重读。最终首屏订阅改为读取已有全局任务终态，回归测试覆盖切页后支持范围刷新，完整 gate 已包含该修复。

额外 `check:import-source-media` 与 `check:import-v2-cutover` 通过：32 个场景、26 个合同、14 类真实夹具声明与 9 个禁止成功捷径检查。旧 S11/S31 声明同步到新的部分 OCR 合同。`check:acceptance` 的 release 红线仍在 `capability-release-catalog` 失败：源码 `capabilities/install-catalog.json` 按既有构建合同保持空 entries，未执行生产全矩阵发布。其余六项红线通过；不能把开发 catalog 或 debug 打包当成生产发行验收通过。

## 仍未完成或未验证

- Windows 安装/WebView2、Intel macOS、Linux 原生 runner 与 GUI 验收。
- 全部格式与平台内容形态逐项使用真实发布 runner 的验收，以及全部平台登录态的实时成功样本，特别是抖音和 X；小红书公开图文/视频已在[后续专项验收](xiaohongshu-import-acceptance.md)形成真实 Source，登录态和真实短链仍未覆盖。不能将稳定夹具通过写作这些平台已经全面可用。
- 原已发布 macOS SenseVoice 包的原生库签名问题需要通过下一次正确 staging/签名发布修复；本轮真实 ASR 使用已修复并重新签名的开发包，没有修改生产信任密钥或已发布资产。
- 旧远端 catalog/ZIP 没有分块描述。新 release tooling 已生成，但尚未实际发布全目标新资产；缓存回收与物理共享占用的独立管理展示未实现。
- 未优化 debug 构建中，已安装组件的启动完整性检查仍有明显成本；本轮没有发布模式启动耗时基准，也没有把健康/签名校验时间计成模型识别速度。
- 完整浏览器当前 macOS 旧包仍约 605 MiB；没有承诺旧资产立即变小。模型质量、生产 release 性能、精确 p95 和上述跨平台结果仍需后续实测。

本地复核日志保留于 `/tmp/import-full-check9.log`、`/tmp/import-desktop-build13.log`、`/tmp/import-real-ocr-clean-final.log`、`/tmp/import-live-bilibili-asr.log`、`/tmp/import-live-wechat-text.log` 和 `/tmp/import-desktop-final-evidence.json`。可复现入口为上面的提交内测试；临时基线工作树和冗余 OCR 回滚备份已清理。当前没有可用 graphify CLI，因此未更新导航图；源码与文档定位使用当前文件和 `rg`。
