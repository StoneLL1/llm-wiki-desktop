# LLM Wiki Desktop — 产品需求文档（PRD）

> 导入、来源库、本地与远程媒体、OCR、ASR、平台登录态和 AI 整理的已确认产品决策，以 [`../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为唯一可信源。本文中的概览不得被解释为“确认导入后自动编译”。
> Batch 9 的可重复验收索引位于 [`../docs/qa/import-source-media-flow-batch9-evidence.json`](../docs/qa/import-source-media-flow-batch9-evidence.json)，并由 `npm run check:import-source-media` 只读校验。
> 原 Agent 主页面已被重新定义为“工作流”。其信息架构、三个内置工作流、项目隔离队列、可观察流水线、状态、确认与跨页面入口，以 [`../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md) 为唯一可信源；Agent / BYOK 是执行路径，不再是一级页面的组织方式。
> 无项目工作台、新建知识库、打开原生或兼容知识库、目录评估、信任 / 受限 / 只读、兼容启用、修复与进入 Import 的已确认产品决策，以 [`../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md) 为唯一可信源。本文中的摘要不得被解释为允许把普通资料文件夹原地初始化成项目。

## 1. 文档信息

| 项目 | 内容 |
|---|---|
| 产品名称 | LLM Wiki Desktop |
| 文档类型 | Product Requirements Document |
| 当前版本 | v0.1 |
| 目标版本 | MVP / 首个可用版本 |
| 主要依据 | `SPEC.md`、前期产品决策、LLM Wiki / Open Design 参考方向 |
| 目标读者 | 产品、设计、前端、后端、Agent/Skill 开发、测试与发布人员 |

## 2. 产品概述

LLM Wiki Desktop 是一款本地优先的跨平台桌面知识库应用。它帮助用户把分散的文章、网页、PDF、Office 文档、Markdown 笔记和剪贴板内容，自动整理成结构化、互相链接、可探索、可问答、可导出的 Markdown Wiki。

产品遵循 Karpathy 的 LLM Wiki 模式：原始资料保留在 Raw Sources，LLM 或 Agent 将资料编译成 Wiki 页面，Schema 定义整理规则和页面结构。与传统 RAG 的差异在于：知识不是每次提问时临时拼接，而是被持续沉淀为可阅读、可维护、可版本化的知识库。

首版采用“零门槛知识库 + 工作流驱动 + Agent 增强”策略：用户通过产品定义的工作流更新、检查和生成知识成果；高级用户可以接入 Claude Code、Codex、OpenClaw、Hermes 等本地 Agent CLI，Agent 与 BYOK 均作为可替换执行路径。

## 3. 背景与问题

### 3.1 用户现状

个人知识管理者和研究型用户常常面对以下情况：

- 资料散落在 PDF、网页、公众号文章、PPT、Word、Excel、Markdown 和本地文件夹中。
- 普通笔记工具依赖手工整理，成本高，长期难以维护。
- RAG 工具能问答，但不擅长把知识沉淀成稳定、可编辑、可复用的页面。
- 图谱工具通常需要用户手动建节点、连线，不适合大量资料导入后的自动探索。
- AI Agent 能整理资料，但命令行门槛高，普通用户不容易获得稳定、可恢复、可视化的工作流。

### 3.2 核心问题

用户需要一个本地桌面应用，把“资料导入、知识整理、图谱探索、问答溯源、HTML 输出、版本恢复”整合成一个可靠闭环。

### 3.3 产品机会

LLM Wiki Desktop 的机会在于：

- 以 Markdown 文件夹作为长期资产，避免锁定在单一应用。
- 用 Agent/LLM 自动完成重复整理工作，降低个人知识库维护成本。
- 先用已确认、可阅读的 Source 兑现首次价值，再用知识图谱提供后续的结构化惊喜。
- 用 Git 自动检查点保护用户资料和 AI 修改结果。
- 用 Skill 系统承接未来扩展，而不是把所有能力硬编码在应用中。

## 4. 产品目标

### 4.1 MVP 目标

MVP 必须让用户完成以下闭环：

1. 新建或打开一个项目。
2. 导入一批多格式资料。
3. 预览最终 Source 并确认导入到来源库。
4. 用户另行选择用这些 Sources 更新 Wiki，生成索引、概览和页面链接。
5. 查看可探索的知识图谱。
6. 基于可读 Source 或 Wiki 内容进行 Chat 问答并查看引用来源。
7. 生成单篇 HTML/知识卡片或项目级 HTML 报告。
8. 通过 Git 检查点、Lint、后台任务和通知保证过程可恢复、可维护。

其中首次价值不以编译、图谱或 Chat 成功为前提：用户确认导入后能立即打开一篇可阅读 Source，即完成 time-to-first-value。

### 4.2 产品原则

- **本地优先**：项目内容默认只保存在本地文件夹中。
- **文件透明**：Wiki 页面是普通 Markdown，可被 Obsidian、Git 和外部编辑器访问。
- **导入不破坏**：原始资料默认保留，导入层尽量无损。
- **AI 可控**：用户显式启动的普通编译可在无冲突时自动完成；高风险删除、覆盖和冲突操作需要确认。
- **Source 先兑现价值**：成功导入并读到 Source 是第一价值；编译、图谱和 Chat 是清晰可见的后续能力，不得反过来阻塞首次成功。
- **Agent 增强而非强依赖**：Agent 提供高级能力；Source 已形成后的 AI 整理、Wiki 编译和 Chat 可由 BYOK API 支撑，Import 解析恢复除外。
- **可恢复**：AI 和批量操作必须被 Git 检查点保护。

## 5. 目标用户

### 5.1 首要用户：个人知识管理者

**画像**：长期收藏文章、PDF、网页、课程材料、项目笔记，希望降低整理成本。

**核心诉求**：

- 快速导入大量资料。
- 自动生成结构化知识库。
- 通过图谱发现主题和关系。
- 能搜索、阅读、编辑和问答。
- 不想先学习复杂 Agent CLI 或 Git。

### 5.2 次要用户：研究者

**画像**：需要整理多来源资料、追踪引用、做主题综合和对比分析。

**核心诉求**：

- 资料来源可追踪。
- 问答有引用依据。
- 能生成综合页面、对比页面和项目报告。
- 能持续维护资料变化，避免旧内容失效。

### 5.3 高级用户：AI/技术从业者

**画像**：熟悉 Claude Code、Codex 等 Agent CLI，希望把本地 Agent 接入知识库工作流。

**核心诉求**：

- 使用本地 Agent 和 Skill 执行高级整理、Lint、HTML 生成。
- 保留项目文件结构、Git 历史和可扩展能力。
- 能控制 Agent 输出和回滚结果。

## 6. 用户场景

### 6.1 从普通资料文件夹开始

用户有一个包含 PDF、Word、PPT、网页摘录、图片和音视频的普通资料文件夹。用户从首屏选择“打开已有知识库”后，应用只读判断它不是知识库，并提供“用这些资料新建知识库”。用户选择新的项目名称与父目录后，应用创建独立知识库，再把原文件以导入方式复制 / 归档进新项目；原资料文件夹不被移动、改名、写入 `.app/` 或原地初始化。导入确认后，用户立即查看 Sources，也可以稍后另行选择“用这些来源更新 Wiki”。

### 6.2 导入资料到已有项目

用户已有一个 LLM Wiki 项目。用户把新的文件夹拖入应用，选择“导入到当前项目”。应用解析文件与媒体、按需请求 OCR / ASR / 登录等操作、展示 Source 预览；用户确认后只写入原始资料与来源库，不自动触发编译。

### 6.3 基于知识库提问

用户在 Chat 中询问“这些资料里关于 Agent Memory 的主要观点是什么？”。应用搜索已提交的可读 Source 与 Wiki 页面，在项目已信任且具体 Agent/BYOK 路径可用时生成回答，并展示类型化引用。只要有可读 Source、项目已信任且 AI 路径可用，用户不必先编译 Wiki。用户可将优质回答保存到 layout-defined queries root（原生知识库为 `wiki/queries/`）；保存属于项目写入，需要可写能力和对应 Git/确认策略。

### 6.4 生成可分享材料

用户打开一篇概念页面，可在不离开 Wiki 的情况下打开单篇快速弹窗，生成新的美化阅读页、知识卡片或概念图；普通 Export 任务立即可见，成功后在当前页面自动预览。需要多页范围、项目报告、显式输出路径或覆盖既有制品时，用户从 Workflows / Exports 进入完整“生成内容”准备、队列与确认流程。两条路径的结果都写入项目布局解析出的导出目录（原生项目为 `exports/html/`），并由 Exports 统一管理，便于阅读、归档和分享。

### 6.5 维护知识库健康

用户从工作流运行“健康检查”。第一次运行时，有可用 AI 路径则默认完整检查，否则默认本地快速检查；后续记住该项目最近模式。完整检查先执行本地规则，再执行深度检查并合并重复发现，把死链、孤立页面、重复主题或来源缺失交给现有 Lint 页面展示。健康检查本身只读；用户随后在 Lint 选择并一次批准整个修复批次后，应用才创建 Git 检查点并验证、应用候选；只有删除、未授权既有路径覆盖或冲突需要二次确认。

## 7. 范围定义

### 7.1 MVP 范围

MVP 包含：

- 知识库创建、打开、最近知识库管理。
- 原生与兼容 Markdown 知识库的只读评估、受限打开、信任、兼容启用与修复确认。
- 普通资料文件夹通过“新建知识库并导入”进入，不在原目录初始化或搬移原文件。
- 多格式资料和媒体导入、解析预览、确认写入来源库。
- Wiki 独立编译与页面浏览。
- WYSIWYG 阅读/编辑切换。
- 页面级知识图谱。
- Chat 问答与引用溯源。
- 工作流主界面：更新 Wiki、健康检查、生成内容。
- Agent CLI 检测与配置保留在设置；工作流任务提供结构化阶段、进度、日志、取消、确认和历史。
- BYOK API 可选执行路径。
- HTML/卡片/项目报告生成。
- Git 自动检查点、合并冲突处理和恢复。
- 本地快速 Lint + Agent 深度 Lint。
- 后台任务、托盘行为、系统通知。
- 中英双语、亮色/暗色主题、更新检查。

### 7.2 非目标

PRD 仅列出当前不纳入 MVP 的内容，后续可重新评估：

- 团队协作与多人实时编辑。
- 云同步和账号系统。
- 移动端应用。
- 浏览器扩展。
- 插件市场。
- 内置复杂权限沙箱。
- 完整网页离线归档。
- 图谱中手动拖线创建关系。
- 反向链接独立面板。
- 普通搜索栏自动触发模型问答。
- 定时或事件触发的自动化。
- 用户自定义工作流、任意提示词任务、自定义运行要求和自定义输出模板。

## 8. 核心产品流程

### 8.0 无项目工作台

1. 应用始终渲染完整桌面 shell，不切换到独立启动 / 营销页。
2. 中心工作区只显示两个紧凑主卡片：“新建知识库”和“打开已有知识库”。
3. 导航保持可见；尚不可用的模块说明缺少的前置条件，并只提供一个上下文动作。
4. 右侧上下文面板只解释本地目录、打开策略与兼容边界，不展示 Agent / BYOK 配置。

### 8.1 新建知识库流程

1. 用户点击“新建知识库”。
2. 输入知识库名称，选择父级保存位置和项目模板；应用根据名称生成最终子目录。
3. 父级位置初始默认是系统 Documents 下的 `LLM Wiki`，之后记住最近一次父级位置。
4. 模板显示通用、研究、阅读、商业、个人成长，默认选择“通用”；模板只在创建时生效，创建后不提供切换。
5. 应用校验跨平台非法字符、Unicode / CJK、大小写和规范化冲突；已存在且非空的目标目录必须阻止创建。
6. 应用生成统一项目结构，根据模板写入 `purpose.md` 和 `schema.md`，初始化本地 Git 并创建初始提交。
7. 创建成功后进入当前知识库的 Import 工作台，但不自动弹出系统文件选择器。

### 8.2 打开已有知识库

1. 用户点击“打开已有知识库”并选择目录。
2. 后端先执行无写入的快速评估：目录格式分类为当前原生、旧版原生、`nashsu/llm_wiki`、Obsidian、普通 Markdown vault、歧义 Markdown、普通资料或未知；健康另行判断为 healthy、repairable、recovery 或 unreadable，损坏 / 不完整不属于格式分类。
3. 健康的当前原生知识库直接进入 Dashboard；健康的旧版 LLM Wiki 与 `nashsu/llm_wiki` 进入兼容 Dashboard；未信任的 Obsidian / 可识别 Markdown vault 进入受限兼容 Dashboard；健康且已信任的兼容知识库直接进入 Dashboard。安全路由确定后，在对应工作台后台执行可取消深度扫描。
4. 歧义 Markdown 目录让用户选择“以 Markdown 知识库打开”或“用这些资料新建知识库”；选择记在全局设置，不向原目录写标记。
5. 普通资料目录只提供“用这些资料新建知识库”，并跳转到新建流程；原目录不被初始化、移动或重命名。
6. 首次信任按规范化后的目录身份保存在应用全局配置，不写入项目；目录移动、替换或身份变化时重新询问。
7. 兼容启用只能在用户确认后写入 `.app/` 与 `.app/compat/{purpose.md,schema.md}`；根目录同名文件始终视为用户内容。
8. 可安全重建的派生状态可以自动生成修复计划，但任何落盘修复必须先展示完整确认页。Markdown 仍可读时，用户也可选择“暂不修复，以受限模式打开”。

快速评估由后端短期 registry 持有：启动只返回 application-scoped `assessmentOperationId`，取消命令只接受该 opaque ID；取消不创建项目任务、丢弃未完成快照并保留无项目工作台。完成后返回独立、短期有效的 `assessmentId`，供打开、信任、兼容启用或修复命令重验。

### 8.3 导入到当前项目

1. 用户点击“导入”或拖拽文件/文件夹。
2. 选择“导入到当前项目”。
3. 应用创建可恢复导入会话，并执行安全扫描、类型识别和确定性提取。
4. 对确实缺少正文的项目，按需等待登录、OCR、ASR、能力安装或 Agent 修复。
5. 展示最终 Source Markdown 预览、质量问题、目标路径；更新项展示 Diff。
6. 所有可提交项默认勾选，用户点击“导入到来源库”后写入布局定义的 evidence 与 Source roots；新建原生知识库映射为 `raw/` 和 `wiki/sources/`。
7. 展示已导入、已更新、重复和待处理摘要。
8. 用户需要时另行点击“用这些来源更新 Wiki”；导入本身不自动编译。

### 8.4 更新 Wiki（编译）流程

1. 用户从工作流、Dashboard 或导入完成摘要进入“更新 Wiki”的运行准备页。
2. 应用自动检测新增和变化的 Source，并允许用户查看或排除范围；完整重新编译属于高级操作。
3. 准备页按设置中的默认路线选择 Agent CLI 或 BYOK API；单次路线覆盖不改变全局默认值。用户确认范围并明确启动后才创建任务，进入准备页本身不修改 Git。
4. 任务先分析变化并锁定基线，再在任何正式 Wiki 写入前创建所需 Git 检查点；检查点失败时零正式写入。
5. 编译器读取 `ProjectContext.layout` 解析出的 purpose/schema 上下文、选定 Source 版本和现有页面根；新建原生知识库分别映射到根 `purpose.md`、`schema.md`、`wiki/sources/` 与 `wiki/`，兼容知识库不得被强制改造成这些根路径。任务按“分析变化 → Git 检查点 → 规划 → 生成候选 → 校验 → 风险检查 → 应用 → 刷新索引与图谱 → 完成”的结构化阶段运行。
6. 低风险且无冲突的变更自动应用；删除、覆盖、大范围重写和人工编辑冲突进入等待确认，先展示风险摘要并可展开逐文件 Diff。
7. 成功后提交 Git 结果并刷新 UI、搜索和图谱缓存。
8. 编译器不得写入或删除任何 layout-defined Source root（新建原生知识库映射为 `wiki/sources/`）。

### 8.5 图谱探索流程

1. 应用读取当前访问模式允许的 Source/Wiki Markdown 与 `[[wikilinks]]`，不要求先编译 Wiki。
2. 结合多信号关联度模型生成页面级图谱。
3. 展示节点、边、页面类型颜色和社区聚类。
4. 用户点击节点进入文章。
5. trusted writable 项目缓存布局；restricted/read-only 项目只保留内存结果；深度扫描未完成时显示部分结果。

### 8.6 Chat 问答流程

1. 用户进入 Chat。
2. 输入自然语言问题。
3. 应用搜索相关的已提交 Source 和 Wiki 页面。
4. 已信任项目通过明确的 Agent CLI 或 BYOK API 路径生成回答；缺少配置时进入配置页，返回后不自动发送。
5. 回答展示 Source/Wiki 类型化引用来源。
6. 用户可保存回答到 layout-defined queries root（新建原生知识库映射为 `wiki/queries/`）；保存时重新校验项目可写性和 Git/冲突策略。

### 8.7 Lint 修复流程

1. 用户从工作流运行“健康检查”或在 Lint 页面点击检查。
2. 应用执行本地快速 Lint。
3. 项目已信任且存在真实可执行的适用 AI 路径时可执行深度 Lint；Agent 路线使用应用内置、版本固定的 `wiki-lint` Skill，项目 purpose/schema/layout context 只作为不可信输入，项目同名 Skill 不得覆盖；否则仍可完成本地快速检查。
4. 健康检查只读，把问题列表与修复建议交给现有 Lint 页面。
5. 用户在 Lint 选择一批 eligible Finding，并一次批准整个 Agent 修复批次；Agent 修复没有 BYOK fallback。
6. 批批准成功后，queued dispatch 在第一次 Agent repair invocation 前创建 clean-HEAD Git 检查点；失败时 Agent invocation 与候选/真实项目 mutation 都为 0。dirty/no-Git/read-only/untrusted 状态直接返回 prerequisite，不 stash、不吸收现有改动。
7. Agent 只在 task-owned candidate 中修改 backend 授权的 Wiki Markdown；`raw/**`、忠实 Source 页面、`wiki/sources/**` 与 layout-defined Source roots 永远只读。后端用既有 manifest/hash/checked apply 验证并应用候选。
8. 普通 selected-path 更新与安全新建由初次批准覆盖；删除、未授权既有路径覆盖和基线/用户编辑冲突进入持久二次确认并提供按需 Diff。Source/raw 越界候选直接失败，不可确认继续。
9. 每轮应用后运行 deterministic Lint，最多三轮 Agent 修复；仍未解决时保留 Diff、checkpoint/final commit 与 Git 回滚事实，返回 partial/manual-review typed result，第四次 Agent invocation 必须为 0。
10. 成功或人工处理终态后提交验证过的结果并刷新现有索引、报告和 UI。

当前实现状态（2026-08-13）：H3–H5 已实现真实 Agent Health route、受保护的 repair task 与 Lint/Workflows 结果入口；只有 deterministic Lint 证明消失的 Finding 才能自动关闭，duplicate_topic、contradiction 等没有 proof 的语义 Finding 保持人工处理。H6 的 full gate 与完整矩阵尚未全绿，因此 Decision Gate H / Batch 7 仍保持阻断。

## 9. 功能需求

### 9.1 项目管理

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-PROJ-001 | 用户可以新建 LLM Wiki 项目 | P0 | 创建后包含 `purpose.md`、`schema.md`、`raw/`、`wiki/`、`.app/`、`exports/` |
| PRD-PROJ-002 | 用户可以打开已有 LLM Wiki / `nashsu` / Obsidian / Markdown 知识库 | P0 | 只读评估分别返回 format、trust、filesystem access 与 healthy / repairable / recovery / unreadable health，再进入正确路径 |
| PRD-PROJ-003 | 普通资料文件夹可以作为新知识库的导入来源 | P0 | 新项目建在用户另选位置；原目录不被初始化、移动、重命名或写入应用状态 |
| PRD-PROJ-004 | 应用记住最近知识库并支持快速切换 | P1 | 最新历史条目有效时自动打开并落 Dashboard；无历史时显示双入口工作台；最新路径缺失 / 不可访问时显示同一工作台与该路径错误，不静默回退到更旧项目 |
| PRD-PROJ-005 | 项目模板只影响创建时的 `purpose.md` 和 `schema.md` | P1 | 默认通用；创建后不提供模板切换，核心目录结构始终一致 |
| PRD-PROJ-006 | 兼容知识库按分类与既有信任进入正确访问模式 | P0 | 旧版 LLM Wiki / `nashsu` 可直接进入兼容 Dashboard；未信任的外部 Vault 进入受限模式；信任保存在全局配置；受限模式允许本地阅读、搜索、内存图和只读盘点，但禁止外部 AI、Agent、Skill、写入型任务和项目改动 |
| PRD-PROJ-007 | 修复与兼容写入必须预览并确认 | P0 | 确认页列出状态、动作、路径、Git 条件和失败回退；取消后仍可受限或只读浏览 |

### 9.2 导入与解析

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-IMP-001 | 支持导入 PDF、Office、CSV、MD、TXT、HTML、常见图片、音频与视频 | P0 | 支持格式均能进入统一会话，并按对应 OCR / ASR / 字幕流程形成候选 |
| PRD-IMP-002 | 支持 URL 和剪贴板导入 | P0 | 网页、平台文章、图文和视频生成可追溯 Source；文本剪贴板生成本地 Source |
| PRD-IMP-003 | 支持文件夹导入 | P0 | Import 只把文件夹内容导入当前项目；识别 / 打开知识库属于首屏“打开已有知识库” |
| PRD-IMP-004 | 导入前展示最终 Source 预览 | P0 | 预览包含来源、状态、质量、Markdown、资源、目标路径；更新项包含 Diff |
| PRD-IMP-005 | 每个成功导入项同时写入原始证据和可读 Source | P0 | 项目布局定义的 evidence 与 Source roots 中可找到成对记录；新建原生知识库映射为不可变 `raw/` 证据与对应 `wiki/sources/` 页面 |
| PRD-IMP-006 | 导入阶段按内容缺口启用本地 OCR / ASR | P0 | 已有可靠文本或字幕时不打扰；缺少必需正文时等待用户启用对应能力 |
| PRD-IMP-007 | 导入与 Wiki 编译相互独立 | P0 | “导入到来源库”不启动编译；用户另行点击“用这些来源更新 Wiki” |
| PRD-IMP-008 | 来源去重、更新和人工编辑保护 | P0 | 完全重复不新建 Source；同一来源更新经 Diff 或三方合并写入新版本 |
| PRD-IMP-009 | 平台登录态安全复用 | P0 | 有效会话自动复用，匿名不足时再登录；Cookie 不进入 React、项目、日志和导出 |
| PRD-IMP-010 | Source 支持 AI 整理 | P1 | 顶部仅新增“AI 整理”；候选含唯一“内容概览”，经 Diff 和确认后更新，忠实原稿可恢复 |
| PRD-IMP-011 | Source 支持永久整包删除 | P1 | 独立二次确认页显示释放空间和引用页面；Git 检查点后按 sourceId 删除所有版本与证据 |

### 9.3 Wiki 编译

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-WIKI-001 | 支持 Agent CLI 编译 Wiki | P0 | trusted writable 项目配置 Agent 后可生成 Wiki 页面 |
| PRD-WIKI-002 | 支持 BYOK API 编译核心内容 | P0 | trusted writable 项目未配置 Agent 时仍能生成基础 Wiki |
| PRD-WIKI-003 | 编译生成布局定义的 index、overview、log | P0 | 原生项目更新 `wiki/index.md`、`wiki/overview.md`、`wiki/log.md`；兼容项目按 `ProjectContext.layout` 写入且不覆盖用户根级指导文件 |
| PRD-WIKI-004 | 编译保护人工编辑 | P0 | 外部修改冲突时展示 Markdown Diff |
| PRD-WIKI-005 | 来源永久删除后识别引用缺失 | P1 | 删除不自动改写派生 Wiki；Lint 标出仍引用该 Source 的页面 |

### 9.4 文章浏览与编辑

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-READ-001 | 展示知识库 Markdown 文件树 | P0 | 原生项目展示 `wiki/`，兼容项目展示后端 layout 允许的页面根；restricted/read-only 可浏览但不伪装可写 |
| PRD-READ-002 | 支持 Markdown 阅读渲染 | P0 | 支持表格、代码、数学公式、wikilinks |
| PRD-READ-003 | 支持阅读/编辑一键切换 | P0 | 用户可在应用内 WYSIWYG 编辑正文 |
| PRD-READ-004 | 保存后自动刷新索引和图谱 | P0 | 保存文章后搜索和图谱能反映最新内容 |
| PRD-READ-005 | 显示相关文章 | P1 | 文章页展示与当前页面相关的页面列表 |

### 9.5 知识图谱

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-GRAPH-001 | 以可读 Markdown 文档作为页面级节点 | P0 | Source-only 与已有 Wiki 的项目都能形成与可读文档基本一致的节点集合，不要求先编译 |
| PRD-GRAPH-002 | 使用 wikilinks 和多信号关联度生成边 | P0 | 相关页面之间能形成连线 |
| PRD-GRAPH-003 | 支持类型着色和社区着色 | P0 | 用户可切换颜色模式 |
| PRD-GRAPH-004 | 支持节点点击跳转文章 | P0 | 点击节点打开对应页面 |
| PRD-GRAPH-005 | 在允许持久化时缓存图谱布局 | P0 | trusted writable 项目二次打开秒级可用；restricted/read-only 使用内存结果且不写缓存 |
| PRD-GRAPH-006 | 连线统一表示“相关” | P1 | 首版不展示复杂关系类型和证据 |

### 9.6 Chat 问答

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-CHAT-001 | 支持多会话 | P0 | 用户可创建、重命名、删除会话 |
| PRD-CHAT-002 | 基于可读 Source 或 Wiki 页面生成回答 | P0 | Source-only 项目在未编译时也能回答，且内容与本地召回结果相关 |
| PRD-CHAT-003 | 回答展示类型化引用来源 | P0 | Source/Wiki 引用可点击跳转到对应内容 |
| PRD-CHAT-004 | 优质回答可保存到布局定义的 queries root | P1 | 保存后生成 Markdown 查询记录；原生知识库映射为 `wiki/queries/` |
| PRD-CHAT-005 | 普通搜索框不自动调用模型 | P1 | 自然语言问答从 Chat 入口触发 |

### 9.7 工作流、Agent 与 Skill

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-WORKFLOW-001 | 提供三个内置工作流 | P0 | 工作流页可启动“更新 Wiki”“健康检查”“生成内容” |
| PRD-WORKFLOW-002 | 工作流按项目隔离并串行执行 | P0 | 当前项目同一时间只有一个工作流运行，后续任务排队；切换项目不混显任务或历史 |
| PRD-WORKFLOW-003 | 提供工作流运行准备页 | P0 | 展示范围、输出、项目访问、Git 策略和次级执行路线；prepare/start 都按 canonical identity 重验，缺少前置条件时给出可恢复引导 |
| PRD-WORKFLOW-004 | 提供可观察流水线 | P0 | 不查看原始日志也能看到任务主状态、当前阶段、当前对象和真实子进度 |
| PRD-WORKFLOW-005 | 统一完整工作流启动并保留 Wiki 快速例外 | P0 | Dashboard、Lint、Exports 和 Workflows 的完整工作流入口使用同一 preparation / Workflow task 合同；Wiki 单篇快速动作留在 Wiki，以普通 Export task 创建新文件且不进入 Workflow history；无项目时不创建任务 |
| PRD-WORKFLOW-006 | 支持队列、取消、显式重试和历史 | P0 | 重试生成关联的新任务；取消、失败、中断、等待确认均有明确可恢复状态 |
| PRD-AGENT-001 | 检测本地 Agent CLI | P0 | 显示可用 Agent、版本和状态 |
| PRD-AGENT-002 | 提供安装引导 | P0 | 未安装 Agent 时展示指引和命令 |
| PRD-AGENT-003 | Agent 执行输出实时展示 | P0 | 任务详情以结构化阶段为主，并可展开 stdout/stderr |
| PRD-AGENT-004 | 支持取消任务 | P0 | 正在运行任务可取消 |
| PRD-AGENT-005 | 支持后台运行 | P0 | 关闭主窗口后任务可继续运行 |
| PRD-AGENT-006 | HTML 和 Lint 通过 Skill 驱动 | P0 | HTML 使用相应 `skills/html-*`；Lint 的唯一权威是应用内置、版本固定且可审计 id/version/hash 的 `wiki-lint`，项目同名 Skill 不得覆盖 |
| PRD-AGENT-007 | Agent 与 BYOK 配置留在设置 | P0 | 工作流主界面不展示 CLI/BYOK 配置仪表盘，仅显示次级路线摘要和设置入口 |

### 9.8 Lint 健康检查

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-LINT-001 | 本地快速 Lint | P0 | 对可读 Markdown 检查适用的死链、孤立页面、缺失 frontmatter、索引漂移、空页面、重复文件名；没有 Wiki 索引根时索引规则为 N/A；project app state 不可写时在内存运行并标注“本次运行有效 / 不会持久化” |
| PRD-LINT-002 | Agent 深度 Lint | P0 | 项目已信任且 concrete route 可执行时，由内置固定 `wiki-lint` 识别六类语义 Finding；首期只允许通过同等 invocation/output 合同的 Claude/Codex，未支持或 forged/stale route 不广告且 invocation 为 0 |
| PRD-LINT-003 | 自动修复可处理问题 | P0 | 仅 trusted writable + clean Git 项目可批准 selected Finding batch；批准后先 checkpoint，Agent 只写 task-owned candidate 的授权 Wiki，backend checked apply；最多三轮 deterministic recheck，无 BYOK fallback |
| PRD-LINT-004 | 高风险修改需要确认 | P0 | 初次批准覆盖整个选中批次的安全 selected-path 更新/新建；删除、未授权既有路径覆盖或冲突进入持久二次确认并提供按需 Diff；raw/Source 越界不可确认 |

### 9.9 Git 版本与恢复

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-GIT-001 | 应用自动初始化 Git | P0 | 新项目创建后包含 Git 仓库 |
| PRD-GIT-002 | 危险操作前创建检查点 | P0 | 删除、覆盖、自动修复前有可回滚提交 |
| PRD-GIT-003 | 成功操作后提交最终结果 | P0 | 操作完成后历史中有结果提交 |
| PRD-GIT-004 | 冲突时展示 Markdown Diff | P0 | 用户可选择保留、覆盖或手动合并 |

### 9.10 HTML/卡片/报告

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-HTML-001 | 支持单篇美化阅读页 | P0 | 指定文章可生成 HTML |
| PRD-HTML-002 | 支持知识卡片 | P0 | 指定文章可生成摘要式卡片 |
| PRD-HTML-003 | 支持项目级 HTML 报告 | P0 | 可将整个 Wiki 导出为报告 |
| PRD-HTML-004 | 输出保存到项目布局定义的导出目录 | P0 | trusted writable 项目的导出文件在项目内可找到；原生项目使用 `exports/html/`；覆盖既有制品执行 checkpoint 与确认 |
| PRD-HTML-005 | HTML 模板只影响输出样式 | P1 | 模板不改变 Wiki schema 或 Lint 规则 |
| PRD-HTML-006 | Wiki 单篇快速导出与完整生成双入口 | P0 | Wiki 快速生成不离开文章，只展示三种单篇类型、立即显示普通任务、只创建新文件并在成功后按 taskId 就地预览；多页、项目报告、显式输出路径和覆盖进入 Workflows，且两条链路共享 ExportService、ExportRecord 与 Exports 管理 |

### 9.11 设置、隐私与更新

| ID | 需求 | 优先级 | 验收标准 |
|---|---|---|---|
| PRD-SET-001 | 支持 LLM Provider 配置 | P0 | 用户可配置 OpenAI、Anthropic、Google、Ollama、Custom |
| PRD-SET-002 | API Key 存系统钥匙串或凭据管理器 | P0 | 项目文件中不明文保存 API Key |
| PRD-SET-003 | 支持中英双语 | P1 | 用户可切换中文/English |
| PRD-SET-004 | 支持亮色/暗色主题 | P1 | 设置后 UI 主题切换生效 |
| PRD-SET-005 | 支持签名应用更新 | P1 | 无项目也可异步检查固定 HTTPS endpoint；展示版本、notes、签名来源与进度；下载可取消/重试，安装与重启必须显式确认并在未保存编辑、Import commit、确认或关键任务存在时阻止 |
| PRD-SET-006 | 支持签名 capability 安装与原任务继续 | P0 | release build 注入同一 tag/run 的 4×5 catalog；缺能力时可续传/安全重下、验证、health rollback，并继续同一个 Import session/item |
| PRD-SET-007 | Provider secret 绑定精确目的地 | P0 | credential 绑定 project/config/canonical origin；官方 origin、Custom 授权、loopback/private/DNS/redirect 策略在后端 fail closed |

## 10. 信息架构

应用主导航包含：

- Dashboard
- 文件树 / 文章浏览
- Chat 问答
- 知识图谱
- 工作流
- 导入
- Lint
- Exports
- 设置

侧边栏原“工作流”分组改名为“知识处理”，包含工作流、Import、Lint 和 Exports；工作流使用 Lucide `Workflow` 图标，不显示导航徽标。侧边栏底部现有 Agent 状态行保持不变。

右侧面板根据主内容切换：

- 文章页：元数据、引用来源、相关文章。
- Source 页：来源信息、忠实原稿、版本时间线、重新 OCR / ASR、换字幕和刷新来源；顶部只新增“AI 整理”。
- Import：当前来源、一个主操作、候选预览、目标路径、质量、折叠技术详情和日志。
- Chat：引用来源、保存到 Wiki。
- 工作流概览：当前项目任务、三个内置工作流和最近运行记录。
- 工作流任务：结构化阶段、范围、执行路线摘要、Git 状态、影响文件、确认、结果和可展开日志。
- Diff 页面：冲突详情、合并选项。
- HTML 预览：预览、导出位置、重新生成。

## 11. 非功能需求

### 11.1 性能

- 面向 200-500 篇 Wiki 页面保持流畅。
- 图谱缓存后秒级打开。
- 搜索、文章打开、文件树切换应接近即时响应。
- 长任务必须进入后台任务系统，不阻塞 UI。

### 11.2 可靠性

- 任何批量修改前必须有 Git 检查点。
- Agent 或 BYOK 更新 Wiki 失败不能破坏已有 Wiki，且不得静默切换执行路线。
- 工作流任务和历史按项目隔离；同一项目的工作流串行执行。
- 导入失败需标记具体文件和错误原因。
- 冲突、失败和自动重命名需记录在布局定义的 Import state（新建原生映射为 `.app/import-conflicts.json`）或任务日志中；项目 app state 不可写时不得旁路落盘。

### 11.3 安全与隐私

- 项目内容默认只保存在本地。
- API Key 使用系统钥匙串或凭据管理器。
- 用户第一次在工作流中使用远程 Provider 时，一次性说明所选内容将离开设备；之后把数据范围收纳在执行详情中。
- 应用不静默安装 Agent 或执行安装命令。
- Agent 执行继承 CLI 自身权限与沙箱机制。

### 11.4 兼容性

- 目录结构兼容 LLM Wiki、`nashsu/llm_wiki` 和 Obsidian。
- 格式类型（原生 / 兼容）、信任（受信任 / 尚未信任）、文件系统访问（可写 / 只读）与健康状态分别建模；`受限` 只是未信任能力集合的 UI 摘要，不得作为后端唯一授权值。
- 打开评估先只读快速扫描，再后台深扫；外部符号链接只展示不跟随、不索引、不写入，内部链接需做包含性和循环保护。
- 大小写或 Unicode 规范化冲突只报告，不自动重命名或改写链接。
- Markdown 文件可由外部编辑器修改。
- 路径处理必须跨平台，内部统一使用正斜杠。
- 必须安全处理 Unicode 和 CJK 文件名。

### 11.5 可维护性

- 项目状态使用 Markdown 和 JSON，不引入数据库。
- Skill 系统采用文件夹 + `SKILL.md` 约定。
- HTML 模板与项目模板职责分离。
- 本地规则与 Agent 判断类规则分层。

## 12. 成功指标

### 12.1 MVP 验收指标

- 小型多格式资料包可完成导入、预览、编译、图谱、问答、HTML 导出。
- 当前 `wiki/` 样本项目可打开，文件树、搜索、图谱可用。
- Agent CLI 和 BYOK API 两条路径均能完成基础编译。
- Lint 可发现并修复至少一类本地确定性问题和一类 Agent 深度问题。
- Git 检查点可用于回滚一次 Agent 自动修改。
- 后台任务关闭窗口后仍可继续，并在完成后发出系统通知。
- 工作流只在等待确认、完成或失败时发送系统通知，不对普通阶段进度连续通知。

### 12.2 用户体验指标

- 新用户从打开应用到读到第一篇已确认 Source，不需要理解项目目录、Git、Agent CLI、BYOK、编译或图谱依赖。
- 导入确认前，用户能看懂哪些文件解析成功、哪些失败。
- 用户能明确区分“打开已有知识库”和当前项目内的“导入”；普通资料文件夹不会被误改造成知识库。
- 兼容类型、信任、文件系统只读和修复健康状态分别可解释，并且每个阻塞状态只有一个明确下一步。
- 冲突发生时，用户能看到差异并做出选择。
- 用户进入工作流后 5 秒内能判断当前任务状态或建议的下一步。
- 启动任一内置工作流不超过三个主要操作步骤。
- 用户不查看原始日志也能理解任务当前阶段、进度和是否需要处理。
- 用户从 Wiki 发起单篇快速生成时不离开当前文章，任务立即可检查，成功后自动预览；复杂生成仍能从 Workflows 获得完整范围、队列、确认和历史。

## 13. 发布计划建议

### Phase 1：项目与导入闭环

- 完整 shell 下的无项目双入口工作台。
- 新建知识库后进入 Import。
- 原生 / 兼容知识库评估、受限打开、信任与修复确认。
- 普通资料文件夹“新建知识库并导入”，原目录保持不变。
- 多格式导入与解析预览。
- Git 自动初始化和基础检查点。

### Phase 2：Wiki 编译与阅读

- Agent/BYOK 编译。
- Wiki 文件树、阅读、WYSIWYG 编辑。
- 搜索、标签、类型、来源过滤。
- 人工修改合并保护。

### Phase 3：图谱与 Chat

- 页面级知识图谱。
- 图谱缓存、社区聚类、节点跳转。
- Chat 问答、引用来源、保存到 Wiki。

### Phase 4：Lint、HTML 与后台任务

- 本地快速 Lint。
- Agent 深度 Lint 与自动修复。
- HTML/卡片/项目报告生成。
- 后台任务、托盘和系统通知。

### Phase 5：打磨与发布

- 中英双语。
- 亮色/暗色主题。
- project-independent 的签名更新检查、下载、取消、安装保护与重启恢复。
- Windows x64、macOS arm64/x64、Linux x64 的同 tag/commit installer、updater、capability catalog、签名、SBOM、provenance 和真实 packaged install/upgrade/uninstall 验证。
- 小资料包与真实 `wiki/` 样本验收。

当前 Batch 6 已完成本地源码/集成/release fixture 与三平台 source CI，public beta 仍为 No-Go：capability trust key、updater/capability protected signing inputs、draft workflow、四平台真实安装升级与匿名 endpoint 尚无真实证据。初始版本明确不要求 Windows Authenticode 或 Apple Developer ID/notarization，但必须披露并实测 SmartScreen/Gatekeeper manual override；不得用本地 fixture、jsdom 或 `cargo check` 替代，详见 `docs/release/batch-6-acceptance-evidence.md`。

## 14. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| 多格式解析质量不稳定 | 导入体验变差，Wiki 质量下降 | 预览明确提示失败和缺失；低于质量门槛不得提交，成功项必须同时保留证据并形成可读 Source |
| 外部知识库识别或修复误判 | 用户目录被错误改造或能力被错误启用 | 两阶段只读评估；format、trust、filesystem access、health 分维度；未信任的 Obsidian / Markdown vault 首次受限，旧版 LLM Wiki / `nashsu` 进入兼容 Dashboard；所有兼容 / 修复写入先展示完整确认页 |
| Agent 输出覆盖人工修改 | 用户失去信任 | Git 检查点 + 基线检测 + Markdown Diff |
| 图谱大项目性能不足 | 核心惊喜时刻受损 | 首版限定 200-500 页面目标；布局缓存；后台构建 |
| BYOK 与 Agent 能力边界不清 | 用户困惑 | UI 明确标识当前执行路径和高级功能依赖 Agent |
| 自动修复误删内容 | 数据风险 | 高风险操作必须确认；修复前检查点；可回滚 |
| 项目结构对普通用户过重 | 首次体验复杂 | 默认隐藏内部细节，以 Dashboard 和导入流程承接 |

## 15. 待产品确认项

当前 PRD 已根据已确认决策编写。后续进入设计或研发前，建议再确认：

- 首批内置项目模板的具体 `purpose.md` / `schema.md` 内容。
- BYOK API 首批支持的 Provider 和默认模型列表。
- HTML/卡片/报告的首批视觉模板风格。
- Agent CLI 安装引导是否需要提供平台差异化命令。
- MVP 是否需要提供 Demo 项目或示例资料包。

## 16. 参考文档

- `SPEC.md`
- `../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`
- Karpathy LLM Wiki：Raw Sources -> Wiki -> Schema 模式。
- `nashsu/llm_wiki`：页面级图谱、Wiki 自动编译、桌面应用化参考。
- Open Design：本地 Agent CLI 检测、BYOK API、Skill 驱动流程。
