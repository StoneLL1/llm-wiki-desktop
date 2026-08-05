# LLM Wiki Desktop 实施路线图

本目录记录当前代码与有效产品规范之间的落差。它用于排期和实施，不取代上层规范。

## 权威顺序

发生冲突时按以下顺序处理：

1. `AGENTS.md` / `CLAUDE.md` 的安全与工程硬边界；
2. 专题确认规范：
   - [首次使用与打开已有知识库](../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)；
   - [Import / Source / Media](../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)；
   - [Workflows](../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md)；
3. `PRD.md`、`SPEC.md`、`APP_flow.md`、`TECH_STACK.md`、`BACKEND_STRUCTURE.md`；
4. 本目录的 living roadmaps；
5. `SPEC/plans/`、`docs/fixes/`、旧 dated specs/plans/audits，仅作为历史证据。

`UI-Frontend-design/` 约束壳层、视觉密度、组件结构和交互细节。其旧启动页、Import 编译行为和 Agent 主入口不覆盖上述专题规范。

## 使用方式

- 一次认领一个模块或一组有明确依赖的 P0。
- 开工前先看对应专题规范与模块 roadmap，随后核对当前源码；不要把 roadmap 的旧行号当作事实。
- 完成 executable code 后按 `AGENTS.md` 选择 `npm run check:quick` 或 `npm run check`，并更新 roadmap 的实现证据。
- 文档、研究和计划类修改无需 npm gate，但仍要做链接、矛盾和 `git diff --check` 校验。
- 每个重要里程碑在根目录 `progress.txt` 顶部追加；只有重复、隐蔽或易复发的问题写入 `gotchas.txt`。

## 当前模块总览

| 模块 | 当前重点 | 路线图 |
|---|---|---|
| 壳层、无项目工作台、Dashboard | 持久工作台；仅新建/打开两条首屏路径；类型化评估、信任、兼容、恢复；Dashboard 统一状态 | [shell-dashboard.md](shell-dashboard.md) |
| Import / Source | Import 只向当前知识库复制资料；预览、确认、可读 Source、媒体处理；不承担打开或恢复项目 | [专题规范](../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md) / [历史审计](import.md) |
| Workflows | 三个内建工作流、统一准备页、项目级串行队列；app state 可写时持久确认/恢复，否则只读检查为 non-persistent | [agent.md](agent.md) / [实施计划](../../docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md) |
| Wiki | 阅读、编辑、frontmatter、生命周期命令、外部修改冲突和兼容/只读能力 | [wiki.md](wiki.md) |
| Chat | Source 或 Wiki 上下文、流式输出、引用、明确 AI 路由和信任边界 | [chat.md](chat.md) |
| Graph | 从可读 Markdown 建图；受限内存索引；部分扫描；筛选、检查器、导出 | [graph.md](graph.md) |
| Lint | 本地只读检查、深度检查、批量修复、severity、ignore 与 Git 安全 | [lint.md](lint.md) |
| Exports | 生成准备、任务状态、结果列表、预览、重试和覆盖确认 | [exports.md](exports.md) |
| Settings | Agent/Provider/模型/密钥、外观、语言与应用行为；不承载已固定的启动策略或项目模板切换 | [settings.md](settings.md) |
| 跨切面 | IPC、任务、Git、秘密、路径、信任、i18n、性能和兼容策略 | [cross-cutting.md](cross-cutting.md) |

## 全局 P0

1. **首次使用与安全打开**
   - 完整壳层中的两张主卡；
   - 新建默认父目录/模板并进入 Import；
   - 零写入类型化评估；
   - 普通资料文件夹另建知识库后复制；
   - 受限、信任、只读、兼容、Git、修复、恢复和深度扫描。
2. **Import 写入安全**
   - 确认前不写入；
   - 任何需要 Git 检查点的导入写入，必须先成功创建检查点；
   - 原始资料默认不可变。
3. **Workflows 项目访问与任务模型**
   - 无项目不创建任务；
   - 外部 AI/Agent/Skill 需要信任；
   - 写入需要可写与真实 Git 策略；
   - 项目级串行队列、去重、持久确认和重启恢复。
4. **Wiki 生命周期与冲突**
   - 新建、重命名、请求删除的 UI 接线；
   - frontmatter 结构化编辑、Milkdown 工具栏；
   - 外部修改与 Agent 结果的三路 Diff。
5. **Lint 批量修复与分级**
   - 本地只读 Lint 可在受限模式工作；
   - 深度/外部检查需信任；
   - 自动修复需可写、确认和 Git 检查点。
6. **生成内容语言与密钥安全**
   - Chat/Compile/Lint/Export/Workflows 使用用户语言偏好；
   - API Key 仅进 OS 凭据存储，绝不进入项目文件、日志或导出。

## 建议波次

### 波次 1：访问与写入红线

- `shell-dashboard`：类型化项目评估、全局信任与持久工作台；
- `cross-cutting`：项目访问策略、路径身份、外部链接、Git 与任务隔离；
- `import`：检查点前置与只向当前项目复制；
- `settings`：移除旧启动策略目标，补齐真实配置能力。

### 波次 2：首个 Source 到可组织知识

- 新建知识库后进入 Import；
- 预览、确认、提交首个可阅读 Source；
- Wiki 生命周期、编辑工具栏和冲突处理；
- Graph/Chat 对空状态、Source-only 和部分索引给出明确下一步。

### 波次 3：统一 Workflows

- 先落后端项目访问、队列、指纹和恢复契约；
- 再实现 Update Wiki、Health Check、Generate Content；
- 最后迁移 Dashboard、Import、Wiki、Lint、Exports 的共享入口并退役旧 Agent UI。

### 波次 4：体验与可访问性

- Dashboard 信息密度和恢复状态；
- Chat 流式渲染、Graph 检查器/导出、Exports 结果体验；
- 全局键盘、焦点、屏幕阅读器、缩放、主题和跨平台路径回归。

## 关键禁区

- 不恢复独立启动页、三张首屏操作卡、最近项目画廊或首屏 Agent/BYOK/模板墙。
- 不把“导入资料”作为无项目第三入口；Import 只能在当前知识库内工作。
- 不把普通资料文件夹原地初始化、移动、重命名或创建项目标记。
- 不要求先编译才能看到 Source、Graph 或使用有足够 Source 上下文的 Chat。
- 不把 `ProjectRegistry` 路径登记当成用户信任。
- 不让前端决定文件、Git、信任、Agent 路由、密钥或任务安全。
- 不把旧计划、修复记录或审计快照中的未完成项直接当作当前需求执行。
