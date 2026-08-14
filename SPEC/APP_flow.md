# LLM Wiki Desktop 应用流程说明

> 导入、来源库、文件与媒体处理、OCR / ASR、登录态和 AI 整理的完整流程见 [`../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md)。本文件中的旧式 Import 概览已按该决策收敛；导入确认不得自动触发编译。
> 上一行中的“兼容迁移”只指旧 Import / Source 数据迁移：它只能从设置中的独立入口进入；正常 Import 工作台不挂载旧来源迁移提示、迁移对话框或旧来源操作入口。
> 工作流总览、准备页、项目隔离队列、任务状态、可观察流水线、确认和恢复的完整流程见 [`../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md)。本文件只描述跨功能应用流程；发生冲突时以前述设计为准。
> 无项目工作台、新建知识库、打开原生 / 兼容知识库、目录评估、信任 / 受限 / 只读、兼容启用、修复与进入 Import 的完整流程见 [`../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md)。普通资料文件夹不得原地初始化；发生冲突时以前述设计为准。

## 1. 文档目的

本文面向后续开发 Agent / Claude Code，用来说明 LLM Wiki Desktop 的应用流程、状态变化、文件读写边界和用户确认规则。

本文不是 UI 视觉稿，也不规定具体前端路由命名。实现时可以使用 React Router、内部视图状态或其他桌面应用导航方式，但必须保持本文描述的视图职责、数据流和安全边界。

主要依据：

- `PRD.md`
- `SPEC.md`
- 当前仓库中的真实样本库：`wiki/wiki/`

## 2. 核心心智模型

LLM Wiki Desktop 是一个本地优先的跨平台桌面知识库应用。它的基本单位不是云端账号，也不是数据库，而是一个本地项目文件夹。

项目文件夹遵循以下长期模型：

```text
Raw Sources
  -> Sources（可读来源）
     ├-> Wiki（独立编译的派生页面）
     ├-> Graph / Chat（Source-only 即可使用）
     └-> 与 Wiki 一起进入 Graph / Chat / HTML Reports
```

下列物理路径是**新建原生知识库**的默认映射。所有服务实际使用 assessment 返回的 `ProjectLayout` 逻辑根；兼容知识库保留原有 Markdown 布局，缺少某个写入根时返回 typed prerequisite，不得自行补建原生目录。原生映射含义如下：

- `raw/sources/` 保存原始资料，默认不可变。
- `raw/extracted/` 仅兼容读取旧项目数据；新 staging、预览和候选不得写入该目录。
- `wiki/sources/` 保存导入确认后的忠实 Source Markdown。
- `wiki/` 其他目录保存 LLM / Agent 独立编译后的结构化知识页面。
- `.app/` 保存应用状态、任务、缓存、聊天记录、设置等 JSON 数据。
- `exports/html/` 保存 HTML、知识卡片和项目报告输出。
- Git 用于检查点、恢复和冲突保护，普通用户不需要理解 Git。

首次价值在 `Sources` 这一层完成：用户确认导入后能立即打开一篇可读 Source。Wiki 编译、Graph 和 Chat 是后续能力，不是首次成功的前置条件。

实现时不要把知识库内容迁入数据库。项目内容必须保持为 Markdown + JSON + 本地文件。

## 3. 全局应用结构

应用主界面包含以下稳定视图：

- Dashboard
- 文件树 / 文章浏览
- Chat 问答
- 知识图谱
- 工作流
- 导入
- Lint
- Exports
- 设置

顶部区域显示当前项目、全局搜索、语言切换和设置入口。底部状态栏显示当前 Agent、操作状态、后台任务和文章数量。右侧面板根据主视图切换，用于展示元数据、引用来源、工作流上下文、相关文章、Diff 确认或 HTML 预览。

即使当前没有项目，这个完整 shell 也保持挂载：左侧导航、顶栏、中心工作面、右侧上下文面板和底部状态栏都存在。无项目中心工作区只显示“新建知识库”和“打开已有知识库”两个紧凑入口；它不是独立启动页。导航仍可见，不可用模块显示一个原因和一个上下文动作；设置始终可用，无作用域的搜索禁用；右栏不展示 Agent / BYOK 配置。

实现不要提前假设 URL 路由名称。文档中的视图名称是产品职责，不是强制路径。

### 3.1 当前已实现的跨视图编排

本节记录已经落地的 orchestration 事实，不替代下文的产品流程、安全确认、持久化或审计要求：

- 导入确认链路：`ImportSession -> SourceCandidate -> commit source -> wikiStore.scan sources`。确认成功后只刷新来源库；编译由用户在完成摘要或历史中另行启动，不属于导入确认链路。
- 任务启动链路：`Task launch -> backend task -> taskStore upsert -> current-project drawer open`。后端任务事件可以进入统一内存状态，但所有列表、抽屉、确认和历史必须按当前项目筛选；仅当请求所属项目仍是当前项目时，才自动打开并选中任务抽屉。
- 高风险确认链路：`PendingAction -> ProjectConfirmationController -> backend revalidation/checkpoint -> result task/update`。`ProjectConfirmationController` 统一承接项目 PendingAction 和编译冲突；后端在真正执行前重新验证项目、目标状态和既有执行计划，并按操作要求创建或确认 Git checkpoint，随后返回任务或项目/文件状态更新。
- 项目切换链路：`Project switch -> project key/epoch invalidation -> stale UI commits and toasts suppressed`。异步 workflow 在提交视图状态、打开抽屉、切换视图或发送 toast 前校验项目 key / epoch，旧项目结果不得覆盖新项目 UI。

项目切换只抑制过期的 UI commit、抽屉自动打开和 toast，不丢弃后台任务。项目应用状态可写时，其他项目的任务继续持久化在各自布局定义的任务根目录（原生映射为 `.app/tasks/`）；受限或只读项目允许的本地只读任务仅保存在运行内存。当前项目的界面不得跨项目展示或操作其他项目任务；用户切换到对应项目后才可查看、确认或取消仍可恢复的任务。

截至 2026-07-30，无项目态仍由独立 `ProjectStartView` 分支承接，项目打开仍使用二元识别并保留普通文件夹原地初始化 continuation。这些只是真实实现差距，不是目标流程；迁移后必须由持续挂载的 shell、typed assessment、access policy、全局 trust 和 repair plan 取代。

## 4. 启动与知识库选择流程

### 4.1 正常启动

1. 应用启动。
2. 读取全局应用设置和最近知识库列表。
3. 只校验最近打开时间最新的一项；不静默回退到更旧知识库。
4. 最新知识库有效时自动打开并固定落 Dashboard；不恢复上次知识库内的功能路由。
5. 没有历史，或最新知识库路径缺失 / 不可访问时，进入完整 shell 下的无项目工作台；后者同时展示简洁路径错误。
6. 只有进入项目后才按项目恢复可展示任务并检测适用的 Agent / Provider 能力；无项目首屏不展示或要求 Agent / BYOK 配置。

### 4.2 关键读写

全局设置的位置可以由应用自身决定，但不要写入知识库内容目录中的用户知识文件。最近父级保存位置、最近知识库、歧义 Markdown 打开偏好和目录信任记录都属于全局应用状态；信任使用 canonical 目录身份及防替换信息，不使用知识库内 marker。项目级应用状态写入当前 `ProjectLayout.appStateRoot`；新建原生知识库映射为 `.app/`。布局没有可写应用状态根目录时，允许的只读结果只保存在运行内存。

新建原生知识库的常见状态文件映射：

- `.app/settings.json`
- `.app/agent-config.json`
- `.app/tasks/`
- `.app/graph-cache.json`
- `.app/bookmarks.json`
- `.app/chats/{id}.json`

### 4.3 失败处理

- 最新知识库路径不存在或不可访问时，将其标记为失效，保留完整 shell 并显示无项目工作台与简洁路径错误；不自动尝试更旧知识库。
- 项目结构不完整时，先以恢复工作台展示格式、健康、权限、Git 和修复计划；自动计算或重建内存 / 临时派生状态不等于授权写盘。
- 任何兼容启用或修复写入前都显示完整确认页；用户可以取消并选择受限或只读打开。
- Agent CLI 检测失败不阻塞应用启动。

## 5. 新建知识库流程

### 5.1 用户路径

1. 用户在无项目工作台选择“新建知识库”。
2. 输入知识库名称，选择父级保存位置和创建时模板。
3. 默认父级位置是系统 Documents 下的 `LLM Wiki`；创建成功后记住最近一次父级位置。
4. 应用以知识库名称生成父目录下的最终子目录，并实时校验非法字符、保留名、空白、Unicode / CJK、大小写与规范化冲突。
5. 最终目录已存在且非空时阻止创建；不得借“新建”覆盖或接管已有目录。
6. 应用创建统一项目结构，根据模板生成 `purpose.md` 和 `schema.md`，初始化本地 Git 并创建初始提交。
7. 创建成功后进入当前项目的 Import 工作台，但不自动打开系统文件选择器。

### 5.2 必须创建的结构

```text
project-root/
├── purpose.md
├── schema.md
├── raw/
│   ├── sources/
│   ├── extracted/
│   └── assets/
├── wiki/
│   ├── index.md
│   ├── log.md
│   └── overview.md
├── exports/
│   └── html/
├── skills/
└── .app/
```

项目模板显示通用、研究、阅读、商业、个人成长，默认选择“通用”。模板只影响 `purpose.md` 和 `schema.md` 的初始内容，不改变核心目录结构；创建后不提供模板切换。

### 5.3 实现注意点

- 新建知识库必须可以被 Obsidian、Git 和外部 Markdown 编辑器直接访问。
- 初始化失败时要回滚未完成的目录创建，或清晰标记失败状态。
- 创建失败时保留用户已输入的名称、父级位置和模板，便于修正后重试。
- 不要在项目文件中明文保存 API Key。

## 6. 打开已有知识库

### 6.1 用户路径

1. 用户在无项目工作台选择“打开已有知识库”并选择目录。
2. 后端执行无写入快速评估，返回 typed assessment；前端不根据单个 marker 自行猜测。
3. 健康的当前原生项目直接打开并进入 Dashboard。
4. 健康的旧版原生与 `nashsu/llm_wiki` 进入兼容 Dashboard；未信任的 Obsidian 或可识别 Markdown vault 进入受限兼容 Dashboard；健康且已信任的兼容知识库直接进入 Dashboard。确定安全路由后启动可取消的后台深度扫描。
5. 歧义 Markdown 目录要求用户选择“以 Markdown 知识库打开”或“用这些资料新建知识库”；偏好记入全局设置，不写目录 marker。
6. 普通资料目录只提供“用这些资料新建知识库”，回到新建流程并预填该目录为待导入来源。
7. 损坏或不完整项目展示恢复工作台；用户确认后可“信任、修复并打开”，也可“暂不修复，以受限模式打开”。

### 6.2 快速评估与深度扫描

- 快速评估只读取浅层目录、关键 marker、Markdown 数量、Git 元数据和基本权限，必须足够快，且不创建 `.app/`、缓存或 Git。
- 启动快速评估只返回 application-scoped `assessmentOperationId`；取消命令只接受该 opaque ID。取消不创建项目任务，丢弃未完成快照并保持无项目工作台；完成后返回独立、短期有效的 `assessmentId`，供打开、信任、兼容启用或修复命令重验。
- 格式分类至少覆盖：当前原生、旧版原生、`nashsu/llm_wiki`、Obsidian、Markdown vault、歧义 Markdown、普通资料和未知；损坏 / 不完整由独立 health 维度表达为 repairable、recovery 或 unreadable。
- 深度扫描在工作台内后台运行，展示阶段、进度和取消；扫描失败不会把已可读项目踢回无项目页。
- 格式类型、trust、filesystem access、health、layout、Git 状态和建议动作分别返回，不压缩成 `is_wiki_project`、单一 permission 或 `compatible: boolean`。

### 6.3 受限、信任与只读

- 受限模式允许读取 Markdown、目录树、本地关键词搜索和内存图谱。
- 受限模式禁用 Agent、Skill、外部 AI、写入型工作流、项目写入、兼容写入和自动修复；后台只读盘点与本地快速健康检查仍可运行，并以 non-persistent 内存结果呈现。
- 信任记录保存在全局应用配置，并绑定 canonical 路径及目录身份；路径移动、目标被替换或身份不匹配时重新询问。
- 文件系统不可写时可以永久保持只读：阅读、搜索和无需落盘的内存图谱仍可用，写操作明确禁用。
- 项目根可以是符号链接，但先 canonicalize；根内链接只在 canonical 目标仍被包含且无循环时读取，指向根外的链接只展示，不跟随、不索引、不写入。

### 6.4 兼容启用、Git 与修复

- 完整功能启用的写入范围是 `.app/` 和 `.app/compat/{purpose.md,schema.md}`；根目录同名文件始终按用户内容处理，不覆盖，也不新增 `.app/project.json`。
- 无 Git 的兼容目录在信任页默认勾选“初始化本地 Git”。用户拒绝时，阅读、搜索和已显式授权的 Chat 仍可用，但高风险自动写入保持禁用。
- 已有脏 Git 不自动提交或 stash；需要高风险写入时，要求用户自行处理，或明确确认把当前全部变更作为检查点。
- 自动修复只可先准备安全派生状态与修复计划。确认页必须列出判断、写入路径、Git 条件、可恢复性和失败回退；只有确认后才写盘。
- 大小写或 Unicode 规范化冲突只报告，不自动重命名文件或改写链接。
- 普通资料目录绝不在原地初始化、移动或整理；“用这些资料新建知识库”只在新项目中归档副本并保留原件。

## 7. 导入到当前项目流程

### 7.1 用户路径

1. 用户点击“导入”，或在新建知识库完成后自动到达 Import 工作台；自动到达不自动弹出系统文件选择器。
2. 工作台把文件、文件夹、链接、粘贴文本作为四个一等入口；拖拽文件 / 文件夹是快捷方式。知识库识别与打开不属于 Import。
3. 应用创建或恢复当前项目的活动导入会话，并自动执行安全扫描、类型识别、轻量抓取、确定性提取、字幕发现和质量检查。
4. 缺少必要正文时，任务聚合显示需要的登录、OCR、ASR、能力安装或 Agent 修复操作。
5. 应用展示最终 Source Markdown 预览、资源状态、目标路径和质量信息；来源更新展示 Diff。
6. 所有可提交项默认勾选，用户点击“导入到来源库”。
7. 后端写入布局定义的不可变 evidence、来源版本状态和当前 Source 页面；新建原生知识库分别映射为 `raw/`、`.app/sources/` 与 `wiki/sources/`。
8. 完成摘要停留在 Import，并选中 / 预览已导入 Source；同时提供“查看已导入来源”和“用这些来源更新 Wiki”，只有后一个操作进入独立编译流程。

文件 / 文件夹扫描若超过后端定义的总文件数、总字节数或预计输出数软阈值，流程停在“确认扫描总量”：session item 数保持不变，状态区同时展示 aggregate totals、原因、跳过项和每个超大表格的独立估算。总量确认先放行普通文件，超大表格仍停留等待独立确认；两次确认都消费同一 saved scan，并在 trusted + writable authority 临界区重验当前 layout import-state root、项目 / session / task identity、token、totals 和全部来源 fingerprint，不重扫原目录。hard file limit 直接 typed 拒绝且不产生可接受的截断扫描；取消只丢弃 app-state scan result，不触碰来源。

### 7.2 候选预览必须包含

- 来源名称、类型和定位信息
- 用户可读状态、阶段和真实进度
- 最终 Source Markdown 快速预览与完整预览
- 本地化资源状态
- layout-defined Source 目标路径（原生映射为 `wiki/sources/`）
- 页数、字数、时长或其他可用元数据
- 质量警告和可定位的问题区间
- 更新项的新旧 Diff
- 折叠的技术错误和日志

### 7.3 导入层边界

导入层负责把输入变成经过确认的可阅读 Source；下列路径是新建原生知识库映射：

- `raw/` 无损保留不可变原文件、页面证据、图片、字幕、OCR / ASR 原始输出和版本证据。
- `wiki/sources/` 保存忠实、规范化、可阅读和可编辑的当前 Source。
- `.app/` 保存会话、来源身份、版本、别名、编辑基线、质量报告和处理尝试。
- 完全重复项不创建新 Source；更新同一来源时使用版本、Diff 和三方合并。
- 失败项不创建占位 Markdown。

OCR 和 ASR 在导入阶段按正文缺口启用，并且必须由用户主动授权。BYOK 不参与解析恢复；本地 Agent 只能在用户主动触发后生成 staging 候选。图片视觉理解不在首版范围。

### 7.4 会话与待办

- 每个项目同一时间只有一个活动导入会话。
- 已提交项进入完成摘要与历史，未解决项继续留在会话。
- 页面切换和最小化不停止后台任务。
- 应用重启后耗时下载、OCR、ASR 显示“已暂停，可继续”，由用户恢复。
- 批次状态区聚合登录、OCR、ASR 和能力安装待办，不连续弹出模态框。
- 未解决项只阻断自身，其他可确认项可以部分提交。
- 一次批量处理在任务抽屉中只显示一个 operation task；逐项状态由 Import session/item 事实呈现，不能用 operation terminal 状态替代 waiting、preview、失败、跳过或取消项。
- `import://session-patch` 先通过 project/root/session/epoch/authority guards，再一次性 patch 当前 cohort；terminal patch 或 terminal task 竞态最终只触发一次 session summary refresh。

### 7.5 导入完成

完成摘要显示已导入、已更新、重复和仍待处理数量。

- “查看已导入来源”打开现有 Wiki 阅读器中的 Source。
- “用这些来源更新 Wiki”携带本次成功版本的 `sourceId + versionId` change set，进入第 8 节的独立编译流程。
- 编译不得写入任何 layout-defined Source root（原生为 `wiki/sources/`）。

## 8. 更新 Wiki（编译）流程

### 8.1 默认策略

执行路径默认值来自设置。Agent CLI 与 BYOK API 都可以承担更新 Wiki；用户可以在准备页的高级设置中覆盖本次路径，但不能借此修改全局默认值。所选路径不可用时必须引导配置，不得静默切换到另一条路径。

### 8.2 用户路径

1. 用户在导入完成摘要点击“用这些来源更新 Wiki”、从工作流总览选择“更新 Wiki”，或从历史重试。
2. 应用打开统一准备页。默认自动选择相对上次成功基线发生变化的 Source；完整重编译只放在高级设置。
3. 首次运行要求用户确认范围；后续相同上下文可以快速重跑。项目、工作流、范围和基线完全相同时打开既有任务，不重复创建。
4. 应用把任务加入当前项目的串行工作流队列；任务开始后在流水线第二阶段创建 Git 检查点。
5. 编译器读取：
   - 项目布局解析出的 purpose 上下文（原生为根 `purpose.md`，兼容为 `.app/compat/purpose.md` 或只读推断）
   - 项目布局解析出的 schema 上下文（原生为根 `schema.md`，兼容为 `.app/compat/schema.md` 或只读推断）
   - 用户选择的 layout-defined Source 版本及其 `sourceId + versionId` change set（原生映射为 `wiki/sources/`）
   - 布局定义的现有 Wiki 页面根目录
6. 主内容区按“分析来源变化 → 创建 Git 检查点 → 规划 Wiki 更新 → 生成页面候选 → 校验链接与结构 → 检查冲突与风险 → 应用文件变更 → 刷新索引与图谱 → 完成并记录结果”的阶段展示进度、当前处理项和数量；原始日志折叠为只读次级信息。
7. 生成或更新布局定义的 Wiki 页面、索引、概览和活动日志；新建原生知识库分别映射到 Wiki 页面根、`wiki/index.md`、`wiki/overview.md` 与 `wiki/log.md`。
8. 写入前重新检查人工编辑和基线变化。
9. 低风险、无冲突修改在检查点后自动应用；删除、覆盖、广泛重写或冲突修改进入非模态“等待确认”状态，并提供影响摘要和按需 Diff。
10. 成功后提交 Git 结果并刷新 UI、搜索和图谱缓存。
11. 编译器不得写入或删除任何 layout-defined Source root（原生映射为 `wiki/sources/`）。

### 8.3 冲突处理

编译前必须记录基线版本。编译后如果发现目标文件已有外部修改，不能直接覆盖。

冲突时用户至少需要能选择：

- 保留当前版本。
- 使用 Agent / BYOK 生成版本。
- 手动合并。

### 8.4 实现注意点

- 编译失败不能破坏已有 Wiki。
- 批量覆盖、删除、重写必须有检查点。
- 用户在任务运行时可以继续编辑 Markdown；应用必须在最终写入前重新核对并进行三方合并。
- `raw/` 默认不可变。来源更新写入新版本并保护当前 Source 的人工编辑。
- 永久删除来源必须进入专用二次确认页，并按整个来源包处理；派生 Wiki 页面不自动删除。

## 9. 文章阅读与编辑流程

### 9.1 阅读

1. 用户进入文件树 / 文章浏览视图。
2. 应用扫描 `ProjectContext.layout.pageRoots`；新建原生知识库对应 `wiki/`，兼容 vault 保留其 Markdown 页面根。
3. 文件树按目录展示页面。
4. 用户选择页面。
5. 应用渲染 Markdown。
6. 右侧面板展示元数据、引用来源和相关文章。

Markdown 渲染必须支持：

- GFM 表格
- 代码高亮
- 数学公式
- `[[wikilinks]]`
- YAML frontmatter 展示

### 9.2 编辑

1. 用户点击编辑。
2. 应用进入 WYSIWYG 编辑模式。
3. 用户保存。
4. 应用写回 Markdown 文件。
5. 刷新索引、搜索缓存和图谱缓存。
6. 必要时记录到布局定义的活动日志（原生映射为 `wiki/log.md`）。

### 9.3 首版不要求

- wikilink 自动补全。
- frontmatter 可视化编辑。
- 块引用面板。
- 图谱拖线编辑。
- 独立反向链接面板。

### 9.4 Source 专属流程

- Sources 仍位于现有 Wiki 文件树和阅读器中，不增加顶层应用。
- 顶部工具栏只新增 `AI 整理`。
- 忠实原稿、来源信息、版本时间线、重新 OCR / ASR、换字幕和刷新来源位于可开关右侧面板。
- AI 整理在可拖动、可调整尺寸、可最小化的非模态浮动工作台中运行；工作台固定绑定启动时的项目、Source 和任务，切页不改绑，切项目时隐藏；关闭工作台不取消任务，显式取消需二次确认。
- AI 整理生成候选稿，并在标题后生成或替换唯一的 `## 内容概览`；完成后默认显示只读最终稿，Diff 与过程按需查看。
- 候选必须经过用户明确确认和必要的 Git 检查点后更新当前 Source；Diff 始终可用但不强制先查看。
- Source 在生成期间变化时，使用 `sourceId + versionId + Markdown hash` 重新 Diff 或三方合并。

## 10. 知识图谱流程

### 10.1 构建

1. 应用读取当前权限允许的可读 Markdown；原生项目优先使用 `wiki/`，兼容 vault 使用其 Markdown 页面。
2. 解析 frontmatter、目录类型和 `[[wikilinks]]`。
3. 构建页面级节点。
4. 通过 wikilinks 和多信号关联度生成边。
5. 执行布局和社区检测。
6. 只有 trusted + writable 且布局提供 `graphCachePath` 的项目才写入缓存（原生映射为 `.app/graph-cache.json`）；受限 / 只读项目只保留内存结果。

### 10.2 展示与交互

知识图谱必须支持：

- 页面级节点。
- 页面类型着色。
- 社区着色。
- Louvain 社区检测。
- ForceAtlas2 力导向布局。
- 悬停高亮相邻节点。
- 点击节点进入文章。
- 缩放、拖拽、Fit-to-screen。
- 布局缓存后秒级打开。

首版边统一表示“相关”，不展示复杂关系类型和关系证据。

### 10.3 性能目标

当前样本库 `wiki/wiki/` 有数百个 Markdown 文件，首版目标应覆盖约 200-500 篇 Wiki 页面。图谱首次构建可以进入后台任务，但必须展示进度并允许取消。

兼容知识库深度扫描未完成时，图谱展示已发现页面形成的 partial 结果和扫描状态，不以空画布暗示知识库没有内容。

## 11. Chat 问答流程

### 11.1 用户路径

1. 用户进入 Chat 视图。
2. 创建或选择一个会话。
3. 输入自然语言问题。
4. 应用先搜索当前可读 Source 或 Wiki 页面。
5. 应用组装上下文：
   - 相关页面
   - 引用信息
   - 聊天历史
   - 项目布局解析出的 purpose 上下文
6. Agent CLI 或 BYOK API 生成回答。
7. UI 展示回答和引用来源。
8. 用户可将优质回答保存到项目布局定义的 queries root；原生知识库映射为 `wiki/queries/`。

外部兼容知识库未信任时不得把内容发送给 Agent 或远程 Provider。已信任且有可读 Source / Wiki、但尚未编译时，Chat 仍可工作；若缺少 AI 执行路径，“去配置”打开设置覆盖层，完成后返回原处，但不自动发送原问题。

### 11.2 存储

- `ProjectLayout.chatStateRoot` 可写时，聊天会话存储在该根目录（原生为 `.app/chats/{id}.json`）；文件系统只读或该路径缺失时，会话只在当前运行内存中存在，并明确提示不会持久化。
- 保存到 Wiki 的回答存储为 `ProjectContext.layout` 定义的 queries root 下的 Markdown 页面；原生知识库使用 `wiki/queries/`。

### 11.3 边界

普通全局搜索只做关键词、标签、类型和来源过滤，不自动调用模型。自然语言问答必须从 Chat 入口触发；Agent / BYOK 只是明确的执行路径。

## 12. Lint 与修复流程

### 12.1 本地快速 Lint

应用本地执行确定性检查：

- 死链。
- 孤立页面。
- 缺失 frontmatter。
- layout-defined index 与实际页面不一致。
- 空页面。
- 重复文件名。
- 路径大小写问题。
- 缺失资源文件。

### 12.2 Agent 深度 Lint

项目已信任且存在真实可执行的 concrete route 时，需要判断的问题可交给应用内置、版本固定的 `wiki-lint` Skill。项目同名 Skill 不读取；purpose/schema/layout context 仅作为显式不可信输入，不能覆盖 Skill id/version/hash、operation、write roots、schema 或三轮上限：

- 重复主题。
- 弱交叉引用。
- 来源缺失。
- 页面结构不符合 layout-resolved schema。
- 内容过期。
- 跨页面矛盾。

### 12.3 修复路径

1. 用户从工作流运行“健康检查”或在 Lint 页面运行检查。
2. 应用对当前可读 Markdown 执行本地快速 Lint；当项目 app state 不可写时（包括未信任受限与 trusted read-only），只在内存运行、不写报告或缓存，并明确标注“本次运行有效 / 不会持久化”。
3. 项目已信任且有适用 AI 路径时，首次默认完整检查，否则默认本地快速检查；后续记住该项目最近模式。完整检查在本地规则后执行深度 Lint 并合并重复发现。
4. 健康检查只读，问题列表与修复建议由现有 Lint 页面承接。
5. trusted writable + clean Git 项目中的用户选择一批 eligible Finding，并一次批准整个 Agent 修复批次；restricted/read-only/dirty/no-Git 只显示 backend prerequisite，修复不回退 BYOK。
6. 批批准成功后，queued dispatch 在第一次 Agent repair invocation 前创建 clean-HEAD Git 检查点；失败时 Agent invocation 与候选/真实项目 mutation 都为 0。
7. Agent 在 task-owned candidate workspace 中只修改 backend 授权的 Wiki Markdown；不复制项目 Skill 或 raw originals，`raw/**`、忠实 Source、`wiki/sources/**` 与 layout-defined Source roots 始终只读。后端复用既有 manifest/hash/checked apply 验证并应用候选。
8. safe selected-path 更新和安全新建由初次批准覆盖；删除、未授权既有路径覆盖、baseline/外部编辑冲突进入持久二次确认并提供 lazy Diff。Source/raw/越界/link 逃逸候选直接失败，不提供确认继续。
9. 每轮应用后执行 deterministic Lint 并按稳定 Finding identity 关联结果；未解决且 round < 3 才进入下一轮。三轮后仍未解决返回 partial/manual-review，保留 Diff、checkpoint/final commit 与 Git rollback；第四轮 invocation 为 0。
10. 修复作为 Lint 发起的隐藏 workflow operation 复用现有项目串行队列、TaskService、history/cancel/recovery；Overview 仍固定三行。成功或人工处理终态后提交验证结果并刷新 UI。

当前实现状态（2026-08-13）：H5 已提供当前 persistent Agent Health report 的 eligible Finding 选择、一次批准和 linked task/result/rollback UI；语义 Finding 没有 deterministic proof 时保持 manual-review。H6 最终 gate 尚未全绿，因此不把 Gate H 或 Batch 7 标记为已解除。

### 12.4 高风险操作

以下操作必须确认：

- 删除页面（Agent repair 为二次确认）。
- 覆盖页面（Agent repair 仅未授权既有路径覆盖为二次确认）。
- 永久删除整个来源包。
- 以新版本替换或合并当前来源。
- 批量重写（Agent repair 已由初次 selected-batch 批批准覆盖；其他产品操作按各自合同确认）。
- 冲突合并（Agent repair 为二次确认）。

## 13. HTML / 卡片 / 报告导出流程

### 13.1 输出类型

首版支持：

- 单篇美化阅读页。
- 知识卡片。
- 思维导图或概念关系图。
- 项目级 HTML 报告。

### 13.2 Wiki 单篇快速导出

1. 用户在 Wiki 阅读页点击“生成 HTML”、无已有 HTML 时点击预览，或使用右侧“生成 HTML 阅读页 / 生成知识卡片”快捷动作。
2. 应用保持在当前 Wiki 页面并打开 `GenerateHtmlDialog`；弹窗只提供美化阅读页、知识卡片和概念图，输入固定为当前一篇 Wiki 页面。
3. 用户确认后，应用沿用直接导出默认路线，创建一个普通、可取消的 Export 后台任务；任务立即进入全局任务抽屉，可查看状态与日志。
4. 快速导出只以 create-new 方式写入新的 layout-defined HTML 文件和带 `taskId` 的 ExportRecord，不选择输出路径、不覆盖既有制品，也不进入项目 Workflow 串行队列或 history。
5. 任务成功后，Wiki 按本次 `taskId` 精确刷新并定位 ExportRecord，在当前页面自动打开内嵌 HTML 预览；制品同时可在 Exports 中继续管理。
6. Wiki 预览中的“重新生成”沿用相同直接链路并创建新文件；需要覆盖指定输出时改走 Exports / Workflows 完整路径。

### 13.3 Workflows Generate Content

1. 用户从 Workflows 总览、Exports 的“新建导出 / 重新生成”或其他项目级入口选择“生成内容”。
2. 应用打开完整主区准备页，选择内建输出类型及其适用范围；单篇阅读页要求恰好一页，知识卡片和概念图可使用多页，项目报告使用项目范围。首版不支持自定义模板或自定义运行指令。
3. 后端确认项目已信任且可写，解析默认或单次覆盖的 Agent/BYOK 路线和 `skills/html-*`，在当前项目的串行 Workflow 队列中创建任务。
4. 任务详情展示九阶段结构化进度、取消、恢复说明、关联重试和历史；复杂范围、显式输出路径、项目报告均只由此路径承担。
5. 新建制品不要求 checkpoint；覆盖既有制品前创建 checkpoint 并等待确认。
6. 生成完成后，Workflows 展示结果摘要与领域跳转，制品进入现有 Exports 页面管理和预览。

### 13.4 共享能力与边界

两条链路共享 `ExportService`、`ExportRecord` 持久化和 Exports 结果管理，但任务编排不同：Wiki 使用普通 Export task，Workflows 使用项目级 Workflow queue/history。HTML 模板只影响输出样式，不影响 Wiki schema、Lint 规则或 Agent 行为。不要恢复通用“运行 Agent”弹窗，也不要在 Exports 恢复大型 `ExportDialog`。

## 14. 工作流流程

工作流视图负责统一启动和观察产品定义的任务，不负责配置 Agent CLI 或 Provider。首版只有“更新 Wiki”“健康检查”“生成内容”三个内建工作流；不提供来源批量整理、用户自定义工作流、定时触发、自定义运行指令或自定义输出模板。

### 14.1 总览与准备

- 总览使用紧凑行列表。运行中、等待确认和失败任务优先；没有需关注任务时展示三个可用工作流和至多一个推荐下一步。
- 工作流顺序固定，推荐状态不改变排序，也不自动启动任务。
- 点击工作流进入完整主区准备页；不使用“运行 Agent”弹窗。
- 项目不完整或执行路径未配置时仍可点击，在准备页解释缺失条件并提供设置入口。
- 外部项目还未信任时，本地快速健康检查仍可运行；完整检查、外部 AI 和写入工作流提供“信任知识库”。只读或无 checkpoint 能力时，只读工作流可以运行，任何不满足条件的写入工作流明确禁用并说明原因。
- 执行路径是次级信息：默认来自设置，可在本次运行的高级选项中覆盖；不可用时不得静默回退。
- 第一次使用远程 Provider 时，一次性说明所选内容将离开设备；常规数据范围收纳在折叠的执行详情中。

### 14.2 队列与任务详情

- 工作流、任务、确认和历史全部按项目隔离；当前视图不展示其他项目任务。
- 每个项目同一时间只运行一个工作流，其余进入串行队列。
- 项目、工作流、输入范围和基线相同的重复请求打开既有任务。
- 用户可见状态为：已排队、运行中、等待确认、已完成、失败、已取消、已中断。
- 任务详情以垂直阶段流水线为主，显示当前阶段、当前处理项、数量进度、活动记录和下一安全动作；stdout/stderr 只在折叠日志中只读展示。
- Update Wiki、Health Check 和 Generate Content 的阶段定义以工作流设计规范 §11 为准。

### 14.3 结果、安全与恢复

- 健康检查只读；结果和后续选中批修复继续由现有 Lint 页面管理。修复 operation 可出现在 task/history/detail 中，但不成为第四个 Overview workflow。
- 生成内容的制品继续由现有 Exports 页面管理。
- Update Wiki 与修复中的安全 selected-path、无冲突变更在所需 Git 检查点后按初次批批准应用；Generate Content 新建制品不要求检查点，覆盖既有制品需先建检查点并等待确认。修复删除、未授权既有路径覆盖或冲突异步等待二次确认；raw/Source 越界永不进入可确认状态。
- 取消保留审计记录，不把未确认的部分结果写入正式 Wiki 或 Exports 路径。
- 重试创建关联的新任务，不覆盖失败记录。
- 应用异常退出后，运行中任务标记为“已中断”，并解释哪些步骤可复用；不得伪装从进程中间继续。只有布局提供可写 task state root 时，等待确认与排队记录才持久化，排队任务重开后需用户明确继续；restricted/read-only 的 ephemeral 只读任务不承诺跨重启恢复。

Agent CLI、BYOK、模型、Provider、默认执行路径和安装引导保留在设置页。应用不能静默安装 Agent、执行安装命令或切换执行路径。

## 15. 设置流程

设置视图至少包含：

- LLM Provider：OpenAI、Anthropic、Google、Ollama、Custom。
- API Key 管理。
- Agent 配置。
- 语言：中文 / English。
- 上下文窗口：4K 到 1M tokens。
- 外观：亮色 / 暗色。
- 启动行为说明：最新知识库条目有效时自动打开并落 Dashboard；无历史时显示无项目工作台；最新路径不可访问时显示该工作台与路径错误，不尝试更旧项目。
- 后台任务关闭窗口行为。
- 更新检查。

API Key 必须存系统钥匙串或凭据管理器，不能明文写入项目文件。

## 16. 后台任务与通知

长任务必须进入后台任务系统，不能阻塞 UI。

后台任务包括：

- 导入解析。
- 更新 Wiki。
- 图谱首次构建。
- 健康检查。
- 生成内容。

关闭主窗口时默认最小化到系统托盘，任务继续运行。用户可以在设置中改为关闭时询问或终止任务。

页面切换、知识库内导航和窗口最小化不停止导入任务。应用真正退出或进程中断后，下载、OCR 和 ASR 保存阶段与已完成分片；再次打开知识库时显示“已暂停，可继续”，由用户明确恢复，不自动启动耗时工作。用户主动取消才清理临时媒体和中间分片。

工作流任务采用独立恢复语义：项目 app state 可写时，等待确认与排队记录写入 `ProjectLayout.taskStateRoot`，排队任务在重开后等待用户明确继续；退出时仍在运行的持久任务重开后标记为“已中断”，展示已完成阶段和可复用产物，由用户显式重试并创建关联的新任务。restricted/read-only 允许的只读任务使用 non-persistent 内存/临时状态。

系统通知用于：

- 任务完成。
- 任务失败。
- 需要用户确认。

点击通知应打开结果页、错误日志或 Diff 确认页。

## 17. 新建原生知识库的关键状态映射与写入时机

下表是 `ProjectLayout` 对新建原生知识库的默认物理映射，不是兼容知识库的固定目录合同。兼容知识库只使用评估返回的逻辑根；缺少所需写入根时返回 typed prerequisite，不得由 service 自行创建原生目录。受限/只读模式允许的本地检查和会话保持为明确标记的内存状态。

| 文件 | 写入时机 | 说明 |
|---|---|---|
| `purpose.md` | 新建原生项目、创建时模板初始化 | 原生知识库目标、关键问题、研究方向；兼容库不自动创建根级文件 |
| `schema.md` | 新建原生项目、创建时模板初始化 | 原生 Wiki 结构规则、页面类型定义；兼容库不自动创建根级文件 |
| `wiki/index.md` | Wiki 编译、索引刷新 | 内容目录和 LLM 导航入口 |
| `wiki/overview.md` | Wiki 编译 | 全局摘要 |
| `wiki/log.md` | 编译、修复、重要操作 | 操作历史记录 |
| `wiki/sources/` | 确认导入、来源更新、AI 整理确认或版本恢复 | 当前可阅读、可编辑的来源库 |
| `.app/import/` | 导入会话、任务、处理尝试和待办变化 | 可恢复的 Import V2 状态 |
| `.app/sources/` | 来源提交、更新、编辑基线和时间线变化 | sourceId、versionId、别名、hash、质量与版本 |
| `.app/compile/` | 用户启动独立编译及其完成时 | Source change set 与已消费版本 |
| `.app/compat/` | 用户确认对兼容知识库启用完整功能时 | 应用自有的兼容 `purpose.md` / `schema.md`；不占用根目录同名文件 |
| `.app/settings.json` | 设置变化 | 项目级应用设置 |
| `.app/agent-config.json` | Agent 配置变化 | Agent 检测与默认绑定 |
| `.app/graph-cache.json` | trusted writable 项目图谱构建后 | 布局缓存；restricted/read-only 不写 |
| `.app/import-conflicts.json` | 导入冲突、重命名、失败 | 导入问题记录 |
| `.app/bookmarks.json` | 收藏、星标变化 | 用户收藏状态 |
| `.app/chats/{id}.json` | 项目应用状态可写时的 Chat 会话变化 | 会话历史；文件系统只读时仅保留本次运行内存状态 |
| `.app/tasks/` | 项目 app state 可写时的后台任务变化 | 可恢复任务状态和日志；受限/只读本地任务不落盘 |
| layout-defined exports root（原生为 `exports/html/`） | trusted writable 项目确认导出生成后 | HTML、卡片、报告 |
| 全局最近位置 / 歧义偏好 / 目录信任记录 | 创建、打开选择或信任变化时 | 写入应用配置目录，不写入知识库 |

## 18. MVP 实现优先级

建议顺序：

1. 完整 shell 下的无项目双入口工作台与固定启动规则。
2. 新建知识库、typed 打开评估、独立 trust / filesystem access / health、兼容启用与修复确认。
3. 普通资料文件夹“新建知识库并导入”，原目录保持不变。
4. 多格式导入与解析预览。
5. Git 初始化和检查点。
6. 工作流统一任务骨架与更新 Wiki 的 Agent / BYOK 双路径。
7. Wiki 文件树、Markdown 阅读和基础编辑。
8. 搜索、索引刷新和基础图谱。
9. Chat 问答与引用来源。
10. Lint 本地规则和 Agent 深度 Lint。
11. HTML / 卡片 / 项目报告导出。
12. 后台任务、托盘、通知。
13. 中英双语、主题、更新检查和多平台打包。

## 19. 禁止误解点

- 不要引入数据库保存项目内容。
- 不要把 API Key 写入项目文件。
- 不要把无项目态实现成独立 landing / 启动页，或在首屏展示 Agent / BYOK 配置。
- 不要提供“打开文件夹为项目”第三入口，也不要把普通资料目录原地初始化、移动或写入应用 marker。
- 不要把兼容、健康、权限和 Git 状态压成单一布尔值。
- 不要在未经确认时写入兼容配置、信任标记或修复结果；信任记录不得写入项目目录。
- 不要让普通搜索自动调用模型。
- 不要把 Agent 作为 Wiki 编译、AI 整理、Chat 的唯一可用路径；BYOK API 在 Source 已存在后支撑这些核心流程。
- 不要让 BYOK 参与导入解析或失败恢复。
- 不要让 BYOK API 替代所有高级 Agent Skill 能力。
- 不要静默安装 Agent。
- 不要把 Agent 配置、Provider 卡片或“运行 Agent”对话框恢复为主导航页面；配置保留在设置，主入口是工作流。
- 不要跨项目展示工作流、任务、确认或历史。
- 不要把工作流队列、启动、普通阶段进度发送为系统通知。
- 不要静默覆盖用户手动编辑。
- OCR / ASR 属于导入阶段的按内容缺口能力，必须由用户主动启用；图片视觉理解不在首版范围。
- 不要把 HTML 模板和 Wiki schema 混在一起。
- 不要把路由命名当成本文规定的接口。
