# Import、来源库与媒体处理当前实现审阅

> 审阅日期：2026-07-25
> 产品基准：[`docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../superpowers/specs/2026-07-24-import-source-media-flow-design.md)
> 目标读者：后续负责拆 batch、制定实施计划、编码、测试和复核的 Agent
> 文档性质：实现差距清单与实施输入，不是新的产品设计；任何冲突均以 2026-07-24 基准文档为准。

## 0. 如何使用本文

后续 Agent 不应把本文当作一份可以跳过产品基准的替代规格。正确顺序是：

1. 先读 2026-07-24 产品基准；
2. 再读本文的优先级、证据、依赖和验收建议；
3. 每个 batch 开始前重新核对工作区当前代码，因为本次审阅面对的是一个有大量未提交修改的工作树快照，而不是一个固定 commit；
4. 先修会继续固化错误产品模型的契约，再扩功能；
5. 每个 batch 只关闭自己声明的验收项，不得用“测试通过”替代产品语义验收；
6. 所有删除、覆盖、批量重写、Source 合并、Source 替换和 Agent 应用候选仍须遵守项目 Git checkpoint 与显式确认规则。

本文的 `P0 / P1 / P2 / P3` 含义：

| 级别 | 含义 | 实施约束 |
| --- | --- | --- |
| P0 | 直接破坏产品不变量，成功状态并不等于获得了可读 Source，或存在基准明确禁止的主路径 | 在继续扩展对应模块前先修；不能带着该分支继续增加 UI/测试 |
| P1 | 核心闭环缺失、数据生命周期不完整，或常见输入无法完成基准流程 | 应进入近期主干批次；不得以“后续优化”无限延期 |
| P2 | 信息架构、批量效率、可理解性、恢复性、无障碍或文案明显偏离基准 | 在核心契约稳定后成组修复 |
| P3 | 完成感、维护性和边缘一致性问题 | 可后置，但应有明确验收而不是自然消失 |

## 1. 审阅范围、方法与快照说明

### 1.1 已使用的项目上下文

本次首先完整读取并遵循了项目内技能：

- `skills/llm-wiki-desktop-context/SKILL.md`
- `skills/llm-wiki-desktop-context/references/project-map.md`

随后核对了：

- `AGENTS.md`
- `SPEC/SPEC.md`
- `SPEC/APP_flow.md`
- `SPEC/TECH_STACK.md`
- `SPEC/BACKEND_STRUCTURE.md`
- `SPEC/FRONTEND_GUIDELINES.md`
- `SPEC/DESIGN.md`
- `progress.txt`
- `SPEC/progress.txt`
- `gotchas.txt`
- `UI-Frontend-design/` 中与 Import 密度和壳层相关的结构
- 2026-07-24 产品基准全文

实现追踪遵循项目真实调用链：

```text
AppShell / WorkspaceController
  -> useImportWorkflow / Import Store
  -> importV2Api
  -> Tauri thin commands
  -> ImportV2Service / CompileService / WikiService
  -> raw/ + wiki/ + .app/ + Git / tasks / OS secrets
```

Source 阅读侧另行追踪：

```text
WorkspaceRouter
  -> WikiView / WikiTree
  -> wikiStore
  -> Wiki commands
  -> generic wiki file operations
```

### 1.2 审阅的是当前工作树，不是 HEAD

审阅开始时工作区已经有大量 modified/untracked 文件，覆盖 Import V2、媒体、i18n、样式、规格、图谱和设计基准本身。本文：

- 不把这些改动归因于本次审阅；
- 不回滚、不覆盖用户已有改动；
- 以磁盘上的当前实现作为事实快照；
- 给出的行号是 2026-07-25 快照行号，后续修改后可能漂移；
- 需要未来 Agent 在每个 batch 启动时重新做一次窄范围证据确认。

### 1.3 审阅方式

本次采用三条并行证据线并最终合并：

1. 主审阅：产品基准逐节映射到真实前后端调用链；
2. 共享上下文子审阅：重点检查 Import 前端、Source 阅读、i18n、无障碍和批量体验；
3. 新鲜上下文子审阅：重点检查后端提交、Source registry、媒体/OCR/ASR、Compile 和测试闭环。

本文只记录能够由代码、类型、测试或缺失的生产调用证明的问题。对于“已有 helper，但生产路径没有调用”的情况，按“未实现生产能力”处理，而不是按“已实现但未接 UI”处理。

## 2. 执行结论

当前实现不满足 2026-07-24 产品基准的核心验收，暂时不能把“Import V2 能完成任务”解释为“Import / 来源库 / 媒体闭环已经成立”。

最关键的结论不是 UI 还需要打磨，而是几条底层语义仍然互相断开：

1. **所有 URL 导入在 commit 时被明确禁止写入 `wiki/sources/`。** 任务可以显示 committed，registry 也会产生 Source ID，但用户没有可读 Source。
2. **Source Markdown 合同没有在 commit 边界统一生成。** 本地文件通常只是原文归一化，网页使用另一套 snake_case/平台 ID frontmatter；稳定的 app `sourceId/versionId/contentHash` 没有进入最终 Source。
3. **Compile 仍读取旧 `.app/source-index.json`，不消费 V2 `.app/source-index-v2.json` 的稳定 Source 版本变更集。** Import 与 Compile 虽没有自动串联，但也没有形成基准要求的显式、可追踪串联。
4. **Import 仍实现完整 BYOK 解析恢复，且确定性解析失败后会按默认设置自动启动本地 Agent。** 前者突破 BYOK 边界，后者违反“用户主动触发本地 Agent”的硬约束。
5. **音视频仍有 `preview_without_transcript` 元数据预览路径。** 非 XHS 平台尤其可以把没有有效正文的媒体推进到可预览/可提交状态。
6. **Source 阅读、Source 右栏、AI 整理、版本时间线和 Source 原子删除基本尚未进入生产实现。**
7. **本地图片、音频、视频、字幕与结构化 Office/PDF 的真实生产覆盖远低于基准。** 多个 OCR/PDF/Office 规划 helper 只在单元测试中存在，没有被 orchestrator 调用。

因此建议实施顺序必须先修“成功意味着什么”和“Source 是什么”，再扩输入格式或重做 UI。否则新格式只会更快地产生不符合合同的成功结果。

## 3. 当前实现中应保留的基础

以下能力与基准方向一致，后续不应在重构时丢失：

- React 没有直接持有文件系统、Git、进程或 secret-storage 逻辑；前端仍通过 typed API 和 Tauri command 进入后端。
- Import session、item、attempt、quality、history 与 Source registry 使用 JSON/本地文件，而非数据库。
- 项目切换已有 `projectKey + epoch + sessionId` 保护，可抑制旧项目异步结果污染新项目。
- 未完成 session、后台 task 和部分进度可在页面恢复时重新挂接。
- task 有取消 token、日志和进度基础；近期 Bilibili 下载/ASR 已有较扎实的真实进度与取消实现。
- 队列的“当前查看行”和“待提交复选框”是两个独立状态。
- 后端 commit 采用逐项事务、hash-checked write 和部分成功模型，失败项不会回滚已成功项。
- Source registry 已有稳定 `source_id`、`version_id`、origin/locator、content hash 和版本数组的骨架。
- 更新 Wiki 文件时已有 hash 漂移检测和 Git checkpoint 基础。
- Web 路由已有 HTTP/browser/平台引擎、SSRF/redirect/DNS 约束、临时媒体工作区、连接器登录隔离和能力包框架。
- 字幕解析能够处理 SRT/VTT/ASS/部分平台 JSON，并保留时间戳。
- 中英文 Import V2 i18n key 数量一致；多数正式 dialog 使用统一 modal hook。
- Import、History、Capabilities 三个顶层区已经存在。
- 当前 Import V2 成功提交后没有自动调用 Compile；这一点符合“导入与编译分离”，后续应补显式入口而不是恢复自动编译。

这些优点说明当前不是“推倒重来”，而是必须把已有工程骨架重新收束到新的 Source 产品合同上。

## 4. 产品基准逐节对照

状态含义：

- `符合`：主生产路径已满足关键语义；
- `部分`：有实现骨架，但缺少关键闭环或仍有冲突分支；
- `不符合`：主路径直接违背基准；
- `缺失`：生产 UI/服务/命令基本不存在。

| 基准章节 | 状态 | 当前证据与结论 | 关联问题 |
| --- | --- | --- | --- |
| §1 设计结论 | 不符合 | URL commit 不写 Source；媒体可无正文预览；Source 阅读闭环缺失 | P0-01、P0-02、P0-05、P0-06 |
| §2 术语与用户文案 | 不符合 | 正常 UI 直出 route、engine、SHA、artifact、session ID、migration fingerprint、error code；仍使用 Keep Wiki 等旧模型 | P2-01、P2-07、P2-09 |
| §3 产品边界与不变量 | 不符合 | BYOK 是 Import 恢复；解析失败会自动启动 Agent；URL 成功不产生可读 Source；Compile 不消费稳定版本变更集 | P0-01、P0-03、P0-04、P0-07 |
| §4 数据模型与目录职责 | 不符合 | registry 有稳定 ID/版本骨架，但目录、Markdown 合同、compiled consumption、Source package 不完整 | P0-02、P0-04、P1-01、P1-04 |
| §5 导入会话与状态 | 部分 | 有持久 session 和 task 恢复；completed item 不移出工作队列、completed session 可继续加输入、无 resume-all/shard 语义 | P1-06 |
| §6 Import 页面构成 | 部分 | 三个区存在；输入不紧凑、缺 clipboard、能力 tile 不可操作、无批量待办、固定栏与预览错误 | P1-02、P2-01 至 P2-06 |
| §7 登录态流程 | 部分 | 有显式连接器登录与 revoke；无账号摘要、同平台全部恢复、受限内容首告/标签/导出提示 | P1-07 |
| §8 OCR | 不符合 | XHS 有局部 OCR continuation；standalone image/PDF selective OCR 未接生产，授权/结果边界不完整 | P1-04、P1-08 |
| §9 ASR 与字幕 | 不符合 | 平台字幕解析与 Bilibili 本地 ASR 可用；仍允许 metadata-only，缺本地媒体/伴随字幕/通用 ASR/无有效语音 fail-closed | P0-05、P1-03、P1-09 |
| §10 多语言 | 部分 | UI 有中英文；未发现翻译导入路径，符合“不翻译”；Source 语言检测与结果标注合同未完整落地 | P1-01、P3-03 |
| §11 本地文件与媒体 | 不符合 | 文本和现代 Office/PDF 有有限引擎；本地图片/音视频/字幕不在 discovery 格式集合，Office/Excel package 未落地 | P1-03、P1-04 |
| §12 网页与平台媒体 | 部分 | 分层 web、Bilibili/XHS、浏览器/能力包已有较多实现；默认永久保留、无正文预览、集合发现、媒体保留确认仍偏离 | P0-05、P1-09、P2-08 |
| §13 重复、更新、合并与删除 | 部分 | content/locator 去重与版本骨架存在；全局冲突策略、Source 三方合并、Source 原子删除缺失 | P1-05、P1-11、P2-03 |
| §14 提交完成与独立编译 | 不符合 | 没有完成摘要和两个动作；Compile 读取旧 index，无 sourceId+versionId consumption | P0-04、P1-10 |
| §15 Source 阅读页与 AI 整理 | 缺失 | Wiki 仍是通用阅读/编辑/Ask AI；没有 Source 专属右栏、候选、内容概览、时间线、重处理 | P0-06 |
| §16 Agent 导入修复 | 不符合 | 本地 Agent candidate 框架存在，但没有 `import-recovery` skill 合同；同时存在禁止的 BYOK 路线和失败后自动调用 | P0-03、P0-07、P1-12 |
| §17 后端职责与状态机 | 部分 | typed command/service/transaction 骨架较好；核心实体合同、自动/显式动作边界、commit finalization 不符合 | P0-01 至 P0-07、P1-01 |
| §18 错误与质量文案 | 不符合 | UI 先展示 raw message/code/technical fields，缺“发生什么、数据是否安全、下一步”稳定用户合同 | P2-07 |
| §19 验收矩阵 | 不符合 | 现有测试数量很多，但部分测试正在固化 URL 无 Source、metadata-only、BYOK Import 等反基准行为 | §12 测试迁移矩阵 |
| §20 文档维护 | 部分 | SPEC 已声明 2026-07-24 为 authority；旧 review/旧测试/旧类型仍表达先前产品模型 | P2-09、§14 批次 0 |

## 5. P0：先于所有扩展修正的产品不变量

### P0-01：URL / Web / 平台导入成功后不生成可读 Source

**基准要求**

每个成功导入项都必须同时拥有：

- `raw/` 原始证据；
- `wiki/sources/` 中可读的当前 Source；
- `.app/sources/` 中稳定身份、版本、别名和基线；
- 导入成功不得以之后的 Compile 来补 Source。

**当前证据**

- `src-tauri/src/services/import_v2/commit.rs:564`：
  `let writes_wiki = item.input.kind != ImportInputKind::Url;`
- `commit.rs:598-599` 只有 `writes_wiki` 时生成 `wiki_markdown`。
- `commit.rs:631-636` URL 的 commit result 返回 `wiki_path: None`。
- `commit.rs:752-762` 只有 `writes_wiki` 时写入最终 Wiki Source。
- `commit.rs:2308-2324` 的测试明确断言 URL commit 后 `wiki_path.is_none()` 且 manifest 指向的 Wiki 文件不存在。

**为什么是 P0**

这是“任务成功但产品结果不存在”。用户可能看到已提交、历史记录和 Source ID，却无法在 Wiki 中阅读、编辑、引用、AI 整理或编译这个来源。所有网页、Bilibili、XHS、知乎、微信等 URL 路线均受影响。

**目标状态**

- commit 的成功定义统一为 `raw + final Source Markdown + manifest/index/history` 原子成立；
- URL 与本地文件使用同一个 Source finalization 边界；
- 任何无法产生有效正文的 URL item 不得 committable；
- URL 的最终路径按基准进入 `wiki/sources/web/<host>/...`；
- 测试改为断言 URL 成功一定产生可读 Source，而不是断言“不写 Wiki”。

**首要修改入口**

- `src-tauri/src/services/import_v2/commit.rs`
- `src-tauri/src/services/import_v2/source_registry.rs`
- `src-tauri/tests/import_v2_web_ingestion.rs`
- `src-tauri/src/services/import_v2/commit.rs` 内联测试
- 所有平台 end-to-end commit fixtures

**最小验收**

1. 普通网页、Bilibili 有字幕、XHS 图文 OCR 成功各导入一次；
2. 每项 success 都能用返回的 `wikiPath` 打开 Markdown；
3. Markdown frontmatter 的 app `sourceId/versionId/contentHash` 与 manifest 完全一致；
4. 删除 staging 后 Source 仍可读；
5. 不运行 Compile，Source 仍然存在。

### P0-02：最终 Source Markdown 没有统一、稳定、可验证的合同

**基准要求**

最终 Source 至少包含 `type/sourceId/versionId/sourceKind/title/platform/canonicalUrl/author/publishedAt/importedAt/contentHash/quality`，并以忠实归一化正文为主，不是 AI 摘要。

**当前证据**

- `src-tauri/src/services/import_v2/native_file_engine.rs:74-101` 将本地 MD/TXT/CSV/HTML 归一化后原样写入 staging `document.md`，没有 Source frontmatter。
- `native_file_engine.rs:194-224` 的结构化文件路径同样只产出 extraction Markdown。
- `src-tauri/src/services/import_v2/generic_web_engine.rs:943-962` 使用旧的 snake_case 合同：`source_url/source_platform/content_type/engine_id/source_id`。
- 其中 `source_id` 是平台内容 ID，不是 commit 时生成的 app stable Source ID。
- `src-tauri/src/services/import_v2/commit.rs:598-599` 只重写资源链接，没有在 commit 边界注入稳定 Source 元数据。
- `src/types/wiki.ts` 的前端 page meta 没有稳定 Source/version/quality 模型，导致本地来源甚至可能不被识别为 Source page。

**连带风险**

- 同一个 `source_id` 字段在平台与 app 中含义冲突；
- 本地来源无法可靠进入 Source reader mode；
- Source 文件被外部编辑后，无法从 frontmatter 校验 manifest 绑定；
- Compile、AI 整理、删除和时间线无法只依赖一个稳定合同；
- 未来每种引擎会继续生成不同 Markdown 方言。

**目标状态**

引擎只负责产生受验证的 `SourceCandidate`：

```text
candidate markdown body
candidate metadata
raw/source snapshot
assets
quality
provenance
```

commit 在分配 `sourceId/versionId` 后统一生成最终 frontmatter、资源链接和目标路径。平台 ID 必须使用独立字段，例如 `platformContentId`，不能复用 app `sourceId`。

**最小验收**

- 本地 Markdown、CSV、普通网页、平台图文、平台视频产生同一组必填 key；
- frontmatter round-trip 后与 manifest/index 一致；
- `sourceId/versionId/contentHash` 不由 engine 伪造；
- frontmatter 不包含 cookie、token、临时 staging 路径或内部 session ID；
- 任意必填字段不一致时整项 commit 原子失败。

### P0-03：Import 阶段仍有完整 BYOK 解析恢复路径

**基准要求**

BYOK 只在可读 Source 已经存在之后用于 AI 整理、Compile 和 Chat。它不是 Import parser、OCR/ASR 备援或恢复路线。

**当前证据**

前端：

- `src/features/import/importStatusPresentation.ts:74-77,133-135`
- `src/features/import/ImportAgentControls.tsx:11-16,90-106`
- `src/features/import/ImportV2Dialogs.tsx:85-98,172-180`
- `src/features/import/useImportSupportingActions.ts:124-177`
- `src/services/importV2Api.ts`

后端：

- `src-tauri/src/services/import_v2/agent_assistance.rs:41-61` 保存 pending BYOK approvals；
- 同文件 `preview_byok_scope`、`start_byok`、`run_byok` 形成完整远程处理路线；
- `src-tauri/src/commands/import_v2_agent_commands.rs:105-150` 暴露 preview/approve/start commands；
- `src-tauri/tests/import_v2_byok_assistance.rs` 等测试固化该路线。

**目标状态**

- 删除 Import item 的 `request_byok` action、dialog、API、command 和 backend execution；
- 不要只隐藏按钮而保留可被直接 invoke 的生产 command；
- BYOK provider/secret 能力继续用于 Source AI 整理、Compile、Chat；
- Import 失败只允许：确定性解析、已声明能力、显式本地 Agent recovery、或明确失败/等待；
- 清理后做一次 command registration 与 dead-code 审计。

### P0-04：Compile 仍依赖旧 Source index，未消费 V2 稳定版本

**基准要求**

Import 完成后产生 `sourceId + versionId` 变更集；用户显式点击“更新 Wiki”后，Compile 只消费确认过的 Source 当前版本并记录消费关系，且绝不改写 `wiki/sources/`。

**当前证据**

- `src-tauri/src/services/compile_service.rs:115-120` 固定读取 `.app/source-index.json`。
- `compile_service.rs:123-146` 允许旧 `raw/extracted/*.md` 和 `wiki/sources/*.md` 混合输入。
- `compile_service.rs:41-52,96-107` 的 prompt 构建会枚举 `raw/extracted` 与整个 `wiki`。
- `src-tauri/src/models/compile.rs:100-107` 的 `CompileRequest` 只有 project/route/agent/provider，没有 Source 变更集。
- `CompileResult` 只有 affected paths/conflicts/checkpoint，没有 consumed Source versions。
- V2 registry 实际写在 `.app/source-index-v2.json`，两套索引没有可靠桥接。

**影响**

- 新 Import V2 Source 可能无法进入 Compile；
- Compile 可能继续消费过期 legacy extraction；
- 无法解释某个 Wiki 页面是基于哪个 Source version 生成；
- 重复 Compile、增量 Compile、Source 更新提示和删除引用检查都没有可信数据。

**目标状态**

- Compile 输入改为显式 `SourceVersionRef[]`；
- 后端从 V2 registry 解析并校验每个 `sourceId/versionId/wikiPath/contentHash`；
- 记录 compile consumption 到 `.app/compile/`；
- prompt 只读目标 Source 和必要的现有派生 Wiki 页面；
- 写入守卫继续禁止 `wiki/sources/**`；
- legacy project 兼容必须是单独、可测试的适配层，不能继续作为主索引。

### P0-05：音视频可以通过“无转写预览”绕过有效正文要求

**基准要求**

音视频优先可靠字幕，其次本地 ASR。两者都没有有效文本时不得写 raw、不得生成 Source、不得提供 metadata-only preview。没有有效语音也必须 fail closed。

**当前证据**

- `src-tauri/src/models/import_v2.rs:249-295` 定义并从 web error 派生 `PreviewWithoutTranscript`。
- `src-tauri/src/services/import_v2/orchestrator.rs:2510` 执行该 recovery action。
- `orchestrator.rs:3748` 的测试允许 Bilibili 走该动作。
- `src-tauri/tests/import_v2_web_ingestion.rs:191` 覆盖该路线。
- `src-tauri/src/services/import_v2/subtitle.rs:16-26` 专门生成“仅包含视频元数据与简介”的 Markdown 提示。
- `src/features/import/importStatusPresentation.ts:79-96` 向用户暴露该动作，仅对部分 XHS 情形额外过滤。

**目标状态**

- 删除前后端 `PreviewWithoutTranscript`；
- quality gate 必须以有效 transcript/body 为 committable 前置条件；
- “无有效语音”与“能力缺失/失败”分开呈现，但两者都不生成 Source；
- 平台 metadata 可以保留在临时诊断证据中，不能被包装成正文；
- 重写正在固化 metadata-only 的测试。

### P0-06：Source 阅读、AI 整理和 Source 右栏尚未形成产品能力

**基准要求**

Source 仍在 Wiki 阅读，但应有 Source 专属工具栏、AI 整理、内容概览、候选稿、当前/原始版本、质量问题、重处理、版本线和技术日志。

**当前证据**

- `src/features/wiki/WikiView.tsx` 的工具栏只有通用 read/edit/HTML preview/export/Ask AI/bookmark。
- `src/features/wiki/WikiView.tsx:534-561` 使用通用 `MarkdownReader`。
- `src/types/wiki.ts:15-31` 的 `WikiPageMeta` 没有 `sourceId/versionId/sourceStatus/quality`。
- `src/features/wiki/wikiStore.ts` 只提供通用 scan/read/save/rename/delete/conflict。
- `src/components/app/RightContextPanel.tsx:269-317` 对 Wiki 使用通用 related pages / page chat，没有 Source 专属面板。
- 没有 Source AI 整理 command/service/task/candidate/time-line DTO。

**目标状态**

该项不是在现有 Ask AI 按钮上改文案。需要完整闭环：

1. 从 final Source contract 可靠识别 Source；
2. 启动对话框只选择范围、路线和模型；
3. 输入严格限制为当前 Source、元数据、OCR/ASR/字幕和 Source 资产引用；
4. 后台任务可取消、可恢复，同一 Source 同时只能一个；
5. 结果始终是绑定 `sourceId/versionId/contentHash` 的 candidate；
6. diff/三方合并/确认/checkpoint 后生成新版本；
7. 右栏时间线同时记录导入、更新、重处理、AI 整理、恢复。

### P0-07：确定性导入失败后会自动启动本地 Agent

**基准要求**

Import recovery 只能使用本地 Agent，并且必须由用户主动触发。用户未选择 Agent 时，普通解析失败只能进入明确失败/等待状态，不能因为项目设置了默认 Agent 就自动执行 CLI。

**当前证据**

- `src-tauri/src/commands/import_v2_commands.rs:349-363` 在后台运行普通 `run_item_with_recovery`；
- 同文件 `:364-380` 捕获任意确定性 run error 后读取 project settings；
- 只要 `settings.agent_default` 存在，`:369-377` 就立即调用 `run_local_agent_candidate(... DeterministicHardFailure ...)`；
- `:438-455` 创建并启动 Agent assistance task；
- 该行为发生在普通 start/retry 命令内部，不需要用户点击“使用本地 Agent”。

**风险**

- 用户可能在没有二次意图的情况下启动 CLI、读取当前 item 证据、执行临时脚本或消耗本地计算资源；
- UI 的失败状态与后台实际动作不一致；
- cancel/retry 的语义难以解释；
- 即使 Agent staging/候选隔离做得正确，也不能替代显式授权。

**目标状态**

- 删除普通 run error 分支中的自动 Agent 调用；
- 失败 issue 只返回 `invoke_local_agent` 这一显式可用动作；
- 用户点击后才创建 Agent task，并在启动前展示范围、能力、可写边界和取消语义；
- 保留现有 staging/candidate/provenance/quality/diff/confirmation 安全骨架；
- 增加回归测试：有默认 Agent 时普通解析失败也不会启动 Agent task。

## 6. P1：核心闭环和数据生命周期缺口

### P1-01：Source registry 只有版本骨架，没有完整逻辑来源模型

**已有**

`src-tauri/src/services/import_v2/source_registry.rs:135-158` 已有：

- `source_id`
- `origins`
- `versions`
- `current_version_id`
- `wiki_path`
- 每版本 content hash/raw/extracted/baseline/route/engine/quality

**缺失或偏离**

- 没有 compiled consumption record；
- 没有规范化的 `sourceKind/platform/canonicalUrl/title/author/publishedAt/importedAt`；
- 没有受限/私有内容标记与安全账号摘要；
- 没有用户编辑 checkpoint/hash 与 AI candidate 关系；
- 没有重处理/恢复事件时间线；
- `source_registry.rs:400-434` 对所有输入统一使用 `raw/sources/{source}/{version}` 和 `raw/extracted/...`；
- 基准要求网页证据进入 `raw/web/`，资源进入 `raw/assets/`，本地媒体原件才进入 `raw/sources/`；
- `derive_wiki_path` 在 `source_registry.rs:656-670` 只分 `files` / `web`，没有基准的 `local` 与 `web/<host>` 组织。

**建议**

先写一个版本化 migration 计划，再演进 manifest；不得让代码一半读旧字段、一半读新字段。兼容适配器必须是 read-only、可删的边界。

### P1-02：缺少文本 / Markdown 粘贴输入

**证据**

- `src/types/importV2.ts:15` 只有 `file | folder | url`；
- `src-tauri/src/models/import_v2.rs:17-21` 同样只有三类；
- `src/features/import/ImportSourceMethods.tsx:148-225` 只有文件/文件夹和 URL；
- 当前 clipboard 动作只是复制 locator，不是创建输入。

**目标**

- 增加明确的 `clipboard_text`（命名可调整，但不能伪装成 file）；
- 先展示识别路线/标题/Markdown preview，再创建或加入 item；
- 仍走统一 session、quality、duplicate、commit；
- plaintext snapshot 的证据与隐私边界需要在 manifest 中明确；
- 不做全局静默监听粘贴。

### P1-03：本地图片、音频、视频和字幕没有进入生产 discovery

**证据**

- `src-tauri/src/models/import_v2_file.rs:5-14` 的 `FileFormat` 只有 Markdown、DOC/DOCX、XLS/XLSX、PPT/PPTX、PDF。
- `src-tauri/src/services/import_v2/file_discovery.rs:278-294` 只识别文本、PDF、OLE、OOXML。
- `file_discovery.rs:455-467` 把 TXT/CSV/HTML 都折叠成 Markdown，其余图片/媒体/字幕返回 unsupported。
- `native_file_engine.rs:35-51` 只支持 MD/TXT/CSV/HTML。
- `media_router.rs` 和 `subtitle.rs` 虽存在，但主要被 web/platform continuation 使用，不构成本地 file discovery 路线。
- `orchestrator.rs:2602-2632` 虽声明了少量 `mp3/wav/m4a/mp4/mov/mkv` 与字幕 route，但 path scanner 在更早阶段已经拒绝这些文件，因此声明不可达；同时仍缺完整图片、音频、视频格式组。
- `file_discovery.rs:271-294` 以扩展名选择 magic/container 检查，内容与扩展名不一致时直接 unsupported，没有落实“内容识别优先”。
- folder discovery 产生的 `relative_path` 没有稳定传入最终 `ImportInput`，无法在预览/Source 中忠实保留相对路径信息。

**目标**

统一前后端 format union 与内容探测，覆盖基准完整矩阵。必须按内容/magic/container 判断，不得只依赖扩展名；错误扩展名应按可信内容分类或给出“检测到的真实格式”，而不是静默接受或简单拒绝。Folder import 应保留 relative path、报告 unsupported 汇总，但不创建 folder Source。

### P1-04：PDF/OCR/Office/Excel 的“设计 helper”没有接入生产 orchestrator

**关键证据**

符号使用扫描显示：

- `inspect_pdf`、`plan_pdf_pages` 只存在于 `pdf_router.rs` 和 `src-tauri/tests/import_v2_pdf_routes.rs`；
- `OcrRouter` 只存在于 `ocr_router.rs` 和 `src-tauri/tests/import_v2_ocr.rs`；
- `WorkbookPlan` / `PresentationPlan` 只存在于 `office_postprocess.rs` 和 `src-tauri/tests/import_v2_office_quality.rs`；
- 生产 orchestrator 没有调用这些 helper。

当前 `NativeStructuredFileEngine` 只调用通用 extraction service：

- `native_file_engine.rs:194-198`：PDF 与 OOXML 直接提取 Markdown；
- 质量 metadata 中多个结构化指标为 `None`；
- 没有 PDF 页级 native/OCR 路由、加密 PDF 显式状态、页选择；
- 没有 Excel index + sheet subpage/CSV chunk 来源包；
- 没有公式显示值与公式证据；
- 没有超大表格预确认和零截断合同；
- 没有 PPT notes/meaningful images 的生产保证。

**目标**

不要继续为未接生产的 helper 追加单元测试。下一步必须建立从 discovery -> route plan -> execution -> candidate/source package -> commit 的真实集成测试。

### P1-05：重复 / 更新 / 合并仍使用旧的 Wiki 冲突模型

**证据**

- `CommitConflictAction` 只有 `create_new / keep_wiki / apply_merged_candidate`；
- `src/features/import/ImportCommitBar.tsx` 暴露全局冲突策略；
- `src/features/import/ImportView.tsx:77-78,179-189` 将全局 decision 批量赋给 ready items；
- `commit.rs:575-595` 以现有 Wiki file/hash 为主要冲突判断；
- `KeepWiki` 可能让 manifest 版本前进而可读 Source 仍保留旧内容，形成“current version 与 current Source 不一致”的风险；
- Agent candidate diff 不能替代正常 Source 更新的三方 diff。

**目标**

每个 item 明确派生：

```text
new_source
exact_duplicate_skip
same_source_new_version
keep_current_source
apply_import_candidate
manual_merge
```

所有选择都必须绑定稳定 Source ID、候选 hash、当前 Source hash 和目标 version；固定栏不得替用户决定冲突。

### P1-06：会话结束、completed item 和重启恢复语义不符合基准

**证据**

- `src/features/import/useImportSessionScope.ts:157-161` 只要 store 中项目/session 相同就复用；
- 新 session 通常只在 bootstrap 找不到 unfinished session 时创建；
- 后端 `create_session` command/service 本身没有用 `find_unfinished_session` 强制“一项目一个活跃 Import session”，绕过前端即可创建多个活跃 session；
- `src-tauri/src/services/import_v2/session_store.rs:214-233` 的 `add_inputs` 会向现有 session 追加 item，没有拒绝 completed session 或创建新 session；
- committed items 仍留在 `session.items`，没有移入只读完成摘要；
- `Paused` 枚举存在，但没有明确的 session-level `resumeAll`；
- restart reconciliation 会把部分缺失/失败 task 对应的 Waiting/Inspecting/Extracting/Validating item 转成 Failed 并清理 staging，而不是统一进入 Paused、保留已完成 shard 并等待 Resume All；
- 未发现生产路径把重任务 shard/中间产物建模为“可复用 / 需重跑 / 已完成”；
- tab 和滚动位置没有 project-scoped 恢复。

**目标**

- 无未提交项后 session 结束；
- completed items 从活跃队列移到摘要/历史投影；
- 下一次 Add 必须创建新 session；
- restart 将重任务置为 paused，并提供“继续全部”；
- 已完成 shard 必须复用，取消则清理临时产物并从头重试；
- 活跃 tab、filter、队列和右栏滚动位置按项目恢复。

### P1-07：登录只恢复单项，没有账号摘要和受限内容闭环

**已有**

- connector profile 不进入 React；
- 有显式 begin/check/revoke；
- cookie/token 没有直接展示；
- 登录窗口关闭不会被伪装成成功。

**缺口证据**

- `ConnectorSessionRef` 在 `connector_session.rs:26-31` 只有 session/platform/profile_ref/state；
- `ImportLoginDialog.tsx:89-103` 显示 connector/domain/state/session ID，没有头像、昵称、last verified；
- `useImportSupportingActions.ts:271-287` 登录成功后只 `startItems([itemId])`；
- `import_v2_web_commands.rs:223-234` 只绑定/释放当前 item；
- 当前 `release_item_after_login` 将单项从 `WaitingLogin` 释放到可重试失败态，再依赖前端重启该项，而不是后端原子地恢复同平台全部 waiting items；
- 未发现同平台等待项批量恢复；
- 未发现项目 session 首次受限内容警告、Source 标签和 export warning。

**目标**

一次登录成功后按平台恢复当前 session 中全部等待项；取消登录保持等待而不是失败。账号 DTO 只返回安全摘要，不返回 cookie/profile path。

### P1-08：OCR 授权、结果和 standalone image 边界不完整

需要分别实现并测试：

- standalone image：没有有效 OCR 文本则不写 raw/Source；
- PDF：只 OCR 无文本/乱码/图像页，而不是整份重跑；
- XHS 图文：OCR 只补齐图片中缺失正文，不做 AI 总结；
- video frame OCR：只在 transcript 失败且后端检测到明显文字帧时提供；
- session 内一次性 OCR 授权与批量授权；
- OCR 结果的页/图定位与 quality issues；
- 失败时 Remove / Retry / Local Agent 三个明确动作。

当前 XHS continuation 可以保留，但必须并入统一 OCR route contract，不能成为平台特例旁路。

另外，`orchestrator.rs:2491-2502` 在 XHS URL 且 OCR route 已注册时会直接返回已授权状态；这与“当前 Import session 首次使用 OCR 前由用户一次性授权”的基准不一致。后续应保存明确的 session-scoped authorization，而不是把“能力已安装”当作“用户已授权本次 OCR”。

### P1-09：ASR 仍是 Bilibili/Web 特例，未成为通用媒体能力

**证据**

- Bilibili local ASR continuation 已经具有真实进度、取消和 temporary media 安全约束；
- orchestrator 的授权消费和 continuation 主要围绕特定 web media target；
- 本地音视频、伴随字幕、独立字幕 file discovery 尚未接入；
- transcript 没有统一 30–60 秒锚点策略；
- `media_router.rs:25-31,125-130` 将平台人工字幕与本地人工字幕同级，把 automatic 排在 embedded 前，缺少基准要求的语言、来源和可靠性优先级；
- `subtitle.rs:46-53` 可解析 VTT/SRT/ASS/SSA/JSON，但没有 LRC，尽管其他 route/readiness 已宣称 LRC；
- 没有有效语音的 fail-closed 尚被 metadata-only 分支破坏；
- ASR 首次启用对话框没有完整展示模型、设备、依赖链、预计资源和偏好。

**目标**

抽取通用 `MediaTranscriptPlan`，让本地媒体与 web media 共用“字幕优先 -> local ASR -> no Source”的门禁，同时保留不同来源的下载/证据策略。

### P1-10：提交后没有完成摘要、Source 导航和显式“更新 Wiki”

**证据**

- `useImportTaskCoordinator.ts:352-359` 确认任务结束后主要清理 confirming 并刷新 session；
- `useImportWorkflow.confirm` 只启动 commit task；
- `ImportView.tsx` 没有 commit completion summary；
- history 主要提供 open result / logs，没有 Source version change set；
- 没有“查看已导入来源”和“更新 Wiki”两个并列但独立的动作。

**目标**

commit task 的 typed result 至少包含：

```text
new[]
updated[]
duplicatesSkipped[]
warnings[]
failed[]
sourceVersionChanges[{sourceId, versionId, wikiPath, contentHash}]
```

部分成功也必须返回精确集合；“更新 Wiki”只消费成功且用户确认的 changes。

### P1-11：Source 仍可被普通 Wiki 新建、改名、移动和删除

**证据**

- `src/features/wiki/WikiTree.tsx:276-293` 对所有文件统一暴露 rename/delete；
- `src/features/wiki/WikiView.tsx:249-271` 与 `src/features/wiki/wikiStore.ts:439-468` 使用通用 Wiki delete；
- `src/features/wiki/WikiPageFormDialog.tsx` 允许普通新建/保存 Markdown，没有 Source registry 边界；
- 没有 Source package/reference/free-space delete preview。

**风险**

只删 `wiki/sources/*.md` 会留下 registry/raw/assets/version；普通改名会破坏 manifest `wiki_path`；普通新建可伪造 `type: source` 绕过 Import。

**目标**

- 禁止普通新建 Source；
- Source rename/move/delete 使用专用 `sourceId` 命令；
- 删除前展示所有路径、版本、引用和预计空间；
- checkpoint、audit、事务 rollback 必须覆盖整个 Source package；
- 专用流程完成前先隐藏通用危险动作。

### P1-12：本地 Agent recovery 没有落实基准的专用 skill 合同

**已有**

- candidate workspace、provenance、hash 校验和 staging 限制已有工程骨架；
- 本地 Agent 与 deterministic candidate 有 diff/选择概念。

**缺口**

- 全仓未找到 `import-recovery` skill 名称或对应稳定合同；
- 当前 ImportAgentControls 同时混入 BYOK；
- 用户动作与 Agent 能读取的最小输入、允许工具、输出 schema、重启/取消语义没有按新基准收束；
- 正常更新/三方合并与 Agent recovery candidate 的边界混杂。

**目标**

建立专用 `import-recovery` skill/protocol，只读当前 item 证据、只写 staging candidate；禁止 direct raw/wiki/git/secret/network provider；安装命令永不静默执行。

### P1-13：平台覆盖与集合发现仍是不完整的点状实现

**已有**

- generic Readability/browser；
- WeChat、Zhihu、Bilibili、XHS、Douyin 等 platform route；
- capability-pack、browser session、受控 remote asset 和平台证据解析基础。

**缺口**

- `orchestrator.rs:2537-2595` 的 platform routes 中，X/Twitter 明确没有可用 route；
- readiness 将 X 等能力标为后续阶段，而当前产品基准要求 Import 主流程能够明确处理支持/缺能力/本地 Agent 恢复，而不是静默没有路径；
- 没有 collection/playlist/author page 的独立 discovery entity/command；
- 当前 URL 会直接进入单 item 处理，不能先展示集合子项、让用户选择后再创建任务；
- 复合平台内容的正文、图片、媒体、字幕和受限标记没有统一组合合同。

**目标**

平台接入应共享 Source finalization、登录、OCR/ASR、remote retention 和 quality gate；平台 provider 只负责发现与证据，不应各自定义成功含义。集合必须先 discovery preview，再按用户选择创建 child items。

## 7. P2：Import 工作台、预览、状态与文案差距

### P2-01：固定确认栏展示错误信息，并提供禁止的全局冲突策略

当前 `ImportCommitBar.tsx`：

- 只显示 selected/unresolved；
- 有全局 `<select>`；
- CTA 是泛化“确认导入”；
- 没有新增、更新、警告、待处理计数。

应改为：

```text
新增 N · 更新 N · 警告 N · 待处理 N
[导入到来源库 N 项]
```

所有冲突都在 item 内解决。固定栏只聚合，不作策略选择。

### P2-02：能力矩阵不可点击、不可键盘操作、不能完成管理动作

- `ImportSourceMethods.tsx:283-309` 使用 `<span>`；
- 没有 focus/Enter/popover/Escape/outside close；
- `ImportCapabilitiesPanel.tsx:40-98` 是只读清单；
- 用户看得到缺能力，但未必能从矩阵完成安装、更新、配置或查看原因。

应使用原生 button，格子只显示 icon/name/status dot；详情放 popover，并遵守显式安装确认。

### P2-03：没有按用户动作聚合的批量待办

`ImportBatchStatus.tsx` 聚合的是后台 task completed/active/failed/cancelled，不是基准要求的：

- 某平台需要登录 N 项；
- 需要 OCR N 项；
- 需要 ASR N 项；
- 需要安装能力 N 项。

应从 item blocking reason 生成 action group，并让一次成功动作恢复该组全部适用项。

### P2-04：队列没有“一项主动作 + …”的层级

当前多项 action 和复制 locator 长驻行内。应：

- 每种状态只显示一个主动作；
- Retry/Login/Install/Resolve/Preview 按状态竞争主动作；
- Skip/Cancel/Copy/Logs/Preserve media/Technical details 收进 `…`；
- 保留 row selection 与 commit checkbox 的独立语义。

### P2-05：两级预览不完整并泄露内部标识

当前：

- Import 右栏没有真正快速 Markdown preview；
- 全量 preview 直接显示 session/item/candidate ID 与 SHA；
- 图片被省略；
- 缺 final target path、Source/version、资源列表、quality 和 normal update diff。

目标：

- 右栏快速预览最终可读结果；
- 全量 preview 展示 Markdown、图片/资源、来源信息、目标路径、质量、版本；
- 内部 ID/hash/engine/path 只在折叠技术详情；
- update 默认展示 current/imported/merged 三方关系。

### P2-06：用户状态没有归并为七类

后端可保留细状态，但当前 `importStatusPresentation.ts` 将约 15 个内部阶段直接映射为顶层 UI。应新增稳定的 `ImportUserState` 派生层，队列筛选、颜色、图标、统计与空状态全部只使用七类；细阶段作为副标题或日志。

### P2-07：错误信息先显示 code 和技术 message

证据入口：

- `ImportRightPanel.tsx:63-111`
- `ImportQueue.tsx`
- `ImportHistoryDetailDialog.tsx`
- `useImportSessionScope.ts:15-23`

用户主路径应先回答：

1. 发生了什么；
2. 已有数据是否安全；
3. 下一步能做什么。

`technicalCode/technicalMessage/route/engine/hash/artifactPath` 只进折叠详情或日志。建议 issue DTO 明确区分用户合同与诊断合同。

### P2-08：远程媒体的默认值和保留时机相反

**证据**

- `src-tauri/src/models/import_v2.rs:23-37` 的 `MediaSaveMode::default()` 是 `PreserveOriginal`；
- `ImportSourceMethods.tsx:115-145,255-275` 在 URL 入队前猜测媒体并强制用户选择 preserve/extract；
- 没有真实 size、disk impact 和 quality。

**目标**

- 远程媒体默认 `ExtractOnly`；
- 发现确为媒体后，在 item `…` 提供“保留远程原件”；
- 后端先给预计大小、磁盘空间、质量信息；
- 文本/字幕/ASR 成功与远程原件保留失败互不回滚。

### P2-09：用户可见迁移工程 UI 不属于正常 Import 工作台

`ImportMigrationNotice.tsx` / `ImportMigrationDialog.tsx` 将 dry-run、fingerprint、legacy inventory、checkpoint、apply/resume 暴露给正常用户。基准顶层只有工作台/历史/能力。

后端兼容迁移可以保留，但正常用户工作台不应要求理解这些术语。若旧项目确需一次性迁移，应做单独的兼容入口和用户语言，不应占据每次 Import 的产品模型。

### P2-10：输入区不够紧凑，缺少文本入口与清晰发现反馈

`ImportSourceMethods.tsx:148-225` 使用两个较大的 article pane；能力矩阵也嵌在输入区域。建议按基准重构为紧凑输入条：

- 文件；
- 文件夹；
- URL；
- 粘贴文本/Markdown；
- 当前发现/扫描进度。

长目录扫描必须在主工作台显示已发现、已跳过、当前阶段和 Cancel，而不是只依赖全局任务抽屉。

### P2-11：登录/能力恢复不是批量的

除 §P1-07 外，能力安装、OCR/ASR 授权后也应采用 grouped resume。当前多为刷新当前 item 或让用户逐项 Retry，会把“一个系统能力问题”变成 N 次操作。

### P2-12：页面 UI 状态与键盘语义不完整

- active tab 是组件本地状态，离开 Import 后重置；
- 工作台/历史/能力各自滚动位置不保存；
- capability tile 看似可点但只是 span；
- dropzone 有 pointer/focus 外观但无完整 Enter/Space 行为；
- 媒体预选择弹窗未使用统一 modal hook；
- Workspace header 与 Import header 可能形成重复主标题；
- queue 大范围 `aria-live` 需要避免整列表反复播报。

## 8. P3：优化与完成感

### P3-01：Import right panel 应从诊断面板改成“下一步助手”

首屏顺序建议严格遵循基准：

1. 来源/状态；
2. 当前唯一主动作；
3. 快速候选预览；
4. 最终路径与 Source/version；
5. quality/issues；
6. 原始资料；
7. 技术详情/attempt timeline/log。

### P3-02：i18n key 完整，但内容模型已过时

需要删除/改写：

- BYOK Import recovery；
- metadata-only preview；
- global conflict policy；
- remote media preselection；
- migration engineering UI。

需要新增：

- 文本/Markdown 粘贴；
- 七类用户状态；
- 新增/更新/警告/待处理计数；
- 完成摘要；
- 查看已导入来源 / 更新 Wiki；
- Source AI 整理、版本线、重新处理、删除影响；
- 受限内容与导出提示。

### P3-03：多语言与 Source 语言需要分层

UI 语言切换不应改变 Source 内容语言。导入只检测/标注语言，不翻译；AI 整理若未来支持目标语言，也属于 Source 已存在后的显式 AI 动作。

### P3-04：平台/格式/质量 icon 与文案应来自 typed model

不要在组件内通过字符串 contains 猜媒体或平台。后端 discovery/route 应返回稳定 `sourceKind/platform/contentKind/capabilities/quality`，前端只做展示派生。

## 9. 建议的目标后端合同

以下不是要求一批全部实现，而是避免各 batch 继续发明不兼容 DTO 的共同方向。

### 9.1 Import input

```ts
type ImportInputKind =
  | "file"
  | "folder"
  | "url"
  | "clipboard_text";
```

输入 DTO 至少区分：

- 用户原始 locator/display name；
- 后端 normalized locator；
- source kind/content kind；
- local/remote；
- media save preference（remote 默认 extract-only）；
- immutable source identity；
- safe preview text，而不是把敏感全文塞进通用日志。

### 9.2 Candidate 与最终 Source 分离

```text
Import engine output = SourceCandidate
Commit output        = SourceVersion + Final Source Markdown
```

engine 不分配 app stable IDs；commit 分配并注入。

### 9.3 Source manifest

建议至少覆盖：

```text
sourceId
sourceKind
currentVersionId
wikiPath
aliases/origins
canonicalUrl/platform/platformContentId
title/author/publishedAt/importedAt/language
versions[]
  versionId
  contentHash
  rawEvidence[]
  assets[]
  baselinePath
  candidate/provenance/quality
  createdAt
  humanEditHash/checkpoint
compiledConsumptions[]
restrictedContent
timeline[]
```

具体 schema 需单独版本化设计和 migration，不要直接原地增加一批 optional 字段后宣称完成。

### 9.4 Import user state 与 technical state 分离

后端 item 可保留细状态；presentation DTO 另行派生七类用户状态、主动作、阻塞组、可提交性和用户错误。前端不得从 error string 猜主动作。

### 9.5 Commit completion

```ts
interface ImportCompletion {
  sessionId: string;
  batchId: string;
  newSources: SourceVersionChange[];
  updatedSources: SourceVersionChange[];
  duplicateSkips: DuplicateResult[];
  warnings: UserIssue[];
  failures: ItemFailure[];
}
```

这份结果同时驱动：

- 完成摘要；
- “查看来源”；
- 显式“更新 Wiki”；
- History；
- 部分成功重试；
- Compile changeset。

### 9.6 Compile request

```ts
interface CompileRequest {
  projectId: string;
  projectRootPath: string;
  sourceVersions: Array<{
    sourceId: string;
    versionId: string;
    contentHash: string;
  }>;
  route: "auto" | "agent" | "byok";
}
```

后端必须重新从 registry 校验，不信任前端 path。

### 9.7 Source lifecycle commands

应有专用：

- `get_source_detail`
- `preview_source_update`
- `apply_source_candidate`
- `list_source_versions`
- `restore_source_version`
- `preview_delete_source`
- `delete_source`
- `reprocess_source_ocr/asr/refresh`
- `start_source_ai_organize`

所有高风险 apply/delete/restore 需要 checkpoint + affected paths + hash guards。

## 10. 建议的目标前端信息架构

### 10.1 Import workbench

```text
Header / tabs
  工作台 | 历史 | 能力

Compact input
  File | Folder | URL | Paste text/Markdown

Capability status strip
  icon + name + status dot

Grouped todos
  Login / OCR / ASR / capability

Continuous queue
  checkbox | type | title | 7-state | one primary action | ...

Fixed commit bar
  new | update | warning | unresolved | Import to Sources N

Right inspector
  user state -> primary action -> preview -> target/version -> quality
  -> original -> collapsed technical details
```

### 10.2 Completion state

完成项不继续占据 active queue。展示：

- 成功新增/更新/重复/警告/失败；
- 可点击 Source；
- Retry failures；
- 查看已导入来源；
- 更新 Wiki。

### 10.3 Source reader

只有 `type: source` 且 registry binding 有效时进入 Source mode。普通 Wiki 页面不出现 Source lifecycle/AI 整理动作。

## 11. 格式与媒体生产覆盖矩阵

后续实现 Agent 应把下表变成真实 end-to-end fixtures，而不是只补 enum：

| 类别 | 基准格式 | 当前生产状态 | 必须补的关键行为 |
| --- | --- | --- | --- |
| Markdown/TXT/HTML | MD/TXT/local HTML | 部分 | 标准 Source frontmatter、HTML sanitize、local asset copy |
| CSV | CSV/TSV-like large tables | 低配 | 零截断、超大预确认、chunk/package、表格质量 |
| PDF | PDF | 低配 | encrypted fail、页级文字/OCR、selective pages、图片/表格证据 |
| Word | DOC/DOCX | 部分/低配 | heading/list/table/image/footnote、legacy capability |
| Excel | XLS/XLSX | 低配 | workbook index、sheet pages/chunks、formula + displayed value |
| PowerPoint | PPT/PPTX | 低配 | slide count、notes、meaningful images、legacy capability |
| Image | PNG/JPEG/WebP/BMP/TIFF/HEIC | 缺失 | OCR authorization、有效文字门禁、quality、remove/retry |
| Audio | MP3/WAV/M4A/AAC/FLAC/OGG/Opus/WMA | 缺失 | companion subtitle、local ASR、no speech fail-closed |
| Video | MP4/MOV/MKV/WebM/AVI/M4V/WMV | 缺失 | subtitle first、audio-only ASR、conditional frame OCR |
| Subtitle | SRT/VTT/ASS/LRC/TXT/MD | 缺失 | companion binding、timelines、encoding、standalone behavior |
| Web page | HTTP/browser Readability | 部分 | final Source commit、canonical contract、asset quality |
| Platform image post | XHS 等 | 部分 | final Source commit、required OCR、restricted tag |
| Platform video | Bilibili/XHS 等 | 部分 | final Source commit、no metadata-only、remote default extract-only |
| Collection | playlist/author/collection | 缺失或不完整 | discovery preview before child task creation |

## 12. 测试审计与迁移要求

### 12.1 现有测试的正确解读

当前 Import 相关测试数量很多，涵盖：

- discovery 与格式识别；
- engine/staging/artifact/hash/path 安全；
- web SSRF/redirect/DNS；
- platform parsing；
- subtitle/ASR/OCR helper；
- transaction/rollback；
- session/task/UI；
- migration 与 capability pack。

但“测试多”不等于“新基准已满足”。以下测试正在固化错误模型，必须先改：

- URL committed 但 `wikiPath` 为空；
- `PreviewWithoutTranscript`；
- Import BYOK recovery；
- remote media 入队前 preserve/extract；
- global conflict policy；
- user-visible migration UI；
- metadata-only Bilibili/视频 Markdown。

### 12.2 必须新增的合同测试

1. 任意 success item 必有 raw evidence、final Source、manifest/version、history。
2. local 与 URL Source 使用相同 final frontmatter 合同。
3. final frontmatter 与 registry 的 stable IDs/hash 一致。
4. URL 不运行 Compile 也可打开 Source。
5. Compile 只消费传入的 V2 `sourceId/versionId`，拒绝过期/篡改 hash。
6. Compile 记录 consumption，且绝不写 `wiki/sources/**`。
7. Import command surface 不存在 BYOK recovery。
8. 配置 default Agent 时，普通解析失败也不会自动创建 Agent task。
9. 音视频无有效 transcript/no speech 时无 raw/Source。
10. clipboard text -> preview -> confirm -> Source。
11. 完整本地格式表驱动 discovery + route。
12. PDF mixed pages 只 OCR 必要页。
13. standalone image OCR 失败不产生 Source。
14. local video 优先 companion subtitle，否则 local ASR。
15. remote media 默认不永久保留。
16. 同平台多个 waiting item 一次登录后全部恢复。
17. restart 只恢复未完成 shard；完成 shard 不重复下载/OCR/ASR。
18. completed session 再 Add 创建新 session。
19. exact duplicate 不产生新 Source/version。
20. changed origin 产生同 Source 新 version。
21. human-edited current Source 进入三方合并。
22. completion summary 的 new/update/duplicate/warning/failure 精确。
23. 点击“查看来源”不触发 Compile；点击“更新 Wiki”才触发。
24. Source AI 整理只生成 candidate，未确认不覆盖。
25. AI 整理应用前 checkpoint；外部改动时三方合并。
26. Source delete 覆盖 registry/raw/assets/versions/references，失败原子回滚。

### 12.3 UI/可访问性测试

- 七类用户状态表驱动映射；
- batch todos 按 login/OCR/ASR/capability 聚合；
- fixed bar 无全局 conflict select；
- queue 每行一个主动作和可键盘 `…`；
- capability tile Tab/Enter/Escape/outside close；
- 快速/完整 preview 不泄露 internal IDs；
- 中英文长文案与 820px/桌面宽度；
- keyboard-only file/url/clipboard/commit/preview；
- screen reader live region 不重复播报整个队列；
- active tab/filter/scroll 在返回 Import 后恢复；
- CJK/Unicode/长路径/Windows/macOS/Linux path/case sensitivity。

### 12.4 真实 fixture 要求

不得只用 hand-built DTO。至少保留：

- 一个多页混合文字/扫描 PDF；
- 一个 encrypted PDF；
- 一个大 workbook（多 sheet、公式、空行、Unicode sheet name）；
- 一个含 notes/image 的 PPTX；
- 一个 CJK Markdown + relative assets；
- 一张有字图片和一张无有效文字图片；
- 一段有语音媒体和一段无有效语音媒体；
- companion subtitle；
- 普通网页、Bilibili、有图 XHS、受限内容 fixture；
- URL redirect、登录取消、能力缺失、磁盘不足、取消中断 fixture。

## 13. 推荐实施批次与依赖

下面是推荐拆分，不是要求未来 Agent 在一个 turn 内完成。每个 batch 都应先写自己的具体计划、变更清单和回滚边界。

### Batch 0：冻结反基准路线，建立新合同的编译期护栏

**目的**

停止继续在错误类型上扩展。

**范围**

- 删除 Import BYOK recovery；
- 删除确定性失败后的自动 Agent invocation，只保留显式用户动作；
- 删除 `PreviewWithoutTranscript`；
- remote media 默认改为 extract-only，移除入队前媒体猜测弹窗；
- 移除全局 conflict policy；
- 将 user-visible migration 从正常工作台移出；
- 定义 `ImportUserState`、item resolution、completion changeset、stable Source frontmatter DTO；
- 把反基准测试改成“这些路径不存在/被拒绝”。

**暂不做**

- 不在本批完整实现所有格式；
- 不做 Source AI 整理；
- 不做视觉大改。

**退出门槛**

- Import command/API surface 无 BYOK；
- 普通解析失败不会自动启动本地 Agent；
- 音视频不存在 metadata-only committable action；
- 类型层能表达后续 Source contract 和 completion；
- 旧行为测试已删除或改写。

### Batch 1：统一 Source finalization 与 URL 原子提交

**依赖**：Batch 0

**范围**

- 引擎 candidate 与 final Source 分离；
- commit 注入标准 frontmatter；
- URL 与 local 都写 final Source；
- canonical directory mapping：`wiki/sources/local`、`wiki/sources/web/<host>`、`raw/web`、`raw/assets`；
- stable platform ID 与 app Source ID 分离；
- manifest/index schema version/migration；
- 所有成功项原子产出 raw + Source + manifest + history。

**退出门槛**

- P0-01、P0-02 关闭；
- 普通网页与至少两个平台 end-to-end commit；
- exact duplicate/update 与 external edit 不被破坏。

### Batch 2：Compile V2 bridge 与提交完成摘要

**依赖**：Batch 1

**范围**

- typed `ImportCompletion`；
- UI 完成摘要；
- 查看已导入来源；
- Compile request 使用 `sourceId/versionId/contentHash`；
- `.app/compile/` consumption records；
- 移除主流程对旧 `.app/source-index.json` 的依赖；
- legacy adapter 隔离；
- 明确 Compile 只写派生 Wiki。

**退出门槛**

- P0-04、P1-10 关闭；
- Import 永不自动 Compile；
- 用户显式 Compile 后能追踪 consumed versions。

### Batch 3：本地输入、格式 discovery 与 Source package

**依赖**：Batch 1；可与 Batch 2 部分并行，但共享 DTO 必须先冻结

**范围**

- clipboard text/Markdown；
- 完整 format/magic/MIME detection；
- image/audio/video/subtitle discovery；
- folder ignored/unsupported summary；
- Markdown assets；
- CSV/Excel package；
- Office/PDF route plan 接入 production orchestrator；
- 大文件/大表预确认。

**退出门槛**

- §11 格式矩阵按真实 fixture 通过；
- folder 不生成 Source；
- 无 truncation；
- helper 不再只存在于单元测试。

### Batch 4：统一 OCR、字幕、ASR 与媒体门禁

**依赖**：Batch 3；复用 Batch 1 finalization

**范围**

- standalone image；
- selective PDF OCR；
- XHS OCR 归入统一 contract；
- local companion subtitle；
- local media ASR；
- no speech fail-closed；
- conditional video frame OCR；
- ASR 首次启用资源/设备/模型/依赖/偏好；
- transcript 30–60 秒锚点；
- 保留近期真实进度/取消能力。

**退出门槛**

- P0-05、P1-08、P1-09 关闭；
- 任意媒体 success 必有有效正文；
- 取消和 restart 不重复已完成重任务。

### Batch 5：Web/平台媒体、登录和远程原件策略

**依赖**：Batch 1、Batch 4

**范围**

- HTTP -> browser 分层完整性；
- unknown platform local Agent/capability；
- one-login-resumes-all；
- account summary/switch/revoke/last verified；
- restricted content first-session warning/tag/export warning；
- collection discovery before child tasks；
- remote original 的 item-level size/disk confirmation；
- best-quality remote images。

**退出门槛**

- §7、§12、§19.4 场景通过；
- cookie/profile 永不进入 React/project/log/export；
- remote preserve 失败不影响 Source 文本。

### Batch 6：Import 工作台、会话和批量效率

**依赖**：Batch 0 的 presentation DTO；最好在 Batch 3/4/5 的 blocker types 稳定后完成

**范围**

- compact input；
- interactive capability matrix；
- seven user states；
- grouped todos；
- one primary action + `…`；
- per-item conflict；
- fixed commit counts/CTA；
- quick/full preview；
- completed item/session lifecycle；
- resume-all/shards；
- tab/filter/scroll persistence；
- error/user safety copy。

**退出门槛**

- §5、§6、§18 的 UI 验收；
- 键盘-only 与中英文窄宽屏；
- no internal IDs in normal mode。

### Batch 7：Source reader、右栏和 Source 生命周期

**依赖**：Batch 1、Batch 2

**范围**

- reliable Source reader detection；
- Source-specific toolbar；
- content overview；
- source status/current/original/quality/paths/related Wiki；
- version timeline；
- refresh/reprocess entries；
- 专用 rename/move/delete；
- delete preview/checkpoint/atomic transaction/reference count/free-space；
- 禁止普通新建 Source。

**退出门槛**

- P0-06 中非 AI 部分、P1-11 关闭；
- Source package 不可被 generic Wiki operation 破坏。

### Batch 8：AI 整理完整候选闭环

**依赖**：Batch 7；Compile V2 route 可复用但不能混同

**范围**

- start dialog；
- Agent/BYOK routes（此处合法）；
- background/cancel/recover；
- per-Source concurrency；
- bounded input；
- exactly-once 内容概览；
- candidate binding；
- diff/three-way merge；
- checkpoint/apply/new version；
- timeline/notifications。

**退出门槛**

- §15、§19.5 全部场景；
- 未确认绝不覆盖；
- external edit 不丢失；
- candidate 不绑定当前 hash 时拒绝应用。

### Batch 9：兼容清理、无障碍、文案与全矩阵回归

**依赖**：前述功能批次

**范围**

- legacy adapter removal/read-only bounds；
- migration UI 收口；
- dead commands/types/i18n/tests 清理；
- a11y/responsive/heading/live region；
- CJK/Unicode/case/path；
- disk full/cancel/restart/concurrency；
- docs/spec/progress/gotchas 同步。

**退出门槛**

- 产品基准 §19 矩阵形成可重复证据；
- `npm run check` 从头通过；
- 两轮独立 review 没有未处理有效问题。

## 14. 每个实施 Agent 的强制工作模板

后续每个 batch 的计划至少写清：

1. 要关闭的本文 finding ID；
2. 对应产品基准章节；
3. 现有调用链与要改的 typed DTO；
4. 写入路径、删除路径、checkpoint 和 rollback；
5. 与其他 batch 的 schema/command 依赖；
6. 要保留的现有能力；
7. 要删除或迁移的反基准测试；
8. 新增 unit/integration/e2e/manual fixtures；
9. 中英文、键盘、CJK/path 覆盖；
10. `npm run check` 与双 review 结果。

禁止用以下方式关闭 finding：

- 只隐藏 UI，保留可直接调用的错误 command；
- 只补 enum，不接 production route；
- 只补 unit helper，不跑真实 commit；
- 只让测试适配当前行为，不核对产品基准；
- 以 Compile 生成内容替代 Source；
- 以 metadata/description 替代音视频 transcript；
- 以 BYOK 替代 Import parser；
- 以普通 Wiki delete/rename 操作 Source；
- 以全局 conflict strategy 替用户做逐项决定。

## 15. 建议的验收顺序

未来不要先做视觉截图再验证数据。建议按以下 gate：

```text
Gate A: Source invariant
  success => raw + final Source + manifest/version + history

Gate B: Input correctness
  each format/media => valid candidate or explicit failure

Gate C: Commit/update/delete safety
  duplicate/version/merge/checkpoint/rollback

Gate D: Completion and Compile
  explicit changeset, explicit compile, consumption record

Gate E: Source reader/lifecycle
  read/reprocess/timeline/delete

Gate F: AI organize
  candidate/diff/checkpoint/new version

Gate G: Workbench UX
  batch/state/preview/a11y/i18n
```

Gate A 未通过时，不应宣称某个平台或格式“导入完成”；Gate D 未通过时，不应宣称 Import 与 Compile 已分离完成；Gate F 未通过时，不应把通用 Ask AI 当作 AI 整理。

## 16. 最终优先级汇总

### 立即阻断继续扩展的 P0

1. URL success 不写 Source；
2. final Source contract 不统一；
3. Import BYOK recovery；
4. 解析失败自动启动本地 Agent；
5. Compile 不消费 V2 Source versions；
6. metadata-only media preview；
7. Source reader / AI 整理闭环缺失。

### 紧随其后的 P1

1. registry/目录/consumption；
2. clipboard；
3. local image/audio/video/subtitle；
4. production PDF/OCR/Office/Excel；
5. per-item update/merge；
6. session finish/resume；
7. login/restricted content；
8. OCR/ASR 通用化；
9. completion summary；
10. Source atomic lifecycle；
11. import-recovery Agent contract；
12. 平台覆盖、复合内容和集合发现。

### 可以成组收尾的 P2/P3

- compact input；
- capability popover；
- grouped todos；
- seven states；
- one primary action；
- fixed bar；
- two-level preview；
- user-first error copy；
- remote media preservation；
- migration visibility；
- tab/scroll/a11y/i18n/responsive。

## 17. 审阅结语

当前代码最有价值的部分是：任务、事务、staging、安全校验、平台解析、进度和 registry 已经有相当多可以复用的工程基础。当前最危险的部分也是同一件事：这些基础足够完整，以至于错误产品路线也已经被类型、UI 和测试“做实”了。

后续工作的第一原则应是：**先让所有成功路径严格等价于“得到一个可读、可追踪、可版本化、可安全删除的 Source”，再谈更多格式、更多平台和更漂亮的工作台。**

一旦 Source finalization、V2 Compile changeset 和 Source lifecycle 三个轴稳定，现有的媒体解析、任务进度、能力包、Agent candidate 和 Wiki 阅读框架才会真正汇合成产品基准描述的闭环。

## 18. 本次审阅验证记录

- 指定的 `llm-wiki-desktop-context` skill 与 project map 已由主审阅完整读取。
- 2026-07-24 产品基准全文已逐节核对。
- 已合并一轮共享上下文前端/Source UX 审阅和一轮新鲜上下文后端/媒体审阅。
- 报告中 45 个明确的仓库文件引用均已做存在性检查，缺失引用为 0。
- `npm run check` 已从头通过：
  - Vitest：103 个测试文件、680 个测试；
  - capability-tool Node/Python tests：通过；
  - lint：通过；
  - Vite build/import resolution：通过；
  - console-log scan：通过；
  - Tauri GUI Rust compile：通过；
  - Rust `--no-default-features` 全套测试：通过。
- 检查只报告了现有 `FileTransaction` 的 `write / track / capture_installed` dead-code warning，没有失败。
- 本次没有修改应用实现、产品基准或 `UI-Frontend-design/`；新增本报告，并按项目规则更新两份 progress 记录。
