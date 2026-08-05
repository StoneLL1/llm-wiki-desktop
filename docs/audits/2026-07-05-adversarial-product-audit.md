# LLM Wiki Desktop 对抗式产品审计

> **历史审计快照：** 本文记录 2026-07-05 当时的规格与实现张力，不再定义首次使用、项目打开或 Import 的目标行为。当前分别以 [首次使用与打开已有知识库规范](../superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md) 和 [Import / Source / Media 规范](../superpowers/specs/2026-07-24-import-source-media-flow-design.md) 为准；下文出现的“导入后编译”等表述只能作为当时问题证据。

日期：2026-07-05

审计目标：从用户第一性原理出发，检查当前项目距离“把个人资料变成可信、可维护、可追溯的 Markdown Wiki 桌面应用”还缺什么，哪些细节会伤害体验，哪些实现与规格存在张力。

## 阅读范围

本地规格与实现：

- `SPEC/PRD.md`
- `SPEC/SPEC.md`
- `SPEC/APP_flow.md`
- `SPEC/TECH_STACK.md`
- `SPEC/BACKEND_STRUCTURE.md`
- `SPEC/FRONTEND_GUIDELINES.md`
- `SPEC/DESIGN.md`
- `UI-Frontend-design/dashboard.html`
- `UI-Frontend-design/assets/app.css`
- 关键实现入口：`src/app/App.tsx`、`src/components/app/AppShell.tsx`、`src/features/project/ProjectStartView.tsx`、`src/stores/*`、`src-tauri/src/services/*`
- 既有审计/修复线索：`docs/fixes/00-codebase-audit.md`

外部对照：

- [Karpathy 的 LLM Wiki 原始 gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki)
- [Astro-Han/karpathy-llm-wiki](https://github.com/Astro-Han/karpathy-llm-wiki)
- [nashsu/llm_wiki_skill](https://github.com/nashsu/llm_wiki_skill)
- [Karpathy LLM Wiki Obsidian 插件页](https://community.obsidian.md/plugins/karpathywiki)
- [lucasastorian/llmwiki](https://github.com/lucasastorian/llmwiki)

## 第一性原理判断

用户真正购买的不是“另一个聊天框”，而是五件事：

1. 我把资料放进去，系统能尽快产出可读、可链接、可继续增长的知识资产。
2. 原始资料永远安全，LLM 只能在明确边界内维护派生层。
3. 每个回答、每次改写、每个图谱连接都能让我知道依据是什么。
4. 当自动化出错时，我能看见、取消、回滚、继续，而不是猜状态。
5. 我不用理解 Agent、BYOK、schema、Git、任务队列的内部细节，也能完成第一轮“导入 -> 编译 -> 提问 -> 修正 -> 导出”。

当前项目的方向是对的：本地 Markdown/JSON、Tauri 后端服务、Git checkpoint、raw/wiki/.app 分层、Chat/Agent/BYOK 双路径、Codex 风格工作台都符合产品目标。主要风险不在“有没有搭骨架”，而在首轮体验、可信度展示、长任务反馈、边界一致性和维护复杂度上。

## 外部项目对照

Karpathy 原始模式强调三层：不可变 Raw Sources、LLM 维护的 Markdown Wiki、约束 LLM 行为的 Schema。它的关键差异不是检索技术，而是“知识先编译，再被查询”。Query 产物也应能保存回 Wiki，成为新的知识资产。

nashsu/llm_wiki 把这个模式产品化时，额外强化了几个点：三栏桌面界面、实时活动面板、会话/设置/项目状态持久化、场景模板、`purpose.md` 作为 Wiki 的方向约束、Obsidian 兼容、以及 Agent 可通过本地 HTTP API 读取 Wiki。它的用户承诺更偏“一打开就是知识工作台”，而不是“先理解一套 Agent 工作流”。

Astro-Han/karpathy-llm-wiki 不是完整桌面产品，而是把流程封装成 Agent Skill：重点是可复用的 ingest/query/lint 规则、示例、日志和 lint。它提醒本项目：Schema/Skill 质量本身是产品能力，不只是隐藏配置文件。

Obsidian 插件路线的优势是借用用户已有笔记工作流，核心体验是“写入 notes/sources 后自动组织、链接、回答”。它对本项目的启发是：用户不想先学习项目内部目录，最好在 UI 中持续显式表达“你写，AI 组织，你问，答案回到图谱”。

lucasastorian/llmwiki 走 Web/MCP/Claude 路线，开放但可能引入云数据库依赖。本项目坚持 local-first、无数据库、OS secret storage 是差异化优势，不应为了追赶功能而放弃。

## 严重度定义

- P0：阻断用户闭环、破坏信任或可能导致数据/隐私风险。
- P1：明显降低核心体验，用户会误解产品或放弃继续使用。
- P2：细节不顺、维护风险或规模增长后会放大。

## P0/P1 审计发现

### 1. Time-to-first-value 仍然偏长

现状：创建项目、导入、预览、确认、编译、图谱、Chat 都有对应模块，但用户第一次使用时仍要理解项目目录、Agent/BYOK、导入预览、编译任务、Graph/Chat 的依赖关系。`ProjectStartView` 仍明显像启动页/行动卡片页，而不是已进入一个可操作工作台。

用户风险：用户导入资料后，如果没有配置 Agent/BYOK 或编译失败，会看到空图谱、空 Wiki、Chat 上下文不足。这是最危险的流失点：用户以为产品“没做成”，而不是知道下一步该做什么。

建议：

- 新项目创建后进入真实工作台，而不是停留在启动感很强的界面。
- 提供“首次导入向导”但保持桌面工具密度：选择来源、显示支持格式、显示是否需要 Agent/BYOK、显示预计输出。
- 没有 Agent/BYOK 时，仍应把可提取文本作为 `wiki/sources/` 立即可读，并在 Graph/Chat 明确标注“尚未编译，当前只可浏览源文本”。
- 加一个小型示例项目或可选 demo source pack，让用户 60 秒内看到 Wiki、Graph、Chat 的完整闭环。

### 2. BYOK 核心流的回答质量容易被检索脆弱性卡住

现状：`chat_service` 会用本地 SearchService 拿 top excerpts，加上 purpose、历史和 pinned page 组成上下文。Agent 路径可以读项目文件；BYOK 路径只能看后端塞进 prompt 的上下文。

用户风险：同一个问题，Agent 能答，BYOK 可能因为关键词没命中而答不全。用户不会理解“这是检索召回问题”，只会觉得模型或产品不可靠。引用列表目前更像“检索到的候选上下文”，不是严格证明每个回答句子都有依据。

建议：

- 把 Chat 中的引用命名区分为“使用的上下文”和“已验证依据”。没有做 claim-level verification 时，不要暗示所有回答都被逐句证明。
- BYOK 召回应至少加入结构化入口：`index.md`、当前页、相邻 wikilink、最近编辑页、source pages、bookmark/starred、标题/别名匹配。
- 保持普通搜索为本地 keyword/filter，但 Chat 内部可以做更强的本地 lexical retrieval，例如 BM25、标题加权、CJK 分词/字符 n-gram、wikilink 邻域扩展。
- 对“上下文不足”给出可执行下一步：去编译、选择页面提问、扩大搜索范围、导入更多资料，而不是只返回失败口吻。

### 3. Import/Extraction 责任边界不一致

现状：后端 `extraction_service` 支持多格式提取；URL 导入的一部分 Readability/Markdown 化逻辑在 React 侧完成，再交给后端。规格强调 React UI 不应拥有 filesystem/Git/Agent/secret-storage 逻辑；虽然 URL 解析不等同于这些高危能力，但它已经属于内容提取与规范化规则。

用户风险：同一篇 HTML/URL 通过不同入口得到不同 Markdown 质量。用户看到“预览没问题、编译结果怪”或“网页解析差”时，很难判断是来源、模型还是前端解析器的问题。

建议：

- 将 URL/HTML 提取的规范化规则集中到后端服务，React 只展示 preview/confirm。
- Import preview 展示 parser、置信度、保留/丢弃的内容类型、失败原因、是否 archive-only。
- 图片、扫描 PDF、复杂表格等不能提取时，用明确状态，不要让用户以为资料已经进入 Chat 可理解范围。

### 4. 自动化信任闭环需要更强的“看得见”

现状：项目已经有 task store、log drawer、toast、通知、PendingAction、checkpoint/rollback 相关能力。规格也要求长任务可取消、可记录、可后台运行。

用户风险：一旦 Agent 编译、lint auto-fix、source replacement、export 等任务耗时或失败，用户最关心的是“现在做到哪一步、改了哪些路径、有没有 checkpoint、能不能取消/回滚”。如果 task drawer 排序、状态恢复、日志归因或失败解释不够清晰，用户会把自动化视为黑箱。

建议：

- 所有高风险任务的 UI 都显示：Git checkpoint 状态、影响路径、取消入口、日志入口、最后一次写入时间、失败是否已回滚。
- Task list 默认按时间线排序，而不是按状态造成“旧失败压住新进度”。
- 任务失败结果应有面向用户的错误代码说明和可重试策略。
- 关闭/重开 app 后，未完成/失败任务要恢复到可理解状态，不能只留下 `.app/tasks/*.json`。

### 5. Launch screen 与设计原则存在张力

现状：规格明确要求“不要 landing-page hero，第一屏是实际可用体验”。但当前 `ProjectStartView` 仍有 `launch__hero`、quick action card、recent card、右侧能力说明式面板。它可用，但心智仍像产品入口页。

用户风险：桌面知识工具的第一屏应该让用户感觉“我已经在工作台里”，而不是“我还在产品介绍页”。这会弱化 Codex desktop 风格，也会让新建/打开/导入的路径显得比实际更复杂。

建议：

- 把无项目状态也做成 shell：左侧 Project/Recent/Templates，中心为打开/创建/导入操作列表，右侧为能力状态和路径说明。
- 保留最近项目，但避免大卡片营销感。
- 顶部和状态栏在无项目状态也可见，减少进入项目后的布局跳变。

### 6. `purpose.md` 与 `schema.md` 的产品化还不够

现状：规格和外部项目都强调 schema/purpose 是 LLM 行为的纪律来源。当前项目有 project init 和模板方向，但 UI 中对 purpose/schema 的可编辑性、质量、变更影响、与 Chat/Compile 的关系还不够显眼。

用户风险：用户会把输出质量全归因于模型，而不知道“项目目的”和“维护规则”才是 Wiki 长期质量的核心旋钮。越到后期，Wiki 漂移、页面风格不一致、分类混乱越难修。

建议：

- Settings 或 Dashboard 中增加 Purpose/Schema 状态：是否存在、最后更新、模板来源、是否被 lint 提醒。
- 编译前显示本次会使用的 purpose/schema 摘要。
- Lint 中加入 purpose/schema drift 类检查，例如页面类型缺失、frontmatter 不一致、source count 不合理、index 未更新。

### 7. Graph 的可解释性不足

现状：GraphService/GraphView 已经搭建节点、边、社区、过滤、导出等能力。规格中 graph 是核心入口之一。

用户风险：图谱如果只显示“这些节点连在一起”，用户会问“为什么连”。没有边依据、关系类型、来源页面或片段，图谱就容易变成漂亮但不可信的导航。

建议：

- 边 hover/右侧 panel 显示关系来源：wikilink、共同来源、同 frontmatter 标签、LLM 生成关系、手动关系。
- 对 LLM 生成的边标注置信/待验证状态。
- 空图谱状态要解释“导入源文本”和“编译 Wiki”分别会产生什么图谱层级。

### 8. Chat “保存回答到 Wiki”是亮点，但需要更强治理

现状：ChatStore 支持保存答案到 `wiki/queries/`，并有 overwrite/hash/checkpoint 处理。

用户风险：如果用户把临时回答大量保存为页面，Wiki 会从“长期知识资产”变成“聊天记录堆”。Karpathy 模式里 query 产物能回写，但前提是答案被整理成有结构的知识页。

建议：

- 保存前让用户选择页面类型：query note、concept page、comparison、decision、task brief。
- 自动加入 frontmatter：source question、context pages、model/agent、created_at、verified 状态。
- 保存后触发轻量 lint：标题重复、孤立页面、无引用、未加入 index。

### 9. 原始资料层与 Wiki source mirror 的心智边界要更清楚

现状：导入后项目会保留 `raw/sources/`，并把可读源文本推广到 `wiki/sources/`。这是非常好的体验，因为用户可立即浏览源文本，Chat 也能在未完整编译时获取内容。

用户风险：用户可能误以为 `wiki/sources/` 就是原始文件，或者手改了 source mirror 后期待它反向修改 raw source。后续 Agent 编译也可能混淆“来源页”和“综合页”。

建议：

- UI 上把 source mirror 标成“可读副本/提取文本”，明确 raw original 不变。
- Editor 对 `wiki/sources/` 默认只读或弱提醒，除非用户明确进入“编辑提取文本副本”。
- Chat/Graph/Export 中区分 source pages 与 synthesized pages。

### 10. Project truth 文档分散，容易让后续 Agent 走偏

现状：主规格在 `SPEC/`，设计真源在 `UI-Frontend-design/`，修复计划在 `docs/fixes/`，进度/踩坑在 `SPEC/progress.txt` 与 `SPEC/gotchas.txt`，还有 `CLAUDE.md` 镜像规则。当前已有多个批次修复文档，且项目文件处于较多 dirty/untracked 状态。

用户风险：产品决策会被“最后读到的文档”影响。Agent 继续开发时，可能不知道哪个是权威：PRD/SPEC、设计 HTML、fix audit、roadmap 还是当前实现。

建议：

- 增加 `SPEC/MVP_TRACE.md` 或 `docs/audits/acceptance-matrix.md`：每个核心用户闭环对应规格、实现文件、测试、当前状态、风险。
- 在 PRD/SPEC 顶部声明设计/实现优先级：产品目标 > hard rules > UI-Frontend-design > feature docs > current implementation。
- 把已有 `docs/fixes/00..05` 转成状态矩阵，而不是散落叙事。

### 11. Windows/中文环境可靠性暴露出维护风险

现状：本次审计中 `rg.exe` 在该仓库报 Access denied，已记录在 gotchas；PowerShell 输出中文文档时多处 mojibake。代码和文档本身可能仍是 UTF-8，但终端读取体验不稳定。

用户风险：本项目明确要求 CJK 文件名、Unicode path、Windows/macOS/Linux 都是测试关注点。若开发/审计工具链本身无法稳定显示中文，后续中文 UI、日志、导出、错误信息可能被误判或破坏。

建议：

- 增加编码/路径 smoke test：中文项目名、中文文件名、中文 frontmatter、中文搜索、中文导出 HTML。
- 文档贡献说明中明确 UTF-8、PowerShell 输出编码建议。
- CI/本地检查加入“中文 fixture 不应变成 mojibake”的快照或 roundtrip 测试。

### 12. `AppShell` 聚合过多流程，后续 UX 迭代成本高

现状：`AppShell.tsx` 同时承载导航、项目状态、导入、替换/删除 PendingAction、编译、能力检测、设置、导出、任务、toast/dialog、workspace switch 等大量逻辑。

用户风险：这不是直接 UI bug，但会放大所有 UX bug 的修复成本。任何一个入口的状态变更都可能影响其他入口，例如导入后编译、任务刷新、右侧面板、Chat focus。

建议：

- 不做大重构，但把高风险流程逐步抽成 hook/controller：import workflow、source replacement workflow、compile workflow、export workflow。
- 每个 workflow hook 暴露明确状态机，而不是一批散落 callback。
- 给用户关键路径写集成测试：首次导入、导入后编译、编译失败、保存 Chat 答案、删除 source pending action。

## 次级细节发现

### P2-1. 搜索框文案需严守“非自然语言搜索”

规格要求普通搜索只能是本地 keyword/filter，NL answer 必须进 Chat/Agent/BYOK。顶栏和搜索视图的 placeholder/empty state 需要避免暗示“直接问问题”。

### P2-2. Agent 安装和能力状态要分离

规格要求不能静默安装 Agent。UI 应清晰区分：未安装、已安装但未授权、已安装但当前模式只读、可用于编译、可用于 Chat、BYOK 可用。不要只显示“Agent ready”。

### P2-3. Export 应强调可复现性

用户导出 HTML/card/report 后，希望知道导出基于哪个 Git checkpoint、哪些页面、是否含 source mirror、是否含 Chat query pages。导出记录应可从 `.app` 或 UI 再次查看。

### P2-4. Bookmarks/Starred 应进入推荐与 Chat 上下文

书签如果只是 UI 收藏，价值有限。它应该影响 Dashboard、Chat retrieval、Graph focus、Export selections。

### P2-5. Lint 不应只像代码检查

知识库 lint 应包含知识质量检查：孤立页、无来源页、陈旧 index、重复标题、frontmatter 缺失、source mirror 未编译、回答页无上下文、页面过长、schema 不一致。

### P2-6. 空状态要分层

空 Wiki、空 Graph、空 Chat、空 Export 不应使用同一种“没有内容”。应分别告诉用户：未导入、已导入未提取、已提取未编译、已编译但无链接、当前筛选无结果。

### P2-7. 右侧 Context Panel 应成为信任面板

当前设计右侧包含 project info、paths、index、route、tasks。建议进一步承担“这个视图的依据”职责：当前页来源、最后编译、关联任务、checkpoint、Chat 使用上下文、Graph 选中节点证据。

## 推荐优先级

### 本周应优先修

1. 首次项目/空状态闭环：新建后直接工作台化，导入后即使未编译也能浏览 `wiki/sources/`，Graph/Chat 明确说明当前层级。
2. Chat 引用和上下文语义：把“使用上下文”与“证据引用”区分；BYOK 增加 index/current page/wiki link/source mirror 召回。
3. Import preview 透明度：显示 parser、可提取性、archive-only、失败原因；把 URL/HTML 规范化下沉到后端计划中。
4. Task/Checkpoint 可见性：高风险任务统一显示 checkpoint、影响路径、取消、日志、回滚。

### 下一阶段

1. Purpose/Schema 产品化：状态、编辑、模板、lint 关联。
2. Graph edge explanation：为什么连接、来自哪里、是否验证。
3. Chat answer governance：保存为结构化页面，而不是简单保存聊天文本。
4. `AppShell` workflow hook 化：降低后续改 UX 的回归风险。

### 稍后但必须跟踪

1. 中文/Unicode/Windows 编码与路径全链路测试。
2. Export provenance。
3. Agent local API/skill 生态对接，参考 nashsu 的 read-only HTTP API skill，但必须保留本项目的 token 和 local-only 边界。

## 结论

这个项目已经不是“空壳”。它最接近成功的地方是：选择了正确的数据边界、正确的本地优先架构、正确的桌面工作台方向，也已经实现了相当多核心服务。

真正需要警惕的是：Karpathy LLM Wiki 的价值不是“把文件丢给模型问答”，而是让用户相信 Wiki 是一个会增长、会自查、能追溯、可回滚的长期知识资产。当前最该补的不是更多 flashy 功能，而是把第一轮用户闭环、上下文证据、长任务状态、purpose/schema、source/wiki 边界这些细节打磨到没有歧义。
