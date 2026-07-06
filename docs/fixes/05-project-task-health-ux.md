# 05 Project, Task, and Health UX Specification

本规范整合以下条目：任务日志按执行时间排序、Lint 历史持久化、项目初始页优化、新建项目用文件资源管理器选择保存位置。

## 条目 A：任务日志（任务和通知页）按照执行时间排序

## 1. 需求概述
- 用户想要什么：任务和通知页中的任务日志按执行时间排序，最近执行/更新的任务更容易找到。
- 为什么：当前按状态排序会把旧的 running/failed/succeeded 分组挤在前面，用户查最近发生的事情不直观。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/components/app/TaskLogDrawer.tsx`、`src/stores/taskStore.ts`、`src-tauri/src/tasks/task_service.rs`、`src-tauri/src/commands/task_commands.rs`、`src/types/task.ts`。
- 当前行为是什么：后端 `TaskService::list_tasks` 已按 `updated_at desc`；前端 `TaskLogDrawer.tsx` 使用 `TASK_STATUS_ORDER` 重新排序，覆盖了后端时间顺序。
- 问题出在哪里：前端排序逻辑与用户需求和后端排序冲突。

## 3. 方案设计
- 第一性原理：任务日志是时间线，默认排序应表达“最近发生了什么”；状态筛选可以作为辅助，不应改变默认时间语义。
- 推荐方案：TaskLogDrawer 默认按 `startedAt` 或 `updatedAt` desc 排序，提供紧凑 segmented sort：`最近执行`、`最近更新`、`状态`。
- 技术方案：
  - 修改 `src/components/app/TaskLogDrawer.tsx`：
    - 新增 `type TaskSortMode = "execution_time" | "updated_time" | "status"`
    - 默认 `execution_time`
    - `sortTasks(tasks, mode)` 抽成纯函数并测试。
    - execution_time 排序 key：`startedAt ?? updatedAt` desc；completedAt 只作 duration 展示。
  - 修改 `src/types/task.ts`，保留 `TASK_STATUS_ORDER` 供 status sort 使用。
  - 可选修改 `src/stores/taskStore.ts`：不在 store 层排序，只保存后端返回顺序和实时 upsert。
- 需要新增哪些文件：可选 `src/components/app/taskSort.ts`、`taskSort.test.ts`。
- 需要修改哪些文件：`TaskLogDrawer.tsx`、`TaskLogDrawer.test.tsx`、i18n、styles。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：Task drawer header 增加排序 segmented control；默认显示“执行时间”。
- 交互流程：用户打开任务抽屉 -> 最新执行任务在顶部 -> 切换到状态排序 -> running/queued 等按状态分组 -> 关闭再打开保持本次 UI 偏好。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` Task logs、Long Tasks。

## 5. 验收标准（Done Definition）
- [ ] 默认任务列表按执行时间倒序排列。
- [ ] 后端恢复的历史任务与前端实时 upsert 任务排序一致。
- [ ] 用户可切换到更新时间或状态排序。
- [ ] 排序不改变 selectedTaskId，也不打断日志加载。

## 6. 风险与注意事项
- 可能影响的现有功能：已有 TaskLogDrawer 测试可能假设状态排序，需要更新测试语义。
- 边界情况：缺失 startedAt 的旧任务、queued 未 started、running 无 completedAt、跨项目恢复任务。

## 7. 实施步骤
- [ ] 抽 `sortTasks` 并写测试。
- [ ] 修改 TaskLogDrawer 默认排序。
- [ ] 加 segmented sort UI。
- [ ] 更新测试与 i18n。

## 条目 B：BUG：Lint 没有历史记录保留，重新打开就没有记录了

## 1. 需求概述
- 用户想要什么：Lint 本地检查和深度检查的历史记录在重新打开项目后仍可查看。
- 为什么：Lint 是知识库健康维护流程，历史记录丢失会让用户无法追踪问题变化、修复结果和 Agent 深度报告。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/stores/lintStore.ts`、`src/features/lint/LintView.tsx`、`src-tauri/src/commands/lint_commands.rs`、`src-tauri/src/services/lint_service.rs`、`src-tauri/src/models/lint.rs`、`src/types/lint.ts`。
- 当前行为是什么：local lint 同步返回并存在前端内存；deep lint 持久化到 `.app/lint-reports/<task_id>.json`；ignore 规则写 `.app/lint-ignore.json`。
- 问题出在哪里：本地 Lint 没有 `.app` 历史索引；LintView mount 不恢复历史列表；local/deep 报告持久化模型不一致。

## 3. 方案设计
- 第一性原理：Lint 报告是项目健康快照，应与任务日志一样可追溯；但它不是知识内容，不应写入 wiki markdown。
- 推荐方案：统一写 `.app/lint-reports/{report_id}.json`，用 `.app/lint-history.json` 存索引。local 和 deep 都进入历史，报告体结构区分 source。
- 技术方案：
  - 修改 `src-tauri/src/models/lint.rs`：
    - `pub enum LintReportKind { Local, Deep }`
    - `pub struct LintHistoryEntry { id, kind, created_at, issue_count, error_count, warning_count, task_id: Option<String>, route: Option<CompileRoute> }`
    - `pub struct LintHistoryFile { version: u32, entries: Vec<LintHistoryEntry> }`
    - `pub struct PersistedLintReport { entry: LintHistoryEntry, local_report: Option<LintReport>, deep_report: Option<DeepLintReport> }`
  - 修改 `src-tauri/src/services/lint_service.rs`：
    - `pub fn persist_local_report(&self, context: &ProjectContext, report: &LintReport) -> Result<LintHistoryEntry, BackendError>`
    - `pub fn list_lint_history(&self, context: &ProjectContext) -> Result<LintHistoryFile, BackendError>`
    - `pub fn read_lint_history_report(&self, context: &ProjectContext, id: &str) -> Result<PersistedLintReport, BackendError>`
  - 修改 `src-tauri/src/commands/lint_commands.rs`：
    - `run_local_lint` 返回 `LintReport` 时同时持久化，或返回 `{ report, historyEntry }` 的新 response。为兼容前端，推荐新增 command `run_local_lint_and_persist` 或扩展 response 前同步更新 TS。
    - deep lint task 成功时把 `.app/lint-reports/<task_id>.json` 同步登记到 `.app/lint-history.json`。
    - 新增 `list_lint_history`、`read_lint_history_report`。
  - 修改 `src/stores/lintStore.ts`：
    - 增加 `history: LintHistoryEntry[]`
    - `loadHistory(projectId, rootPath): Promise<void>`
    - `openHistoryReport(projectId, rootPath, id): Promise<void>`
  - 修改 `LintView.tsx`：
    - mount 时加载 history；
    - 增加“历史记录”侧栏或顶部 dropdown；
    - 当前报告仍优先显示最新。
- 需要新增哪些文件：可选 `src/features/lint/LintHistoryList.tsx`。
- 需要修改哪些文件：`models/lint.rs`、`lint_service.rs`、`lint_commands.rs`、`types/lint.ts`、`lintStore.ts`、`LintView.tsx`、i18n、tests。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：LintView 左/上方增加历史记录列表，行显示时间、local/deep、issue counts、route/task。
- 交互流程：用户运行 local lint -> 报告显示并写入历史 -> 关闭重开项目 -> 进入 Lint -> 历史列表出现 -> 点击历史项 -> 当前问题列表切换到该报告。
- 需要参考的设计规范：`Spec/APP_flow.md` Lint 与任务流程；`.app/` JSON 状态文件规则。

## 5. 验收标准（Done Definition）
- [ ] 运行本地 Lint 后，`.app/lint-history.json` 和对应 `.app/lint-reports/*.json` 被写入。
- [ ] 重新打开项目后，LintView 可展示历史记录并打开最新报告。
- [ ] Deep lint 报告也进入同一历史列表。
- [ ] 损坏的单个历史报告不会导致整个 LintView 崩溃。
- [ ] 不把 Lint 历史写入 `wiki/` 或数据库。

## 6. 风险与注意事项
- 可能影响的现有功能：`run_local_lint` command response 变化会影响前端测试；要么同步 TS，要么新增 command 避免破坏。
- 边界情况：历史过多需要上限或分页；建议保留最近 50 条，旧报告文件可暂不自动删除，后续再做清理。

## 7. 实施步骤
- [ ] 添加 Lint history DTO 和 serde default 测试。
- [ ] 实现 LintService 持久化/读取历史。
- [ ] 修改 local/deep lint command 写历史。
- [ ] 扩展 lintStore 和 LintView。
- [ ] 补后端报告损坏、前端重开恢复测试。

## 条目 C：优化项目总览页面（初始页）

## 1. 需求概述
- 用户想要什么：初始页去掉“打开项目路径”手填栏，只保留“新建空项目”“打开文件夹为项目”“打开已有项目”；最近项目展示更美观，并显示项目基本属性。
- 为什么：手填路径不符合桌面应用体验，且“导入资料到已有项目”在未打开项目时概念不清；最近项目应帮助用户判断进入哪个知识库。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/project/ProjectStartView.tsx`、`src/stores/projectStore.ts`、`src-tauri/src/commands/project_commands.rs`、`src-tauri/src/services/project_service.rs`、`src/types/project.ts`、`src/styles.css`。
- 当前行为是什么：ProjectStartView 有 hero、filter、quick actions、新建/打开/导入 note、手动 open path form、recent cards、右侧 agent/byok/template。
- 问题出在哪里：启动页仍偏 landing/hero 和手填路径，不符合 AGENTS 中“不要 landing page，实际可用体验”的要求；recent card 属性较少。

## 3. 方案设计
- 第一性原理：项目初始页的本质是“选择或创建本地项目”，不是介绍产品。入口必须少、明确、系统化。
- 推荐方案：三主动作 + 最近项目列表 + 项目属性摘要；移除手填路径表单。
- 技术方案：
  - 修改 `src/features/project/ProjectStartView.tsx`：
    - 删除 open path manual form。
    - Quick actions 固定三项：Create Empty Project、Open Folder as Project、Open Existing Project。
    - `Open Existing Project` 调用 `pickDirectory({ title: t("project.openExisting") })` 后直接执行 `projectStore.openProject(path)`；若后端返回 `kind: "needs_confirmation"`，说明用户选到普通文件夹，前端显示现有 `ConfirmationDialog`，文案明确“这不是已有项目，是否初始化为项目？”。
    - `Open Folder as Project` 也使用同一个目录选择器，但按钮文案和确认说明不同：它主动表达“可以把普通资料文件夹初始化为项目”，因此选中普通文件夹后进入 `preview_open_folder_as_project`/`open_project` 的 PendingAction 是预期流程。
    - 两个入口都使用 `src-tauri/src/commands/project_commands.rs::open_project` 作为唯一后端入口，不在前端判断目录类型；差异只体现在按钮意图、空状态文案和确认弹窗标题。
    - “导入资料到已有项目”移出初始页，仅在打开项目后的 Import view 提供。
    - recent grid 显示：name、compactPath、wikiPageCount、sourceCount、graphState、indexState、lastOpenedAt、missing badge。
  - 修改 `src-tauri/src/services/project_service.rs::list_recent_projects`：
    - 对 recent project 轻量 scan，填充 `ProjectSummary` 中已有属性；失效路径标记而不是静默删除。
  - 修改 `src/types/project.ts::RecentProject`，按后端字段扩展，保持旧 JSON default。
  - 修改 `styles.css` `.launch*`、`.projgrid`、`.projcard`，去 hero 化，使用 compact tool surface。
- 需要新增哪些文件：可选 `src/features/project/projectStartSelectors.ts`。
- 需要修改哪些文件：`ProjectStartView.tsx`、`projectStore.ts`、`types/project.ts`、`models/project.rs`、`project_service.rs`、`styles.css`、i18n。
- 是否需要新增依赖：不需要。

## 4. UI / 交互设计
- 界面变化描述：首屏是紧凑项目选择器，顶部三按钮，下面 recent table/card list；不使用大 hero 文案。
- 交互流程：用户打开 app -> 看见三入口和最近项目 -> 点击“打开已有项目”选择一个已经包含 `wiki/`/`.app/`/Obsidian 结构的文件夹 -> 直接进入项目；如果选中普通文件夹，显示初始化确认而不是静默改造。
- 交互流程：用户点击“打开文件夹为项目”选择普通资料文件夹 -> 显示初始化确认，确认后归档 loose files、创建项目结构和 Git 检查点。
- 交互流程：点击 recent 直接 `openProject(rootPath)`；失效 recent 显示移除/重新定位。
- 需要参考的设计规范：`Spec/FRONTEND_GUIDELINES.md` Project Start and Dashboard；AGENTS 禁止 landing hero。

## 5. 验收标准（Done Definition）
- [ ] 初始页没有手填路径输入框。
- [ ] 初始页只有新建空项目、打开文件夹为项目、打开已有项目三个主入口。
- [ ] “打开已有项目”和“打开文件夹为项目”都通过系统目录选择器，不要求手填路径。
- [ ] 选择普通文件夹时不会静默初始化，必须显示 PendingAction 确认。
- [ ] 最近项目展示项目名、路径摘要、页面数/来源数、索引/图谱状态、最近打开时间。
- [ ] 失效项目有明确 badge 和处理入口。
- [ ] 页面中英双语下不溢出，不像营销 landing page。

## 6. 风险与注意事项
- 可能影响的现有功能：App.test 里可能依赖 “Local file or folder paths” textbox，需要更新测试。
- 边界情况：recent project 很多时只显示最近 8-12 个，其余可滚动；scan recent folders 不应明显拖慢启动。

## 7. 实施步骤
- [ ] 改 ProjectStartView 结构，先保留已有 create/open callbacks。
- [ ] 为 Open Existing Project 和 Open Folder as Project 接入 `pickDirectory`，并复用 `open_project` 后端确认模型。
- [ ] 扩展 RecentProject DTO 和 list recent scan。
- [ ] 更新 recent card UI 和样式。
- [ ] 更新 ProjectStartView 测试。

## 条目 D：新建项目选择保存位置使用文件资源管理器

## 1. 需求概述
- 用户想要什么：新建项目时保存位置通过系统文件资源管理器选择，而不是手动输入路径。
- 为什么：手填路径容易出错，对普通用户不友好，且跨平台路径/CJK 路径更容易出现格式问题。

## 2. 现状分析
- 当前代码实现在哪里：审计报告定位到 `src/features/project/ProjectStartView.tsx`、`NewProjectDialog`、`@tauri-apps/plugin-dialog`、`src/features/import/nativeFilePicker.ts`、`src/features/import/OpenFolderAsProjectDialog.tsx`、`src-tauri/capabilities/main.json`、`src-tauri/src/services/project_service.rs`。
- 当前行为是什么：NewProjectDialog 当前手动输入 `rootPath`；项目 service 要求目标路径不存在或为空。
- 问题出在哪里：前端没有 folder picker helper；产品语义需明确选择“父目录 + 项目名”，而不是直接选择最终目录。

## 3. 方案设计
- 第一性原理：新建项目需要两个输入：项目名称和父目录。最终 rootPath 应由应用拼接并展示，让用户确认，而不是让用户手写完整路径。
- 推荐方案：NewProjectDialog 改为“项目名称 + 保存到父目录 + 模板”；点击 Browse 使用 `@tauri-apps/plugin-dialog.open({ directory: true })` 选择父目录；最终 rootPath = parent/name。
- 技术方案：
  - 修改 `src/features/import/nativeFilePicker.ts`：
    - 新增 `export async function pickDirectory(options?: PickDirectoryOptions): Promise<string | null>`
  - 修改 `src/features/project/ProjectStartView.tsx::NewProjectDialog`：
    - state 从 `rootPath` 改为 `parentPath` + `name`。
    - `const rootPath = joinDisplayPath(parentPath, sanitizeProjectName(name))` 前端仅展示，最终仍由后端 validate。
    - Browse 按钮调用 `pickDirectory({ title: t("project.chooseParent") })`。
  - 修改 `src-tauri/capabilities/main.json`，确认已有 `dialog:allow-open` directory 权限；若缺失则补。
  - 后端 `ProjectService::create_project` 保持最终 root path 校验，不信任前端拼接。
- 需要新增哪些文件：不需要，可扩展 `nativeFilePicker.ts`。
- 需要修改哪些文件：`ProjectStartView.tsx`、`nativeFilePicker.ts`、`nativeFilePicker.test.ts`、`src-tauri/capabilities/main.json`（如权限缺失）、i18n。
- 是否需要新增依赖：不需要，`@tauri-apps/plugin-dialog` 已安装。

## 4. UI / 交互设计
- 界面变化描述：New Project dialog 中“保存位置”显示父目录只读输入 + Browse button；下方用 monospace preview 展示将创建的完整路径。
- 交互流程：用户输入项目名 -> 点击 Browse -> 系统文件夹选择器 -> 选择父目录 -> dialog 展示完整路径 -> 点击 Create -> 后端创建项目。
- 需要参考的设计规范：`Spec/APP_flow.md` 新建项目流程；gotchas 中 Tauri dialog capability 和 drag/drop 权限问题。

## 5. 验收标准（Done Definition）
- [ ] 新建项目不要求用户手填完整 rootPath。
- [ ] Browse 打开系统目录选择器并返回父目录。
- [ ] 完整项目路径由父目录 + 项目名生成并展示。
- [ ] CJK 项目名和路径可创建。
- [ ] 若 dialog 权限缺失或选择取消，UI 有清晰状态，不崩溃。

## 6. 风险与注意事项
- 可能影响的现有功能：ProjectStartView 现有测试使用 textbox 输入路径，需要改成 mock `open` dialog。
- 边界情况：项目名为空、包含非法路径字符、父目录不可写、目标目录已存在且非空、用户取消选择。

## 7. 实施步骤
- [ ] 扩展 nativeFilePicker 并写 mock 测试。
- [ ] 改 NewProjectDialog state 和 UI。
- [ ] 检查/补 Tauri capability。
- [ ] 更新 ProjectStartView 测试和 i18n。
