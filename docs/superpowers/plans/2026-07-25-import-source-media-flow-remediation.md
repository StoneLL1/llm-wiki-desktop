# Import、来源库与媒体处理全流程整改执行计划

> **产品基准**：`docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`（唯一权威）。
> **问题清单**：`docs/reviews/2026-07-25-import-source-media-flow-implementation-review.md`（7 个 P0、13 个 P1、12 个 P2、4 个 P3）。
> **本文性质**：执行计划，不是新的产品决策。任何与产品基准冲突的实现选择都以基准为准；本文若与基准冲突，改本文。
> **计划日期**：2026-07-25
> **状态**：待逐批实施。每个 batch 开工前先读本文对应章节，再读基准对应章节，最后才读代码。
>
> **For agentic workers**：动手前先用 `skills/llm-wiki-desktop-context` 载入项目上下文（`AGENTS.md` → `SPEC/SPEC.md` §16 → `SPEC/progress.txt` 最新条目 → `SPEC/gotchas.txt` 按模块检索 → `references/project-map.md`）。不要跳过 `git status --short`，本仓库工作区长期有未提交改动，必须保留无关改动。

**Goal**：把 Import 从"任务能跑完"整改成"每个成功项都等价于一个可读、可追踪、可版本化、可安全删除的 Source"，并在此之上补齐格式覆盖、OCR/ASR 门禁、登录闭环、工作台交互、Source 阅读与 AI 整理。

**Architecture**：沿用现状分层，不重构架构。
- 后端：`commands（薄层）-> AppState -> 稳定 service facade -> 私有 use-case 模块 -> 本地文件 / Git / Agent / 能力包 / OS 凭据`。
- 前端：`AppShell -> WorkspaceController -> WorkspaceRouter -> lazy feature view`，跨视图行为留在 `useImportWorkflow` / `useAiCapabilities` / `useTaskLauncher` / `useProviderWorkflow` / `useAgentWorkflow` 这些聚焦 workflow 里，不合并成巨型 controller。
- 数据：Markdown + JSON + 本地文件。不引入数据库。

**Tech Stack**：Tauri v2 + Rust；React 19 + TypeScript + Vite；Tailwind v4 + shadcn/ui；Zustand；react-i18next（zh-CN / en）；Milkdown；sigma.js + graphology；Lucide。

---

## 0. 如何使用本文

1. **一个 batch 一个会话**。不要在一个会话里跨 batch 实施。每个 batch 的退出门槛没过，不要开下一个 batch。
2. **每个 batch 开工第一件事**：把本文该 batch 的"冻结合同"与"依赖"读完，确认前置 batch 的 DTO 已经落地。若前置 DTO 缺失，停下来先补前置，不要在本 batch 里临时发明兼容字段。
3. **每个 batch 收尾**：`npm run check` 从头跑通 → 双子代理审查（共享上下文 A + 全新上下文 B）→ 修复 → 再 `npm run check` → 追加 `SPEC/progress.txt` → 有踩坑就追加 `SPEC/gotchas.txt`。
4. **本文的 batch 编号沿用 review §13**，不要重新编号，避免后续沟通错位。

---

## 1. 全局约束（每个 batch 都生效）

这些是硬边界，任何 batch 都不得以"更优雅"为由推翻：

- **Source 不变量**：成功 = `raw/` 证据 + `wiki/sources/` 可读 Source + `.app/sources/` manifest/version + history，四者原子成立。任一缺失即该项 commit 失败。
- **失败不落地**：失败项不写 `raw/`、不创建占位 Source、不留半个 manifest。
- **Import 不自动编译**；Compile 不写 `wiki/sources/**`。
- **BYOK 不参与 Import**：不做解析、不做恢复、不做 OCR/ASR 备援。BYOK 只在 Source 已存在后用于 AI 整理、Compile、Chat。
- **Agent 只能显式触发**，只写 staging candidate，不碰 `raw/`、`wiki/`、Git、密钥；安装命令永不静默执行。
- **密钥只进 OS 凭据存储**。API key、token、cookie 不进项目文件、日志、导出、frontmatter、DTO、前端。UI 只显示"已配置"。
- **登录用隔离平台会话**。React 不读原始 Cookie，不提供 cookie 查看/复制/导出/编辑/手动导入。账号信息只以安全摘要返回。
- **高风险操作先建 Git 检查点**：删除、覆盖、批量重写、冲突合并、来源替换、Agent 自动修复。纯新增不需要高风险检查点。
- **`raw/sources/` 默认不可变**；替换或删除需显式确认。
- **React 不拥有文件系统 / Git / Agent 进程 / 密钥逻辑**，全部走 Tauri IPC。
- **路径安全**：内部路径统一正斜杠；所有来自 UI 的路径校验在项目范围内；CJK / Unicode / 长路径 / Windows·macOS·Linux 风格 / 大小写敏感性都是必测项。
- **长任务**可取消、可后台、可报进度、可恢复；取消要清理临时产物。
- **用户文案不出现工程术语**：`staging`、`artifact`、`manifest`、`baseline`、`session ID`、`fingerprint`、`route`、`engine`、`SHA`、error code 只进折叠技术详情与日志。
- **不修改、不提交 `UI-Frontend-design/`**；`wiki/wiki/` 是验证数据，不是测试场，需要测导入就另复制一份目录。
- **验证只认 `npm run check`**。焦点测试可用于开发中，但不替代它。若 Tauri 应用正在运行锁住默认 target，用独立 `CARGO_TARGET_DIR` 完整重跑，不要杀掉用户的应用。

### 1.1 九条禁止的"关闭方式"

review §14 明确禁止，逐条落到本文每个 batch 的验收里：

| # | 禁止 | 本文对应的强制验收 |
|---|---|---|
| 1 | 只隐藏 UI，保留可直接 invoke 的错误 command | 删除 command 注册 + 删除 handler + 加"该 command 不存在"契约测试 |
| 2 | 只补 enum，不接 production route | 每个格式必须有 discovery → route → execution → candidate → commit 的端到端集成测试 |
| 3 | 只补 unit helper，不跑真实 commit | helper 必须被生产 orchestrator 调用，并有真实 fixture 走完 commit |
| 4 | 只让测试适配当前行为 | 反基准测试必须改写为断言"被拒绝"，不能改断言去迎合现状 |
| 5 | 以 Compile 生成内容替代 Source | 契约测试：不运行 Compile，Source 依然可读 |
| 6 | 以 metadata/description 替代音视频 transcript | 契约测试：无有效 transcript ⇒ 无 raw、无 Source、不可提交 |
| 7 | 以 BYOK 替代 Import parser | 契约测试：Import command surface 无 BYOK |
| 8 | 以普通 Wiki delete/rename 操作 Source | 契约测试：generic wiki 命令对 `type: source` 页面拒绝或改路由到专用命令 |
| 9 | 以全局 conflict strategy 替用户逐项决定 | 固定栏无策略控件；每个 resolution 绑定 `sourceId + candidateHash + currentHash + targetVersionId` |

---

## 2. Finding → Batch 归属总表

36 个 finding 全部有唯一"关闭 batch"。跨 batch 的 finding 标注了前置交付批次。

| Finding | 关闭 batch | 前置交付 | 基准章节 |
|---|---|---|---|
| P0-01 URL 成功不写 Source | 1 | 0（DTO） | §1、§3.1、§4、§19.1 |
| P0-02 final Source 合同不统一 | 1 | 0（DTO） | §4.3、§19.1 |
| P0-03 Import BYOK recovery | **0** | — | §3.1.9、§16 |
| P0-04 Compile 不消费 V2 版本 | 2 | 1 | §14、§17.4 |
| P0-05 metadata-only 媒体预览 | 0（删路线）+ **4**（门禁闭环） | 3 | §9、§19.3 |
| P0-06 Source 阅读 / AI 整理缺失 | **7**（非 AI）+ **8**（AI 整理） | 1、2 | §15、§19.5 |
| P0-07 失败后自动启动 Agent | **0** | — | §3.1.7、§16、§17.3 |
| P1-01 registry 逻辑来源模型不完整 | 1 | 0 | §4.1、§4.2 |
| P1-02 缺文本 / Markdown 粘贴输入 | 3 | 0、1 | §6.2、§11.6 |
| P1-03 本地图片 / 音频 / 视频 / 字幕未进 discovery | 3 | 1 | §11.1、§11.7 |
| P1-04 PDF/OCR/Office/Excel helper 未接生产 | 3（route 接线）+ 4（OCR 执行） | 1 | §11.3–§11.5 |
| P1-05 冲突仍是旧 Wiki 模型 | **6**（UI 关闭） | 1（后端派生） | §13.2、§6.6 |
| P1-06 会话 / completed / 重启语义 | 6 | 0、1 | §5 |
| P1-07 登录只恢复单项 | 5 | 1 | §7 |
| P1-08 OCR 授权与 standalone image | 4 | 3 | §8 |
| P1-09 ASR 未通用化 | 4 | 3 | §9 |
| P1-10 无完成摘要 / Source 导航 / 显式更新 Wiki | 2 | 1 | §14 |
| P1-11 Source 可被普通 Wiki 操作 | 7 | 1 | §13.4、§15 |
| P1-12 `import-recovery` skill 合同缺失 | 5 | 0 | §16 |
| P1-13 平台覆盖与集合发现 | 5 | 1、4 | §12.2、§12.6 |
| P2-01 固定栏错误信息 + 全局冲突策略 | 0（删控件）+ **6**（计数与 CTA） | — | §6.6 |
| P2-02 能力矩阵不可操作 | 6 | — | §6.3 |
| P2-03 无按用户动作聚合的批量待办 | 6 | 4、5 | §6.5 |
| P2-04 队列无"一主动作 + …" | 6 | 0 | §6.4 |
| P2-05 两级预览不完整且泄露内部 ID | 6 | 1 | §6.7 |
| P2-06 状态未归并七类 | 6 | 0（`ImportUserState`） | §5.2 |
| P2-07 错误先显示 code | 6 | 0（issue DTO） | §18 |
| P2-08 远程媒体默认值与时机相反 | 0（默认值）+ **5**（item 级确认） | — | §12.3、§12.7 |
| P2-09 迁移工程 UI 在正常工作台 | 0（移出）+ **9**（收口） | — | §6.1 |
| P2-10 输入区不紧凑、缺文本入口 | 6 | 3 | §6.2 |
| P2-11 登录 / 能力恢复不是批量 | 6（前端）| 5（后端） | §7.3、§6.5 |
| P2-12 UI 状态与键盘语义不完整 | 6 | — | §6.3、§6.4 |
| P3-01 右栏应从诊断改成"下一步助手" | 6 | — | §15.6 类比、§6 |
| P3-02 i18n 内容模型过时 | 0（删）+ 6（新增）+ 9（校验） | — | §2、§18 |
| P3-03 UI 语言与 Source 语言分层 | 9 | — | §10 |
| P3-04 icon / 文案应来自 typed model | 3（后端返回）+ 6（前端消费） | — | §17.2 |

### 2.1 Gate → Batch 映射

review §15 的验收顺序落到批次：

```text
Gate A  Source invariant            -> Batch 1（Batch 3/4/5 每加一种输入都必须重跑）
Gate B  Input correctness           -> Batch 3 + Batch 4
Gate C  Commit/update/delete safety -> Batch 1（提交侧）+ Batch 6（逐项决定）+ Batch 7（删除）
Gate D  Completion and Compile      -> Batch 2
Gate E  Source reader/lifecycle     -> Batch 7
Gate F  AI organize                 -> Batch 8
Gate G  Workbench UX                -> Batch 6 + Batch 9
```

**硬规则**：Gate A 未过，不得声称任何平台或格式"导入完成"；Gate D 未过，不得声称 Import 与 Compile 已分离；Gate F 未过，不得把通用 Ask AI 当作 AI 整理。

---

## 3. 每个 batch 的强制交付模板

review §14 要求的十项，本文已在每个 batch 内以固定小节体现。实施 Agent 收尾报告必须逐项回答：

1. 关闭的 finding ID；
2. 对应产品基准章节；
3. 现有调用链与改动的 typed DTO；
4. 写入路径、删除路径、checkpoint、rollback；
5. 与其他 batch 的 schema / command 依赖；
6. 要保留的现有能力；
7. 删除或迁移的反基准测试；
8. 新增 unit / integration / e2e / manual fixture；
9. 中英文、键盘、CJK/path 覆盖；
10. `npm run check` 与双 review 结果。

---

# Batch 0：冻结反基准路线，建立新合同的编译期护栏

**目的**：停止在错误类型上继续扩展。本批**主要是删除和定型**，不追求新功能。

**依赖**：无。**可与任何 batch 并行？** 不可以，这是所有后续批次的地基。

## 0.1 关闭的 finding

- 完全关闭：**P0-03**、**P0-07**
- 本批交付其"删除"部分：**P0-05**（删除 `PreviewWithoutTranscript` 路线）、**P2-01**（删除全局冲突控件）、**P2-08**（远程默认改 extract-only、删入队前弹窗）、**P2-09**（迁移 UI 移出正常工作台）、**P3-02**（删除过时 i18n key）
- 本批只定义、不实现：`ImportUserState`、per-item resolution、`ImportCompletion`、Source frontmatter 合同（供 Batch 1/2/6 使用）

## 0.2 基准章节

§2（术语与用户文案）、§3.1（不变量 3/7/9/10）、§5.2（七类状态）、§6.6（固定栏）、§9（无 metadata-only）、§12.3 / §12.7（远程媒体）、§16（Agent 修复边界）、§17.2 / §17.3（实体与自动·显式动作边界）

## 0.3 现有调用链与要改的 DTO

**BYOK Import recovery（P0-03）现存表面**

后端：
- `src-tauri/src/commands/import_v2_agent_commands.rs:105` `preview_import_byok_scope_v2`
- 同文件 `:128` `approve_import_byok_assistance_v2`
- `src-tauri/src/lib.rs:206-207` 两个 command 注册
- `src-tauri/src/services/import_v2/agent_assistance.rs`：`PendingByokApproval`（:41）、`byok_approvals()`（:55）、`approved_byok_runs()`（:60）、`preview_byok_scope`（:121）、`start_byok`（:190）、`run_byok`（:352）、`validate_byok_item`、`finalize_byok_error`、`attempt.route.starts_with("byok_assistance/")`（:297）
- `src-tauri/src/models/import_v2_agent.rs`：`auto_byok`（:16、:25）、`RequestByok`（:49）、`PreviewImportByokScopeRequest`（:124）、`ApproveImportByokAssistanceRequest`（:135）、`byok_provider` / `byok_destination`（:269-270）
- `src-tauri/src/services/import_v2/orchestrator.rs` 的 `begin_byok_assistance` 调用点
- `src-tauri/src/tasks/byok_progress.rs`：判断其是否仅服务 Import；若同时服务 Compile/Chat/Export，只摘掉 Import 分支，**不要整文件删除**

前端：
- `src/features/import/ImportByokApprovalDialog.tsx` + `.test.tsx`（整文件删除）
- `src/features/import/ImportAgentControls.tsx`（:11-16、:90-106）
- `src/features/import/ImportV2Dialogs.tsx`（:85-98、:172-180）
- `src/features/import/useImportSupportingActions.ts`（:124-177）
- `src/features/import/importStatusPresentation.ts`（:74-77、:133-135）
- `src/features/import/ImportItemActions.tsx`、`ImportView.tsx`、`importWorkflow.ts`、`useImportWorkflow.ts`、`importStore.ts`
- `src/services/importV2Api.ts`、`src/types/importV2Agent.ts`（`autoByok`、`"request_byok"`、`byokProvider`、`byokDestination`）、`src/types/importV2Api.ts`、`src/types/importV2Presentation.ts`

> **注意**：`byok` 关键字在 `chat`、`compile`、`export`、`lint`、`settings`、`project`、`task` 模块中是**合法**的。删除范围严格限于 `import_v2*` / `src/features/import/` / Import 专属 DTO 字段。`src-tauri/src/services/settings_service.rs`、`src-tauri/src/tasks/task_service.rs` 的通用 BYOK 能力保留。

**自动 Agent（P0-07）现存位置**

- `src-tauri/src/commands/import_v2_commands.rs`：后台 `run_item_with_recovery` 的 error 分支读取 `settings.agent_default` 后直接调用 `run_local_agent_candidate(..., AgentAssistanceTrigger::DeterministicHardFailure, agent_kind)`，并创建 Agent assistance task。
- 保留 `AgentAssistanceTrigger` 中的显式触发变体与整套 staging / candidate / provenance / quality / diff / confirmation 骨架；只删自动调用与 `DeterministicHardFailure` 这一自动入口。

**metadata-only（P0-05 删除部分）**

- `src-tauri/src/models/import_v2.rs:249-295` `PreviewWithoutTranscript` 定义与从 web error 的派生
- `src-tauri/src/services/import_v2/orchestrator.rs:2510` 执行分支、`:3748` 内联测试
- `src-tauri/src/services/import_v2/subtitle.rs:16-26` 只含视频元数据与简介的 Markdown 生成器
- `src-tauri/tests/import_v2_web_ingestion.rs:191`
- `src/types/importV2.ts` 的 `"preview_without_transcript"`
- `src/features/import/importStatusPresentation.ts:79-96`、`ImportItemActions.tsx`、`ImportView.tsx`
- `src/i18n/locales/zh-CN.json`、`src/i18n/locales/en.json` 对应 key
- `gotchas.txt`（仓库根，非 `SPEC/gotchas.txt`）中提到该动作的条目：改写为"该动作已按 2026-07-24 基准删除"，不要留下"可以用它绕过"的暗示

**远程媒体默认值（P2-08 本批部分）**

- `src-tauri/src/models/import_v2.rs`：`MediaSaveMode::default()` 由 `PreserveOriginal` 改为 `ExtractOnly`
- `src-tauri/src/services/import_v2/orchestrator.rs:369` `add_text_input` 及所有构造 `ImportInput` 的位置复核默认值来源
- `src/features/import/ImportSourceMethods.tsx:115-145、255-275`：删除入队前媒体猜测与 preserve/extract 单选
- **本地媒体不受影响**：本地原件在确认导入后完整保存到 `raw/`（基准 §11.7.7），默认值改动只作用于远程

**全局冲突策略（P2-01 本批部分）**

- `src/features/import/ImportCommitBar.tsx:26-35` 的 `<select>` 与 `conflictAction` prop
- `src/features/import/ImportView.tsx:77-78、179-189` 把全局 decision 批量赋给 ready items 的逻辑
- 本批只删控件与批量赋值；固定栏计数与 CTA 文案在 Batch 6

**迁移 UI（P2-09 本批部分）**

- `src/features/import/ImportView.tsx:431` 卸载 `ImportMigrationNotice`
- `src/features/import/ImportV2Dialogs.tsx:227` 卸载 `ImportMigrationDialog`
- 组件与后端 migration command **保留**，改为只能从设置或诊断入口进入；`src-tauri/src/services/import_v2/migration/` 与 7 个迁移测试文件不动
- `npm run check:import-v2-cutover` 与 `docs/import-v2-cutover-evidence.json` 已确认不含 `byok` / `transcript` 引用，本批删除不会破坏该脚本；但迁移入口迁移后需复核该脚本是否断言 UI 挂载点

## 0.4 本批冻结的合同（后续 batch 不得各自发明）

以下类型在本批**定义并落地到类型层**（Rust model + TS type + 前后端序列化契约测试），实现留给对应 batch。命名可微调，但字段语义必须一致。

```ts
// src/types/importV2.ts
export type ImportInputKind = "file" | "folder" | "url" | "clipboard_text";

// 基准 §5.2 的七类用户状态，前端只消费这层
export type ImportUserState =
  | "discovering"      // 正在发现
  | "processing"       // 正在处理
  | "needs_action"     // 需要操作
  | "ready"            // 可确认
  | "committing"       // 正在导入
  | "committed"        // 已导入
  | "failed";          // 失败 / 已取消

// 每项的提交决议，绑定稳定 ID 与 hash，替代旧 CommitConflictAction
export type ImportItemResolution =
  | { kind: "new_source" }
  | { kind: "exact_duplicate_skip"; sourceId: string }
  | { kind: "same_source_new_version"; sourceId: string; baseVersionId: string }
  | { kind: "keep_current_source"; sourceId: string }
  | { kind: "apply_import_candidate"; sourceId: string; baseVersionId: string }
  | { kind: "manual_merge"; sourceId: string; baseVersionId: string; mergedHash: string };

// 用户合同与诊断合同分离（P2-07）
export interface UserIssue {
  code: string;                  // 稳定机器码，不直出给用户
  title: string;                 // 发生了什么
  dataSafety: string;            // 已有数据是否安全
  primaryAction: ImportPrimaryAction | null; // 下一步
  detail?: ImportIssueDiagnostics;           // 折叠技术详情
}

export interface ImportCompletion {
  sessionId: string;
  batchId: string;
  newSources: SourceVersionChange[];
  updatedSources: SourceVersionChange[];
  duplicateSkips: DuplicateResult[];
  warnings: UserIssue[];
  failures: ItemFailure[];
}

export interface SourceVersionChange {
  sourceId: string;
  versionId: string;
  wikiPath: string;
  contentHash: string;
}
```

Source frontmatter 合同（Batch 1 实现，本批定型为 Rust struct + 校验函数签名）：

```yaml
type: source            # 必填，固定值
sourceId: src_...       # 必填，commit 分配，engine 不得伪造
versionId: ver_...      # 必填，commit 分配
sourceKind: ...         # 必填
title: ...              # 必填
importedAt: ...         # 必填
contentHash: ...        # 必填，与 manifest 一致
platform: ...           # 可空
canonicalUrl: ...       # 可空
platformContentId: ...  # 可空，**与 sourceId 严格分离**
author: ...             # 可空
publishedAt: ...        # 可空
language: ...           # 可空，检测结果
quality: ...            # 必填
restricted: true|false  # 受限内容标记
```

**硬约束**：frontmatter 不得出现 cookie、token、临时 staging 路径、内部 session ID、engine 内部 ID。

## 0.5 写入 / 删除 / checkpoint / rollback

- 本批不改任何项目文件写入路径，不需要 Git 检查点。
- 唯一的持久化影响：`MediaSaveMode` 默认值变化会影响**新建**的 session JSON。已有 `.app/import-sessions/*` 中显式写了 `preserve_original` 的记录保持原值，反序列化不得因为默认值变化而报错——加一条反序列化回归测试。
- 回滚边界：本批是纯删除 + 类型新增，回滚等于 `git revert`。

## 0.6 必须保留的现有能力

- 本地 Agent candidate 全套：workspace 隔离、provenance、hash 校验、staging 限制、diff/选择、quality gate（`src-tauri/src/services/import_v2/agent_candidate.rs`、`quality_gate.rs`）
- `FileTransaction` / `transaction.rs` 的事务与回滚
- `session_store.rs` 的持久化会话与任务恢复
- 全部平台解析、能力包、浏览器会话、SSRF/redirect/DNS 防护
- 迁移后端与迁移测试
- 通用 BYOK（Chat / Compile / Export / Lint / Settings）

## 0.7 要删除或改写的反基准测试

| 测试 | 处理 |
|---|---|
| `src-tauri/tests/import_v2_byok_assistance.rs` | 整文件删除，替换为 `import_v2_no_byok_recovery.rs`：断言 command 未注册、`RequestByok` 变体不存在 |
| `src-tauri/tests/import_v2_agent_assistance.rs` / `_candidate.rs` / `_contracts.rs` / `_orchestration.rs` / `_policy.rs` | 摘除 BYOK 断言，保留本地 Agent 断言；新增"配置了 default agent 时确定性失败不创建 Agent task"回归 |
| `src-tauri/tests/import_v2_web_ingestion.rs:191` | 改写为"无可靠字幕时不产生可提交项、不写 raw" |
| `src-tauri/src/services/import_v2/orchestrator.rs:3748` 内联测试 | 删除或改写为拒绝断言 |
| `src/features/import/ImportByokApprovalDialog.test.tsx` | 删除 |
| `src/features/import/ImportAgentControls.test.tsx` | 改写为只有本地 Agent 动作 |
| `src/features/import/importStatusPresentation.test.ts` | 删除 BYOK 与 `preview_without_transcript` 用例 |
| `src/features/import/ImportV2.integration.test.tsx`、`ImportView.test.tsx`、`useImportWorkflow.test.tsx` | 删除 BYOK 流程、全局冲突 select、入队前媒体选择用例 |
| `src/services/importV2Api.test.ts`、`src/types/importV2Agent.test.ts` | 删除 BYOK 断言，新增"API 表面无 BYOK"断言 |
| `src/stores/importStore.test.ts` | 删除 BYOK 状态用例 |
| `src-tauri/tests/task8_contracts.rs`、`mvp_flow.rs` | 只在涉及 Import BYOK 时调整；通用 BYOK 断言保留 |

## 0.8 任务清单

- [ ] T0.1 删除后端 Import BYOK：两个 command + 注册 + `agent_assistance.rs` 的 preview/start/run/approvals + `models/import_v2_agent.rs` 的 BYOK 字段与 `RequestByok`；`orchestrator.rs` 的 `begin_byok_assistance` 调用点一并清理
- [ ] T0.2 复核 `tasks/byok_progress.rs`：Import 专属则删，通用则只摘 Import 分支
- [ ] T0.3 删除前端 Import BYOK：`ImportByokApprovalDialog*`（整文件）+ 上列 12 个文件的引用 + API + 类型 + store 状态
- [ ] T0.4 删除 `import_v2_commands.rs` 中确定性失败后的自动 Agent 调用；失败 issue 只暴露显式 `invoke_local_agent` 动作
- [ ] T0.5 删除 `PreviewWithoutTranscript` 全链路（model / orchestrator / subtitle metadata-only 生成器 / 前端 / i18n / 测试）
- [ ] T0.6 `MediaSaveMode::default()` 改 `ExtractOnly`；加旧 session JSON 反序列化回归测试
- [ ] T0.7 删除 `ImportSourceMethods.tsx` 入队前媒体 preserve/extract 弹窗与相关 state
- [ ] T0.8 删除 `ImportCommitBar.tsx` 全局 conflict `<select>` 与 `ImportView.tsx` 的批量赋值
- [ ] T0.9 迁移 UI 从工作台/对话框卸载，改挂到独立兼容入口（设置或诊断）
- [ ] T0.10 新增冻结类型：`ImportInputKind` 加 `clipboard_text`、`ImportUserState`、`ImportItemResolution`、`UserIssue`、`ImportCompletion`、`SourceVersionChange`、Source frontmatter struct + 校验签名（Rust ↔ TS 序列化契约测试）
- [ ] T0.11 i18n：删除 BYOK / metadata-only preview / 全局冲突 / 入队前媒体 / 迁移工程术语 key（zh-CN 与 en 同步）
- [ ] T0.12 反基准测试按 §0.7 表逐条处理
- [ ] T0.13 dead code / dead command 审计：`cargo` warning 清零到不新增，`npm run check:console` 通过
- [ ] T0.14 复核 `npm run check:import-v2-cutover` 是否依赖被卸载的迁移 UI 挂载点

## 0.9 中英文 / 键盘 / CJK / path 覆盖

- 删除的 key 在 zh-CN 与 en 同步删除，`npm run check` 的 i18n 一致性检查必须通过
- 被删控件不留下孤立 tab stop：`ImportCommitBar` 与 `ImportSourceMethods` 的键盘序列回归测试
- 本批不新增路径逻辑，CJK/path 只需保证反序列化回归 fixture 含一个 CJK 文件名 session

## 0.10 退出门槛

- [ ] Import command / API surface 搜索 `byok` 零命中（`src/features/import/`、`src/services/importV2*`、`src-tauri/src/**/import_v2*`）
- [ ] 配置了 default Agent 时，普通解析失败不创建 Agent task（有回归测试）
- [ ] 音视频不存在 metadata-only 可提交动作（有拒绝测试）
- [ ] 固定栏无全局冲突控件；正常工作台无迁移工程 UI
- [ ] 类型层能表达 Source frontmatter 合同、七类用户状态、per-item resolution、completion changeset
- [ ] 旧行为测试已删除或改写为拒绝断言
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 1：统一 Source finalization 与 URL 原子提交

**目的**：让"成功"只有一个含义。这是整个整改的地基，也是 Gate A。

**依赖**：Batch 0 的冻结 DTO。

## 1.1 关闭的 finding

**P0-01**、**P0-02**、**P1-01**；交付 **P1-05** 的后端派生部分。

## 1.2 基准章节

§1（设计结论）、§3.1（不变量 1/4/5/10）、§4.1（一个逻辑来源）、§4.2（目录职责）、§4.3（Source Markdown 合同）、§4.4（来源包）、§13.1–§13.2（重复与更新）、§17.4（提交事务）、§19.1（核心闭环）

## 1.3 现有调用链与要改的 DTO

**当前 commit 链**：`import_v2_commands.rs::confirm` → `commit.rs::commit_items` / `commit_items_cancellable`（仅这两个 public fn）→ `source_registry.rs::build_commit_plan` → `FileTransaction`。

必须改的现存事实：

| 位置 | 现状 | 目标 |
|---|---|---|
| `commit.rs` `writes_wiki` | `item.input.kind != ImportInputKind::Url` | 删除该分支，所有 kind 一律写 final Source |
| `commit.rs` `wiki_markdown` | 仅 `writes_wiki` 时做资源链接重写 | 统一走 finalization：注入 frontmatter + 重写资源链接 + 规范化 |
| `commit.rs` commit result | URL 返回 `wiki_path: None` | 恒返回非空 `wikiPath`；`Option` 收窄或保留但由校验保证非空 |
| `commit.rs` `KeepWiki` | 允许 manifest 版本前进而 Source 不变 | 换成 `ImportItemResolution`；`keep_current_source` 不得推进 `current_version_id` |
| `source_registry.rs::derive_wiki_path` | 只分 `files` / `web` | `wiki/sources/local/...` 与 `wiki/sources/web/<host>/...` |
| `source_registry.rs::build_commit_plan` | 一律 `raw/sources/{sid}/{vid}/original.{ext}` + `raw/extracted/...` | 按基准 §4.2 分流：本地原件 → `raw/sources/`；页面/平台证据与原始字幕 → `raw/web/`；资源 → `raw/assets/` |
| `generic_web_engine.rs:943-962` | snake_case 旧合同，`source_id` 是平台内容 ID | engine 只产 `SourceCandidate`；平台 ID 走 `platformContentId` |
| `native_file_engine.rs:74-101、194-224` | 只产 extraction Markdown，无 frontmatter | 同上，engine 不写 frontmatter |
| `.app/source-index-v2.json` / `.app/sources/{sid}.json` | 版本骨架 | 加 `schemaVersion` 升级 + 一次性 in-place 迁移；不得半新半旧读字段 |

**candidate / final 分离**（review §9.2）：

```text
Import engine output = SourceCandidate { markdown body, metadata, raw snapshot(s), assets, quality, provenance }
Commit output        = SourceVersion + Final Source Markdown（commit 分配并注入 sourceId/versionId/contentHash）
```

**manifest 目标字段**（review §9.3，本批落地能落的，其余留 `Batch 5/7/8` 填充但**字段与 schemaVersion 本批一次定完**）：`sourceId`、`sourceKind`、`currentVersionId`、`wikiPath`、`aliases/origins`、`canonicalUrl`、`platform`、`platformContentId`、`title`、`author`、`publishedAt`、`importedAt`、`language`、`versions[]`（`versionId`、`contentHash`、`rawEvidence[]`、`assets[]`、`baselinePath`、`candidate/provenance/quality`、`createdAt`、`humanEditHash/checkpoint`）、`compiledConsumptions[]`、`restrictedContent`、`timeline[]`。

## 1.4 写入 / 删除 / checkpoint / rollback

- **写入**：`raw/sources/**`、`raw/web/**`、`raw/assets/**`、`wiki/sources/local/**`、`wiki/sources/web/<host>/**`、`.app/sources/{sourceId}.json`、`.app/source-index-v2.json`、`.app/import/**` history。路径白名单需同步加入 `raw/web`、`raw/assets`、`wiki/sources/local`、`wiki/sources/web`。
- **checkpoint**：纯新增不建高风险检查点（基准 §17.4）。`apply_import_candidate` / `manual_merge` / 覆盖既有 Source 前必须建 scoped checkpoint（沿用 `commit.rs` 已有的 `create_scoped_checkpoint(..., CheckpointPurpose::HighRiskOperation, ...)`）。
- **rollback**：单项失败清理该项部分写入（raw + source + manifest + index 条目 + assets），不回滚其他成功项。原子边界 = `sourceId`。
- **legacy 兼容**：`resolve_wiki_asset_path` 现有的只读 legacy fallback（`raw/sources/{sid}/{vid}/assets/...`）保留为 read-only，标注为可删边界，Batch 9 移除。

## 1.5 必须保留的现有能力

- `SourceResolution::{New, ExactDuplicate, UpdatedOrigin, SameContentNewOrigin}` 的去重语义与 `by_content_hash` / `by_locator` 索引（含拒绝重复 key 的手写 `Deserialize`）
- `portable_wiki_stem` 对 Windows 保留设备名的处理
- 外部编辑保护：`baseline_path` 与三方合并前提
- `FileTransaction` 事务与 `sources_promotion.rs` 已有断言中不与基准冲突的部分

## 1.6 要删除或改写的反基准测试

- `commit.rs` 内联测试中断言"URL commit 后 `wiki_path.is_none()` 且 manifest 指向的 Wiki 文件不存在"的用例 → 改写为断言可读 Source 存在且 frontmatter 与 manifest 一致
- `src-tauri/tests/import_v2_web_ingestion.rs` 所有"URL 成功但无 Source"断言 → 反向
- `src/types/importV2.ts` 的 `ImportItemCommitResult.wikiPath: string | null` 前端测试 → 断言成功项必有 `wikiPath`
- `CommitConflictAction` 相关测试 → 迁移到 `ImportItemResolution`

## 1.7 新增测试与 fixture

契约测试（review §12.2 第 1–4、19–21 条）：

- [ ] 任意 success item 必有 raw evidence + final Source + manifest/version + history
- [ ] local 与 URL Source 使用相同 final frontmatter 合同（同一组必填 key）
- [ ] frontmatter round-trip 与 registry 的 stable ID / hash 完全一致
- [ ] 不运行 Compile 也能打开 URL 来源的 Source
- [ ] 删除 staging 后 Source 仍可读
- [ ] `sourceId/versionId/contentHash` 不由 engine 伪造（engine 伪造时 commit 原子失败）
- [ ] frontmatter 不含 cookie / token / staging 路径 / session ID
- [ ] exact duplicate 不产生新 Source / version，只追加别名
- [ ] changed origin 产生同 Source 新 version
- [ ] human-edited current Source 进入三方合并
- [ ] `keep_current_source` 不推进 `current_version_id`
- [ ] 任一必填字段不一致时整项 commit 原子失败，且不留残留文件

fixture（新增到 `tests/fixtures/import-v2/`）：

- [ ] 一个 CJK Markdown + 相对资源
- [ ] 一个普通网页快照
- [ ] 两个平台 end-to-end fixture（Bilibili 有字幕、XHS 图文）
- [ ] 一个 legacy manifest（旧 schemaVersion）用于迁移测试
- [ ] 一个人工编辑过的 current Source + 新解析版（三方合并）

## 1.8 中英文 / 键盘 / CJK / path 覆盖

- `wiki/sources/web/<host>/` 的 host 归一化：IDN / punycode / 端口 / 大写 host / 尾点 host
- CJK 标题 → 稳定 slug；同名冲突走 `collision_free_wiki_path`
- Windows 长路径与保留设备名；macOS NFD 与 Linux 大小写敏感
- 本批无新 UI，键盘覆盖 N/A（在收尾报告里明确写 N/A 及原因）

## 1.9 退出门槛（= Gate A）

- [ ] P0-01、P0-02 关闭，P1-01 的目录与 manifest 部分关闭
- [ ] 普通网页 + 至少两个平台 end-to-end commit，每项都能用返回的 `wikiPath` 打开 Markdown
- [ ] exact duplicate / update / external edit 三条路径都不被破坏
- [ ] 单项失败不回滚其他成功项，且失败项零残留
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 2：Compile V2 bridge 与提交完成摘要

**目的**：让"导入完成"与"更新 Wiki"成为两个可追踪的独立动作。这是 Gate D。

**依赖**：Batch 1。

## 2.1 关闭的 finding

**P0-04**、**P1-10**。

## 2.2 基准章节

§1（导入与编译独立）、§3.1.3（导入不自动编译）、§14（提交完成与独立编译）、§17.4（提交后更新索引但不启动编译）、§19.1

## 2.3 现有调用链与要改的 DTO

**当前**：
- `src-tauri/src/models/compile.rs:100-107` `CompileRequest { project_id, project_root_path, route, agent, provider }` —— 无 source 变更集；`CompileResult { route, affected_paths, conflicts, checkpoint }` —— 无 consumed versions
- `src-tauri/src/services/compile_service.rs:115`、`:918` 固定读 `context.app_dir.join("source-index.json")`（legacy）
- `compile_service.rs:123-146` 混合 `raw/extracted/*.md` 与 `wiki/sources/*.md`
- `compile_service.rs:41-52、96-107` prompt 枚举 `raw/extracted` 与整个 `wiki`
- 前端 `useImportTaskCoordinator.ts:352-359` 确认结束只清理 confirming + 刷新 session；`useImportWorkflow.confirm` 只启动 commit task；`ImportView.tsx` 无完成摘要

**目标**：

```ts
interface CompileRequest {
  projectId: string;
  projectRootPath: string;
  sourceVersions: Array<{ sourceId: string; versionId: string; contentHash: string }>;
  route: "auto" | "agent" | "byok";
}
```

后端**必须从 registry 重新校验**每个 `sourceId / versionId / wikiPath / contentHash`，不信任前端传来的 path；hash 过期或被篡改则拒绝该项并给出用户可读原因。

`CompileResult` 增加 `consumedVersions[]`，并把消费记录写入 `.app/compile/`。

## 2.4 写入 / 删除 / checkpoint / rollback

- **写入**：`.app/compile/**`（消费记录）、派生 `wiki/**`（非 `wiki/sources/`）
- **写入守卫**：Compile 对 `wiki/sources/**` 的任何写入必须在服务层被拒绝并记为错误，不是静默跳过。加专项测试
- **checkpoint**：Compile 覆盖既有派生页沿用现有 Compile checkpoint 逻辑
- **legacy**：`.app/source-index.json` 移出主链路，包成独立 read-only 适配层（单文件、可删、有独立测试）。主链路只读 `.app/source-index-v2.json`

## 2.5 必须保留的现有能力

- 现有 Compile 的 Agent / BYOK 路线选择、进度、取消、冲突与 checkpoint（BYOK 在 Compile 中**合法**）
- `compile_instructions.rs` 的指令构建骨架，只收窄输入范围
- History 页重新启动编译的能力（基准 §14）

## 2.6 要删除或改写的反基准测试

- 断言 Compile 读取 `.app/source-index.json` 为主索引的测试 → 迁到 legacy 适配层测试
- 断言 Compile 枚举整个 `wiki` 的 prompt 测试 → 改为断言只读目标 Source + 必要派生页
- 任何"导入后自动编译"的测试 → 改为断言不会自动编译

## 2.7 新增测试与 fixture

review §12.2 第 5、6、22、23 条：

- [ ] Compile 只消费传入的 V2 `sourceId/versionId`；过期或篡改 hash 被拒绝
- [ ] Compile 记录 consumption 到 `.app/compile/`
- [ ] Compile 绝不写 `wiki/sources/**`（含尝试写入时的显式失败）
- [ ] completion summary 的 new / update / duplicate / warning / failure 计数精确（含部分成功）
- [ ] 点击"查看已导入来源"不触发 Compile；点击"用这些来源更新 Wiki"才触发
- [ ] 无变化的重复 Compile 被识别（已编译版本记录生效）
- [ ] legacy 项目通过 read-only 适配层仍可编译，且不写回 legacy 索引

前端测试：完成摘要渲染、两个并列动作、部分成功重试入口、`ImportCompletion` 到 UI 的表驱动映射。

## 2.8 中英文 / 键盘 / CJK / path 覆盖

- 完成摘要文案中英文都不溢出（820px 与桌面宽度）
- 摘要中的 Source 链接键盘可达，Enter 打开
- 摘要不出现 `sourceId` / `versionId` / hash（只在折叠技术详情）
- CJK Source 标题在摘要中正确截断，不破坏字形

## 2.9 退出门槛（= Gate D）

- [ ] P0-04、P1-10 关闭
- [ ] Import 永不自动 Compile（有测试）
- [ ] 用户显式 Compile 后可追踪 consumed versions
- [ ] 主链路不再依赖 `.app/source-index.json`
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 3：本地输入、格式 discovery 与 Source package

**目的**：把"格式支持"从 enum 声明变成真实端到端路线。这是 Gate B 的前半。

**依赖**：Batch 1（必须先有统一 finalization）。可与 Batch 2 部分并行，但 Batch 0 冻结的共享 DTO 必须已落地。

## 3.1 关闭的 finding

**P1-02**、**P1-03**、**P1-04**（route 接线部分；OCR/ASR 执行在 Batch 4）；交付 **P3-04** 的后端部分。

## 3.2 基准章节

§6.2（来源输入）、§11.1（支持范围）、§11.2（文件夹）、§11.3（PDF）、§11.4（Word/PPT）、§11.5（Excel/CSV）、§11.6（Markdown/TXT/HTML）、§11.7（本地音视频与伴随文件）、§4.4（来源包）、§19.6（扩展名与真实格式不一致）

## 3.3 现有调用链与要改的 DTO

**当前的阻塞事实**：

| 位置 | 现状 | 目标 |
|---|---|---|
| `src-tauri/src/models/import_v2_file.rs` | `FileFormat` 只有 `Markdown, Doc, Docx, Xls, Xlsx, Ppt, Pptx, Pdf` | 覆盖基准 §11.1 全矩阵：文档 + 图片 + 音频 + 视频 + 字幕 |
| `file_discovery.rs:271-294` | 按扩展名选 magic/container 检查，内容与扩展名不一致直接 unsupported | 内容识别优先；不一致时按可信内容分类并报"检测到的真实格式" |
| `file_discovery.rs:455-467` | TXT/CSV/HTML 折叠成 Markdown，图片/媒体/字幕 unsupported | 各自独立 format + route |
| `native_file_engine.rs:35-51` | 只支持 MD/TXT/CSV/HTML | 拆出 CSV/Excel package、HTML sanitize、Markdown 资源复制 |
| `orchestrator.rs:2602-2632` | 声明了 mp3/wav/m4a/mp4/mov/mkv 与字幕 route，但 path scanner 更早阶段已拒绝，声明不可达 | 打通 scanner → discovery → route |
| folder discovery `relative_path` | 未稳定传入最终 `ImportInput` | 作为元数据保留到预览与 Source |
| `pdf_router.rs` `inspect_pdf` / `plan_pdf_pages` | 只被 `tests/import_v2_pdf_routes.rs` 使用 | 接入生产 orchestrator |
| `office_postprocess.rs` `WorkbookPlan` / `PresentationPlan` | 只被 `tests/import_v2_office_quality.rs` 使用 | 接入生产 orchestrator |
| `src/services/importV2.ts:49` `addImportTextV2` | 后端 `add_import_text_v2` 已存在，前端 API 已存在，**零 UI 消费者** | 接上真实粘贴入口 |
| `orchestrator.rs:369` `add_text_input` | 把剪贴板文本伪装成 `ImportInputKind::File`，落 `.app/import-sessions/{sid}/inputs/{uuid}.{ext}` | 改为真正的 `clipboard_text` kind，证据与隐私边界写入 manifest |

**Source package**（基准 §4.4）：一个 `sourceId` 对应多个可读文件，一起更新 / Diff / 删除 / 版本化。

- Excel 工作簿：`index.md` + 每个非空 Sheet 子页
- 超大 CSV：`index.md` + 连续行分片
- **零截断**：不静默截断；超大数据先显示预计文件数与体积，用户确认后继续

## 3.4 写入 / 删除 / checkpoint / rollback

- 新增写入：`raw/sources/**`（本地原件，含音视频完整原件）、`raw/assets/**`（Markdown/HTML 的本地与远程资源）、`wiki/sources/local/**`（含 package 子页）
- Markdown 相对图片与附件随来源复制并改写为稳定路径；本地 HTML 移除脚本 / 事件处理 / 跟踪像素 / 危险嵌入后转 Markdown；有意义的远程图片下载到来源资源目录；iframe / 宏 / 脚本 / 外部对象不运行
- package 的 rollback 边界仍是 `sourceId`：任一子页写入失败，整个 package 回滚
- 加密 PDF：检测后显式提示"当前不支持"，不写 raw、不建 Source（基准 §3.2、§11.3.8）

## 3.5 必须保留的现有能力

- 现有 PDF / OOXML extraction service 与质量 metadata
- `media_router.rs`、`subtitle.rs` 已有的解析能力（本批只是让本地路线也能用到）
- 文件夹扫描的忽略规则：隐藏 / 系统 / 临时文件与 `.git/`、`.app/`、`node_modules/`
- 现有 discovery 的路径安全校验与 magic 检查骨架

## 3.6 要删除或改写的反基准测试

- 断言图片 / 音频 / 视频 / 字幕为 `unsupported` 的 discovery 测试 → 反向
- 断言 TXT/CSV/HTML 折叠为 Markdown format 的测试 → 拆分
- 只在单元层验证 `inspect_pdf` / `plan_pdf_pages` / `WorkbookPlan` / `PresentationPlan` 的测试 → 保留单元测试，但**必须新增**走生产 orchestrator 的集成测试（review §14 禁止第 3 条）
- 把剪贴板文本当 file kind 的测试 → 改为 `clipboard_text`

## 3.7 新增测试与 fixture

契约测试（review §12.2 第 10、11、12 条）：

- [ ] clipboard text → preview → confirm → Source（走统一 session / quality / duplicate / commit）
- [ ] 完整本地格式表驱动 discovery + route（基准 §11.1 全矩阵逐项）
- [ ] PDF mixed pages 只 OCR 必要页（本批只验证 route plan 正确，执行在 Batch 4）
- [ ] 扩展名与真实格式不一致时按内容分类，并在 UI 报"检测到的真实格式"
- [ ] folder import 不创建 folder Source；`relative_path` 保留到 Source 元数据
- [ ] folder 的 unsupported 文件在扫描摘要中单列，不混入失败项
- [ ] Excel workbook → 一个 `sourceId` + `index.md` + Sheet 子页；公式显示值在正文、公式本身在证据
- [ ] 超大 CSV 分片且零截断；超大时先预确认
- [ ] Markdown 相对资源复制 + 路径改写；本地 HTML sanitize（脚本 / 事件 / 跟踪像素被移除）
- [ ] 加密 PDF 显式不支持，不写 raw、不建 Source
- [ ] 动态 GIF 按视频型媒体处理（抽帧路线在 Batch 4）

真实 fixture（review §12.4，不得只用 hand-built DTO）：

- [ ] 多页混合文字 / 扫描 PDF
- [ ] encrypted PDF
- [ ] 大 workbook（多 sheet、公式、空行、Unicode sheet name）
- [ ] 含 notes / image 的 PPTX
- [ ] CJK Markdown + 相对资源
- [ ] 有字图片 + 无有效文字图片
- [ ] 有语音媒体 + 无有效语音媒体
- [ ] companion subtitle（同名 SRT/VTT/ASS/LRC）
- [ ] 磁盘不足 / 取消中断 fixture

## 3.8 中英文 / 键盘 / CJK / path 覆盖

- CJK 文件名 / Unicode sheet name / 长路径 / Windows·macOS·Linux 风格 / 大小写敏感
- 粘贴入口键盘全流程：聚焦 → 粘贴 → 预览 → 确认；**不监听全局 `Ctrl+V` 静默导入**（基准 §6.2）
- "检测到的真实格式"文案中英文都完整
- 图片剪贴板不在首版范围，需显式给出不支持提示而非静默失败

## 3.9 退出门槛（= Gate B 前半）

- [ ] P1-02、P1-03 关闭；P1-04 的 route 接线部分关闭
- [ ] 基准 §11 格式矩阵按真实 fixture 通过
- [ ] folder 不生成 Source
- [ ] 无 truncation
- [ ] helper 不再只存在于单元测试（有生产集成测试证据）
- [ ] 每加一种输入都重跑 Gate A 契约测试
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 4：统一 OCR、字幕、ASR 与媒体门禁

**目的**：让"没有有效正文就没有 Source"成为所有媒体的统一门禁。这是 Gate B 的后半。

**依赖**：Batch 3（格式与 discovery）；复用 Batch 1 的 finalization。

## 4.1 关闭的 finding

**P0-05**（门禁闭环部分）、**P1-08**、**P1-09**；交付 **P1-04** 的 OCR 执行部分。

## 4.2 基准章节

§8（OCR 启用逻辑）、§9（ASR 与字幕逻辑）、§10（多语言）、§11.3（PDF 选择性 OCR）、§11.7（本地音视频与伴随文件）、§11.8（本地媒体能力包）、§12.7（远程媒体下载）、§19.2、§19.3

## 4.3 现有调用链与要改的 DTO

| 位置 | 现状 | 目标 |
|---|---|---|
| `ocr_router.rs` `OcrRouter` | 只被 `tests/import_v2_ocr.rs` 使用 | 接入生产：standalone image / PDF 选择性页 / XHS 图文 / 视频画面 |
| `orchestrator.rs:2491-2502` | XHS URL 且 OCR route 已注册时直接返回"已授权" | 保存**明确的 session-scoped 授权**；"能力已安装" ≠ "用户已授权本次 OCR" |
| `media_router.rs:25-31、125-130` | 平台人工字幕与本地人工字幕同级；automatic 排在 embedded 前 | 按基准 §9.1 / §9.2 的语言 + 来源 + 可靠性优先级重排 |
| `subtitle.rs:46-53` | 可解析 VTT/SRT/ASS/SSA/JSON，**无 LRC**，但其他 route/readiness 已宣称 LRC | 补 LRC |
| ASR 授权与 continuation | 围绕特定 web media target | 抽取通用 `MediaTranscriptPlan`，本地媒体与 web media 共用门禁 |
| transcript 结构 | 无统一锚点策略 | 每 30–60 秒或主题停顿保留时间锚点；不每句堆时间戳 |
| ASR 首次启用弹窗 | 未完整展示模型 / 设备 / 依赖链 / 预计资源 / 偏好 | 按基准 §9.3 / §9.4 补全 |

**统一门禁**（基准 §9.1 / §9.2）：

```text
字幕获取顺序：
1. 平台或作者提供的原语言字幕
2. 媒体内嵌字幕
3. 同目录可靠伴随字幕或稿件
4. 已存在的可信转录结果
5. 本地 ASR

字幕轨道优先级：
1. 作者上传的原语言字幕
2. 平台生成的原语言自动字幕
3. 作者上传的其他语言字幕
4. 平台机器翻译字幕
```

**只有机器翻译字幕、没有原语言字幕时，仍视为缺少可靠原稿，要求本地 ASR。**

**fail-closed 规则**：

- 音频 ASR 无有效语音或低于最低质量门槛 ⇒ 不生成 Source、不把音频提交到 `raw/`、显示"无法生成来源：未识别到有效语音"，提供调整语言重试 / 重新转录 / 让 Agent 尝试 / 移除
- 视频 ASR 无效 ⇒ **条件性**画面 OCR：先检查是否存在大量稳定文字画面（不直接跑完整 OCR）→ 命中后进入"需要操作：识别视频画面文字"→ 用户启用后按场景变化抽关键帧、OCR、去重 → 得到有效文字才生成 Source
- 两者都无有效文字 ⇒ 不落 `raw/`、不生成 Source
- 已有完整语音稿的普通视频**不自动**做画面 OCR
- standalone image OCR 无有效文本 ⇒ 显示"无法生成来源：未识别到文字"，不写 `raw/`、不创建 Source、不影响批次其他项

**OCR 触发判断**（基准 §8.1）：先提取已有文本，再判断图片是否承担正文。独立图片 / 扫描 PDF / 聊天记录长图默认视为正文载体；图片轮播即使已有配文也进 OCR 候选；网页封面 / 头像 / 装饰图 / 普通插图不提示 OCR。**不为判断而偷偷运行完整 OCR。**

## 4.4 写入 / 删除 / checkpoint / rollback

- OCR / ASR 原始输出与原始分段、逐词时间写入 `raw/`（基准 §4.2、§9.5）；Source 正文只用规范化可读版本
- OCR 内容与作者原文分开保留和展示；低置信度区域可定位到页码 / 图片 / 时间点
- 临时媒体在任务完成 / 取消 / 失败清理后删除；本地原媒体在确认导入后完整保存到 `raw/`
- 取消任务清理临时媒体与中间分片，之后重试从头开始；重启进入"已暂停，可继续"，已完成分片不重做
- 无有效正文的失败项：零写入（本批最重要的 rollback 断言）

## 4.5 必须保留的现有能力

- Bilibili 本地 ASR continuation 的**真实进度、取消、临时媒体安全约束**（最近两次提交刚修好，不要回退）
- XHS OCR continuation 的解析能力（并入统一 contract，不作平台旁路）
- 能力包安装的显式确认与来源 / 许可证 / 下载量 / 磁盘占用 / 安装位置展示（基准 §11.8）
- 依赖链解析：媒体处理能力 → ASR 引擎 → 模型 → 语言包，一次汇总，中途失败保留已成功组件并从失败步骤重试

## 4.6 要删除或改写的反基准测试

- 断言"OCR 能力已安装即视为已授权"的测试 → 改为要求 session-scoped 显式授权
- 断言 automatic 字幕优先于 embedded 的测试 → 按新优先级
- 断言无语音媒体仍产生 Source 的测试 → fail-closed
- 断言视频默认跑画面 OCR 的测试 → 条件性

## 4.7 新增测试与 fixture

契约测试（review §12.2 第 9、12、13、14、17 条）：

- [ ] 音视频无有效 transcript / no speech ⇒ 无 raw、无 Source、不可提交
- [ ] PDF mixed pages 只 OCR 必要页（生产执行，非仅 plan）
- [ ] standalone image OCR 失败不产生 Source，且不影响批次其他项
- [ ] local video 优先 companion subtitle，否则 local ASR
- [ ] 多个伴随字幕候选 ⇒ "需要操作：选择字幕"
- [ ] restart 只恢复未完成 shard；完成 shard 不重复下载 / OCR / ASR
- [ ] 只有机器翻译字幕时仍要求本地 ASR
- [ ] 完整原语言自动字幕可直接使用，不强制 ASR
- [ ] transcript 锚点密度在 30–60 秒区间，不每句堆时间戳
- [ ] 导入阶段不做 AI 章节猜测（有章节按原章节，无章节只按自然段与时间段）
- [ ] OCR 授权只对当前会话有效；下一会话重新确认
- [ ] 语言包缺失 ⇒ "安装对应识别包" ⇒ 安装后自动恢复
- [ ] 中英混合内容按同一次识别处理；导入不翻译
- [ ] 视频 ASR 无效 → 条件性画面 OCR → 有效文字才生成 Source；两者都无则零写入

fixture：复用 Batch 3 的有 / 无语音媒体、有 / 无文字图片、companion subtitle、混合 PDF；新增机器翻译字幕 fixture 与"稳定文字画面"视频 fixture。

## 4.8 中英文 / 键盘 / CJK / path 覆盖

- ASR 首次启用弹窗中英文都不溢出；"仅在本机处理"必须可见
- 快速 / 均衡 / 高质量与语言自动检测 / 手动指定键盘可达
- CJK 语音识别与中英混合；CJK 字幕文件名匹配
- OCR 结果定位信息（页码 / 图片序号 / 时间点）在中英文下都可读

## 4.9 退出门槛（= Gate B 后半）

- [ ] P0-05、P1-08、P1-09 关闭；P1-04 的 OCR 执行部分关闭
- [ ] 任意媒体 success 必有有效正文
- [ ] 取消和 restart 不重复已完成重任务
- [ ] OCR 授权是 session-scoped 显式授权，不是能力探测
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 5：Web/平台媒体、登录和远程原件策略

**目的**：让平台接入共享同一套 Source finalization、登录、OCR/ASR、保留策略与质量门禁；平台 provider 只负责发现与证据，不各自定义"成功"。

**依赖**：Batch 1、Batch 4。

## 5.1 关闭的 finding

**P1-07**、**P1-12**、**P1-13**；交付 **P2-08** 的 item 级确认部分；交付 **P2-11** 的后端批量恢复部分。

## 5.2 基准章节

§7（登录态流程）、§12.1（普通网页）、§12.2（未知平台）、§12.3（视频平台）、§12.4（图文平台）、§12.5（复合平台内容）、§12.6（集合、播放列表与作者主页）、§12.7（远程媒体下载）、§12.8（远程图片）、§16（Agent 导入修复）、§19.4

## 5.3 现有调用链与要改的 DTO

| 位置 | 现状 | 目标 |
|---|---|---|
| `connector_session.rs:26-31` `ConnectorSessionRef` | 只有 session / platform / profile_ref / state | 加安全账号摘要 + last verified；**不返回 cookie / profile path** |
| `ImportLoginDialog.tsx:89-103` | 显示 connector / domain / state / **session ID** | 显示平台 + 账号或资料摘要 + 最近验证时间 + 打开登录 / 重新验证 / 退出并清除 |
| `useImportSupportingActions.ts:271-287` | 登录成功只 `startItems([itemId])` | 后端原子恢复同平台**全部**等待项 |
| `import_v2_web_commands.rs:223-234` | 只绑定 / 释放当前 item | 平台级批量释放 |
| `release_item_after_login` | 单项从 `WaitingLogin` 释放到可重试失败态，再靠前端重启 | 后端原子恢复，不经过失败态 |
| `orchestrator.rs:2537-2595` | X/Twitter 明确无可用 route | 明确处理"支持 / 缺能力 / 本地 Agent 恢复"，不静默无路径 |
| 集合 URL | 直接进入单 item 处理 | 独立 discovery entity / command：先展示子项 → 用户选择 → 才创建 child items |
| `MediaSaveMode` | Batch 0 已改默认 extract-only | item `…` 提供"保留远程原件"，后端先给预计大小 / 磁盘空间 / 质量 |

**登录默认策略**（基准 §7.1）：已有有效登录态自动复用 → 无登录态先匿名解析 → 只有匿名拿不到核心内容 / 平台明确要求登录 / 内容属账号权限范围时才进"登录并继续" → 登录成功自动恢复原任务 → **关闭登录窗口只表示暂不登录，任务继续等待，不算失败**。

**受限内容**（基准 §7.4）：当前账号本来有权访问的私密 / 仅成员 / 付费内容可以导入；Source 标记 `private` 或 `restricted`，记录平台身份摘要，**不记录 Cookie**；当前项目第一次提交受限内容前提示一次，后续只显示锁形标记；导出包含受限来源时导出流程再次提示。**不绕过付费墙、验证码、风控或访问权限。**

**`import-recovery` skill 合同**（P1-12，基准 §16）：全仓当前无此 skill，需要建立。

- 可以：读当前来源 / 页面证据 / 媒体 / 转换产物 / 日志；编写运行临时 Python·Node·Shell·PowerShell 脚本；用现有命令行、浏览器、已安装能力包、OCR、ASR、媒体工具；分析页面脚本 / 网络请求 / 公开 API / 同站资源 / 公开技术文档；多轮尝试与验证；下载普通内容与临时数据
- 不可以：静默安装软件；执行未知下载二进制；读取或输出原始 Cookie / API Key / 秘密；绕过验证码 / 风控 / 付费墙 / 访问权限；直接修改 `raw/` / `wiki/` / Git
- 只写 staging 候选；候选仍需质量检查、预览、用户确认

## 5.4 写入 / 删除 / checkpoint / rollback

- 页面证据、平台证据、原始字幕 → `raw/web/`；资源 → `raw/assets/`；远程原件仅在用户显式选择"保留原始媒体"并确认清晰度 / 预计大小 / 磁盘影响后永久保存
- 远程图片：图片帖子保存可访问的原始发布尺寸（不用列表缩略图）；网页文章保存正文实际使用的最高有效尺寸；过滤头像 / 图标 / 跟踪像素 / 重复缩略图；同一图片多尺寸只留最佳版本
- **主体图片缺失阻断提交；普通配图缺失只警告**
- **文本 / 字幕 / ASR 成功与远程原件保留失败互不回滚**
- cookie / profile 永不进入 React / project / log / export（本批最重要的安全断言）

## 5.5 必须保留的现有能力

- generic Readability + 浏览器升级分层；SSRF / redirect / DNS 防护
- WeChat / Zhihu / Bilibili / XHS / Douyin 已有 platform route（`connectors/` 下 5 个 connector）
- 显式 begin / check / revoke 登录；登录窗口关闭不被伪装成成功
- capability-pack、browser session、受控 remote asset 解析基础

## 5.6 要删除或改写的反基准测试

- 断言登录成功只恢复单项的测试 → 平台级批量恢复
- 断言登录对话框显示 session ID 的测试 → 安全摘要
- 断言 `release_item_after_login` 经过失败态的测试 → 原子恢复
- 断言远程媒体默认永久保留的残留断言（Batch 0 已改默认，本批清理）

## 5.7 新增测试与 fixture

契约测试（review §12.2 第 15、16 条 + 基准 §19.4）：

- [ ] 同平台多个 waiting item 一次登录后全部恢复
- [ ] remote media 默认不永久保留
- [ ] 有效登录态自动复用；无登录态先匿名尝试
- [ ] 关闭登录窗口不算失败，任务继续等待
- [ ] 登录后仍无权限的项标记"当前账号无权访问"，不反复要求登录
- [ ] 同平台登录失效只暂停该平台受影响项
- [ ] 受限内容：首次提交前提示一次 → 后续只锁形标记 → Source 带 `restricted` → 导出再次提示
- [ ] cookie / token / profile path 不出现在任何 DTO、日志、frontmatter、导出
- [ ] 普通网页：HTTP 轻量 → 正文过短/JS 依赖时升级浏览器 → 登录墙 → Agent 修复；任一层成功即停止，不跑更重路线
- [ ] 未知平台：普通网页可得完整文章 ⇒ 作为 `web` 来源；主要是媒体且无正文 ⇒ "当前平台没有专用解析能力" + 安装并继续 / 让 Agent 尝试导入
- [ ] 集合 URL：只发现不建任务 → 展示总数 / 时长 / 预计需登录·ASR 数量 → "加入导入队列 N 项"才创建 → 每项独立 Source，合集本身只记录关系 → 再次导入只加新增或变化项
- [ ] 复合平台内容：一个详情页一个 `sourceId`，原配文 / 图片 / 视频按原顺序；任一必需处理未完成整条停在"需要操作"；**不拆成图片 Source 和视频 Source**
- [ ] 图文平台：标题+配文 → 图片按平台顺序 → 每图下紧跟该图 OCR 文本并标"图片文字"；评论 / 推荐 / 导航不导入
- [ ] 远程原件保留失败不影响 Source 文本
- [ ] `import-recovery` skill：只读当前 item 证据、只写 staging；尝试写 `raw/`/`wiki/`/Git 或输出秘密时被拒绝

fixture：普通网页、Bilibili、有图 XHS、受限内容、URL redirect、登录取消、能力缺失、集合 / 播放列表 / 作者主页。

## 5.8 中英文 / 键盘 / CJK / path 覆盖

- 登录对话框、受限内容提示、集合发现预览的中英文长文案
- 集合发现列表键盘可达（全选 / 反选 / 分页继续加载）
- `wiki/sources/web/<host>/` 的 IDN host 与 CJK 标题
- 平台 slug 与 CJK 标题的稳定映射

## 5.9 退出门槛

- [ ] P1-07、P1-12、P1-13 关闭；P2-08 的 item 级确认关闭
- [ ] 基准 §7、§12、§19.4 场景通过
- [ ] cookie / profile 永不进入 React / project / log / export（有专项测试）
- [ ] remote preserve 失败不影响 Source 文本
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 6：Import 工作台、会话和批量效率

**目的**：把工作台从"诊断面板"改成"批量导入工具"。这是 Gate G 的主体，也是本整改中前端改动最集中的一批。

**依赖**：Batch 0 的 presentation DTO 必须已落地；最好在 Batch 3/4/5 的 blocker 类型稳定后再做，避免七类状态映射反复改。

## 6.1 关闭的 finding

**P1-05**（UI 逐项决定）、**P1-06**、**P2-01**（计数与 CTA）、**P2-02**、**P2-03**、**P2-04**、**P2-05**、**P2-06**、**P2-07**、**P2-10**、**P2-11**（前端）、**P2-12**、**P3-01**；交付 **P3-02** 的新增 key 部分、**P3-04** 的前端消费部分。

## 6.2 基准章节

§2（术语与用户文案）、§5.1（会话模型）、§5.2（七类状态）、§5.3（重启恢复）、§6.1–§6.7（页面构成全节）、§13.2（逐项更新）、§13.3（部分提交）、§18（错误与质量文案）

## 6.3 现有调用链与要改的 DTO

**工作台结构**（基准 §6.1，自上而下）：

```text
Header / tabs        工作台 | 历史 | 能力管理
Compact input        文件 | 文件夹 | 粘贴链接 | 粘贴文本/Markdown
Capability strip     icon + name + status dot
Grouped todos        登录 / OCR / ASR / 能力安装
Continuous queue     checkbox | 类型 | 标题 | 七类状态 | 一个主动作 | …
Fixed commit bar     新增 N · 更新 N · 警告 N · 待处理 N   [导入到来源库 N 项]
Right inspector      状态 -> 唯一主动作 -> 快速预览 -> 目标/版本 -> 质量 -> 原始资料 -> 折叠技术详情
```

**逐项改造点**：

| finding | 现状锚点 | 目标 |
|---|---|---|
| P2-10 | `ImportSourceMethods.tsx:148-225` 两个大 article pane，能力矩阵嵌在输入区 | 紧凑输入条 + 四个第一等入口 + 当前发现/扫描进度（已发现 / 已跳过 / 当前阶段 / Cancel 在主工作台可见，不只靠全局任务抽屉） |
| P2-02 | `ImportSourceMethods.tsx:300` 用 `<span className="import-v2-source-tile">`；`ImportCapabilitiesPanel.tsx:40-98` 只读 | 原生 `<button>`；格子只显示 icon + name + status dot（**不直接显示状态文字**）；悬停或键盘聚焦显示状态摘要；点击或 Enter 固定 popover；Escape 或点击外部关闭；必须有可访问名称，不能只靠颜色；完整管理进"能力管理"页签 |
| P2-06 | `importStatusPresentation.ts` 把 15 个内部阶段直接映射到顶层 UI | 新增稳定 `ImportUserState` 派生层；队列筛选、颜色、图标、统计、空状态只用七类；细阶段作副标题或日志 |
| P2-03 | `ImportBatchStatus.tsx` 聚合后台 task 状态 | 从 item blocking reason 生成 action group：某平台登录 N 项 / 需要 OCR N 项 / 需要 ASR N 项 / 需要安装能力 N 项；**不连续自动弹出多个模态框**；解决一种待办后其余继续留在状态区 |
| P2-04 | 多项 action 与复制 locator 长驻行内 | 每种状态只显示一个主动作；Retry/Login/Install/Resolve/Preview 按状态竞争；Skip/Cancel/Copy/Logs/保留远程原件/技术详情收进 `…`；**行选中与提交复选框语义分开** |
| P2-01 | `ImportCommitBar.tsx` 只显示 selected/unresolved，CTA 泛化 | `新增 N · 更新 N · 警告 N · 待处理 N` + `导入到来源库 N 项`；所有可提交项默认勾选；失败 / 等待操作 / 未解决冲突 / 完全重复项不可勾选 |
| P1-05 | `CommitConflictAction` 三值 + 全局赋值 | 每项用 `ImportItemResolution`，绑定 `sourceId + candidateHash + currentHash + targetVersionId`；未解决项只阻断自身 |
| P2-05 | 右栏无真正快速预览；全量预览直出 session/item/candidate ID 与 SHA；图片被省略 | 右栏快速预览最终可读结果；全量预览展示 Markdown + 图片/资源 + 来源信息 + 目标路径 + 质量 + 版本；内部 ID/hash/engine/path 只在折叠技术详情；update 默认展示 current/imported/merged 三方关系；**批次选择留在队列，不在预览里复制一套** |
| P2-07 | `ImportRightPanel.tsx:63-111`、`ImportQueue.tsx`、`ImportHistoryDetailDialog.tsx`、`useImportSessionScope.ts:15-23` 先显示 code / raw message | 用 Batch 0 的 `UserIssue`：先答"发生了什么 / 数据是否安全 / 下一步能做什么"；`technicalCode/technicalMessage/route/engine/hash/artifactPath` 只进折叠详情 |
| P3-01 | 右栏是诊断面板 | 严格按基准顺序：来源/状态 → 唯一主动作 → 快速候选预览 → 最终路径与 Source/version → quality/issues → 原始资料 → 技术详情/attempt timeline/log |
| P2-12 | active tab 是组件本地 state；各页签滚动位置不保存；dropzone 有 focus 外观但无完整 Enter/Space；媒体预选弹窗未用统一 modal hook；Workspace header 与 Import header 可能重复主标题；queue 大范围 `aria-live` | tab/filter/scroll 按项目恢复；dropzone 完整键盘行为；统一 modal hook；单一主标题；live region 不重复播报整列表 |

**会话语义**（P1-06，基准 §5）：

| 现状锚点 | 目标 |
|---|---|
| `useImportSessionScope.ts:157-161` 只要 project/session 相同就复用 | 无未提交项后 session 结束；下一次 Add 创建新 session |
| 后端 `create_session` 未用 `find_unfinished_session` 强制"一项目一活跃 session" | 后端强制；绕过前端也不能创建多个活跃 session |
| `session_store.rs:214-233` `add_inputs` 向现有 session 追加，不拒绝 completed session | 拒绝向 completed session 追加，改为创建新 session |
| committed items 仍留在 `session.items` | 移入只读完成摘要 / 历史投影，不占活跃队列 |
| `Paused` 枚举存在但无 session 级 `resumeAll` | 提供"继续全部" |
| restart reconciliation 把缺失/失败 task 的 Waiting/Inspecting/Extracting/Validating item 转 Failed 并清理 staging | 统一进入 Paused、保留已完成 shard、等待 Resume All |
| 重任务 shard 无"可复用 / 需重跑 / 已完成"建模 | 建模并复用；取消则清理临时产物并从头重试 |

## 6.4 写入 / 删除 / checkpoint / rollback

- 本批主要改前端与 session 状态，不新增项目文件写入路径
- session / tab / filter / scroll 的 UI 偏好按项目持久化到 `.app/`（不要写进 `wiki/`）
- 所有异步项目态提交必须用 `projectKey (projectId + rootPath)` + epoch 守卫；view state / drawer / navigation / toast 各自独立守卫
- **例外**：有效后端 task 记录始终 upsert 到全局 `taskStore`（含项目切换后）；但过期项目的结果不得打开或接管当前项目的 drawer

## 6.5 必须保留的现有能力

- `AppShell -> WorkspaceController -> WorkspaceRouter -> lazy view` 结构，`React.lazy` / `Suspense` / `ViewErrorBoundary` / type-only imports 跨 bundle 边界
- 聚焦 workflow 拆分（不合并成巨型 hook）
- Import 确认顺序：`confirm_import_preview -> wikiStore.scan -> optional start_wiki_compile`（其中 compile 改为**仅在用户显式点击"用这些来源更新 Wiki"时**触发）
- 三个页签存在；切换页签不丢会话、任务、筛选、滚动位置
- 现有拖放、native file picker、locator 复制能力

## 6.6 要删除或改写的反基准测试

- `ImportCommitBar` 全局 select 测试（Batch 0 已删控件，本批清理残留）
- 断言 15 个内部状态直出 UI 的 `importStatusPresentation.test.ts` 用例 → 改为七类表驱动
- 断言 `ImportBatchStatus` 聚合后台 task 状态的测试 → 改为按 blocking reason 聚合
- 断言 completed item 留在活跃队列的测试 → 反向
- 断言 restart 把等待项转 Failed 的测试 → 改为 Paused + Resume All
- 断言 capability tile 是 `<span>` 的测试 → 改为 button + 键盘语义

## 6.7 新增测试与 fixture

契约与 UI 测试（review §12.2 第 18 条 + §12.3 全节）：

- [ ] completed session 再 Add 创建新 session
- [ ] 七类用户状态表驱动映射（15 个内部状态 → 7 类，逐项断言）
- [ ] batch todos 按 login / OCR / ASR / capability 聚合；一次成功动作恢复该组全部适用项
- [ ] fixed bar 无全局 conflict select；计数精确（新增 / 更新 / 警告 / 待处理）
- [ ] queue 每行一个主动作；`…` 键盘可达
- [ ] capability tile Tab / Enter / Escape / 点击外部关闭；有可访问名称
- [ ] 快速 / 完整 preview 不泄露 internal IDs
- [ ] 中英文长文案在 820px 与桌面宽度都不溢出
- [ ] keyboard-only 走通 file / url / clipboard / commit / preview
- [ ] screen reader live region 不重复播报整个队列
- [ ] active tab / filter / scroll 返回 Import 后按项目恢复
- [ ] 未解决项只阻断自身，其他可确认项可提交（部分提交）
- [ ] restart 后重任务显示"已暂停，可继续"，点击"继续全部"后已完成分片不重做
- [ ] 正常模式不出现 `staging` / `artifact` / `manifest` / `baseline` / session ID / route / engine / SHA / error code

## 6.8 中英文 / 键盘 / CJK / path 覆盖

- 全部新增 key zh-CN 与 en 同步；文案模型按 P3-02 新增：文本/Markdown 粘贴、七类状态、四个计数、完成摘要、查看已导入来源 / 更新 Wiki、受限内容与导出提示
- 键盘-only 全流程；焦点顺序与 popover 焦点陷阱
- CJK 长标题在队列行截断不破字形；CJK 路径在预览与技术详情正确显示
- 视觉密度按 `UI-Frontend-design/assets/app.css`：UI 正文 13px、次要 12px、muted/mono 11px、小标签 10.5px；顶栏 48px、主区头 52px、右面板头 52px、状态栏 28px、导航项 30px、面板头 44px；section 标签 10.5px 大写 `letter-spacing: 0.08em`；只引用 `src/styles.css` token，不硬编码 hex

## 6.9 退出门槛（= Gate G 主体）

- [ ] P1-05（UI）、P1-06、P2-01 至 P2-07、P2-10 至 P2-12、P3-01 关闭
- [ ] 基准 §5、§6、§18 的 UI 验收通过
- [ ] 键盘-only 与中英文窄宽屏通过
- [ ] normal mode 无内部 ID
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 7：Source reader、右栏和 Source 生命周期

**目的**：让 Source 在 Wiki 里被当作 Source 对待，而不是普通 Markdown。这是 Gate E。

**依赖**：Batch 1（final Source 合同）、Batch 2（完成摘要与导航）。

## 7.1 关闭的 finding

**P0-06** 的非 AI 部分、**P1-11**。

## 7.2 基准章节

§13.4（删除来源）、§15.1（页面位置）、§15.6（右侧面板）、§15.7（版本时间线）、§19.5（Source 阅读）

## 7.3 现有调用链与要改的 DTO

| 位置 | 现状 | 目标 |
|---|---|---|
| `src/types/wiki.ts:15-31` `WikiPageMeta` | 无 `sourceId/versionId/sourceStatus/quality` | 加稳定 Source 绑定字段，供 reader 检测 |
| `src/features/wiki/WikiView.tsx` 工具栏 | 只有通用 read/edit/HTML preview/export/Ask AI/bookmark | **只新增一个 Source 专属按钮 `AI 整理`**（实现在 Batch 8，本批只占位并保证只对 Source 出现） |
| `WikiView.tsx:534-561` | 通用 `MarkdownReader` | 只有 `type: source` 且 registry binding 有效时进入 Source mode |
| `src/components/app/RightContextPanel.tsx:269-317` | Wiki 用通用 related pages / page chat | Source 专属右栏（按基准 §15.6 的八段顺序） |
| `src/features/wiki/WikiTree.tsx:276-293` | 对所有文件统一暴露 rename/delete | Source 走专用 `sourceId` 命令；专用流程完成前先隐藏通用危险动作 |
| `WikiView.tsx:249-271`、`wikiStore.ts:439-468` | 通用 Wiki delete | 专用 Source delete |
| `WikiPageFormDialog.tsx` | 允许普通新建/保存 Markdown，无 registry 边界 | **禁止普通新建 Source**（不能伪造 `type: source` 绕过 Import） |

**需要新增的 Source lifecycle commands**（review §9.7）：`get_source_detail`、`preview_source_update`、`apply_source_candidate`、`list_source_versions`、`restore_source_version`、`preview_delete_source`、`delete_source`、`reprocess_source_ocr/asr/refresh`。（`start_source_ai_organize` 在 Batch 8。）

**右栏顺序**（基准 §15.6，不得重排）：

```text
1. 来源与当前状态
2. 一个最重要的主操作
3. 候选预览
4. 目标路径和证据保留说明
5. 质量与问题定位
6. 原始稿
7. 版本时间线
8. 折叠技术详情与日志
```

导入完成后的媒体处理操作也放右栏，不加顶部按钮：视频/音频 → 重新转录、更换字幕；图片/扫描文档 → 重新识别文字；平台来源 → 刷新来源。处理完成后生成新候选版本，Diff 后确认更新。

**版本时间线**（基准 §15.7）只记录有意义的事件：来源导入版本、OCR/ASR 补充或重做、AI 整理应用、有意义的人工编辑检查点、来源刷新、版本恢复。**不记录每次按键**；只有可靠快照提供恢复操作。

**删除来源**（基准 §13.4）：项目没有回收站。删除进入独立二次确认页，显示将删除的 Source Markdown、raw 证据、图片、字幕、转录、基线、所有版本 + 预计释放空间 + 引用该 Source 的 Wiki 页面数量；派生 Wiki 页面不自动删除（后续 Lint 标记引用缺失）；确认按钮文案 `永久删除此来源`；删除前自动创建 Git 检查点；以 `sourceId` 为原子边界；记录轻量审计（名称、时间、检查点、结果）。

## 7.4 写入 / 删除 / checkpoint / rollback

- **删除路径**：`wiki/sources/**`（含 package 全部子页）、`raw/sources/**`、`raw/web/**`、`raw/assets/**` 对应条目、`.app/sources/{sourceId}.json`、`.app/source-index-v2.json` 条目、`.app/source-artifacts/**` baseline
- **checkpoint**：删除前、rename/move 前、apply candidate 前、restore version 前必须建 Git 检查点
- **rollback**：整个 Source package 原子事务；任一步失败全量回滚，不留半删状态
- rename/move 必须同步更新 manifest `wiki_path`（普通改名会破坏它，这是 P1-11 的核心风险）

## 7.5 必须保留的现有能力

- 通用 Wiki 的 scan / read / save / rename / delete / conflict 对**非 Source** 页面完全不变
- 现有 `MarkdownReader`、bookmark、HTML preview、export
- 外部编辑保护与冲突提示

## 7.6 要删除或改写的反基准测试

- 断言 `WikiTree` 对所有文件暴露 rename/delete 的测试 → Source 走专用路径
- 断言可通过 `WikiPageFormDialog` 新建 `type: source` 页面的测试 → 拒绝
- 断言通用 delete 可删 `wiki/sources/*.md` 的测试 → 改为拒绝或路由到专用命令

## 7.7 新增测试与 fixture

契约测试（review §12.2 第 26 条 + 基准 §19.5）：

- [ ] Source delete 覆盖 registry / raw / assets / versions / references，失败原子回滚
- [ ] Source package 不可被 generic Wiki operation 破坏（rename / move / delete / 新建）
- [ ] 只有 `type: source` 且 registry binding 有效时进入 Source mode
- [ ] 普通 Wiki 页面不出现 Source lifecycle / AI 整理动作
- [ ] 顶部只新增 `AI 整理` 一个按钮；原始稿 / 版本 / 重处理入口在右栏
- [ ] 删除预览显示所有路径、版本、引用数、预计释放空间
- [ ] 删除前创建 Git 检查点；派生 Wiki 页面不自动删除
- [ ] rename/move 同步更新 manifest `wiki_path`
- [ ] 版本时间线只记录基准列出的 6 类事件
- [ ] 重处理（重新转录 / 更换字幕 / 重新识别文字 / 刷新来源）产生新候选版本，Diff 确认后才更新
- [ ] 外部编辑过的 Source 刷新来源触发三方合并

fixture：一个已导入的 Source package（Excel 或 CSV 分片）、一个被其他 Wiki 页面引用的 Source、一个人工编辑过的 Source。

## 7.8 中英文 / 键盘 / CJK / path 覆盖

- 删除二次确认页中英文；`永久删除此来源` 文案不可弱化
- 右栏八段键盘可达；折叠技术详情可展开复制
- CJK Source 标题与 CJK 路径在删除预览中正确显示
- 引用计数在 CJK 链接与相对路径下正确

## 7.9 退出门槛（= Gate E）

- [ ] P0-06 的非 AI 部分、P1-11 关闭
- [ ] Source package 不可被 generic Wiki operation 破坏
- [ ] 删除是原子的，失败全量回滚
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 8：AI 整理完整候选闭环

**目的**：把 `AI 整理` 从按钮变成"候选 → Diff → 检查点 → 新版本"的完整闭环。这是 Gate F。

**依赖**：Batch 7。Compile V2 route 可复用基础设施，但**不能混同**：AI 整理写 `wiki/sources/`，Compile 不写 `wiki/sources/`。

## 8.1 关闭的 finding

**P0-06** 的 AI 部分（收尾 P0-06）。

## 8.2 基准章节

§15.2（启动）、§15.3（输入与边界）、§15.4（内容概览）、§15.5（候选与版本）、§19.5

## 8.3 现有调用链与要改的 DTO

新增 `start_source_ai_organize` command + 任务 + candidate DTO。BYOK 在此**合法**（基准 §3.1.9：Source 已存在后参与 AI 整理与编译）。

**启动对话框**（基准 §15.2）：固定任务说明 + 当前 Agent/BYOK 路线和模型 + 发送/读取范围 + 可选自定义要求 + 未保存编辑检查 + "生成候选稿"主按钮。任务后台运行、可取消、可恢复；创建成功后打开任务抽屉并选中新任务，但不切换当前工作页面或编辑器焦点；离开页面后继续；完成时通知；**同一个 Source 同时只运行一个 AI 整理任务**，不同 Sources 可以排队。

**执行路线**：Source Agent 明确支持 Claude Code、Codex、OpenClaw、Hermes，并复用所选 CLI 的本地登录态。四者都在临时候选 workspace 中运行；应用只主动提供当前 Source 的有界输入，不把项目目录作为可写工作区，也不接受绕过候选 Diff、显式确认和 Git checkpoint 的直接项目写入。Claude Code / Codex 使用无会话、忽略项目规则/扩展的执行配置，其中 Codex 保留认证目录但不加载用户 `config.toml`；OpenClaw 使用 `agent exec` 临时状态的一次性执行；Hermes 使用 `-z` 一次性执行并跳过项目规则。四种本地 Agent 都接受较宽松的认证/配置目录和工具读取边界；该边界不是操作系统级只读沙箱。Agent 与 BYOK 必须消费同一份内置 `source-rewrite` 合同。

**本地配置解析**：环境清理后只恢复所选 Agent 必需的路径选择器；OpenClaw 保留活动 state/config/profile、auth secret dir 与 include roots；Hermes 优先使用显式 `HERMES_HOME`，否则从平台默认根解析 sticky `active_profile`（Windows 默认 `%LOCALAPPDATA%\hermes`），并保留 OAuth/model 路径覆盖。临时候选目录正常结束立即清理；崩溃残留需超过 24 小时且不在当前进程活动 workspace 租约中才可回收。

**失败与重试**：普通 recoverable failure 只保存任务绑定的 Source 基线、路线和设置，等待用户显式重试；BYOK 锁定 provider/model，Agent 锁定 Agent 种类并使用重试时当前的 Source 执行 profile，不把 CLI 版本当作模型，也不声称冻结本机 profile/默认模型；同一次运行不静默 fallback、不自动调用第二模型、不做双调用。

**输入边界**（基准 §15.3，首版严格限制）：当前 Source Markdown + Source 元数据 + 已有 OCR/ASR/字幕和图片引用。**不默认上传或重新处理 raw 附件**；内容不足时先完成 OCR 或 ASR。

允许：修正标点与明显 ASR 错误、删除口头填充与机械重复、合并碎片段落、重建章节和标题、修复 OCR 换行、改善阅读顺序；按用户自定义要求重写或纠错有界输入支持的事实、数字、人名、URL、引语和时间。
语义约束：只依据有界输入，不引入外部事实，不把不确定内容伪装为确定事实。事实类变化不做 token / 姓名词表 / 数字集合硬拒绝，而是完整进入 candidate Diff 和显式确认。

**内容概览**（基准 §15.4）：整理后的 Markdown 在标题之后、正文之前必须包含

```markdown
## 内容概览

一至三段对本文主线、主要观点和结论的忠实概括。
```

重新运行时**替换**现有"内容概览"，不得不断追加重复概览。

**候选与版本**（基准 §15.5）：结果永远先作为候选；候选绑定 `sourceId + versionId + Markdown hash`；Source 在生成期间变化时必须重新 Diff 或三方合并；用户查看 Diff、确认后创建检查点并更新当前 Source；**忠实原稿永久可恢复**。

## 8.4 写入 / 删除 / checkpoint / rollback

- 候选写 staging，不直接改 `wiki/sources/`
- 用户确认后：建 Git 检查点 → 写新版本 → 更新 manifest `current_version_id` → 追加时间线事件
- 原稿版本永久保留在 `.app/sources/` 的 versions 与 `raw/`
- 候选 hash 与当前 Source hash 不匹配时**拒绝应用**，要求重新 Diff

## 8.5 必须保留的现有能力

- 现有 Agent / BYOK provider、密钥（OS 凭据）、任务进度、取消、通知
- Chat / Compile / Export 的 BYOK 路线不受影响
- 未保存编辑检测与冲突提示

## 8.6 要删除或改写的反基准测试

- 任何把通用 Ask AI 当作 AI 整理的测试或文案
- 断言 AI 结果直接覆盖 Source 的测试（若存在）→ 候选优先
- 断言事实 token、姓名、数字、URL、引语或时间变化必须在生成阶段失败的测试 → 改为候选可生成、Diff 可见、未确认不应用
- 断言 recoverable failure 会自动切换 Agent / BYOK、调用第二模型或双调用的测试 → 改为保存基线与设置后等待用户显式重试

## 8.7 新增测试与 fixture

契约测试（review §12.2 第 24、25 条 + 基准 §19.5）：

- [ ] AI 整理只生成 candidate，未确认不覆盖
- [ ] 应用前创建 checkpoint；外部改动时进入三方合并
- [ ] candidate 不绑定当前 hash 时拒绝应用
- [ ] 生成的 Markdown 有且仅有一个 `## 内容概览`；重跑替换而非追加
- [ ] 同一 Source 同时只允许一个 AI 整理任务；不同 Source 可排队
- [ ] 输入范围严格限定（不含 raw 附件、不含 cookie/token）
- [ ] 内容不足时提示先完成 OCR / ASR，而不是硬跑
- [ ] 忠实原稿可恢复
- [ ] 任务可取消、可恢复、离开页面继续、完成时通知
- [ ] Agent 与 BYOK prompt 复用同一份内置 `source-rewrite` 合同
- [ ] 事实/数字/人名/URL/引语/时间变化可生成结构合法 candidate，在 Diff 可见且未确认不应用
- [ ] Claude Code、Codex、OpenClaw、Hermes 均在临时候选 workspace 中执行；应用只提供有界 Source 输入且只接收候选结果，Claude/Codex 无会话并忽略项目规则/扩展，OpenClaw/Hermes 使用当前官方一次性入口，同时明确四者复用本地登录时不是操作系统级只读沙箱
- [ ] 普通 recoverable failure 不静默 fallback 或双调用，必须由用户按保存基线和设置显式重试

fixture：一个 ASR 转录 Source（含口头填充与断句错误）、一个 OCR Source（含换行错误）、一个含数字与引语的 Source。

## 8.8 中英文 / 键盘 / CJK / path 覆盖

- 启动对话框与 Diff 视图中英文
- `## 内容概览` 标题在英文 UI 下的处理：基准写死中文标题，**不随 UI 语言变化**（Source 内容语言与 UI 语言分层，见 P3-03）
- Diff 视图键盘可达；候选接受 / 丢弃有明确焦点
- CJK 内容的 Diff 分词与 hash 稳定性

## 8.9 退出门槛（= Gate F）

- [ ] P0-06 完全关闭
- [ ] 基准 §15、§19.5 全部场景通过
- [ ] 未确认绝不覆盖
- [ ] external edit 不丢失
- [ ] candidate 不绑定当前 hash 时拒绝应用
- [ ] `npm run check` 从头通过；双 review 无未处理有效问题

---

# Batch 9：兼容清理、无障碍、文案与全矩阵回归

**目的**：删掉过渡期的兼容层，补齐无障碍与边界，形成可重复的验收证据。

**依赖**：前述所有功能批次。

## 9.1 关闭的 finding

**P2-09** 收口、**P3-02** 校验、**P3-03**；并对基准 §19 全矩阵形成可重复证据。

## 9.2 基准章节

§2（术语）、§10（多语言）、§19（验收矩阵全节）、§20（文档维护规则）

## 9.3 范围

- **legacy adapter 移除或收窄为只读边界**：`.app/source-index.json` 适配层、`resolve_wiki_asset_path` 的 legacy fallback（`raw/sources/{sid}/{vid}/assets/...`）；确认无生产路径依赖后删除，或明确标注为只读且有独立测试
- **migration 收口**：迁移功能保留在独立兼容入口；正常工作台零迁移术语；`docs/import-v2-cutover-checklist.md` 与 `npm run check:import-v2-cutover` 同步更新
- **dead code 清理**：dead commands / types / i18n key / 测试；`FileTransaction` 的 `write` / `track` / `capture_installed` dead-code warning（review §18 记录的既存 warning）在此处理或明确保留原因
- **a11y / responsive**：heading 层级、live region、focus order、`aria-current`、图标按钮 tooltip、820px 与桌面宽度
- **CJK / Unicode / case / path**：跨平台路径风格、长路径、大小写敏感、NFC/NFD
- **异常边界**：磁盘不足、取消中断、重启恢复、并发（多 session 尝试、同 Source 并发操作）
- **P3-03 语言分层**：UI 语言切换不改 Source 内容语言；导入只检测/标注语言，不翻译
- **文档同步**（基准 §20）：`SPEC/PRD.md`、`SPEC/SPEC.md`、`SPEC/APP_flow.md`、`SPEC/TECH_STACK.md`、`SPEC/BACKEND_STRUCTURE.md`、`SPEC/FRONTEND_GUIDELINES.md` 索引 2026-07-24 基准并删除或改写冲突结论；历史设计文档顶部标注"部分已被本文件取代"并链接基准；`skills/llm-wiki-desktop-context/references/project-map.md` 更新 Import/Source 代码归属

## 9.4 全矩阵回归清单（基准 §19）

按基准逐条形成可重复证据，不是抽查：

- [ ] §19.1 核心闭环 6 条
- [ ] §19.2 OCR 5 条
- [ ] §19.3 ASR 6 条
- [ ] §19.4 登录与权限 5 条
- [ ] §19.5 Source 阅读 5 条
- [ ] §19.6 边界与兼容 5 条

同时复核 review §12.2 的 26 条契约测试全部存在且通过，§11 格式矩阵 14 行全部有真实 fixture。

## 9.5 退出门槛

- [ ] 产品基准 §19 矩阵形成可重复证据（每条对应到具体测试或手动脚本）
- [ ] review §12.2 的 26 条契约测试全部存在且通过
- [ ] review §11 格式矩阵 14 行都有真实 fixture
- [ ] 九条禁止的"关闭方式"逐条有反向测试
- [ ] `npm run check` 从头通过
- [ ] 两轮独立 review 没有未处理有效问题

---

## 附录 A：批次依赖图

```text
Batch 0 ──┬─> Batch 1 ──┬─> Batch 2 ──┬─> Batch 7 ──> Batch 8 ──┐
          │             │             │                          │
          │             ├─> Batch 3 ──> Batch 4 ──> Batch 5 ─────┤
          │             │                                        │
          └─────────────┴──────────────> Batch 6 ────────────────┴─> Batch 9
```

- Batch 0 是所有批次的前置。
- Batch 1 是 Gate A，Batch 3/4/5 每加一种输入都要重跑它的契约测试。
- Batch 2 与 Batch 3 可部分并行（共享 DTO 已在 Batch 0 冻结）。
- Batch 6 依赖 Batch 0 的 presentation DTO，但建议放在 3/4/5 的 blocker 类型稳定后，避免七类映射反复改。
- Batch 9 必须最后做。

## 附录 B：每批收尾固定动作

1. `npm run check` 从头跑通（Tauri 应用占用 target 时用独立 `CARGO_TARGET_DIR`，不要杀用户的应用）
2. 双子代理审查：A 共享上下文查设计意图 / 逻辑 / 一致性；B 全新上下文查盲点 / 隐性 bug / 缺失测试 / 行为不清
3. 合并结果 → 修完所有有效问题 → 再 `npm run check`
4. `SPEC/progress.txt` 追加一条（倒序，最新在上）：`[YYYY-MM-DD] Import 整改 Batch N — 完成内容摘要 — 关键决策/遗留问题`
5. 有踩坑就追加 `SPEC/gotchas.txt`：`现象 — 根因 — 规避做法`
6. 收尾报告逐项回答 §3 的十项模板

## 附录 C：风险与已知张力

| 风险 | 说明 | 缓解 |
|---|---|---|
| Batch 0 删除面大 | BYOK 关键字在 Chat/Compile/Export/Lint/Settings 中合法，误删会伤及无关功能 | 删除范围严格限定 `import_v2*` / `src/features/import/` / Import 专属字段；每删一处跑 `npm run check` |
| manifest schema 反复改 | 若各 batch 各自加 optional 字段，会出现"一半读旧字段一半读新字段" | Batch 1 一次定完 review §9.3 全部字段与 `schemaVersion`，后续 batch 只填值不改结构 |
| Gate A 回归成本 | Batch 3/4/5 每加输入都要重跑 Gate A | Gate A 契约测试必须表驱动，加输入只加数据行 |
| 前端大改与视觉基准冲突 | Batch 6 改动集中，容易偏离 `UI-Frontend-design/` 密度 | 每个组件对照 `assets/app.css` 的 px 尺寸与 token；不修改 `UI-Frontend-design/` |
| 迁移功能悬空 | Batch 0 把迁移 UI 移出工作台后，若无人接手入口会变成死代码 | Batch 0 必须明确落到设置或诊断入口；Batch 9 最终确认或删除 |
| 长任务恢复语义复杂 | Paused + shard 复用横跨 Batch 4 与 Batch 6 | shard 模型在 Batch 4 定型（后端），Batch 6 只消费 |
| `import-recovery` skill 新建 | 全仓无先例，容易越权 | 严格按基准 §16 的可以/不可以两张清单写合同，并加拒绝测试 |
