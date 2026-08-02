# First-run / Project-open 授权主干实施 + Workflows Batch 8 最终收尾

> 生成日期：2026-08-02（v2，经二次 review 修订）
> 状态：仅计划，尚未开始任何实施；已确认方案 1（untracked 权威文件随 Batch A 入库）
> 目标：先落地 First-run / Project-open 中「解除 Workflows 阻塞所必需」的 P0/P1 授权主干，再返回闭环 Workflows Batch 8
> 唯一产品与交互权威：
> - [`../specs/2026-07-30-first-run-project-open-workbench-design.md`](../specs/2026-07-30-first-run-project-open-workbench-design.md)
> - [`../specs/2026-07-30-workflows-panel-redesign.md`](../specs/2026-07-30-workflows-panel-redesign.md)
> 分批执行前序：[`2026-07-30-workflows-panel-implementation.md`](2026-07-30-workflows-panel-implementation.md)
> 硬边界与门禁：根 `AGENTS.md`、`CLAUDE.md`

---

## v2 修订摘要（相对 v1 的实质变化）

| # | 变化 | 原因 |
|---|---|---|
| 1 | **A3 诊断收窄并更正** | v1 称「被更旧快照覆盖」不成立：`workflowStore.replaceRuns` 已走 `mergeRunSnapshots` 按 `updatedAt` 合并，`upsertRun` 已有陈旧守卫。真实缺陷只剩「overview 提交前事件被整条丢弃且不排定刷新」 |
| 2 | **B2 修复面扩大** | Local Quick 真实读取链是 `health_check.rs` → `search_service.scan_wiki` → `wiki_index.refresh` → `list_markdown_files(&context.wiki_dir)`。v1 只写了 `preparation.rs`，漏了 `wiki_index.rs` 与 `lint_service/rules.rs:533` |
| 3 | **`ProjectContext` 扩展策略明确化** | 全仓库 126 处构造点；必须保持 `ProjectContext::new` 纯净（无 IO）以零改动编译，layout 走独立解析路径 |
| 4 | **可写性探测时序更正** | 规范 §7.2 明确 restricted 不得创建 / 修复 `.app`，且 `ProjectFilesystemAccess` 只有 `writable \| read_only`（无 `unknown`）。未信任项目**不得**做写探测 |
| 5 | **`safe_native_path` 复用方式明确化** | 它是 `app_state.rs` 私有 fn，签名 `(root, relative, kind)`；A4 需要先提取到共享模块 |
| 6 | **批次重排：A4/A3 提前独立成批** | 二者不依赖 B/C/D。原计划把路径安全洞压在三个无关批次之后 |
| 7 | **新增架构测试边界** | 扩展既有 `src/test/workflows-architecture.test.ts` 断言 workflows 内无信任 / Git 授权逻辑 |
| 8 | **新增提交卫生守卫** | 量化基线：91 个无关改动文件，每批提交后必须复核该计数 |
| 9 | **门禁 lane 组成具体化** | 引用 `scripts/run-checks.mjs` 真实 lane，使门禁预期可核 |
| 10 | **detector 冻结可行性确认** | `src/styles.css` 已有 130 个 token（含 `--sp-*` / `--radius-*` / `--text-inverse`），新 UI 可全部复用，冻结不产生冲突 |
| 11 | **真正的死锁被点名并前置为红灯测试** | `is_strict_native_layout`（`app_state.rs:223`）要求 `purpose.md` / `schema.md` / `.app/tasks` 等八项，兼容库**结构上永远拿不到 `TrustedNative`**。Batch D.0 先写这条失败测试 |
| 12 | **A2 修复链补全到 coordinator** | 除 `revalidate_workflow_replay` 丢弃派生值外，`coordinator.rs:485` 的 retry 直接取 `tasks.workflow_persistence_dir(task_id)`（旧值）。v1 只写了命令层 |
| 13 | **§8.1 现状核对表（G.0）** | 只读核实：四个 legacy 文件已删、`agent` AppView 别名已移除、两个 README 已就位；`.agentmini` / `.agent-activity-*` CSS 与 65/66 处 `agent.*` i18n **仍在使用，不得清理** |
| 14 | **新增 §6 明确范围外表** | 列出 11 项 First-run 规范条目不在本计划内，防止交付时被误读为「First-run 已完成」 |
| 15 | **E.1 保留旧路径** | `health_report()` 与 `preview_open_folder_as_project` 不在本批删除，新 assessment 并行存在，减小 blast radius |
| 16 | **新增 §7.3 停止条件清单** | 五种必须停下来问用户的情形，避免为了让计划「跑完」而擅自放宽 |

---

## 0. 定位与权威顺序

本计划**不新增产品决策**，只把两份已确认规范中「Workflows 已经依赖、后端却尚未提供」的部分实现出来，并闭环 Batch 8。

冲突时的权威顺序：

1. 根 `AGENTS.md` / `CLAUDE.md` 硬边界
2. `docs/superpowers/specs/2026-07-30-*.md` 两份规范
3. 仓库代码与测试的既有事实
4. `SPEC/progress.txt` 最新记录
5. `2026-07-30-workflows-panel-implementation.md`
6. 历史审计与 `SPEC/plans/agent.md`

**核心原则：Workflows 消费后端派生的访问策略，自己不授予信任、不把只读项目改成可写、不创建项目上下文。**
本计划严禁把项目打开、信任与 Git 权限逻辑复制进 `src/features/workflows/`。

---

## 1. 审计基线（2026-08-02 只读证据）

### 1.1 Git 与工作树

| 项 | 值 |
|---|---|
| 分支 | `master` |
| HEAD | `398e89e feat(workflows): complete shared launch and recovery UX`（Batch 7） |
| 暂存区 | 空 |
| `git status --porcelain` 总计 | 111 |
| 其中 untracked | 5 |
| 其中已跟踪已修改 | 100 |
| 其中 Batch 8 目标文件 | 9 |
| **无关改动文件（提交卫生基线）** | **91** |
| `UI-Frontend-design/` | **零改动** |
| `wiki/` | **零改动** |

Untracked（方案 1：随 Batch A 入库）：

- `docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`
- `docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`
- `docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md`
- `PRODUCT.md`
- 本计划文件

Batch 8 未提交改动（9 个）：`src/components/app/LeftSidebar.tsx`、`LeftSidebar.test.tsx`、
`src/components/app/RightContextPanel.tsx`、`src/features/workflows/README.md`、
`WorkflowTaskDetail.tsx`、`useWorkflowsController.ts`、`useWorkflowsController.test.tsx`、
`workflows.test.tsx`、`src/styles.css`。

### 1.2 与既有描述的差异（以仓库证据为准）

1. **checkbox 不是进度信号。** `2026-07-30-workflows-panel-implementation.md` 中 253 个 checkbox
   全部未勾，但 Batch 0–7 已提交。真实进度信号是**提交历史 + `SPEC/progress.txt`**。
2. **Batch 8 §8.1「旧 UI 退休」在代码中其实已完成：**
   - `src/features/agent/` 只剩 `README.md`
   - `navigationStore.ts` / `WorkspaceRouter.tsx` / `shellNavigation.ts` **无 `agent` AppView 别名残留**
   - `useTaskLauncher` 仍被 `features/import/*` 合法调用（`useImportBatchController`、
     `useImportTaskCoordinator`、`useImportWorkflow`），按规范应保留
3. **文档超前于事实。** `SPEC/roadmap/agent.md` 开头已声称「Batch 0–8 已实施；旧 Agent 主界面已退休」，
   但 Batch 8 未提交、双审未闭环、外部依赖未落地。Batch A 必须改回。
4. **进度文档已分叉。** 根 `progress.txt`（179 行）落后于 `SPEC/progress.txt`（467 行，含
   `[2026-08-02] Workflows Batch 8 backend review hardening`）。按 `CLAUDE.md`，
   **后续只追加 `SPEC/` 版本**，不动根副本。
5. `agent.*` i18n 键各 locale 17 个，覆盖合法用途，**不做批量删除**。
6. 前端测试文件数 113。

### 1.3 门禁 lane 真实组成（`scripts/run-checks.mjs`）

| 模式 | frontend lane | rust lane |
|---|---|---|
| `npm run check:quick` | `lint`、`build`、`check:console` | `check:rust:core`（`--no-default-features`） |
| `npm run check` | `check:import-source-media`、`test`、`test:capability-tools`、`lint`、`build`、`check:console` | `check:rust:gui`、`test:rust` |

两 lane 默认并行；`LLM_WIKI_CHECK_SERIAL=1` 可串行。**完整门禁必须从头跑，不得只跑单个 lane 冒充。**

---

## 2. 待关闭问题清单（逐条复现确认，含 v2 更正）

### 2.1 A 组 — Batch 8 范围内

**A1 — `set_active_project` 不消费权威**
`src-tauri/src/commands/task_commands.rs:208`。走裸 `resolve_project_context`，随后无条件
`set_project_context(project_id, root, context.app_dir.join("tasks"))`，不消费 trust / access /
persistence。对未信任或只读项目也会恢复并写 `.app/tasks`，直接违反规范 §7.2「restricted 不得创建 `.app`」。

**A2 — 重新准备丢弃当前持久化**
`src-tauri/src/commands/workflow_commands.rs:349-419`。`revalidate_workflow_replay` 重算 access 并比对
baseline / route / blocking prerequisite，却**丢弃**重新派生的 persistence 与 task_state_root；
`coordinator.retry(..., context.root)` 仍以 `tasks.workflow_persistence_dir(task_id)`（原任务目录）入队。

**A3 — 事件在 overview 提交前被整条丢弃（v2 更正，范围比 v1 小）**
`src/features/workflows/useWorkflowsController.ts:175-191`。

已核实**不成立**的部分（v1 误判，此处更正）：

- `workflowStore.replaceRuns`（`workflowStore.ts:64`）已走 `mergeRunSnapshots`
  （`:92-99`，按 `Date.parse(updatedAt) >=` 合并），**不会**用更旧快照覆盖更新
- `upsertRun`（`:66-76`）已有 `previous.updatedAt > run.updatedAt` 陈旧守卫

仍然成立的真实缺陷：

```ts
const access = state.overview?.projectAccess;
if (event.projectId !== project.projectId || run.projectId !== project.projectId ||
    !access || run.canonicalIdentityKey !== access.canonicalIdentityKey ||
    run.identityRevision !== access.identityRevision) { return; }   // ← 整条 return
state.upsertRun(run);
void refresh();
```

首次 overview 提交前 `access` 为 `undefined` → 事件**既不入 store，也不排定 `refresh()`**。
若该事件是某次终态转换的唯一通知，UI 会停留在陈旧态直到下一次外部触发。
修复面：仅需「缺 access 时缓冲事件或置 pending-refresh 标记，access 到位后校验身份再落地」。

**A4 — 任务持久化路径只做祖先包含检查**
`src-tauri/src/tasks/task_service.rs::validate_persistence_dir`：

```rust
let mut ancestor = persistence_dir;
while !ancestor.exists() { ancestor = ancestor.parent()...; }
let canonical_ancestor = ancestor.canonicalize()...;
if !canonical_ancestor.starts_with(canonical_root) { return Err(...); }
```

只 canonicalize「最近存在的祖先」，**无逐段 symlink / junction / reparse 拒绝**；
`persist_task_to_dir` 之后 `create_dir_all` + 写入不再校验（TOCTOU）。

仓库内**已有正确范式**可复用：`src-tauri/src/app_state.rs:241`

```rust
fn safe_native_path(root: &Path, relative: &Path, kind: NativePathKind) -> bool {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        let Ok(metadata) = std::fs::symlink_metadata(&current) else { return false };
        if metadata.file_type().is_symlink() || metadata_is_reparse_point(&metadata) { return false }
        ...
```

配套 `metadata_is_reparse_point`（`:266`）用 Windows `file_attributes() & 0x400`，非 Windows 返回 false。
**注意：`safe_native_path` 目前是 `app_state.rs` 私有 fn**，复用前必须先提取（见 Batch B.1）。

### 2.2 B 组 — First-run 外部依赖（零实现）

**B1 — 四个 prerequisite 无生产宿主**
全仓库**无** `trust_project` / `make_writable` / `configure_git` / `resolve_dirty_git` 任何 Rust 命令。
前端仅有 `WorkflowProjectPrerequisiteAction` 类型（`src/types/workflow.ts:159`）与
`useWorkflowsController.ts:69,90` 的未接线 `onProjectPrerequisite` 缝。后端 prerequisite 派生
（`preparation.rs:917-948`）判定正确 —— **缺的纯粹是宿主，不是判定**。

**B2 — 无 `ProjectLayout`，兼容库恒空（v2：修复链比 v1 长）**
全仓库**无** `ProjectLayout` / `markdown_roots`（Rust 与 TS 皆无）。
`ProjectContext`（`src-tauri/src/models/paths.rs:9-30`）硬编码：

```rust
app_dir: root.join(".app"), raw_dir: root.join("raw"),
wiki_dir: root.join("wiki"), exports_dir: root.join("exports"), skills_dir: root.join("skills"),
```

Local Quick 的**真实读取链**（v1 只写了链尾，漏了中段）：

```text
runners/health_check.rs:130   services.search_service.scan_wiki(context, &HashSet::new())
  └─ search_service/catalog.rs:29   self.index.refresh(&context, &self.file_store)
       └─ wiki_index.rs:127         file_store.list_markdown_files(&context.wiki_dir)   ← 真正的扫描根
runners/health_check.rs:131   health_source_paths(context)
  └─ lint_service/rules.rs:533-535  list_markdown_files(&context.raw_dir.join("extracted"))
preparation.rs:1136 list_wiki_pages / :1151 list_readable_markdown   （同样读 wiki_dir / raw_dir）
```

因此兼容 / restricted 库下 `tree.pages` **整体为空**，Local Quick 无内容可读。

附带结论：`runners/health_check.rs:139-141` 的 `not_applicable_rules` 推导本身正确
（`wiki_pages == 0 && source_pages > 0` → 跳过 `index_drift`，从 page tree 推导而非硬编码路径）。
**B2 修好后此项自动正确，不单独改**；但需在 Batch C 补一条兼容库 `mixed` role 下的回归。

**B3 — 可写性判定不足**
`preparation.rs`：

```rust
let writable = std::fs::metadata(&context.root).map(|m| !m.permissions().readonly()).unwrap_or(false);
persistence: if trusted && writable && context.app_dir.is_dir() { Persistent } else { MemoryOnly },
```

无法表达 Windows ACL、只读卷、Unix ACL / effective permission。

**B4 — 无 typed assessment，兼容库结构上永不可信任**
无 `ProjectOpenAssessment` / `assessmentId` / `ProjectFormat` / `ProjectTrustState` /
`ProjectHealth` / `ProjectFilesystemAccess`。`app_state.rs`：

```rust
pub enum ProjectTrustAuthority { Untrusted, TrustedNative }
```

且 `register_trusted_native` 要求 `is_strict_native_layout`（purpose.md / schema.md / raw/sources /
wiki / .app / .app/tasks / exports / skills 全备）→ **兼容库无法被信任，这是 Workflows 的真正死锁点**。
项目识别仍是二元 `is_wiki_project`（`project_service.rs:523-528`，六个条件任一为真即 true），
规范 §15 已判其过时。

### 2.3 规范给定的枚举（实现不得自行扩充）

```ts
ProjectFormat = "native_current" | "native_legacy" | "nashsu_llm_wiki"
              | "obsidian_vault" | "markdown_vault" | "ambiguous_markdown"
ProjectTrustState      = "trusted" | "untrusted"
ProjectFilesystemAccess = "writable" | "read_only"      // ← 无 unknown
ProjectHealth          = "healthy" | "repairable" | "recovery" | "unreadable"
ProjectMarkdownRoot.role = "source" | "wiki" | "mixed"
```

后端既有 `WorkflowGitState = Clean | Dirty | Unavailable`（`models/workflow.rs:291`）保持不变。

**关键推论（驱动 Batch D 时序更正）：** `ProjectFilesystemAccess` 无 `unknown` 值，
且规范 §7.2 明确 restricted **不得**创建 / 修复 `.app`。因此**未信任项目不得做任何写探测**，
其 `filesystemAccess` 只能由 metadata 派生，判不准时 fail-closed 取 `read_only`。
真实写探测只允许在**已信任**项目、且目标目录已存在时进行。

---

## 3. Impeccable detector 证据完整性协议（硬约束）

Batch 8 §8.2 的 detector 已按指定命令**运行过一次，且不可再次运行**。

事实澄清：该次运行**没有落盘产物**。`.impeccable/` 下仅有一份
`2026-07-28T12-05-27Z__src-features-import-importview-tsx.md` 旧 critique，且该目录被 `.gitignore:19` 忽略。
因此证据 =「当次 JSON 结论 + 已应用到工作树的修复」，它证明的是**四个受覆盖路径在当前这一刻的状态**。

### 3.1 规则 1 — 哈希锚点（2026-08-02 实测）

| 覆盖路径 | SHA-256 |
|---|---|
| `src/features/workflows/`（全树聚合） | `fbb6ae797bff9fb5b7d272cdf3debdd6c31b2e7f2db746c154bca39a7a5ef652` |
| `src/components/app/LeftSidebar.tsx` | `50dffbeb417d1330…` |
| `src/components/app/RightContextPanel.tsx` | `7314e507b569281e…` |
| `src/styles.css` | `2a45a53ea61c0c5a…` |

复算命令（可复现）：

```bash
find src/features/workflows -type f | sort | xargs sha256sum | sha256sum
```

```bash
sha256sum src/components/app/LeftSidebar.tsx src/components/app/RightContextPanel.tsx src/styles.css
```

### 3.2 规则 2 — Batch B–F 表现层冻结

- **不得**修改 `src/styles.css`
- **不得**修改 Workflows 任何 JSX 结构或 className
- **不得**修改 `src/components/app/LeftSidebar.tsx` / `RightContextPanel.tsx`
- 唯一例外：**A3**（Batch B）改 `src/features/workflows/useWorkflowsController.ts`，
  **限定纯非表现层**（事件缓冲 / 合并逻辑），零 JSX、零 className、零 CSS
- 新 First-run UI 全部落在覆盖范围**之外**的新文件

**冻结可行性已核实：** `src/styles.css` 现有 **130 个 token**，含 `--sp-1..--sp-8`、
`--radius-sm/md/lg/pill`、`--text-inverse`。Batch E 的新 First-run UI **只复用既有 token**，
不新增 token，因此「新 UI」与「styles.css 冻结」不冲突。
若确实需要新 token，**停下来询问用户**，不擅自写入。

### 3.3 规则 3 — Batch G 收尾披露

- 重算四个哈希，**逐项披露 delta**
- 对 delta 做**人工**对齐复核，依据 `SPEC/FRONTEND_GUIDELINES.md` 与 `UI-Frontend-design/assets/app.css` 密度规则
- 交付报告写明：「detector 运行一次，覆盖 SHA 为 X；此后变更为 Y，经人工复核，未二次运行 detector」
- **绝不二次运行 detector**

---

## 4. 全局约束（每批都适用）

### 4.1 继承硬边界

- 本地优先、无数据库；项目内容只用 Markdown + JSON；**全局最近位置、目录信任、歧义意图存应用配置目录**；密钥只进 OS 凭据存储
- 删除 / 覆盖 / 批量替换前必须创建 Git 检查点；高风险操作必须用户确认
- 内部路径统一正斜杠；所有来自 UI 的路径必须校验在项目范围内；必须覆盖 Unicode / CJK 与 Windows / macOS / Linux 风格路径
- 执行路径显式，不静默回退，不静默安装 Agent
- 项目访问由后端判定：format / trust / filesystem access / health / layout / capabilities **独立建模**；
  `restricted` 只是派生 UI 摘要，`recovery` 属 health；前端禁用态不是授权
- `ProjectLayout` 每个路径均由后端派生、项目相对、经包含校验。
  **缺失的 write / state 路径表示该能力不可用，服务必须返回 typed prerequisite，不得凭空创建目录**

### 4.2 禁止的绕过手段（一项都不许用）

1. 通过 `ProjectRegistry` 路径登记来授信
2. 扫描整个 compatible root
3. 把 `wiki_dir` 指向项目根目录
4. 直接调用 legacy `initialize_git_repository` 命令
5. 向普通材料文件夹写 `.app`

### 4.3 提交卫生守卫（量化，可核）

- **只 `git add` 逐条枚举的路径**；**禁止** `git add .` / `git add -A` / `git commit -a`
- 每批提交前：`git status --porcelain | grep -c '^ M'` 减去本批目标文件数，
  应仍等于 **91**（基线，见 §1.1）
- 每批提交后：复核 `UI-Frontend-design/`、`wiki/`、根 `progress.txt`、根 `gotchas.txt` 仍无本任务改动
- 任一计数不符：**停下来报告，不继续下一批**

### 4.4 流程纪律

- 每批独立提交，**不合批**；保留所有与本任务无关的既有改动
- 每批完成后**立即**追加 `SPEC/progress.txt`（倒序在上，格式
  `[YYYY-MM-DD] 模块/任务 — 完成内容摘要 — 关键决策/遗留问题`）；反复出现或隐蔽的坑追加 `SPEC/gotchas.txt`
- **不动**根 `progress.txt` / `gotchas.txt` 副本
- 纯文档批不跑 npm 检查；涉及跨层 / 文件写入 / Git / 密钥 / IPC / 并发 / 后台任务的批跑**完整 `npm run check`**
- Batch B–G 每批各一次双审：
  - **Reviewer A（共享上下文）**：设计意图 → 逻辑 → 与已确认规范一致性
  - **Reviewer B（全新上下文，零偏见）**：盲点、隐性 bug、被忽略边界
  - 修复有效发现后重跑同级门禁；完整门禁失败后的修复必须**从头**重跑 `npm run check`
- 每批收尾输出 §10 交付报告后**暂停**，再进入下一批

---

## 5. 批次总览（v2 重排）

| 批 | 内容 | 关闭 | 依赖 | 完整门禁 | 双审 |
|---|---|---|---|---|---|
| **A** | 文档校正与权威文件入库 | — | — | 否 | 否 |
| **B** | 独立加固：路径安全 + 事件缓冲 | A4、A3 | — | **是** | **是** |
| **C** | 后端 layout 主干 | B2 | — | **是** | **是** |
| **D** | 真实信任与可写性 | B3、B4 前半 | C | **是** | **是** |
| **E** | Assessment 与 First-run 宿主 | B1、B4 后半 | D | **是** | **是** |
| **F** | Workflows 消费权威 | A1、A2 | E | **是** | **是** |
| **G** | Batch 8 最终收尾 | Batch 8 DoD | F | **是** | **是**（即 §8.4 cutover 审查） |

**v2 重排理由：** A4 是路径安全洞、A3 是事件丢失，二者**不依赖** C/D/E。v1 把它们压在
三个无关批次之后（原 Batch F），风险窗口无谓延长。提前独立成批后：安全洞先关、
最终批变小、`useWorkflowsController.ts` 的 detector delta 一次产生而非分散在两批。

**代价（如实说明）：** 批次由 6 变 7，完整门禁由 5 次变 **6 次**（单次约 4–6 分钟），
双审由 5 次变 **6 次**。不压缩门禁换取批次数。

**先红后绿的批次（实施顺序有硬要求）：**

| 批 | 必须先写的失败测试 | 目的 |
|---|---|---|
| C | 原生库索引结果逐条一致基线 | 防 `wiki_index.rs:127` 迁移回归 |
| D | 兼容库授信 → `PROJECT_TRUST_AUTHORITY_INVALID` | 证明死锁真实存在 |
| F | 架构断言（workflows 内无信任逻辑） | 反向验证守卫有效 |

---

## Batch A — 文档校正与权威文件入库

**目的：** 让后续所有批次有稳定、可审查、已入库的判定基线，并修正超前于事实的文档。

**门禁：** 纯文档，无 npm 检查。**双审：** 不需要。**回滚：** `git checkout` / `git reset` 该提交。

### A.1 权威文件入库（方案 1）

- [ ] 逐条 `git add`（**不用** `git add .`）：
  - [ ] `docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`
  - [ ] `docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`
  - [ ] `docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md`
  - [ ] `PRODUCT.md`
  - [ ] `docs/superpowers/plans/2026-08-02-first-run-authority-and-workflows-batch8-closure.md`
- [ ] 入库前逐份确认内容与只读读取一致，**不做任何内容修改**
- [ ] `git status --porcelain | grep -c '^??'` 应由 5 降为 0

### A.2 文档校正

- [ ] `SPEC/roadmap/agent.md` 开头状态改为：Batch 0–7 已实施并提交；
      **Batch 8 未提交、双审未闭环、外部依赖 B1–B4 未落地**
- [ ] `2026-07-30-workflows-panel-implementation.md` 顶部加「进度信号声明」：
      253 个 checkbox 不代表进度，提交历史 + `SPEC/progress.txt` 为准；
      补记 §8.1 已在代码中完成的三项事实（见 §1.2 第 2 条）
- [ ] 在实施计划 §8.2 交叉引用本文件 §3 detector 协议与四个哈希锚点

### A.3 记录与守卫

- [ ] 追加 `SPEC/progress.txt` 一条
- [ ] 提交卫生守卫：无关改动计数仍为 91

### A.4 禁止

- 不改任何 `.ts` / `.tsx` / `.rs` / `.css` / `package.json`
- 不改根 `progress.txt` / `gotchas.txt`、`UI-Frontend-design/`、`wiki/`

**建议提交：** `docs(project): commit first-run and workflows authorities with closure plan`

---

## Batch B — 独立加固：任务路径安全 + 事件缓冲（关闭 A4、A3）

**目的：** 先关掉不依赖任何其他批次的两个缺陷。

**前置：** Batch A。**门禁：** 完整 `npm run check`。**双审：** 需要。**回滚点：** Batch A。

**主要文件**

- 新增：`src-tauri/src/utils/path_safety.rs`
- 修改：`src-tauri/src/app_state.rs`、`src-tauri/src/tasks/task_service.rs`、
  `src/features/workflows/useWorkflowsController.ts`、`useWorkflowsController.test.tsx`

### B.1 提取共享路径安全工具（前置动作）

- [ ] 把 `app_state.rs:241 safe_native_path` 与 `:266 metadata_is_reparse_point` 提取到
      `src-tauri/src/utils/path_safety.rs`，标 `pub(crate)`
- [ ] `NativePathKind` 一并迁移或在新模块重新导出
- [ ] `app_state.rs` 改为引用新模块，**行为与既有测试断言完全不变**
- [ ] 以既有 `app_state` 测试（含 `strict_native_open_is_trusted_without_silently_initializing_git`）证明零回归

### B.2 A4 — 任务持久化路径逐段加固

- [ ] `validate_persistence_dir` 改为：先 canonicalize project root，再对
      `persistence_dir` 相对 root 的**每一段**执行 `symlink_metadata` 检查，
      拒绝 `is_symlink()` 或 reparse point，最后确认 `starts_with(canonical_root)`
- [ ] 尚不存在的段：逐段创建时**边创建边校验**，不允许「先 `create_dir_all` 再检查」
- [ ] `persist_task_to_dir` 在 `create_dir_all` / 写入**紧邻前**重新校验，收窄 TOCTOU 窗口
- [ ] 在代码注释中如实说明：非原子 open-with-flags 语义下窗口只能收窄不能归零

### B.3 A3 — 事件缓冲（纯逻辑）

- [ ] `access` 缺失时**不再整条 return**：把事件写入 `pendingEventsRef` 缓冲，
      并置 pending-refresh 标记
- [ ] 首次 overview 提交后，对缓冲事件**逐条重新校验**
      `event.projectId` / `run.projectId` / `canonicalIdentityKey` / `identityRevision`，
      通过者交 `upsertRun`，不通过者丢弃
- [ ] **保持**既有隔离语义不放宽：跨项目或 identity 不匹配的事件仍必须丢弃
- [ ] 复用既有 `updatedAt` 陈旧守卫，**不重写** `mergeRunSnapshots` / `upsertRun` 的合并规则
      （二者已正确，见 §2.1 A3）
- [ ] `activeKeyRef.current !== projectKey` 的早退保持不变（切项目时缓冲必须清空）

### B.4 测试

- [ ] Rust：junction / symlink 指向项目外被拒；**中途替换**（校验后创建前）被拒；
      CJK 目录名通过；Windows 与 POSIX 风格相对路径各一例
- [ ] Rust：既有 task 持久化 / 恢复测试全绿
- [ ] 前端：overview 提交前到达的事件在 access 到位后落地；
      跨项目缓冲事件被丢弃；切项目清空缓冲
- [ ] `cargo fmt --check`

### B.5 禁止

- 改 `src/styles.css`、Workflows JSX / className、`LeftSidebar.tsx`、`RightContextPanel.tsx`
- 重写 `mergeRunSnapshots` / `upsertRun` 既有合并语义
- 触碰 layout / 信任 / assessment（属 C/D/E）

**建议提交：**

```text
refactor(paths): extract shared per-component path safety helper
fix(tasks): reject reparse points in task persistence paths
fix(workflows): buffer task events arriving before first overview
```

---

## Batch C — 后端 layout 主干（关闭 B2）

**目的：** 让后端能诚实描述兼容 / restricted 知识库的真实 Markdown 结构，为 D/E/F 提供事实来源。

**前置：** Batch B。**门禁：** 完整 `npm run check`。**双审：** 需要。**回滚点：** Batch B。

**主要文件**

- 新增：`src-tauri/src/models/layout.rs`
- 修改：`src-tauri/src/models/paths.rs`、`src-tauri/src/app_state.rs`、
  `src-tauri/src/services/wiki_index.rs`、`src-tauri/src/services/lint_service/rules.rs`、
  `src-tauri/src/services/workflow_service/preparation.rs`、`src/types/project.ts`

### C.1 类型定义

- [ ] 新增 `ProjectLayout` / `ProjectMarkdownRoot` / `ProjectMarkdownRootRole`
      （`source` \| `wiki` \| `mixed`）与 `ProjectContextDocument`，serde `camelCase`
- [ ] 字段集合严格照规范 §（`appStateRoot`、`evidenceRoot`、`markdownRoots`、`sourceWriteRoot`、
      `wikiWriteRoot`、`wikiIndexPath`、`wikiOverviewPath`、`activityLogPath`、`queriesWriteRoot`、
      `exportRoot`、`skillsRoot`、`importStateRoot`、`sourceStateRoot`、`compileStateRoot`、
      `chatStateRoot`、`taskStateRoot`、`workflowStateRoot`、`graphCachePath`、`lintReportRoot`、
      `lintIgnorePath`、`exportRecordPath`、`bookmarksPath`、`settingsPath`、`agentConfigPath`、
      `purposeContext`、`schemaContext`）
- [ ] **只填当前能诚实提供的路径**，缺失即 `None`；遇 `None` 返回 typed prerequisite，不发明目录
- [ ] `src/types/project.ts` 镜像 + 契约测试（字段名、可选性、union 完整性）

### C.2 ProjectContext 扩展策略（v2 明确化，避免 126 处爆炸）

仓库现状：`ProjectContext::new` / `ProjectContext { .. }` 共 **126 处**构造点，多数在测试中。
`new()` 目前是**纯路径运算，无文件 IO**；layout 派生需要 IO。策略：

- [ ] **保持 `ProjectContext::new` 签名与纯净性不变** → 126 处构造点零改动编译
- [ ] `new()` 内部填入**原生派生 layout**（由现有 `wiki_dir` / `raw_dir` / `app_dir` 等推导，纯运算）
- [ ] **保留** `wiki_dir` / `raw_dir` / `app_dir` / `exports_dir` / `skills_dir` 字段
      （分别 78 / 5 / 75 处引用，**不做大爆炸重构**）
- [ ] 新增 `ProjectContext::with_resolved_layout(...)`（或等价解析入口），做 IO 发现，
      **只在 `app_state.rs` 的 `resolve_project_context` / assessment 路径调用**
- [ ] 原生库经两条路径得到的 layout 必须**等价**，以测试断言
- [ ] 原生库运行时行为**零变化**，以既有测试证明

### C.3 兼容库 layout 发现

- [ ] **有界**、按角色标注的 markdown root 发现：依 `.obsidian` 等标记与**顶层结构**判定，带 `exclude`
- [ ] **不扫整个 root**（禁止手段 2）、**不把 `wiki_dir` 指向 root**（禁止手段 3）
- [ ] write / state 路径全部 `None`
- [ ] 判定不确定时**保守**：少纳入并返回 typed warning，不越界扫描
- [ ] 记录 `confidence`（`high` / `medium` / `low`）供 Batch E assessment 复用

### C.4 迁移 layout 敏感读取点（v2：三处，不止一处）

- [ ] `src-tauri/src/services/wiki_index.rs:127` —— `list_markdown_files(&context.wiki_dir)`
      改为遍历 `layout.markdown_roots`（按 role 过滤 + 应用 `exclude`）。
      **这是 Local Quick / 搜索 / Wiki 视图 / 图谱的共同扫描根，影响面最大，必须先做**
- [ ] `src-tauri/src/services/lint_service/rules.rs:533` `health_source_paths` ——
      `raw_dir.join("extracted")` 改为按 `source` / `mixed` role 解析
- [ ] `src-tauri/src/services/workflow_service/preparation.rs:1136` `list_wiki_pages`
      与 `:1151` `list_readable_markdown` —— 同样改走 `layout.markdown_roots` + role
- [ ] 其余调用点仍用原生字段并保持原生专属，通过 typed prerequisite fail-closed
- [ ] 验证兼容库经 layout 能被 Local Quick 正确读到真实 Markdown

### C.5 测试

- [ ] `wiki_index` 迁移后：原生库索引结果与迁移前**逐条一致**（防回归基线）
- [ ] 兼容库（`.obsidian` + 根 `index.md`）能列出真实页面
- [ ] `mixed` role 下 `not_applicable_rules` 推导正确（§2.2 B2 附带结论）
- [ ] Source-only 兼容库：`index_drift` 报 not-applicable，Source 完整性检查仍运行
- [ ] CJK 与 Windows / POSIX 风格路径；`cargo fmt --check`

### C.6 禁止

- 改 `src/styles.css` / Workflows JSX / `LeftSidebar.tsx` / `RightContextPanel.tsx`
- 改信任模型（属 D）、加 assessment 命令（属 E）
- 改 `ProjectContext::new` 签名或使其产生 IO

**建议提交：** `feat(project): derive typed project layout and migrate markdown scan roots`

---

## Batch D — 真实信任与可写性（关闭 B3、B4 前半）

**目的：** 解开真正的 Workflows 死锁——**兼容库在结构上无法被授信**——并让可写性成为
后端实测事实而非 `readonly()` 位。

**前置：** Batch C。**门禁：** 完整 `npm run check`。**双审：** 需要。**回滚点：** Batch C。

**主要文件**

- `src-tauri/src/app_state.rs`（信任权威）
- `src-tauri/src/services/project_service.rs`（信任持久化、可写性探测）
- `src-tauri/src/services/workflow_service/preparation.rs`（消费可写性）
- 新增：信任存储 JSON 读写模块（位于应用配置目录）

### D.0 死锁复现（实施前先写成失败测试）

```rust
// app_state.rs:223 —— 兼容库永远过不了这一关
pub fn is_strict_native_layout(root: &Path) -> bool {
    safe_native_path(root, Path::new("purpose.md"), ...)      // 兼容库没有
        && safe_native_path(root, Path::new(".app/tasks"), ...) // 兼容库没有
        && ...
}
// app_state.rs:114 —— 不满足即 PROJECT_TRUST_AUTHORITY_INVALID
```

- [ ] 先加**红灯**测试：`.obsidian` + 根 Markdown 的兼容库尝试授信 → 现状必然报
      `PROJECT_TRUST_AUTHORITY_INVALID`。这条测试是本批的存在理由，**先红后绿**

### D.1 信任权威扩展（不放宽原生校验）

- [ ] `ProjectTrustAuthority` 增加 `TrustedCompatible`（或等价第三态），
      **`TrustedNative` 的判定条件一字不改**
- [ ] 兼容授信的**前置条件**：Batch C 的 layout 发现返回至少一个 `markdown_roots`
      且 `confidence != low`，**且**用户已在 First-run 显式确认（Batch E 提供）
- [ ] `revalidate_project_authority`（`:180`）对 `TrustedCompatible` 同样做
      **identity_revision 漂移即永久撤销**——沿用现有语义，不新增放松分支
- [ ] 兼容库的 `identity_revision` 复用 `project_identity`（`persistence.rs:22`，
      基于 canonical root + dev/ino 或 created/len），**不新造身份算法**
- [ ] `resolve_workflow_access`（`:314`）的 `trusted` 判定改为 `trust != Untrusted`，
      并**额外**透出 `trust_kind`，让下游能区分「可写原生」与「兼容」

### D.2 信任持久化（规范 §7.3：全局设置，绑定 canonical 文件夹身份）

- [ ] 存储位置：`project_service.rs::default_config_dir()`（`:816`，
      APPDATA → XDG_CONFIG_HOME → HOME/.config → temp，均拼 `llm-wiki-desktop`）
- [ ] 新文件与 `RECENT_PROJECT_FILE`（`:28`）并列，**不写入任何项目文件夹**
      （禁止手段 5：不向普通材料文件夹写 `.app`）
- [ ] 记录：canonical path、`canonical_identity_key`、授信时的 `identity_revision`、
      `trust_kind`、授予时间。**不记录**任何密钥或内容摘要
- [ ] 启动加载：只有 canonical path **且** identity 均匹配才恢复为已授信；
      任一不符 → 视为未授信并清理该条（**路径匹配单独不足**，禁止手段 1）
- [ ] 写入用原子替换（临时文件 + rename），损坏文件视为空并记 warn，不 panic
- [ ] 用户撤销信任的路径必须存在（Batch E 提供 UI 入口）

### D.3 可写性真实判定（关闭 B3）

现状（`preparation.rs`）：

```rust
let writable = std::fs::metadata(&context.root)
    .map(|m| !m.permissions().readonly())
    .unwrap_or(false);
```

在 Windows 上目录的 `readonly` 位基本无意义，ACL 拒写会被误判为可写。

- [ ] 新增后端可写性判定，产出规范枚举 `writable` / `read_only`（**无 `unknown`**）
- [ ] **未授信项目：只做 metadata 级推导，fail-closed 到 `read_only`，不做任何写探测**
      （规范 §7.2：受限模式不得创建 `.app` 或修复）
- [ ] **已授信项目：** 对**已存在**的目标目录做临时文件写探测，
      文件名带唯一后缀，`finally` 语义确保删除；探测失败 → `read_only`
- [ ] **不创建**目标目录来使探测成功；目录不存在即返回 typed prerequisite
- [ ] 结果**不缓存跨 identity_revision**；同一 revision 内可短时缓存以免抖动
- [ ] `preparation.rs` 的 `persistence` 判定改为消费该结果：
      `Persistent` 仅当「trusted **且** writable **且** state root 存在且逐段安全」，
      其余一律 `MemoryOnly`
- [ ] 复用 Batch B.1 的 `path_safety`，不复制逻辑

### D.4 测试

- [ ] D.0 红灯测试转绿：兼容库可授信为 `TrustedCompatible`
- [ ] 原生授信条件**未被放宽**：缺 `purpose.md` 的目录仍不能拿 `TrustedNative`
- [ ] `low` confidence 兼容库拒绝授信
- [ ] identity_revision 漂移后 `TrustedCompatible` 被永久撤销，重建路径不复活
- [ ] 信任文件缺失 / 损坏 / 路径匹配但 identity 不匹配 → 均判未授信
- [ ] 只读目录（POSIX `0o555`）判 `read_only`，且**未**留下探测残留文件
- [ ] 未授信项目上**没有发生任何写调用**（以临时目录内容快照断言）
- [ ] `MemoryOnly` 路径下确认 `.app/` **未被创建**
- [ ] CJK 路径、Windows / POSIX 风格路径

### D.5 禁止

- 放宽 `is_strict_native_layout`、跳过 identity 校验
- 用 registry 已登记推断信任（禁止手段 1）
- 向项目文件夹写信任状态；把密钥写入任何文件
- 改 `src/styles.css` / Workflows JSX；改 Workflows 消费逻辑（属 F）

**建议提交：** `feat(project): add verified trust states and real writability probing`

---

## Batch E — Assessment 与 First-run 宿主（关闭 B1、B4 后半）

**目的：** 给 Batch C/D 的后端权威一个**真实的产品宿主**：typed assessment、短生命周期
assessment 注册表、信任 / 启用 / Git 补救的用户入口。

**范围纪律（重要）：** 本批**只实施解除 Workflows 阻塞所必需的 P0/P1 authority spine**，
不做 First-run 全量重设计。明确排除见 §6。

**前置：** Batch D。**门禁：** 完整 `npm run check`。**双审：** 需要。**回滚点：** Batch D。

**主要文件**

- 新增：`src-tauri/src/services/project_service/assessment.rs`（或等价 use-case 模块）
- 修改：`src-tauri/src/commands/project_commands.rs`、`git_commands.rs`、`lib.rs`（注册命令）
- 修改：`src/features/project/ProjectStartView.tsx`、`src/stores/projectStore.ts`
- 新增：兼容启用 / 信任 / Git 补救的 React 承载组件与 i18n 词条（zh-CN + en）

### E.1 Typed assessment（替换二元探测）

现状 `project_service.rs::health_report()`（`:502-538`）用二元 `is_wiki_project`（6 个 OR
条件）判定；`preview_open_folder_as_project`（`project_commands.rs:125`）只回
Created / Opened 二元预览。

- [ ] 新增 `assess_project_folder`（只读）→ 返回规范 §12 的 `ProjectOpenAssessment`
- [ ] 六个维度**独立派生**：`format`（8 值）、`trust`、`filesystemAccess`（无 `unknown`）、
      `health`（4 值）、`layout`（Batch C）、`capabilities`
- [ ] `restricted` **不作为后端枚举**，只在前端由上述维度派生为 UI 摘要
- [ ] `health = recovery` 表示 `.app` 状态残缺但**可读 Markdown 仍必须能打开**
- [ ] `confidence` 复用 Batch C.3 的判定，不重算
- [ ] `git: ProjectGitAssessment` 只读汇报（复用 `git_status`），**不做任何写操作**
- [ ] 保留 `health_report()` / `preview_open_folder_as_project` 现有签名与调用点，
      新命令并行存在；本批不删旧路径（减小 blast radius，删除留待 First-run 全量批）

### E.2 两级 ID 与生命周期（规范 line 221，容易做错）

规范区分两个 ID，**不能混用**：

| ID | 作用域 | 用途 |
|---|---|---|
| `assessmentOperationId` | application-scoped | 由 `start_project_open_assessment` 返回；**只有** `cancel_project_open_assessment` 接受它 |
| `assessmentId` | short-lived | 扫描**完成后**返回；供 open / trust / repair 命令引用 |

- [ ] 两个 ID 用**不同类型**（newtype 或独立字段名），编译期防混用
- [ ] 取消：**不创建项目任务**，丢弃未完成快照，保留完整 no-project shell
- [ ] `assessmentId` 短生命周期注册表：有 TTL、容量上限、可显式失效
- [ ] `assessmentId` 过期 / 未知 → typed 错误，**不回退到重新扫描**（不静默回退）
- [ ] 引用 `assessmentId` 执行前**重新校验** canonical path + identity_revision，
      漂移即拒绝并要求重新评估（沿用 Batch D 的撤销语义）

### E.3 信任 / 启用 / Git 的真实宿主（关闭 B1）

- [ ] `trust_project`：接受 `assessmentId`，调用 Batch D 的授信路径 + 持久化
- [ ] `revoke_project_trust`：对称入口，UI 可达
- [ ] `enable_compatible_full_features`：规范 §7.4 **最小写入** ——
      只写 `.app/compat/{purpose.md,schema.md}`；**不建**根 `purpose.md` / `schema.md`；
      **不移动 / 不重命名**既有 Markdown 或 `.obsidian`
- [ ] Git 初始化选择：默认开启但**可关**；复用现有 `initialize_git_repository`
      （`git_commands.rs:42`）**经 assessment 门控**调用，
      **不绕过 assessment 直接调旧命令**（禁止手段 4）
- [ ] 脏工作树闸门：规范 §7.5 —— **绝不** auto-commit / auto-stash；
      只呈现事实 + 用户显式选择
- [ ] 所有高风险入口经统一 `PendingAction` 确认模型，不新造确认机制
- [ ] 交回 Workflows 的安全交接：启用 / 授信成功后重新解析权威 →
      刷新 Workflows overview（**不在 Workflows 内重算信任**）

### E.4 前端（最小必要，不动 shell 骨架）

- [ ] `ProjectStartView.tsx` 渲染 typed assessment 状态（六维度独立呈现）
- [ ] 兼容 / 受限 / 只读 / recovery 的文案只依 assessment 字段，**不在前端推断**
- [ ] 异步展示提交用 initiating project key + epoch 守卫（沿用 Batch B.3 的模式）
- [ ] i18n：zh-CN + en 同步，**无硬编码字面量**
- [ ] 只用 `src/styles.css` 既有 130 个 token；**需要新 token 即停止并询问用户**（§3.2）
- [ ] React **不**碰文件系统 / Git / 修复 / 密钥（规范 §12 明列）

### E.5 测试

- [ ] 六维度独立性：`format` 相同但 `trust` / `access` / `health` 不同的组合逐一断言
- [ ] `assessmentOperationId` 传给 open/trust 命令 → 编译失败或 typed 拒绝
- [ ] 取消后：无任务创建、无 `.app` 写入、shell 完好
- [ ] `assessmentId` 过期 / identity 漂移 → typed 错误且**未执行副作用**
- [ ] 兼容启用只产生 `.app/compat/` 两个文件；根目录与 `.obsidian` **逐字节未变**
- [ ] 脏工作树：断言**没有** commit / stash 发生（`git_status` 前后一致）
- [ ] Git 初始化关闭时**不创建** `.git`
- [ ] recovery：`.app` 损坏但 Markdown 仍能列出
- [ ] 前端：assessment 各状态渲染快照 + zh/en 词条完整性

### E.6 禁止

- 实施 §6 明确排除项（新建向导重构、两卡工作台、深扫、右面板重排等）
- 在前端派生信任 / 可写性；在 Workflows 内加信任逻辑（属 F）
- 改 `src/styles.css`；改 Workflows JSX
- 向普通材料文件夹写 `.app`（禁止手段 5）

**建议提交：** `feat(project): add typed open assessment with trust and git remediation hosts`

---

## Batch F — Workflows 消费权威（关闭 A1、A2）

**目的：** 让 Workflows / TaskService **消费** Batch C–E 的权威，而**不复制**任何项目打开、
信任或 Git 逻辑。

**前置：** Batch E。**门禁：** 完整 `npm run check`。**双审：** 需要。**回滚点：** Batch E。

**主要文件**

- `src-tauri/src/commands/task_commands.rs`（A1）
- `src-tauri/src/commands/workflow_commands.rs`（A2）
- `src-tauri/src/services/workflow_service/coordinator.rs`（retry 持久化传递）
- `src/test/workflows-architecture.test.ts`（边界断言）

### F.1 A1 — `set_active_project` 消费持久化权威

现状（`task_commands.rs:208`）：用普通 `resolve_project_context`，然后**无条件**
`set_project_context(project_id, root, context.app_dir.join("tasks"))`。

- [ ] 改为先 `resolve_workflow_access`（拿 trust + writable + trust_kind）
- [ ] 派生持久化模式（复用 Batch D.3 的判定，**不在此重算**）：
  - `Persistent`：绑定 `layout.taskStateRoot`，恢复既有任务
  - `MemoryOnly`：**不绑定磁盘、不恢复、不创建 `.app/tasks`**，返回空列表 + typed 说明
- [ ] `task_state_root` 一律取自 `layout.taskStateRoot`（Batch C），**不再**手拼
      `app_dir.join("tasks")`
- [ ] `layout.taskStateRoot` 为 `None` → 直接 `MemoryOnly`，**不发明目录**
- [ ] 现有的 identity 匹配 + 确认恢复循环（`:239-270`）保持不变，只在 `Persistent` 下执行
- [ ] 前端 `projectStore` 感知返回的持久化模式并透出到右侧面板既有字段
      （**不新增视觉元素**，§3.2）

### F.2 A2 — 重播路径不丢弃新派生的持久化

现状：`revalidate_workflow_replay`（`workflow_commands.rs:370`）重新 `prepare()` 得到
新的 persistence / task_state_root，但**只**比对 fingerprint / route / blocking 后丢弃；
`coordinator.rs:485` 的 retry 仍用 `tasks.workflow_persistence_dir(task_id)`（**旧值**）。

- [ ] `revalidate_workflow_replay` 返回重新派生的 persistence 模式与 `task_state_root`
      （不再返回 `()`）
- [ ] `retry`（`coordinator.rs:414`）接受该值并**覆盖** `task_state_root`，
      不再从 `workflow_persistence_dir` 取旧值
- [ ] `continue_queued_workflows`（`task_commands.rs:281`）同样应用新派生值
- [ ] 由 `Persistent` 降级为 `MemoryOnly` 时：**停止写盘**，
      并对既有磁盘状态做**明确处置**（保留原文件、不再更新、写一条 warn 日志），
      **不静默继续写旧 `.app/tasks`**
- [ ] 由 `MemoryOnly` 升级为 `Persistent` 时：绑定新 root，**不追溯回写**历史内存态
- [ ] 降级 / 升级都产生用户可见的 typed 状态，不只写日志

### F.3 边界断言（防止把信任逻辑抄进 Workflows）

- [ ] 扩展 `src/test/workflows-architecture.test.ts`：遍历 Workflows 生产源，
      断言**不出现**信任 / 可写性 / Git 初始化 / assessment 派生逻辑
      （按符号名与调用模式断言，而非仅字符串黑名单）
- [ ] Rust 侧等价断言：workflow 模块不直接调用信任持久化或写探测，
      只经 `resolve_workflow_access` / layout 消费

### F.4 测试

- [ ] 未授信项目 `set_active_project` → 返回空 + `MemoryOnly`，
      临时目录快照证明 **`.app/` 未被创建**
- [ ] 已授信只读项目 → `MemoryOnly`，同样无写入
- [ ] 已授信可写原生项目 → `Persistent` 且任务正常恢复（既有行为回归）
- [ ] `layout.taskStateRoot = None` → `MemoryOnly` 且无目录创建
- [ ] retry：信任在两次运行之间被撤销 → 新 attempt 走 `MemoryOnly` 且不写旧目录
- [ ] retry：信任在两次运行之间被授予 → 新 attempt 走新 root
- [ ] `continue_queued` 在权威变化后行为与 retry 一致
- [ ] 架构断言测试在故意注入违规代码时**会失败**（反向验证守卫有效）

### F.5 禁止

- 在 Workflows 内派生 / 缓存信任或可写性
- 直接调用旧 Git 初始化命令（禁止手段 4）
- 改 `src/styles.css`；新增 Workflows 视觉元素
- 降低任何既有 identity / fingerprint 校验强度

**建议提交：** `fix(workflows): consume project authority for task persistence and replay`

---

## Batch G — Batch 8 最终收尾

**目的：** 在权威主干齐备后完成 Batch 8 的 §8.2 验收、§8.4 双审与 §8.5 文档收尾，
**并诚实披露 detector 一次性约束的处理方式**。

**前置：** Batch F。**门禁：** 完整 `npm run check`（从头）。
**双审：** 需要（即 Batch 8 §8.4 本体）。**回滚点：** Batch F。

### G.0 §8.1 现状核对（已完成，不重做）

只读核实结论：

| §8.1 项 | 现状 |
|---|---|
| 四个 legacy 文件删除 | **已删**；`src/features/agent/` 只剩 `README.md` |
| `agent` AppView 别名移除 | **已移除**（`navigationStore.ts` 无 `agent`） |
| `AgentSettings` / `AgentService` / 类型 / 能力检测 / 侧栏 Agent 脚保留 | **已保留** |
| `useTaskLauncher` 保留给 Import / Chat | **已保留**，8 处调用 |
| `.agentmini` / `.agent-activity-*` CSS | **仍在使用**（侧栏脚 + Chat），**不得删除** |
| `agent.*` i18n | 65（zh）/ 66（en）处，均为 Chat / Settings / Source / Workflows route 技术标签，**不清理** |
| `src/features/agent/README.md` 标记历史 | **已完成** |
| `src/features/workflows/README.md` 归属与非目标 | **已完成** |

- [ ] 本批**不再动** §8.1；只在交付报告中引用上表作为已完成证据

### G.1 §8.2 验收场景（Batch C–F 之后才真正可测）

18 个场景中，以下**在 Batch C–F 之前无法真实执行**，现在必须逐个实测并留证：

- [ ] restricted compatible project（依赖 C + D）
- [ ] trusted read-only project（依赖 D.3）
- [ ] project without Git when a checkpoint is required（依赖 E.3）
- [ ] project with pre-existing dirty Git state（依赖 E.3）
- [ ] Sources but no Wiki（依赖 C.4 的 role 解析）
- [ ] interrupted after restart（依赖 F.1 的持久化模式）
- [ ] no open project / empty project / healthy Wiki / queued second / waiting confirmation /
      failed route / cancelled / valid-invalid quick rerun / remote disclosure /
      CJK 与长英文标签 / 窄右面板 overlay —— 逐项复测（既有能力，回归性质）
- [ ] 五秒可识别当前任务、三步内启动、无需原始日志即可理解阶段
- [ ] 每个写结果显示受影响文件、Git 状态与恢复路径
- [ ] prepare 与 start **双端**强制 canonical identity / trust / access / writability /
      checkpoint 策略，陈旧 UI token 无法绕过
- [ ] 键盘可达、焦点顺序、SR 标签、状态文本、进度语义、reduced motion、200% 缩放
- [ ] 浅色 / 深色 / 支持的主题预设

### G.2 detector 约束的诚实处理（对应 §3）

- [ ] 核对四个受覆盖路径的当前哈希与 §3.1 锚点：
  - `src/features/workflows/`：Batch B.3 改过 `useWorkflowsController.ts` → **预期变化**
  - `src/components/app/LeftSidebar.tsx`、`RightContextPanel.tsx`：**预期未变**
  - `src/styles.css`：**预期未变**（全程冻结）
- [ ] **不重跑 detector**（用户明确一次性约束）
- [ ] 对 Batch B.3 的 delta 做**等价人工核查**：按 detector 规则类目逐条自查
      （硬编码色值、非 token 间距、魔法字号、重复选择器等），记录逐条结论
- [ ] 若 §3.1 锚点显示 `styles.css` 或两个 shell 组件**意外变化** →
      **停止并向用户报告**，不自行决定是否重跑

### G.3 §8.3 自动化验证

- [ ] 聚焦前端 Workflows 与共享入口测试
- [ ] 全部 workflow Rust 集成测试
- [ ] compile / lint / export / task / Import / Chat / Settings / shell 回归
- [ ] `cargo fmt --check`
- [ ] `npm run check` **从头**完整运行
- [ ] 任何修复之后**重新从头**运行 `npm run check`

### G.4 §8.4 双审

- [ ] Reviewer A（共享上下文）：设计意图、工作流逻辑、领域归属、与已确认规范一致性
- [ ] Reviewer B（全新上下文）：持久化迁移、队列竞态、陈旧项目事件、取消、重启恢复、
      确认重播、路径安全、密钥、缺失测试
- [ ] **额外要求 Reviewer B 明确检查**：Batch C–F 的权威是否被**消费**而非**复制**
- [ ] 采纳有效结论 → 修复 → 重跑聚焦测试与完整 `npm run check`

### G.5 §8.5 文档收尾

- [ ] `SPEC/roadmap/agent.md`：由 Batch A 的校正状态更新为**实施证据**
      （Batch 0–8 完成 + 本文件 A–G 完成，附提交哈希）
- [ ] `SPEC/SPEC.md` / `APP_flow.md` / `BACKEND_STRUCTURE.md` / `TECH_STACK.md`：
      **仅当**最终 wire 名与计划不同才改
- [ ] `2026-07-30-workflows-panel-implementation.md`：Batch 8 标记完成并交叉引用本文件
- [ ] `SPEC/progress.txt` 追加里程碑（时间倒序，只追加）
- [ ] `SPEC/gotchas.txt` 只记录反复出现 / 隐蔽的陷阱，候选：
      ①「registry 已登记 ≠ 已授信」；②「Windows 目录 `readonly()` 位不反映 ACL」；
      ③「事件早于首次 overview 会被永久丢弃」；④「retry 复用旧 `task_state_root`」
- [ ] **不改**根 `progress.txt` / `gotchas.txt`（SPEC 副本才是活的，见 §1.2）

**建议提交：**

```text
test(workflows): cover authority-dependent acceptance scenarios
docs(workflows): record batch 8 closure evidence
```

---

## 6. 明确范围外（First-run 全量重设计留待独立计划）

本计划**只实施解除 Workflows 阻塞所必需的 authority spine**。以下 First-run 规范条目
**不在范围内**，且不得因为「顺手」而夹带：

| 规范条目 | 为何排除 |
|---|---|
| P0-1/2/3 无项目体验搬入 `AppShell`、两卡工作台 | 纯 shell 重构，不阻塞 Workflows |
| P0-4 新建向导（父目录 + 生成子路径、默认模板、记住上次父目录） | 不阻塞 |
| P0-5 创建成功导航到 Import | 不阻塞 |
| P0-8 普通材料文件夹路由到「用这些资料新建知识库」 | 需要新建向导（P0-4）先落地 |
| P1-1 歧义 Markdown 意图确认与全局记忆 | 可用 Batch C 的 `confidence` 保守降级替代 |
| P1-6 typed repair plan 与完整修复确认页 | 独立大特性，本计划只做 `health` 分类 |
| P1-7 recovery 模式完整 UI | 本计划只保证 `health = recovery` 时 Markdown 可读 |
| P1-8 两级扫描与可取消后台深扫 | 本计划的 assessment 是单级只读扫描 |
| P1-9 右面板独立 type / trust / access / health 行 | 受 §3.2 表现层冻结约束 |
| 删除 `health_report()` / `preview_open_folder_as_project` 旧路径 | 减小 blast radius（E.1） |

**交付时必须在报告中列出本表**，不得因为「看起来完成了 First-run」而隐瞒。

---

## 7. 依赖与风险（不隐藏、不降门槛）

### 7.1 真实依赖链

```
A(文档) ──┬──> B(路径安全+事件缓冲) ────────────────┐
          └──> C(layout) ──> D(信任+可写性) ──> E(assessment+宿主) ──> F(Workflows消费) ──> G(Batch8收尾)
```

B 与 C 之间**无依赖**，可并行；但为保持每批一次完整门禁与一次双审的可审查性，
建议仍按 A→B→C→D→E→F→G 串行。

### 7.2 风险登记

| 风险 | 影响 | 缓解 |
|---|---|---|
| C.4 迁移 `wiki_index.rs:127` 影响搜索 / Wiki / 图谱 | 高 | 先写「原生库结果逐条一致」基线测试再改 |
| 126 处 `ProjectContext` 构造点 | 高 | C.2 保持 `new` 签名与纯净性，零改动编译 |
| 兼容库 layout 误判导致越界扫描 | 高 | C.3 有界 + `exclude` + 低置信保守降级 |
| D.1 引入第三信任态可能被误用来放宽原生校验 | 高 | D.4 断言原生条件未放宽；双审专项检查 |
| D.3 写探测留下残留文件 | 中 | 唯一后缀 + finally 删除 + 测试断言无残留 |
| E.2 两级 ID 混用 | 中 | 不同类型，编译期防混用 |
| F.2 持久化降级时的既有磁盘状态处置 | 中 | 明确「保留不再更新 + warn + typed 状态」 |
| detector 一次性证据被破坏 | 中 | §3 三条规则 + G.2 哈希核对 + 停止上报 |
| 91 处无关改动被误提交 | 中 | §4.3 量化守卫，每批提交前核数 |
| 6 次完整门禁的时间成本 | 低 | 如实接受，不压缩 |

### 7.3 需要用户决策才能继续的情形（遇到即停止）

- 需要在 `src/styles.css` 新增 token（§3.2）
- §3.1 哈希锚点显示未预期路径变化（G.2）
- 无关改动计数偏离 91（§4.3）
- 兼容库 layout 无法在不越界的前提下发现任何 markdown root
- Batch D 的兼容授信在不放宽原生校验的前提下无法实现

---

## 8. 完成定义（逐条可核，不降低门槛）

### 8.1 八个问题全部关闭

| 问题 | 关闭批次 | 验收证据 |
|---|---|---|
| A1 | F.1 | 未授信 / 只读项目 `set_active_project` 返回 `MemoryOnly` 且 `.app/` 未创建 |
| A2 | F.2 | retry / continue 在权威变化后使用新派生持久化，降级不写旧目录 |
| A3 | B.3 | 首次 overview 之前到达的事件被缓冲并应用，非丢弃 |
| A4 | B.2 | 任务持久化路径逐段拒绝 symlink / junction / reparse，写前重校验 |
| B1 | E.3 | 信任 / 启用 / Git 补救有真实命令与 UI 宿主，`PendingAction` 确认 |
| B2 | C.3+C.4 | 兼容 / 受限库经 `layout.markdownRoots` 被 Local Quick 读到真实 Markdown |
| B3 | D.3 | 可写性为后端实测结果，未授信 fail-closed 到 `read_only` |
| B4 | C.1+D.2+E.1–E.3 | typed assessment、短生命周期 ID、兼容适配、信任持久化、Git 补救、安全交接 |

### 8.2 Batch 8 DoD

- [ ] §8.1 已完成（G.0 表为证）
- [ ] §8.2 18 个场景全部实测留证
- [ ] §8.3 完整 `npm run check` 从头通过
- [ ] §8.4 双审完成、有效结论已修
- [ ] §8.5 文档与 `SPEC/progress.txt` / `SPEC/gotchas.txt` 已更新
- [ ] §8.2 detector 约束按 G.2 诚实披露

### 8.3 过程约束

- [ ] 五个禁止手段（§4.2）一项未用
- [ ] 无关改动计数全程为 91（§4.3）
- [ ] `UI-Frontend-design/` 与 `wiki/` 零改动
- [ ] 每个需要完整门禁的批次都真跑了完整门禁（共 6 次）
- [ ] 每个需要双审的批次都做了双审（共 6 次）
- [ ] Workflows 内无信任 / 可写性 / Git / assessment 派生逻辑（F.3 断言为证）

---

## 9. 交付报告模板（每批结束时填写）

```text
批次：<A|B|C|D|E|F|G>
提交：<hash> <message>

改动文件（逐条，含新增/修改/删除）：
  - ...

关闭的问题：<A1|A2|A3|A4|B1|B2|B3|B4|—>
证据：<测试名 / 命令 / 断言，逐条>

门禁：
  - 级别：<无 | check:quick | 完整 check>
  - 结果：<通过 | 失败并修复后重跑通过>
  - 真实输出摘要：<文件数 / 测试数 / 耗时>

双审：
  - Reviewer A 结论与采纳项：...
  - Reviewer B 结论与采纳项：...
  - 未采纳项及理由：...

detector 证据状态：
  - 四个路径哈希 vs §3.1 锚点：<未变 | 变化及原因>
  - 是否重跑：否（一次性约束）
  - 等价人工核查结论：...

提交卫生：
  - 无关改动计数：<应为 91>
  - `UI-Frontend-design/` / `wiki/`：未改动

记录：
  - SPEC/progress.txt：已追加 <摘要>
  - SPEC/gotchas.txt：<已追加 <条目> | 无需追加>

遗留与下一批前置：...
```

---

## 10. 起步动作（获得批准后按序执行）

1. Batch A：逐条 `git add` 五个文件（§A.1）→ 校正 `SPEC/roadmap/agent.md` 与实施计划
   进度信号（§A.2）→ 追加 `SPEC/progress.txt` → 核对无关改动仍为 91 → 提交。
2. Batch B：先建 `src-tauri/src/utils/path_safety.rs` 并让 `app_state.rs` 改为引用它
   （行为不变，以既有测试证明）→ 再做 A4 逐段校验与写前重校验 → 再做 A3 事件缓冲 →
   完整门禁 → 双审 → 提交。
3. Batch C：**先写「原生库索引结果逐条一致」基线测试** → 再加 layout 类型与派生 →
   再迁移 `wiki_index.rs:127` / `lint_service/rules.rs:533` / `preparation.rs:1136,1151`。
4. Batch D：**先写 D.0 红灯测试**（兼容库授信失败）→ 再实施第三信任态与写探测。
5. Batch E：先定两级 ID 类型与 assessment DTO → 再接命令 → 最后接最小前端。
6. Batch F：先扩展架构断言测试（此时应失败）→ 再改 A1 / A2 使其转绿。
7. Batch G：按 G.0→G.5 顺序，detector 部分严格按 §3 与 G.2。

**当前状态：本文件是计划本体，尚未开始实施。等待用户批准后从 Batch A 起步。**
