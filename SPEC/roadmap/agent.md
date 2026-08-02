# Workflows 迁移落差与实施路线

> 状态：Workflows Batch 0–7 已实施并提交；Batch 8 尚未提交、双审尚未闭环，且 First-run / Project-open 外部依赖 B1–B4 尚未落地。旧 Agent 主界面的当前工作树退休改动仍待 Batch 8 最终收口，技术 Agent 能力继续保留。
> 唯一产品与交互权威：[`../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md)。
> 项目访问权威：[`../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。Workflows 不创建项目上下文；外部 AI/Agent/Skill 需要已信任项目，写入还需要可写能力与真实 Git 策略。
> 分批执行计划：[`../../docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md`](../../docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md)。
> 历史实现账本：[`../plans/agent.md`](../plans/agent.md) 只记录旧 Agent 页面曾经完成的工作，不再定义目标界面。
> 视觉密度仍参考 `UI-Frontend-design/assets/app.css` 与现有 Codex-like shell；legacy `agent.html` 不再定义信息架构。

## 0. 迁移目标

把当前面向执行器配置的 Agent 主页面迁移为面向用户任务的 `工作流 / Workflows`：

- 首版固定提供 `更新 Wiki`、`健康检查`、`生成内容`。
- Agent CLI、BYOK、模型和 Provider 是执行路径，配置继续留在设置页。
- Workflows 统一负责准备、排队、进度、确认、结果、重试和历史。
- Lint 与 Exports 页面保持现状，继续分别拥有检查结果 / 修复和制品 / 预览。
- 所有工作流、任务、确认和历史按 canonical project identity 隔离；一个项目内串行运行。项目 app state 不可写时，允许的只读检查与结果仅在内存中存在并标注 non-persistent。
- 无项目时不创建任务；untrusted restricted 仍可运行 Local Quick，trusted read-only 在有具体 AI 路径时可运行 ephemeral Complete Check。写入工作流才要求 writable 与适用 Git；后端在 start 时重新校验，不能依赖前端状态。

不在本轮范围：

- 来源批量整理。
- 用户自定义工作流。
- 定时或事件触发。
- 自定义运行指令。
- 自定义输出模板。
- 跨项目任务汇总。

## 1. 已实现基线

以下能力可复用，不应因导航迁移而删除：

- `AgentService` 的 CLI 检测、版本、进程启动、stdout/stderr、取消和日志。
- Agent/BYOK 编译、深度 Lint、Exports 与 Chat 的现有调用链。
- `TaskService`、当前原生实现的 `.app/tasks/`、任务事件、日志抽屉和系统通知；目标持久路径由 `ProjectLayout.taskStateRoot` 决定。
- Git checkpoint、候选 workspace、基线 hash、编译冲突与 `PendingAction`。
- `ProjectConfirmationController`、`CompileConflictDialog` 和跨项目 epoch 防护。
- 设置页现有 Agent、Provider、模型和默认路径配置。
- 左侧栏底部现有 Agent 状态行。

Batch 8 收口后的生产基线：

- 导航使用 Knowledge Processing / Workflows 与 Lucide `Workflow`，且只展示三个固定内建工作流。
- 所有共享入口进入同一项目绑定的准备、队列、确认、结果与历史模型。
- 后端按 canonical identity、backend-issued trust authority、filesystem access 与 Git 状态重新验证准备、启动、重试、确认和异步 dispatch。
- 健康原生项目由后端创建或严格原生检测 provenance 授信；普通注册和兼容库不会被提升为 trusted。
- Agent 页面、右面板、`RunAgentDialog`、四卡启动器和跨项目通用启动路径已删除；Settings、Lint、Exports 与侧栏 Agent 状态脚继续拥有原领域职责。

## 2. 目标与落差

| 领域 | 当前实现 | 目标 | 优先级 |
|---|---|---|---|
| 导航 | Agent + `Bot`，位于“工作流”分组 | Workflows + `Workflow`，分组改为“知识处理”，无导航徽标 | P0 |
| 主页面 | 配置、入口和任务混排 | 自适应总览；需关注任务优先，否则展示三个固定工作流 | P0 |
| 启动 | `RunAgentDialog` | 占据主内容区的结构化准备页 | P0 |
| 内建操作 | Ingest / Lint / Query / HTML | 更新 Wiki / 健康检查 / 生成内容；Query 留在 Chat | P0 |
| 执行路径 | 页面显著展示 Agent/BYOK | 设置决定默认值；准备页高级区允许单次覆盖；不可静默回退 | P0 |
| 项目访问 | 当前命令主要依赖 ProjectContext 与分散校验 | 无项目不建任务；untrusted 禁止外部执行但保留 Local Quick；trusted read-only 可运行无需写入的 ephemeral Complete Check；mutation 需要 trusted + writable；检查点策略与脏 Git 状态由后端重验 | P0 |
| 任务隔离 | 统一 store，部分全局呈现 | 当前项目内可见、确认与操作；其他项目需切换后查看 | P0 |
| 队列 | 通用任务状态 | 每项目一个工作流运行位，其余串行排队 | P0 |
| 重复启动 | 可以产生重复任务 | 项目 + 工作流 + 范围 + 基线输入指纹去重 | P0 |
| 可观察性 | 状态、百分比、stdout/stderr | 垂直阶段、当前项、数量进度、活动；日志为次级只读信息 | P0 |
| 风险确认 | 以 dialog / conflict 为主 | 按后端 `CheckpointPolicy` 执行：Update Wiki/修复的适用低风险变更在所需检查点后自动应用；Health Check 不写入；Generate Content 新建不需检查点、覆盖需检查点与确认；其他高风险变更持久等待确认 | P0 |
| 重试恢复 | 通用重试 / 持久化 | app/task state 可写时重试创建关联新任务并支持中断恢复；ephemeral 只读检查不承诺跨重启历史 | P0 |
| 结果归属 | Agent 页与各功能入口分散 | Workflows 显示摘要；Lint / Exports / Wiki 保留领域结果页 | P1 |
| 右侧面板 | Agent 配置区 | 随选择切换项目摘要、准备信息、任务状态、确认或结果 | P1 |

## 3. P0 实施切片

### WF-P0-01：导航与命名

涉及：

- `src/components/app/LeftSidebar.tsx`
- `src/stores/navigationStore.ts`
- `src/components/app/WorkspaceRouter.tsx`
- `src/i18n/locales/*`
- 相关 shell / route contract tests

验收：

- 用户看到 `工作流 / Workflows`，使用 Lucide `Workflow`。
- 原 `工作流` 分组显示为 `知识处理 / Knowledge Processing`。
- Workflows 项不显示状态、数量或运行徽标。
- 侧栏底部 Agent 状态行保留。
- URL、内部枚举或 feature 文件夹可以渐进迁移，但所有用户可见文案统一。

### WF-P0-02：项目级工作流任务合同

涉及：

- `src-tauri/src/models/task.rs`
- `src-tauri/src/tasks/task_model.rs`
- `src-tauri/src/tasks/task_service.rs`
- `src-tauri/src/commands/task_commands.rs`
- `src/types/task.ts`
- `src/stores/taskStore.ts`

目标字段与行为：

- runtime `project_id`、后端 opaque `canonical_identity_key + identity_revision`、`workflow_kind`、结构化 scope、route/options、baseline、input fingerprint。
- stage id / order、current item、completed / total、activity records。
- `attempt_of` 关联重试。
- 用户可见状态：queued、running、waiting for confirmation、succeeded、failed、cancelled、interrupted。
- 按 canonical identity/revision 的单项目串行队列；同 identity + workflow + scope + route/options + baseline 输入去重，跨重开不依赖 runtime `project_id`。
- command、event、selector、drawer、confirmation 和 history 均校验项目归属。

验收：

- 同一项目同时只运行一个工作流。
- 完全相同输入再次启动时打开已有任务。
- 切换项目后不再看到或操作前一项目任务。
- `ProjectLayout.taskStateRoot` 可写时，等待确认和排队任务可恢复；重开后排队任务等待用户明确继续，崩溃时运行任务变成 interrupted。restricted/read-only 的 ephemeral 检查只在当前运行可见，并明确“不持久化”。

### WF-P0-03：总览与准备页

涉及：

- legacy `src/features/agent/AgentView.tsx` 及 `RunAgentDialog.tsx`
- 新的或迁移后的 Workflows feature components
- `src/components/app/RightContextPanel.tsx`
- `src/features/agent/useAgentWorkflow.ts` 或其 Workflows 替代

总览验收：

- 紧凑行列表，固定顺序为更新 Wiki、健康检查、生成内容。
- 运行中、等待确认或失败任务优先于可用工作流。
- 没有需关注任务时最多推荐一个下一步，不重排、不自动运行。
- 没有卡墙、BYOK 卡、CLI 配置表或大号全局新建按钮。

准备页验收：

- 点击工作流进入完整主内容区，不弹 Run Agent dialog。
- 显示范围、先决条件、输出、执行路径摘要和主操作。
- 路径选择折叠在高级设置；覆盖只影响本次任务。
- 缺少路径时仍可点击，随后提示去设置配置。
- 无项目时引导新建/打开知识库且不创建任务；untrusted 项目的 Local Quick 可运行，Complete/外部执行提供“信任知识库”；trusted read-only 的 Complete 可运行但结果 non-persistent，写入工作流说明“需要可写知识库”；Git 条件只门禁对应 checkpoint-required 写入。
- 第一次确认范围，后续相同上下文支持快速重跑。
- Health Check 第一次在项目已信任且有具体 AI 路径时默认完整检查，否则默认本地快速检查；后续仅在 workflow state 可写时持久记住最近模式，否则本次运行内记忆。
- 从设置或信任流程返回时保留结构化范围，但不自动开始运行。

### WF-P0-04：三个可观察流水线

阶段顺序必须逐项实现设计规范 §11：

- Update Wiki：9 个阶段。
- Health Check：8 个阶段。
- Generate Content：9 个阶段。

验收：

- 任务详情首先显示整体主状态和垂直阶段。
- 当前阶段显示正在处理的文件 / Source / 规则 / 制品。
- 只在总量可靠时显示百分比，否则显示阶段与活动。
- stdout/stderr 在折叠的只读日志中，不承担用户主状态。
- 阶段、失败和等待确认都能由键盘与屏幕阅读器理解，不能只依赖颜色。

### WF-P0-05：安全、取消、重试与恢复

验收：

- Update Wiki 与修复中的低风险、无冲突修改在所需 Git checkpoint 成功后自动应用；Generate Content 新建制品不要求 checkpoint，覆盖既有制品需要 checkpoint 与确认。
- `prepare_workflow` 与 `start_workflow` 都按 canonical project identity/revision 校验独立 trust、filesystem access、health、layout capabilities、适用 Git policy 和 baseline；任何漂移都要求重新准备。只读检查不得被无关的 writable/Git 条件阻断。
- 既有脏 Git 状态不自动清理、提交、重置或 stash；只读/受限项目不能借由旧 preparation token 绕过写入边界。
- 删除、覆盖、广泛重写和冲突在可写 `taskStateRoot` 中进入持久 waiting-for-confirmation；若无法持久化确认状态，后端不得启动这类 mutation。
- 确认入口显示受影响路径 / 数量、风险、checkpoint，并允许按需查看 Diff。
- 用户等待确认时可以继续浏览和编辑；应用写入前重新核对 baseline，并进行三方合并。
- 排队任务立即取消并提供短暂撤销；运行中任务取消前解释候选将被丢弃并要求确认。
- 取消不把未确认候选提升到正式 Wiki 或 Exports 路径。
- 重试创建新 attempt，保留原失败记录。
- 进程中断后说明已完成阶段与可复用产物，不显示虚假的“继续运行”。
- 通知只用于等待确认、完成和失败。

### WF-P0-06：入口统一

以下入口必须落入同一个准备模型和项目级 TaskService：

- Import 完成摘要 → Update Wiki。
- Dashboard → 对应工作流。
- Wiki 文章 → Generate Content，并预填当前文章。
- Lint → Health Check 或后续修复动作。
- Exports → Generate Content。
- Workflows → 三个内建工作流。

验收：

- 不同入口产生相同的 scope、route、task、confirmation 和 history 语义。
- Import 确认仍不得自动编译。
- Health Check 只读，修复继续在 Lint。
- 生成内容完成后，制品继续在 Exports 管理。

## 4. P1 收口

### WF-P1-01：右侧上下文

- 无选择：项目工作流摘要、待更新 Source、最近健康结果、最近制品。
- 选择工作流：先决条件、范围、路径、Git 策略、输出位置。
- 活动任务：阶段、当前项、队列位置、取消。
- 等待确认：影响摘要、checkpoint、查看 Diff、确认 / 拒绝。
- 已完成：结果摘要和领域结果页入口。

### WF-P1-02：历史与结果

- 仅显示当前项目。
- 可按工作流与状态筛选。
- 记录 route、scope、baseline、结果、错误、checkpoint、attempt link。
- 完成结果可以建议下一工作流，但永远不自动启动。

### WF-P1-03：首次披露与国际化

- 第一次使用远程 Provider 时，只披露一次将发出的数据范围。
- 通知不含敏感路径、密钥或模型输出正文。
- 中英文状态、阶段、风险和空状态都能在规定宽度内显示。

## 5. 明确保留现状

- Settings 的 Agent / BYOK / Provider 配置首轮保持现状，只补必要的默认路径读取和配置跳转。
- Lint 页面首轮保持现状。
- Exports 页面首轮保持现状。
- 侧栏底部 Agent 状态行保持现状。
- AgentService、LlmService、LintService、ExportService 和 CompileService 继续作为后端能力，不因页面更名而机械重命名。

## 6. 删除或退役

- Agent 主导航名称与 `Bot` 图标。
- Agent 页面中的配置卡、Provider 卡和 CLI 管理主区。
- 四宫格 Ingest / Lint / Query / HTML 启动器。
- `RunAgentDialog` 作为通用工作流入口。
- 工作流主区的可编辑任意 prompt / 任意 Skill 选择。
- 任何跨项目任务列表或确认入口。

退役代码前先确认没有其他入口依赖；按小步迁移保留可回滚提交，不进行无关文件夹重命名。

## 7. 验证与交付

这是跨前后端、任务安全和文件写入行为的高风险迁移，完成时必须：

1. 覆盖队列、去重、项目隔离、状态迁移、重试和 crash recovery 的 Rust 测试。
2. 覆盖导航、总览排序、准备页、阶段时间线、右面板、i18n 和可访问性的前端测试。
3. 覆盖六个共享入口进入同一 task model 的集成测试。
4. 覆盖 Git checkpoint 失败阻断、高风险确认、并发编辑和三方冲突。
5. 运行完整 `npm run check`，修复后从头重跑。
6. 按 `AGENTS.md` 启动两轮代码审查；本文档迁移本身不需要代码审查子代理。

完成定义以 Workflows 设计规范 §20 的验收标准为准。
