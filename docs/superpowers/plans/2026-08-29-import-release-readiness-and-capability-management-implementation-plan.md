# Import 全形式 Release 闭环与能力包管理实施计划

> 日期：2026-08-29
> 状态：In progress — 2026-08-30 functionality-first decision rebaseline
> 当前发布判断：No-Go
> 依据：`docs/reviews/2026-08-29-import-release-functionality-and-capability-management-review.md`
> 权威产品设计：`docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`
> 关联设计：`SPEC/APP_flow.md`、`SPEC/FRONTEND_GUIDELINES.md`、`SPEC/TECH_STACK.md`、`SPEC/BACKEND_STRUCTURE.md`
> 关系：本计划取代 `2026-08-21-import-capability-pack-installation-plan.md` 的实施顺序与 Release 完成定义；旧计划保留为能力安装专项设计记录。

## 0. 2026-08-30 决策修订

1. 产品只执行随应用官方 catalog 发布、通过固定 key 签名与 hash / manifest / target 校验的能力包；这些能力包属于受信任应用组件，与应用内置 helper / codec 采用同一信任模型。
2. Windows AppContainer、macOS App Sandbox helper、Linux namespace / seccomp / Landlock 等 OS 级 runner confinement 不再是首版功能、Batch 6 或 Release 的前置条件。产品和发布材料不得把普通子进程执行描述为“沙箱化”。
3. 功能正确性门保持不变：固定发布来源、签名与完整性校验、路径安全解压、条目级 staging / output 协议、有界日志与超时、取消和进程树终止、single-flight、全部 route 健康检查、原子激活以及旧健康版本回滚。
4. 已完成的原 Batch 5 stop commit 作为历史证据保留；后续以 **Batch 5R** 移除临时 `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE` 禁用、恢复受信任官方包安装，并完成未落地的多 route 原子激活。Batch 5R 的功能合同通过后，Batch 6 的完整 mutation UI 可以启用，不再等待四平台 OS 隔离证明。
5. 第三方市场、任意 URL、本地压缩包、自定义签名源和用户 PATH runtime 仍不在首版范围；若未来开放不受官方发布链控制的 runner，必须另立信任与权限设计，不能沿用本次决策。

## 1. 目标与完成定义

本计划的目标不是“让现有测试变绿”，而是让正式构建出来、可分发的桌面应用在真实平台上完成当前权威设计内的全部 Import 形式，并让能力包管理页可以主动、可靠、可恢复地下载和安装官方能力包。

只有以下条件同时成立，才能把 Import 的发布判断从 No-Go 改为 Go：

1. 文件、文件夹、剪贴板文本 / URL、普通网页、平台内容、集合、本地图片 / 音频 / 视频、远程媒体、字幕、OCR、ASR、登录恢复、重复 / 更新 / 合并和最终提交都按权威设计工作。
2. 201、600、1000、1001 与 10k 项批次由后端冻结完整工作范围，UI 分页、筛选、滚动和项目切换不改变实际处理集合。
3. 大于 64 MiB 的真实音视频可以进入 discovery、流式 staging、字幕 / ASR、取消、进程中断与恢复，不被统一文档上限提前丢弃。
4. native 与 compatible knowledge base 的 preview、commit、history、cleanup 和 recovery 只从 `ProjectContext.layout` 派生路径；普通进程中断通过独立恢复事实进入“已暂停，可继续”。
5. 用户可见格式、平台、ASR profile、恢复动作、runtime route 与 Release catalog 共同消费同一份产品能力清单，不再存在测试夹具扩大正式承诺的情况。
6. 能力包管理在没有当前项目、没有等待项时也能列出、下载、安装、更新和观察任务；Import 缺能力时复用同一全局任务并精确续接原条目。
7. 同一能力包的并发请求 single-flight；多 route pack 全部健康后原子激活；失败或更新回滚不破坏旧健康版本。
8. capability runner 只消费条目级 staging / output 协议；官方签名能力包作为受信任应用组件执行，四平台 OS 级文件系统、网络和子进程沙箱不作为发布前置，且产品不得宣称该执行已被沙箱化。
9. 标记为 distributable 的本地或 CI 构建缺少完整 catalog、可信 key、当前 target 条目或 exact-tag 绑定时直接失败；空 catalog 只允许明确标记的 development artifact。
10. exact tag 生成的 sealed candidate 在 Windows x64、macOS arm64、macOS x64、Ubuntu x64 完成真实安装包和代表样本验收，最后从头通过 `npm run check`。

## 2. 范围边界

### 2.1 本计划覆盖的 Import 形式

- 本地输入：单文件、多文件、文件夹递归、拖放、显式剪贴板文本 / Markdown / URL。
- 文档：PDF、DOC / DOCX、XLS / XLSX、PPT / PPTX、CSV、Markdown、TXT、本地 HTML。
- 图片：PNG、JPG / JPEG、WebP、BMP、TIFF、HEIC / HEIF；动态 GIF 按视频型媒体处理。
- 音频：MP3、WAV、M4A、AAC、FLAC、OGG、Opus、WMA。
- 视频：MP4、MOV、MKV、WebM、AVI、M4V、WMV。
- 字幕与稿件：SRT、VTT、ASS、LRC、TXT、Markdown。
- 网络输入：普通网页、需要浏览器升级的动态网页、当前产品承诺的平台路线、复合图文 / 视频、集合 / 播放列表 / 作者页和按需远程媒体。
- 恢复路线：登录并继续、启用 OCR、启用 ASR、安装能力并继续、失败重试、进程中断恢复、部分成功和剩余待办。
- Source 生命周期：预览、重复识别、更新 Diff、人工编辑保护、提交、历史、恢复和外部编辑冲突。

### 2.2 明确不扩大的范围

- 不恢复“导入后自动编译”；Import 提交与 Compile / Update Wiki 保持分离。
- 不用 BYOK 解析或修复 Import；BYOK 只在 Source 已存在后参与 AI 整理和 Wiki 流程。
- 不增加网页附件递归导入、加密 PDF 密码流程、全局剪贴板监听、图片视觉理解或自动图表描述。
- 不引入第三方能力市场、任意 URL、本地压缩包、自定义签名源或用户 PATH 运行时。
- 首轮能力管理不做卸载和自动磁盘清理；卸载需要单独的占用、任务和回滚设计。
- 不修改 `UI-Frontend-design/`；它只作为不冲突部分的视觉密度参考。

## 3. 不可破坏的实施约束

1. 项目内容继续使用 Markdown、JSON 和本地文件，不引入数据库。
2. React 不直接访问文件系统、下载 / 解压能力包、启动 runner、操作 Git 或处理 secret；所有动作经过 typed Tauri IPC 和后端服务。
3. `raw/sources/` 默认不可变；替换、删除、批量改写、冲突合并和高风险自动修复必须先有 Git checkpoint 与用户确认。
4. 注册在 `ProjectRegistry` 的路径不等于信任；继续 Import 前必须重验 trusted、writable、identity revision 和 authority epoch。
5. compatible vault 不得被重组为 native 目录；缺失逻辑写根时返回 typed prerequisite，不旁路写 `.app/`、`raw/` 或 `wiki/` 字面路径。
6. app-global capability 安装不要求项目 write permit；安装成功后的 Import continuation 必须逐项重新获得项目执行与写权限。
7. Task Drawer 只展示“应用级全局任务 + 当前项目任务”，绝不混入其他项目任务。
8. 所有长任务可取消、有日志、有可靠进度、有 terminal flush，并能区分主动取消、可恢复暂停和失败。
9. capability catalog、URL、hash、签名、公钥、临时路径和命令行只由后端掌握；前端只提交稳定 id、固定版本和确认 revision。
10. 每个 executable Batch 只做一个可审阅的纵向结果，不同时夹带无关重构。
11. 只有官方 catalog 中经过固定 key 签名、hash / manifest / target 校验的 pack 才进入 native runner；不得用本次受信任组件决策扩大为任意第三方代码执行。

## 4. 总体实施顺序

| Batch | 结果 | 前置依赖 | 发布停线点 | 风险 |
| --- | --- | --- | --- | --- |
| 0 | 冻结产品能力清单、分发模式和红线合同 | 无 | 清单未闭合不得做下载 UI 或打包 | 高 |
| 1 | 后端拥有完整批次范围与会话级待办事实 | Batch 0 的格式 / route 标识 | 201–10k 任一漏跑即停 | 高 |
| 2 | compatible layout、恢复与 read-only preview 收口 | Batch 1 DTO 稳定 | 任一 native 字面路径或恢复漏检即停 | 高 |
| 3 | 大媒体、集合、URL locator 与远程下载耐久化 | Batch 2 路径 / task 约束 | 整文件读内存、无取消或无恢复即停 | 高 |
| 4 | app-global capability control plane 与任务编排 | Batch 0；可与 1–3 的部分工作并行 | 无 single-flight / continuation 持久化即停 | 高 |
| 5R | 恢复受信任官方能力包安装并完成多 route 原子激活 | Batch 4；历史 Batch 5 stop 已完成 | 任一 route probe、原子切换或旧版本回滚不可靠即停 | 高 |
| 6 | 能力包管理可操作 UI 与全局任务体验 | Batch 4、5R DTO / runtime contract 冻结 | UI 绕过后端、无项目不可安装或状态混淆即停 | 中高 |
| 7 | Import 剩余状态、错误、分页与可访问性收口 | Batch 1–3、6 | 跨项目竞态或乐观误报未清零即停 | 中高 |
| 8 | 正式能力制品、全格式矩阵与 distributable build 闭环 | Batch 0、5R、7 | 任一承诺缺 target asset / 真 runner 证据即停 | 极高 |
| 9 | sealed candidate 与四平台 packaged acceptance | Batch 0–8 全绿 | 任一平台矩阵 Pending 即保持 No-Go | 极高 |

关键路径是 `0 → 4 → 5R → 6 → 8 → 9`。Batch 1–3 可以在 Batch 0 冻结标识后并行准备，但由于会共同修改 Import session、task 和 recovery 合同，落地时仍应按 1、2、3 顺序合并。正式能力制品的来源锁定、许可证盘点和四平台 staging 可在 Batch 0 后提前启动，只有在 Batch 5R runtime contract 冻结后才能封版。

## 5. Batch 0：冻结产品能力清单与 Release 红线

### 5.1 目标

建立唯一 `ProductCapabilityDefinition` 真源，使 Import 格式矩阵、recovery action、ASR profile、runtime route、pack manifest、catalog verifier、能力管理和 release workflow 不再各自维护集合。

### 5.2 实施项

1. 新增 `capabilities/product-manifest.json` 与 schema。每个定义至少包含：
   - `capabilityId`、产品名称 i18n key、分类和用途；
   - `routes`、extensions / formats、平台内容类型、协议版本；
   - `distributionTier`：`built_in | published | experimental | unsupported`；
   - 支持 target、固定 license policy、第三方 notices；
   - 下载 / 安装 / 模型体积字段来源；
   - 对应 Import recovery actions 与 ASR / OCR profile；
   - 是否允许主动安装、更新与 runtime network；
   - runner 权限声明和资格测试入口；
   - release staging script 和 owner。
2. 以“实现当前设计全部形式”为默认决策，逐项闭合：legacy Office、`document-standard` fallback、`asr-whisper`、HEIC / HEIF、WMA / WMV、动态 GIF、X route，以及当前 runtime 中存在但 catalog 未发布的 definition。
3. 禁止固定“5 包 / 20 条”或“7 包 / 28 条”假设；所有精确数量由产品清单 × supported targets 计算，仍要求 exact set、不可多也不可少。
4. Rust 端增加只读、编译期验证的产品清单 loader；`PACK_SPECS`、route 选择和 readiness 通过稳定 definition 查询，不在多个 match 中复制扩展名。
5. Node verifier 校验：
   - 每个用户可见 recovery action / profile / route 都有 built-in 或全部目标平台 published provider；
   - catalog 不能引入产品清单外能力；
   - 测试 fixture 不得声明产品清单外扩展并被当作 Release 证据；
   - product manifest、pack manifest、runner qualification 和 catalog target 完整一致。
6. 定义两种构建模式：
   - `development`：允许嵌入空 source fallback，但 About 与能力页明确显示不可分发；
   - `distributable`：必须注入完整 catalog、可信 key、exact version/tag、当前 target 条目和 provenance，否则 `build.rs` 直接失败。
7. `.github/workflows/capability-release.yml` 与 `desktop-release.yml` 只消费清单派生矩阵；不手写第二份包列表。
8. 先加入会失败的合同测试，再实现生成 / 校验逻辑，确保当前 catalog 漂移被测试真实捕获。

### 5.3 预计文件范围

- 新增：`capabilities/product-manifest.json`、`capabilities/product-manifest.schema.json`、`scripts/verify-product-capabilities.mjs` 及其测试。
- 后端：`src-tauri/build.rs`、`src-tauri/src/services/import_v2/capability_runtime.rs`、`capability_embed.rs`、`capability_pack.rs`，必要时新增聚焦的 `product_capability.rs`。
- 发布：`scripts/verify-capability-catalog.mjs`、`verify-embedded-capability-catalog.mjs`、`release-assets-contract.mjs`、两条 release workflow、`release/release-contract.json`。
- 合同：`src-tauri/tests/import_v2_format_pipeline.rs`、`import_v2_capability_packs.rs`、相关 Node tests。

### 5.4 验收

- 产品清单可生成 / 校验当前平台能力视图和 release exact matrix。
- 删除任一用户可见能力的目标平台 entry 会使合同测试和 distributable build 同时失败。
- 向测试 fixture 添加正式 definition 没有的 HEIC / WMA 等扩展，不再扩大 Release 通过结论。
- development 空 catalog 显示 `catalog_unavailable`；distributable 空 catalog 无法构建。
- X、legacy Office、accurate ASR、document fallback 等每项都有明确 provider 与四目标资产计划，不留“以后再说”的死按钮。

### 5.5 回滚点

本 Batch 只新增清单与 fail-closed 合同，不切换生产 runtime。若 loader 尚未稳定，可保留旧 `PACK_SPECS` 运行，但 distributable 模式继续保持禁用，不能以旧 catalog 发包。

## 6. Batch 1：后端冻结完整批次工作范围

### 6.1 目标

让扫描、确认、创建 session item 和启动处理成为一个后端拥有的完整控制面；前端只负责分页展示和用户动作，不能用已加载的 200 项窗口决定任务范围。

### 6.2 实施项

1. `accept_import_scan_v2` 在同一 authority transition 中消费保存扫描、创建全部 item，并调用 `start_import_batch_v2` 绑定全部新增 item id。
2. 不需要总量确认的普通扫描也返回一个绑定全部新增 item 的 operation task；不能再依赖 `useImportTaskCoordinator` 遍历 `itemById` 自动启动。
3. 返回结构化结果：session identity / revision、operation task、accepted item 总数、全量 overview patch 和稳定失败码。
4. `start_import_items_v2` 仅保留 `<= 200` 的兼容调用；大 cohort 返回 `IMPORT_BATCH_COMMAND_REQUIRED`，且任何调用链不能在同一 item 上双启动。
5. `ImportSessionOverview` 增加后端聚合的：
   - 全状态计数；
   - `actionGroups`（OCR、ASR、登录、能力、冲突等）；
   - `unresolvedCount` / `remainingCount`；
   - 当前 operation task identity 与进度；
   - 下一页 cursor 和 revision。
6. action group 的 capability id 来自 Batch 0 definition，不从前端 recovery action 字符串猜测。
7. operation task 使用一个 cancellation token，逐项 partial success；进度 / patch 有节流，terminal 强制 flush。
8. 前端删除“加载到哪就启动到哪”的所有路径；切换筛选、滚动、右侧选中和加载更多仅改变视图。

### 6.3 预计文件范围

- 后端：`commands/import_v2_file_commands.rs`、`import_v2_async_commands.rs`、`services/import_v2/scan_confirmation.rs`、`session_store.rs`、`orchestrator.rs`、`models/import_v2*.rs`。
- 前端：`useImportTaskCoordinator.ts`、`useImportSessionScope.ts`、`importStore.ts`、`ImportActionGroups.tsx`、`ImportCompletionSummary.tsx`、对应 types / API。
- 测试：`import_v2_scale_contract.rs`、`import_v2_file_orchestration.rs`、`import_v2_file_ingestion.rs`、`importScaleContract.test.ts`、store / integration tests。

### 6.4 验收矩阵

| 输入数 | 预期 |
| ---: | --- |
| 1 / 200 | 一个 operation task；全部 item 有 operation claim |
| 201 / 600 / 1000 | 不需要通过 UI 加载更多即可全部开始或进入真实等待状态 |
| 1001 | 总量确认后只消费保存扫描一次，并启动完整 cohort |
| 10k | 工作范围稳定、DOM 仍窗口化、取消有界、进度和日志不按项爆炸 |

附加断言：筛选、加载更多、页面离开、项目切换后返回，不改变后端 item 总数、action group 总数或 task scope；单项失败不回滚其他项。

### 6.5 回滚点

在前端删除旧自动启动前，先让新响应携带 operation task，并加“同 item 不得双 claim”测试。若前端迁移失败，可临时继续显示旧视图，但后端始终是唯一启动 owner。

## 7. Batch 2：layout、恢复与只读预览收口

### 7.1 目标

消除 native 字面路径旁路，使 compatible vault 与普通中断恢复成为一等行为，并允许 restricted / read-only 项目先在 application temp 完成只读 discovery / preview。

### 7.2 实施项

1. 在 `ProjectLayout` 上集中形成 `ImportLayoutPaths` / `SourceLayoutPaths` 逻辑访问器，覆盖：
   - session、attempt、staging、preview、history、cleanup；
   - source manifest / index / baseline / artifact；
   - evidence、Source write root、clipboard temp 和 recovery journal。
2. 替换 presentation、commit、manual merge、history、cleanup、source lifecycle 与 recovery 中所有 `.app/import-sessions`、`.app/sources`、`.app/source-artifacts`、`raw/` 等字面拼接。
3. `ImportSessionOverview` 返回独立 `recoveryRequired` 与 reasons，由 stale in-flight item、interrupted task / attempt、incomplete journal、partial remote / capability download 和残留 staging 推导。
4. 项目打开后先运行有界 reconciliation：将可解释的处理中状态映射为 paused，清理无引用临时文件，保留可恢复分片；不自动恢复耗时工作。
5. TaskService 的进程重启映射与 Import reconciliation 对齐，不把普通可恢复中断永久显示为普通 failed。
6. restricted / trusted read-only 的 discovery / preview 使用 application temp 和 non-persistent task；commit 时重新请求 trusted + writable authority，不能提前写项目 app-state。
7. 所有 path API 保留 `OsString` / 原始路径身份；Unix 非 UTF-8 路径若某路线确实不支持，返回 typed 可见跳过原因，不能静默消失。

### 7.3 预计文件范围

- `src-tauri/src/models/layout.rs`、`app_state.rs` 与 ProjectContext 解析。
- `import_v2_presentation_commands.rs`、`import_v2_file_commands.rs`、`import_v2_commands.rs`、`import_v2_web_commands.rs`。
- `services/import_v2/commit.rs`、`history_store.rs`、`source_registry.rs`、`source_lifecycle.rs`、`orchestrator.rs`、`session_store.rs`、`transaction.rs`。
- 前端 `useImportSessionScope.ts`、overview types、恢复提示与对应测试。

### 7.4 验收

- native 与 compatible 各执行：preview → commit → app 强杀 → reopen → reconciliation → continue → history。
- compatible 普通 Markdown 目录结构与不相关文件的 byte inventory 不变，不出现 native 目录回写。
- sidecar / index 完好时强杀 OCR、ASR 或媒体任务，仍能看到“已暂停，可继续”。
- restricted / read-only 可完成安全 preview，但 commit 清楚说明需要写权限；撤权竞态 fail closed。
- CJK、空格、长路径、大小写差异、symlink / junction 与 Unix 非 UTF-8 边界有自动化覆盖。

### 7.5 回滚点

路径迁移保持访问器兼容旧 native 映射，不移动用户文件。任何需要重写旧状态的迁移都必须先做 Git / 文件 checkpoint，并作为独立确认流程，不能隐藏在项目打开或 Import 恢复里。

## 8. Batch 3：大媒体与网络输入耐久化

### 8.1 目标

让真实音视频、集合和远程媒体使用流式、有界、可取消、可恢复的后台任务，不再依赖整文件 `Vec`、双复制或一次性 30 秒请求。

### 8.2 实施项

1. discovery 先流式读取有界头部做 MIME / 容器识别，再按产品 definition 和媒体类型应用不同安全上限；文档、图片、音频、视频不共享 64 MiB 默认值。
2. 安全上限同时考虑：格式、声明时长 / 尺寸、可用磁盘、临时放大系数和用户确认；越界返回 typed 原因与预计需求，不静默 skip。
3. local media fingerprint、归档、staging 和 decoder 输入改为文件句柄 / 流式 copy + incremental hash；避免同时保存完整内存、`source.bin` 和第二临时副本。
4. remote media 增加 versioned partial journal，绑定 canonical URL / secure locator identity、ETag / Last-Modified、总长度、已完成 Range 和 hash 状态；服务端不支持安全续传时明确重下。
5. 主动取消按产品规则清理 partial；进程中断保留可验证 partial 并显示 paused；terminal 成功 / skip / failure 统一清理不再需要的 secret locator 与 temp。
6. secure URL locator 支持长任务续期与 terminal deletion；秘密仍在 OS credential storage，不进入项目、日志或诊断导出。
7. collection discovery 接入 durable / cancellable task，分页发现和“继续加载全部”有进度、resume cursor 与项目 / epoch 守卫。
8. 有字幕时不下载媒体；ASR 只取适合识别的音轨；画面 OCR 才下载视频流 / 关键帧；默认不永久保留远程完整媒体。

### 8.3 预计文件范围

- `models/import_v2_file.rs`、`services/import_v2/file_discovery.rs`、`local_media_engine.rs`、`media_router.rs`。
- `services/import_v2/generic_web_engine.rs`、`web_target_store.rs`、`web_fetch.rs`、`remote_media_retention.rs`、`commands/import_v2_web_commands.rs`。
- task / journal DTO、Import 进度 UI 与对应 tests / fixtures。

### 8.4 验收

- 使用至少一个 >64 MiB 音频和一个 >64 MiB 视频完成 discovery、取消、强杀、重开、Range 继续和最终 Source commit。
- 峰值内存不随媒体完整字节线性复制；测试证明没有整文件 `read_to_end` 进入核心路径。
- 0%、25%、75%、校验前和激活前中断的语义可区分；损坏 partial、identity drift 和无 Range 服务端安全降级。
- 集合离开页面 / 重启后仍可观察，不因组件卸载丢任务。
- 任务完成、取消、skip 后无孤儿 locator、临时媒体或可访问 secret。

### 8.5 回滚点

先在新 journal version 上实现，旧一次性下载只作为小体积、无恢复的明确 fallback；不得让新代码读取未绑定 identity 的旧 partial。大媒体路线未通过三平台源测试和四平台 packaged 验收前，不扩大支持文案。

## 9. Batch 4：app-global capability control plane

### 9.1 目标

把“全局安装事实、项目附属命令”的错位拆开，建立统一应用级 catalog snapshot、task、single-flight、continuation registry 和兼容的 Import facade。

### 9.2 后端合同

必须提供并显式注册：

```text
list_app_capabilities_v1() -> AppCapabilityView[]
install_app_capability_v1(capabilityId, expectedVersion, acknowledgementVersion) -> BackendTask
pause_app_capability_install_v1(taskId, taskRevision) -> BackendTask
resume_app_capability_install_v1(taskId, taskRevision) -> BackendTask
cancel_app_capability_install_v1(taskId, taskRevision) -> BackendTask
```

若通用 task commands 能安全支持 app-global scope，后三个动作可复用通用命令，但 DTO 必须显式携带 scope / revision，不能伪造 active project。

`AppCapabilityView` 至少包含四组正交事实：

- `distribution`：published / source catalog empty / not published for target / unsupported；
- `installation`：absent / healthy / unhealthy，以及当前健康版本；
- `operation`：queued / downloading / paused / verifying / installing / health checking / activating / recovering / failed / cancelled / succeeded；
- `update`：none / available / in progress / rollback restored。

并返回派生 display state、全部 routes / formats、target、版本、大小、license、权限摘要、active task id、当前项目等待数和稳定错误码。

### 9.3 实施项

1. 在 `AppState` 增加稳定 `AppCapabilityCoordinator` facade，组合 embedded product/catalog、installed runtime snapshot、installer、app-global tasks、locks / partials 和 continuation registry。
2. app-global task 持久化在应用数据目录，不写任何知识库；task kind 与项目任务可区分，日志和状态有版本 schema。
3. single-flight key 固定为 `(capabilityId, targetVersion, targetTriple, archiveIdentity)`；后续请求 join 同一 task 并订阅，不返回“另一个任务正在安装”的普通失败。
4. continuation registry 持久化：project identity revision、注册项目 id、session / item、requirement revision、requested route、恢复动作、创建时间和状态。不得持久化 Cookie、下载 secret 或未脱敏 URL。
5. `install_import_capability_v2` 保留为 item-linked facade：先验证条目需求，再注册 continuation，最后 join 全局 coordinator。
6. 全局任务成功后 fan-out；每个 continuation 独立重验 project / root / session / item / requirement / authority。非活动、撤权、只读或漂移项只记录“能力已安装，原项未继续”。
7. 更新失败不回滚健康全局能力事实；某个 continuation 失败也不回滚其他项目或已安装 runtime。
8. 错误分类至少覆盖 offline / proxy、TLS / DNS、Range、hash / signature、manifest、target、disk、file lock / AV、health、rollback 和 build catalog unavailable。

### 9.4 预计文件范围

- 新增聚焦 service / model：`app_capability_coordinator.rs`、`app_capability.rs`、versioned continuation store。
- `src-tauri/src/app_state.rs`、`commands/mod.rs`、`lib.rs`、新增 app capability commands。
- `services/import_v2/capability_installer.rs`、`capability_embed.rs`、`capability_runtime.rs`、现有 item facade。
- `tasks/task_model.rs`、`task_service.rs`、`commands/task_commands.rs`、前端 task types / store。

### 9.5 验收

- 无项目时可以列出和安装 published capability。
- 只读项目中可以完成应用级安装，但不会静默继续写项目。
- 两项目、管理页和多个 item 同时请求同一能力，只产生一个 archive download / verify / install task。
- 进程中断后恢复全局 task、partial 和 continuation；主动取消与 crash pause 不混淆。
- Task Drawer 显示全局任务和当前项目任务，不显示另一个项目的任务。

### 9.6 回滚点

所有新 IPC 使用 `_v1` 新名字；旧 `install_import_capability_v2` 只改内部委托，不立即删除。全局 schema 必须 versioned、append-safe；无法读取新状态时 fail closed 并保留 archive / 旧健康 runtime，不做猜测性清理。

## 10. Batch 5R：受信任 runner 合同与多 route 原子激活

### 10.1 目标

恢复官方签名能力包的安装与执行，让一个 pack 的全部产品 route 作为一个原子 runtime snapshot 激活；不再把 OS 级 runner confinement 作为功能或发布前置。

### 10.2 多 route 激活

1. installer 从 Batch 0 definition 读取 pack 的全部 routes，不接收管理页虚构的 `requestedRoute`。
2. 新版本安装到独立 version dir 后，对每条声明 route 运行真实 protocol probe / qualification。
3. 所有 route 成功后一次性发布 immutable runtime snapshot，再提交 activation journal。
4. 任一路线失败：不发布新 snapshot，保留旧健康版本，记录 rollback receipt，UI 同时显示旧版本可用与更新失败。
5. startup reaper 能解释 prepared / probed / activated / rollback 各阶段，不删除仍可能恢复的旧版本。

### 10.3 受信任 runner 功能合同

1. 产品只接受固定官方 catalog 中、通过签名、hash、manifest、target 与产品清单闭合验证的 pack；不接受用户指定 executable、任意 URL、本地 archive 或第三方签名源。
2. 为每次调用创建条目级 invocation root，只包含：
   - 明确复制 / 绑定的只读输入；
   - capability 自身只读 runtime；
   - 独立可写输出与 temp；
   - 有界、白名单环境变量和标准输入协议。
3. 不主动传入项目根、项目目录 handle、Git 配置、凭据、shell profile、用户日常浏览器 profile 或无关代理 secret；cwd 使用 invocation root。需要登录态的平台 route 只允许接收应用管理、能力专用且在权限摘要中披露的 connector profile；该 profile 是路线输入，不代表可访问用户日常浏览器资料。该约束用于稳定协议和减少无关耦合，不宣称构成 OS 沙箱。
4. runner 按普通受信任子进程执行，可以使用其实现路线所需的网络与子进程；产品清单和确认页如实展示用途与运行权限摘要。
5. 保留有界 stdout / stderr、deadline、取消、进程树终止、协议校验、输出大小限制和 terminal flush；这些是可靠性与可恢复性合同。
6. `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE` 及其派生的全局只读安装状态属于历史 Batch 5 临时 stop，Batch 5R 必须通过 additive migration 移除该硬编码 gate，不能让 UI 自行覆盖后端禁用。

### 10.4 功能与故障测试矩阵

- 篡改 archive、hash、signature、manifest、target、route set 与 protocol version，验证安装不会开始或不会激活。
- 尝试 archive / manifest 路径穿越、symlink / junction、绝对路径输出与越界 output 声明，验证 installer 和输出接收端拒绝。
- 模拟单 route probe 失败、健康结果格式错误、输出超限、超时、取消、进程崩溃与孤儿进程，验证 stable error、进程树终止和 invocation root 清理。
- 在 prepared / probed / activated / rollback 各阶段强杀应用，验证 startup reaper 恢复到新版本完整可用或旧健康版本完整可用，不出现半激活 route set。
- 验证管理页、Import A 与 Import B 并发请求只产生一个 task，并在完成后按条目重新校验 authority 再续接。

### 10.5 预计文件范围

- `services/import_v2/pack_engine.rs`、`pack_protocol.rs`、`capability_runtime.rs`、`activation.rs`、`capability_installer.rs`、`execution_control.rs`。
- `runner_confinement.rs` 临时 stop gate、相关 DTO / presentation 派生与 superseding ADR；不新增平台 sandbox adapter。
- `src-tauri/tests/import_v2_capability_packs.rs`、`import_v2_agent_workspace.rs`、新增 atomic activation、故障恢复和四 target 功能条件测试。

### 10.6 验收与回滚

- browser multi-route pack 任意一个 route probe 失败时，所有新 route 都不可见，旧版本继续可用。
- 管理页安装无需 route；Import 触发安装也不会只激活触发路线。
- 四个 target 均能从受信任官方 catalog 安装、重启恢复并运行真实代表 route；不要求 OS sandbox / malicious-runner confinement 证据。
- `APP_CAPABILITY_CONFINEMENT_UNAVAILABLE` 不再阻断官方 pack；Batch 6 mutation actions 只能在后端安装、single-flight、全 route 原子激活与回滚合同全部通过后启用。

## 11. Batch 6：能力包管理可操作 UI

### 11.1 目标体验

能力管理是 app-global 官方目录，而不是当前 Import 队列摘要。主入口保留在 Import 的“能力管理”，可在无项目时通过 shell 复用；若 Settings 也提供入口，两处必须渲染同一个 view / store，不复制业务状态。

页面使用紧凑连续表格，不做卡片墙：

```text
能力名称           用途 / 覆盖              状态 / 版本        大小       操作
文档兼容解析       DOC / XLS / PPT           可安装 1.0.0       286 MB     下载并安装
中文图片 OCR       PNG / JPG / TIFF …        下载中 42%         640 MB     取消
本地语音转写       音频 / 视频               已安装 1.0.0       1.2 GB     查看详情
高精度语音转写     音频 / 视频               当前平台未发布     —          查看原因
```

### 11.2 实施项

1. 新增 app-global capability API / store / controller，由 `AppShell` 挂载，不放在项目级 `WorkspaceController` 内。
2. `ImportCapabilitiesPanel` 消费 `AppCapabilityView[]`，提供：
   - 顶部汇总：已安装、可安装、可更新、进行中、需处理、当前平台未发布；
   - 分类筛选：全部、文档、网页、OCR、媒体与 ASR；
   - 状态筛选和名称搜索；
   - 名称 / 用途、routes / formats、版本、四组状态、包 / 模型 / 安装大小、license、主操作和详情。
3. 主操作严格由后端 facts 派生：下载并安装、继续、取消、重试、更新、查看详情；`catalog_unavailable` 与 `not_published_for_target` 不显示假安装按钮。
4. 泛化确认对话框：
   - management origin 不要求 session / item；
   - import origin 显示将继续的条目数和逐项重验说明；
   - 展示固定版本、target、签名 key id、来源域、体积 / 磁盘、SPDX license、权限摘要和失败不激活说明。
5. 行内进度与 Task Drawer 订阅同一个 app-global task。下载阶段显示真实 bytes；verify / install / health / activate 只显示可靠步骤。
6. Import waiting item 的“安装并继续”登记 continuation 后跳转 / 订阅同一任务；成功 copy 区分“正在继续原项”和“能力已安装，但原项已变化”。
7. 错误 UI 只显示本地化摘要、下一步与稳定码；URL、temp path、runner 输出放在脱敏折叠详情。

### 11.3 硬化与可访问性

- 键盘可遍历行、筛选、主操作和详情；焦点环清晰，状态变化通过 `aria-live` 有界播报。
- 不能只靠颜色表达 published / healthy / failed / rollback。
- 验证中英文长名称、许可证表达式、超长 route 列表、200% zoom、窄中央 pane 和任务同时更新时不发生布局跳动。
- 覆盖离线、catalog 空、target 未发布、磁盘不足、文件锁、签名失败、旧版本健康 + 更新下载、更新回滚、项目切换和窗口最小化。
- 遵循现有 13 px 主体、12 px 次级、11 px muted/mono、hairline border 和 token；不新增硬编码 hex、渐变或营销式视觉。

### 11.4 预计文件范围

- `src/services` 下新增 app capability API。
- `src/stores` 下新增 app-global capability store，扩展 `taskStore.ts` 的 global selector。
- `AppShell.tsx`、`TaskLogDrawer.tsx`、`ImportCapabilitiesPanel.tsx`、`ImportCapabilityDialog.tsx`、`capabilityInstallState.ts`、types、i18n 和 tests。

### 11.5 验收

- 没有项目、没有等待项时也能完成管理页安装。
- 切项目、换页签、最小化后任务继续且仍可观察；其他项目任务不泄露。
- management、Import A、Import B 同时请求只出现一个任务和一致进度。
- 所有失败 / 空态有明确下一步，没有乐观支持或无效按钮。
- 键盘与屏幕阅读器可以确认当前行、进度、失败与操作结果。

### 11.6 回滚点

先以只读 snapshot 接入，再开放按钮；只有 Batch 4、5R 的命令、single-flight、官方包安装、全 route 原子激活和回滚测试全部绿后才启用 mutation actions。UI 回滚不影响后台全局任务和已安装健康 runtime。

## 12. Batch 7：Import 状态、错误与边界收口

### 12.1 目标

清零审阅中未被 Batch 1–6 覆盖的 P1 / P2，确保功能闭环不会在项目切换、分页、部分失败或可访问性边界上失真。

### 12.2 实施项

1. 受限内容确认：每个 async commit 点重验 project key / epoch / session / digest；项目 A 迟到响应不能在项目 B 打开 dialog。
2. 显式剪贴板动作：单一 HTTP(S) URL 进入 URL preview；文本 / Markdown 进入本地文本 Source；不监听全局剪贴板。
3. Completion summary 使用 overview 的 `remainingCount`，提供“继续处理剩余 N 项”，不把 waiting 项误当完成。
4. OCR / ASR 分组操作改成后端 batch authorization，或返回逐项结果并在 `finally` 统一 refresh / start；部分失败不吞掉已成功授权项。
5. 删除 `created-session-window` 假 cursor；异常恢复重新调用正常一致窗口加载，并验证 session / filter / revision。
6. readiness 请求失败时显示“状态暂不可用 + 重试”，不能默认把七类格式标为可用；LRC、GIF、keyframes 和其他矩阵全部来自产品 definition。
7. API 与 workflow 统一 `normalizeBackendError`；用户区只显示本地化 summary / action，技术 message 进入诊断。
8. 右侧 panel 按 store 的稳定 selected item 读取，不因当前 page window 不含该项而突然空白；preview 失败显示原因和重试。
9. Import Queue 使用 roving focus 或 `aria-activedescendant`，active row 有可见样式；checkbox selection 与 row focus 独立。
10. CJK、Unicode、Windows / Unix 路径与 Unix 非 UTF-8 跳过原因进入 UI 和诊断矩阵。

### 12.3 预计文件范围

- `useImportWorkflow.ts`、`useImportSessionScope.ts`、`useImportBatchController.ts`、`useImportSupportingActions.ts`。
- `ImportSourceMethods.tsx`、`ImportCompletionSummary.tsx`、`ImportActionGroups.tsx`、`ImportQueue.tsx`、`ImportRightPanel.tsx`。
- `importStore.ts`、`importV2Api.ts`、types / i18n / corresponding backend batch commands and tests。

### 12.4 验收

- 快速 A → B → A 切项目并人为延迟响应，不出现跨项目 dialog、toast、task 或 selected item。
- 筛选、分页、load more 后 action groups / completion / right panel 与全会话事实一致。
- readiness 断网、backend 英文错误、preview 失败、OCR / ASR 部分失败均有本地化、可恢复表现。
- Import Queue 完成纯键盘选择、查看详情、执行主操作和返回；屏幕阅读器可获知 active row。

## 13. Batch 8：正式能力制品与全格式闭环

### 13.1 目标

把 Batch 0 中所有 `published` definition 变成四目标真实、签名、可健康检查的制品，并用正式 runner 而不是测试 fake pack 证明每个格式和平台路线。

### 13.2 实施项

1. 对每个 published pack 锁定：上游版本 / URL / SHA-256、许可证与 notices、确定性 staging、manifest inventory、协议、健康检查、压缩 / 安装 / 模型大小。
2. 补齐产品清单要求的正式制品与 route：
   - document fallback 与 legacy Office；
   - Accurate ASR / `asr-whisper`；
   - HEIC / HEIF decoder、WMA / WMV、动态 GIF；
   - X 的官方 production route；
   - 其他由清单标为 published 的 document / OCR / media / browser definition。
3. 每个 pack × target 恰好一个 catalog entry；archive、manifest、runner 与 definition 的 routes / formats 精确相等。
4. qualification 使用可再分发、脱敏的小型真实样本语料；每个格式至少含正常、扩展名伪装、损坏、取消和边界样本。
5. X / 微信 / 图文 / 视频等网络路线同时验证公开、登录墙、受限内容、unknown platform fallback 和 endpoint policy；fixture 成功不等于 production route 成功。
6. 新增 app-global 集成测试：IPC list → confirm → Range download → verify → install → all-route probe → atomic activation → restart → continuation fan-out → Source commit。
7. distributable build 在本地和 CI 使用同一 preflight；embedded catalog bytes 与同 run sealed catalog 完全一致。
8. 更新 release checklist、known limitations、support matrix 和证据模板，删除固定 20 entries 的历史表述，改为 manifest-derived exact count。

### 13.3 代表样本矩阵

- 每个本地文件格式至少一项；混合 PDF、旧 Office、大 CSV / XLSX 独立确认。
- 图片 OCR：普通图片、长图、扫描 PDF、HEIC / HEIF；普通网页配图不误触 OCR。
- 音频 / 视频：有字幕、无字幕启用 ASR、无有效语音、关键帧 OCR、WMA / WMV / GIF。
- 网页 / 平台：普通静态页、动态浏览器升级、登录并继续、图文、视频、复合内容、集合分页、X route。
- Source 生命周期：完全重复、别名、更新 Diff、外部编辑三方合并、部分失败、剩余待办。
- capability：主动安装、Import 安装并继续、并发 join、25% / 75% 中断、篡改拒绝、旧版本回滚和重启复用。

### 13.4 预计文件范围

- `capabilities/*` manifests / runners / package locks / qualification fixtures。
- `capabilities/release-sources.json`、staging scripts、catalog verifiers、两条 release workflows。
- `services/import_v2/capability_runtime.rs`、format / media / browser routers 和 integration tests。
- `docs/release/*`、support / known limitation 文档和 packaged smoke scripts。

### 13.5 验收

- 产品清单的每个 user-visible form 都能追踪到 built-in implementation 或四目标 signed asset。
- 正式 runner 的 format matrix 与 UI 完全一致；删除 fake pack 的额外扩展后测试仍绿。
- distributable build 缺任一 asset / key / definition coverage 立即失败。
- app-global 端到端测试覆盖安装、回滚、重启与多项目 continuation。
- 所有源码级测试完成后仍保持 No-Go，直到 Batch 9 真实 packaged matrix 完成。

## 14. Batch 9：sealed candidate 与四平台 Release 验收

### 14.1 前置条件

- Batch 0–8 的代码、合同、两轮 review 和完整 gate 全绿。
- 同一 exact commit 的 Windows / macOS / Ubuntu CI 全绿。
- capability / updater protected secret、trusted public key、Environment reviewer 和 release owner 条件完整；不读取或记录 secret 值。
- 由用户另行明确批准创建 immutable candidate tag；本计划本身不授权创建 tag、发布 Draft、修改 `latest.json` 或公开 Release。

### 14.2 Sealed candidate

1. 使用一个 tag / commit / workflow run 构建全部 capability archives、catalog、桌面安装包、updater signatures、checksums、SBOM、provenance 和 packaged smoke。
2. workflow 先生成 sealed `draft-release-bundle`；不为了获得 GitHub Draft 提前批准 `publish-stable`。
3. 记录每个 artifact 的名称、大小、SHA-256、target、签名 / provenance identity 和同 tag URL。
4. capability 数量从产品清单计算，不沿用历史 5 × 4 固定值。

### 14.3 四平台必测旅程

每个平台使用干净真实主机或 VM：

1. 安装、首次启动、重启、卸载；记录 OS warning 与项目 byte inventory。
2. 无项目打开能力管理，主动安装一个能力，重启后仍 healthy。
3. 新建 native 与打开 compatible 项目，复用已安装能力完成真实 Import。
4. 清除测试能力后，从 waiting item 执行“安装并继续”。
5. 管理页 + 两项目并发请求，验证 single-flight 和逐项 authority 重验。
6. 下载在 25% / 75% 断网、主动暂停 / 取消、强杀应用，再安全续传或重下。
7. 篡改 archive / hash / signature / manifest / target，验证拒绝；健康失败验证旧版本回滚。
8. 执行 201 / 1000 / 1001 / 10k 批次和 >64 MiB 音视频；验证 UI 分页不改变工作范围。
9. 执行完整代表格式 / 平台矩阵、登录、集合、远程媒体和 Source commit。
10. restricted、read-only、untrusted、撤权、项目切换与 requirement drift 的 Import continuation 全部 fail closed；畸形 / 超时 / 部分健康 runner 不得发布新 runtime snapshot。

### 14.4 最终 Go 门

在候选 commit 上从头运行：

```powershell
npm run check:import-source-media
npm run check:import-v2-cutover
npm run test:capability-tools
npm run check:release-config
npm run test:final-four-redlines
npm run check:final-four-redlines
npm run check
```

然后要求：

- 四个平台所有必测矩阵均为 Passed，不能以 Pending 或源码测试替代。
- 匿名反向下载安装包、capability archive、catalog、`latest.json`、checksums、SBOM 和 provenance 成功。
- 从公开候选制品重复至少一遍能力管理与 Import 续接，不使用 workflow 临时路径。
- `docs/release/batch-6-acceptance-evidence.md` 与 first-release checklist 更新为 exact candidate 证据。
- Release owner 完成 Go / No-Go 记录；只有获得单独最终发布批准后才能执行 protected publisher。

## 15. 审阅问题到 Batch 的追踪

| 审阅项 | 负责 Batch | 关闭证据 |
| --- | --- | --- |
| P0-1 首 200 项决定工作范围 | 1 | 201 / 600 / 1000 / 1001 / 10k 集成测试 |
| P0-2 64 MiB 与整文件内存 | 3 | >64 MiB packaged media、内存 / IO 证据 |
| P0-3 产品能力与 catalog 漂移 | 0、8 | 单一 manifest、四目标 exact catalog |
| P0-4 fake pack 扩大格式 | 0、8 | production runner qualification matrix |
| P0-5 compatible 路径旁路 | 2 | compatible preview / commit / recovery E2E |
| P0-6 普通中断不恢复 | 2、3 | 独立 recovery fact、强杀测试 |
| P0-7 X route 不可执行 | 0、8 | official runner + signed asset + packaged sample |
| P0-8 管理页不能主动下载 | 4、6 | 无项目主动安装旅程 |
| P0-9 多 route 只激活一条 | 5 | all-route probe 与原子 snapshot 测试 |
| P0-10 release 静默空 catalog | 0、8 | distributable build negative tests |
| P0-11 runner 无 OS 沙箱 | 5R | 产品决策接受官方签名 pack 作为受信任组件；四平台功能与完整性 evidence，不宣称 sandbox |
| P0-12 无 sealed candidate | 9 | exact-tag 四平台 acceptance |
| P1-1～6 全局任务 / single-flight / continuation / 状态 / pause / 集成链路 | 4–6 | app-global task E2E |
| P1-7 会话待办受分页影响 | 1、7 | overview action groups tests |
| P1-8 跨项目受限确认竞态 | 7 | delayed A/B project switch tests |
| P1-9 剪贴板 URL 路由 | 7 | explicit paste routing tests |
| P1-10 完成摘要漏待办 | 1、7 | remaining count contract |
| P1-11 OCR / ASR 部分失败 | 7 | batch per-item result tests |
| P1-12 假 cursor | 7 | backend-bound cursor recovery tests |
| P1-13 readiness 乐观误报 | 0、7 | manifest-driven unavailable state |
| P1-14 英文技术错误透传 | 4、7 | normalized/localized error tests |
| P1-15 read-only preview 过早写入 | 2 | temp preview + commit revalidation |
| P1-16 URL locator 生命周期 | 3 | TTL renew / terminal cleanup tests |
| P1-17 collection 非 durable task | 3 | cancel / restart discovery tests |
| P1-18 远程媒体无 Range | 3 | partial identity / resume tests |
| P2 queue / right panel / preview / 非 UTF-8 | 2、7 | a11y、paging、path edge tests |

## 16. 每个 executable Batch 的固定执行模板

每个 Batch 0–8 均按以下顺序实施，Batch 9 使用同样标准并额外执行 packaged matrix：

1. **开始前**
   - 重读本计划对应 Batch、权威 Import spec 和相关 gotcha。
   - 检查 dirty worktree，保护用户已有修改；不清理、不 reset、不改无关文件。
   - 用 Graphify query / path 确认当前 facade、task、layout 和测试依赖。
   - 先补红合同或失败 fixture，记录该 Batch 的可验证失败。
2. **实现**
   - command 保持薄层；DTO 先行；服务经 `AppState` 稳定 facade。
   - 所有新长任务接 TaskService、取消、日志、进度、terminal flush 和恢复语义。
   - 文件 mutation 保持 checkpoint / permit / retained-path 安全边界。
3. **聚焦验证**
   - 先跑修改区域的 Vitest、Rust integration / unit、Node / Python capability tests。
   - Import 合同变更至少跑 `check:import-source-media` 与 `check:import-v2-cutover`。
   - capability / release 变更至少跑 `test:capability-tools` 与 `check:release-config`。
4. **双审阅**
   - Reviewer A 带共享上下文：检查设计意图、跨层逻辑、与权威文档一致性。
   - Reviewer B 使用新鲜上下文：检查盲点、安全、边界、缺失测试和模糊行为。
   - 每个 Batch 只进行一次正式双审阅轮次；合并有效结论，修复后自行复核。
5. **完整门禁**
   - 因 Batch 0–8 都涉及 release、文件系统、IPC、并发、任务或安全，最终修复后从头运行 `npm run check`。
   - 若因本 Batch 失败，修复后必须再次从 `npm run check` 起点完整执行。
6. **收尾**
   - executable code 修改后运行 `graphify update .`，确认图谱无异常漂移。
   - 在 `progress.txt` 顶部追加里程碑；出现隐蔽 / 复发问题时在 `gotchas.txt` 顶部追加一条。
   - 更新本计划 Batch 状态、release evidence 与未关闭风险；未满足 exit gate 不开始依赖它的下游 Batch。

## 17. 回滚与数据安全策略

1. 每个 Batch 采用 additive / versioned DTO 和状态文件；旧 schema 只读迁移，不覆盖未知字段。
2. 新 operation task 上线时保留小批量兼容 facade，但以“同 item 单一 claim”防止双执行。
3. layout 改造不移动 compatible vault 文件；若需要修复历史状态，单独展示 affected paths、checkpoint 和 confirmation。
4. remote / capability partial 都绑定版本与 identity；不能识别的 partial 保留为隔离 orphan 供受控清理，不猜测续传。
5. capability 激活始终先保留旧健康版本；新版本全部 route 健康后才切换，失败写 rollback receipt。
6. UI 可随时退回只读 capability snapshot；后台 task / runtime 不依赖页面存活。
7. sealed candidate 前不创建稳定 tag、不上传正式 Release、不修改 latest channel；真实发布需单独授权。

## 18. 主要风险与处理

| 风险 | 早期信号 | 处理 |
| --- | --- | --- |
| 受信任官方 runner 拥有与普通应用子进程相近的宿主权限 | 第三方资产混入 catalog、发布材料误称 sandbox、权限摘要与实际路线不符 | 首版只允许固定官方 key / catalog / product manifest 闭合资产；review 发布来源并如实披露权限，不以 OS confinement 阻断功能 |
| 多 route 激活产生半新半旧 runtime | 单 route probe 失败后仍有新 route 可见、回滚丢失旧健康版本 | Batch 5R 以 immutable snapshot 一次发布全部 route，阶段 journal + rollback receipt 覆盖强杀恢复 |
| 能力包体积 / 许可证不可发布 | staging 不确定、notices 缺失、模型来源不可复现 | Batch 0 锁来源与 license；Batch 8 缺一项不进入 published |
| 10k 批次引发锁、FD、日志或 UI 压力 | operation task 延迟、descriptor 激增、patch 风暴 | 共享父 capability、有界并发、100 ms patch coalesce、窗口化 DOM、专门 scale tests |
| compatible 路径修复误写用户目录 | 出现 native 字面路径或目录 byte inventory 变化 | layout accessor 合同、retained path、真实 compatible E2E、mutation 前 checkpoint |
| Range 续传复用了错误内容 | ETag / length / tag 漂移、hash 最终不符 | identity-bound journal、最终全量 hash/signature、不安全即删除重下 |
| 全局任务泄露其他项目状态 | Task Drawer 混入非当前项目 task | global + active-project 两段 selector；project continuations 只显示计数而不泄露内容 |
| source 测试替代 packaged 证据 | fixture 全绿但 catalog / binary 为空 | 分离 source、distributable、sealed 三层证据；Batch 9 前始终 No-Go |
| 本地 full gate 受并行资源抖动影响 | lazy route 偶发 timeout | 按 gotcha 先复现聚焦测试，保持断言不放宽，再从头重跑完整 gate |

## 19. 最终交付物

- 权威、受 schema 校验的 capability product manifest。
- 后端拥有完整 scope 的 Import operation task 与 session overview。
- layout-safe、recovery-aware、streaming / resumable 的 Import pipeline。
- app-global capability snapshot、task、single-flight、continuation 和原子 runtime activation。
- 受信任官方 runner 合同、四平台真实功能证据与不宣称 sandbox 的发布说明。
- 可操作、紧凑、可访问的能力包管理页面。
- 逐格式、逐 route、逐 target 的正式能力制品与真实 qualification matrix。
- distributable build fail-closed 合同。
- exact-tag sealed candidate、四平台 packaged acceptance 和更新后的 release evidence。

计划完成后，产品才可以对用户作出以下承诺：

> LLM Wiki Desktop 的正式 Release 可以处理当前支持矩阵中的本地文件、文件夹、网页、平台、图片、音视频、字幕、OCR、ASR 与恢复流程；所需官方能力可在能力包管理中主动下载并安装，也可在导入过程中安装后安全续接。能力安装可观察、可取消、可恢复、跨项目复用，并经过固定来源、hash、签名、条目级运行协议、全 route 原子激活和真实平台健康检查；官方 runner 作为受信任应用组件执行，不宣称 OS 级 sandbox。
