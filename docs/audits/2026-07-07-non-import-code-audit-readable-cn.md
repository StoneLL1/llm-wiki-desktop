# Non-Import Audit 中文白话对照版

日期：2026-07-07

对应原文：`docs/audits/2026-07-07-non-import-code-audit-and-plan.md`

这份文档不是新的审计结论，而是原审计文档的中文白话版。目标是方便逐条对照：每个条目在说什么，为什么要修，大概应该怎么修。

范围仍然保持不变：只看导入以外的部分。导入问题只放在附录，不混进本轮主计划。

## 总体结论

当前项目的方向是对的：用户内容仍然是 Markdown/JSON/local files，搜索是本地关键词搜索，Chat/Agent/BYOK 才负责自然语言回答，React 也基本没有直接接管文件系统、Git、Agent、secret-storage 这些后端职责。

真正最需要补的是“可信闭环”：

- Compile 要先有可审计的计划，再写文件。
- Chat 的 citation 要变成模型实际引用过的证据，而不是检索命中列表。
- BYOK 和 Agent 能力不一样，prompt 和 retrieval 也要分开设计。
- Lint 里能本地确定的问题要本地查，不要全交给 Agent 猜。
- Agent 能写文件或跑命令的地方，要把权限边界讲清楚、测清楚、收紧。

## P0：必须优先修

### P0-1：Compile 缺少“先计划、再写入”的阶段，也缺少语义校验

原文标题：Compile Has No Auditable Plan Stage And Semantic Validation Gate

白话解释：

现在的 Compile 更像是直接让模型吐出一批文件，然后代码只检查路径是否安全、核心文件是否存在。它没有先问模型：“你打算新建哪些页、更新哪些页、合并哪些页、为什么这么做、用了哪些 source？”所以程序很难在写入前判断模型的决策是不是靠谱。

为什么要修：

LLM Wiki 的核心不是“一个 source 生成一个 summary”，而是把新 source 编译进已有 wiki。这个过程一定会涉及合并、更新、冲突标注、级联更新。如果没有计划阶段，模型可能生成很多看起来合法但结构很差的页面，后面的 graph、chat、lint 都会被污染。

修复内容：

- 增加 `CompilePlan`：先描述 create/update/merge/conflict/cascade 决策。
- 再生成 `CompileManifest`：具体文件内容和删除/更新列表。
- 增加 `validate_plan`：检查目标路径、source id、merge 目标、风险标记。
- 加强 `validate_manifest`：检查 frontmatter、`sources`、`type`、source section、是否错误写入 `wiki/sources/`、是否只是 source mirror。
- 计划或 manifest 校验失败时，不写任何项目文件。

验收重点：

- 没有 `sources` 的派生页必须失败。
- 写 `wiki/sources/` 必须失败。
- merge 必须指向已有页面。
- 只有 `index/overview/log` 这种空壳 compile 不能算成功。

### P0-2：Chat citation 现在是检索命中，不是模型实际引用

原文标题：Chat Citations Are Retrieval Hits, Not Model-Used Evidence

白话解释：

现在系统先检索 top pages，然后把这些页面直接当成 citations 存下来。问题是模型未必真的用了这些页面。用户看到 citation 会以为“回答就是根据这些来源来的”，但实际可能只是“这些是检索器当时找出来的候选材料”。

为什么要修：

这是信任问题。用户点 citation 时，应该能找到回答里的依据。如果 citation 只是检索命中，保存到 `wiki/queries/` 的 query page 也会带上不可靠 sources，wiki 的证据链就不干净了。

修复内容：

- 给检索到的 source 编号，例如 `[S1]`、`[S2]`。
- Prompt 要求模型在回答中显式引用这些编号。
- 增加 parser，只把模型回答中实际出现的 source id 存为 citations。
- 检索命中列表保留为 `retrievalHits`，用于调试和 UI 展示，但不要冒充 citations。
- Agent 如果额外读了别的页面，也必须用路径或编号明确引用。

验收重点：

- 模型只引用 `[S2]`，最终 citations 只能有 S2。
- 模型没有引用任何来源，就不要假装有 citation。
- 保存到 `wiki/queries/*.md` 的 `sources` 必须和模型实际引用一致。

### P0-3：Chat retrieval 是固定 top-k 摘要，BYOK 和 Agent prompt 混在一起

原文标题：Chat Retrieval And Prompting Are Fixed Top-K And Mixed Across BYOK/Agent

白话解释：

当前 Chat 大致是：找 6 个结果，每个截 1200 字，再把同一套 prompt 发给 BYOK 或 Agent。可是 BYOK API 不能读本地文件，Agent 可以。两者能力边界不同，不能用同一段提示词糊过去。

为什么要修：

BYOK 只能基于你塞进上下文的内容回答，所以要精心控制上下文预算。Agent 可以读文件，就应该先读 index，再按需要 read-more。固定 top-k 很容易漏掉真正重要的相邻页面，也容易让历史聊天挤占 source 内容。

修复内容：

- 拆成 `assemble_byok_prompt` 和 `assemble_agent_prompt`。
- BYOK prompt 明确说明没有文件系统/工具访问。
- Agent prompt 明确要求 index-first、按需读更多页面。
- 增加 retrieval planner：先 index，再关键词命中，再可选 graph expansion，再按 token/char budget 选择内容。
- 每条回答记录 retrieval 诊断：选了哪些页、扩展了哪些页、哪些页因为预算被省略。

验收重点：

- BYOK prompt 不能暗示自己能读文件。
- Agent prompt 必须有 index-first/read-more 行为。
- retrieval 要受上下文预算控制，而不是固定 top-k。
- 普通 Search 仍然保持关键词搜索，不调用 LLM。

### P0-4：Deep lint 证据太少，而且太相信 Agent 给的 severity

原文标题：Deep Lint Is Under-Evidenced And Agent Severity Is Trusted

白话解释：

现在 deep lint 给 Agent 的每页内容片段很短，而且没有清晰的 severity 规则。Agent 说 error 就当 error，warning 就当 warning。这会让 lint 结果更像“模型主观判断”，不够可重复。

为什么要修：

矛盾、过时、schema 不一致、source 证据不足，通常都要看完整段落或多页内容。240 字左右的片段太短，很容易误判或漏判。severity 也必须有规则，否则 UI 上红黄绿的可信度不够。

修复内容：

- deep lint 输入改成 section-aware 或更长的 page excerpt。
- 把本地 lint 已经发现的问题作为 baseline 给 Agent，避免重复报。
- 在 `wiki-lint` Skill 和 prompt 里增加 error/warning/info rubric。
- 本地二次校正 Agent severity：没有路径、没有证据、类型不明确的 issue 降级或拒收。

验收重点：

- Skill 里能说清楚什么是 error、warning、info。
- Agent 报告没有 evidence 时不能直接当高危。
- deterministic lint 和 heuristic lint 分层清楚。

### P0-5：Compile Agent 写入 profile 允许 Bash，缺少受控工具面

原文标题：Compile Agent Write Profile Allows Bash Without A Controlled Tool Surface

白话解释：

当前 compile Agent 有 temp workspace 和 manifest validation，这对“最终写入项目”有保护。但如果 Agent 在运行过程中能用 Bash，那么恶意 source 或 prompt injection 可能在校验之前就影响系统环境。

为什么要修：

这不是说现在一定会出事，而是安全边界不够理想。LLM Wiki 会读取用户导入的内容，而这些内容可能带 prompt injection。只靠“最后检查 manifest”不够，因为 shell 命令可能已经执行过。

修复内容：

短期：

- 保留 temp workspace。
- 日志里明确显示本次 Agent compile 的权限 profile。
- 测试确认不会在 manifest 通过前写入真实项目。
- 支持的话优先使用不含 Bash 的写文件 profile。

中期：

- 默认 safe compile 改成：Agent/BYOK 只返回 plan/manifest/content，真正写文件由 Rust 在验证后执行。
- Agent 查询 wiki 时走受控 read-only API，而不是随便读写文件系统。

验收重点：

- 未通过 plan/manifest 校验时绝不写项目。
- Bash-enabled profile 是显式高级/残余风险路径，不是默认安全路径。

## P1：重要，但可以排在 P0 之后

### P1-1：`wiki-ingest` Skill 缺少 create/update/merge/conflict 的决策规则

白话解释：

当前 Skill 已经说了不要写 `wiki/sources/`、不要一源一页，但还没有明确告诉模型什么时候更新旧页、什么时候新建页面、什么时候合并、什么时候标冲突。

为什么要修：

没有决策规则，模型容易把 wiki 做碎：相同主题重复建页，冲突被藏进正文，相关概念没有 see-also 或 cascade update。

修复内容：

- 在 `wiki-ingest/SKILL.md` 加 `Decision Rules`。
- 规则覆盖 create/update/merge/see-also/conflict/cascade。
- BYOK 和 Agent compile prompt 都复用同一套规则。

### P1-2：Compile 指令分散在多个地方，容易漂移

白话解释：

BYOK prompt、Agent prompt、Skill template 里都有 compile 规则。现在靠人工维护一致，后面很容易改了一处忘了另一处。

为什么要修：

同一个项目如果用 BYOK 编译和 Agent 编译，结果不应该因为 prompt 漂移而完全不一样。source traceability、cascade、merge/create 这种核心规则必须一致。

修复内容：

- 做共享 instruction builder 或共享常量。
- BYOK、Agent、Skill template 测试都检查关键条款。
- 测试覆盖 `wiki/sources` 保护、source citation、cascade、merge/create。

### P1-3：Schema/source traceability 太依赖 Agent lint

白话解释：

有些问题其实不用 Agent 判断。例如 page 的 `type` 是否合法、派生页有没有 `sources`、source 路径是否存在、某类页面是否缺 required section。这些是本地代码能确定的。

为什么要修：

本地能查的问题交给 Agent，会变慢、变贵、变不稳定，还可能因为 prompt 或 excerpt 不足漏报。

修复内容：

- 增加本地 deterministic schema/source checks。
- 检查 `type`、`sources`、source path、source section、结构页 index membership。
- Agent deep lint 只做真正需要判断的 heuristic 问题，例如矛盾、陈旧、缺失概念。

### P1-4：还没有 `wiki-query` / `wiki-chat` Skill，也没有受控 read-only API

白话解释：

现在内部 Chat 可以用，但外部 Agent 没有一个稳定的“只读查询 wiki”的工具面。要么读原始文件，要么只能走内置 Chat。

为什么要修：

后续 Agent 扩展如果没有受控 API，会不断堆文件系统 prompt。这样安全审查、citation 规则、权限控制都会变难。

修复内容：

- 先新增 `wiki-query/SKILL.md`，定义只读、index-first、编号 citation、不可改 source。
- 后续再设计 `127.0.0.1` read-only API：health/projects/search/read/graph/lint-summary。
- 第一阶段不加任何 write endpoint。

### P1-5：Retrieval 没利用 graph expansion 或 source overlap

白话解释：

项目已经有 graph 服务，知道页面之间有 wikilink 和 tag 关系。但 Chat retrieval 现在只看关键词 top hits，没有利用这些图谱关系。

为什么要修：

很多好答案不在关键词命中的第一页，而在相邻的 synthesis/comparison/entity 页面。只用关键词会让回答变窄。

修复内容：

- 在 citation 可信问题修好后，retrieval planner 可以从 seed hits 扩展一跳 graph neighbors。
- 扩展必须受预算控制。
- expanded pages 要在诊断信息里标出来。
- 不把这一步变成 vector DB 优先。

### P1-6：Codex deep-lint Agent profile 比 compile/chat 更不明确

白话解释：

Chat 的 Codex route 明确 read-only，compile 的 Codex route 明确 workspace-write。但 deep lint 的 Codex 调用参数更少，容易吃到默认行为或 repo rules。

为什么要修：

Lint 是审查操作，按理应该只读而且可重复。如果它依赖当前 Codex 默认设置，后续行为会不稳定，也不够安全。

修复内容：

- Codex deep lint 加明确参数：ephemeral、ignore-rules、read-only sandbox、skip git repo check、指定 workspace。
- 对 HTML export 的类似模式另开审查，不混入本轮主任务。

## P2 / Later：后续改进或边界提醒

### P2-1：Graph edge 缺少 evidence，但这符合 MVP 范围

白话解释：

现在 graph 只告诉你两个页面 related，有权重，但不解释为什么 related。外部实现可能会展示更多信号，比如 wikilink、shared tag、source overlap。

为什么不是 P0：

当前 SPEC 里 v1 edge 本来就是统一 `related`，所以这不是立即要修的信任漏洞。

后续修复：

- 可以给 `GraphEdge` 加可选 `signals`。
- UI 能展示边来自 wikilink、shared tag 还是 source overlap。

### P2-2：Vector search 只能做可删除缓存，不能变成内容源

白话解释：

外部项目有 vector search，但本项目硬规则是用户 wiki 内容源必须是 Markdown/JSON/local files，不是数据库。

为什么要强调：

vector search 可以提高召回，但如果第一步就引入 DB/vector store，很容易把项目拉回普通 RAG 架构，削弱 local-first 和文件透明性。

后续修复：

- 不在第一批实现 vector search。
- 如果以后加，只能作为 `.app/` 下可删除、可重建的 derived cache。
- 删除 cache 不能丢用户内容。

### P2-3：MCP-like surface 应该在 read-only API 稳定后再做

白话解释：

MCP 工具面很诱人，但本项目应该先把本地只读 API 设计清楚，再映射成 MCP。

为什么要修：

如果先做 MCP，很容易一口气暴露太多工具，甚至绕过 Git checkpoint、PendingAction 和路径保护。

后续修复：

- 先做 read-only endpoints。
- endpoint 稳定后再设计 MCP server。
- MCP 第一版不能有 write tool。

### P2-4：Chat convenience write mode 要单独做安全审查

白话解释：

普通 Chat 是问答，应该只读。但 convenience write mode 是“从聊天里顺手让 Agent 改文件”，风险等级完全不同。

为什么要修：

用户可能以为自己只是在问问题，但背后 Agent 已经有写权限。这个体验必须非常清楚地标出来。

后续修复：

- 普通 Chat 保持 read-only。
- convenience write mode 单独入口、单独标签、必须 checkpoint、必须有 audit log。
- 不要把它混进正常 Chat。

### P2-5：Local lint 的 dead-link 行号是 body 相对行号

白话解释：

如果 Markdown 顶部有 YAML frontmatter，当前 dead-link 的行号可能从正文第一行开始算，导致 UI 跳转到真实文件里的错误位置。

为什么要修：

这不是 LLM Wiki 语义大问题，但会影响用户信任。Lint 报第 10 行，用户点过去却不在那里，很容易怀疑工具不靠谱。

后续修复：

- `split_frontmatter` 暴露正文起始行。
- 或者查 wikilink 行号时加上 frontmatter offset。

### P2-6：Lint regenerate index 可能把 source/query 页混进主 index

白话解释：

现在 regenerate index 可能把 `wiki/sources/**` 和 `wiki/queries/**` 当普通 wiki 页写进 index。source 是导入原文，query 是保存的问答，它们不一定应该和 concept/entity 页混在一起。

为什么要修：

`wiki/index.md` 是给人和 Agent 导航的入口。如果被 source mirror 或 query record 塞满，会降低导航质量。

后续修复：

- 明确定义 index inclusion policy。
- `wiki/sources/**` 默认排除或单独分组。
- `wiki/queries/**` 默认排除或单独分组。
- concept/entity/overview 等派生页继续进入主 index。

## 已经改善的点

这些旧问题不要再当成当前 P0：

- `WikiIndex` 已经有共享 in-memory index，不再是完全没有索引。
- Compile 已经有 `wiki/sources/` 保护。
- Agent Chat 已经有 read-only profile。
- BYOK compile prompt 已经不是早期的一行 stub。
- Search 边界明确：本地关键词搜索，不调用 LLM/Agent。
- API key/base URL 相关 secret 泄漏已有基本防护。

换句话说，下一步不是“重做全部架构”，而是把现有架构的可信链条补完整。

## 明确不要做的事

- 不要引入数据库作为用户 wiki 内容源。
- 不要把普通 Search 变成自然语言问答。
- 不要让 React 拥有文件系统、Git、Agent、secret-storage 逻辑。
- 不要把导入重做混进本轮非导入计划。
- 不要把 vector/LanceDB 当作 P0。
- 不要第一版 read-only API 就暴露 write endpoint。
- 不要让 Agent CLI 成为唯一主路径，BYOK 仍然要能跑核心流程。

## 导入专项附录

这些确实是问题，但属于“导入重做专项”，本轮只标记边界：

- URL import 的 redirect、encoding、HTML 解析。
- Readability 目前在 React 侧调用。
- batch confirm、partial success、persistent ingest queue。
- source replacement、raw/source 保护。
- clipping/highlighting。

为什么不混进本轮：

导入会牵涉网络、编码、重定向、原始 source 替换、用户确认、队列恢复等另一套风险模型。如果和 Compile/Chat/Lint 一起改，范围会失控，也更难验收。

## 执行计划白话版

### Phase 0：先保现场和基线

做什么：

- 看 `git status`，记录已有脏文件。
- 跑 `npm run test`、`npm run lint`。
- 能跑 Rust targeted tests 就跑。
- 确认不碰导入代码。

为什么：

后续要改的是信任链条，不能在一个不清楚的工作树上动手，也不能把用户已有改动误当成自己的改动。

### Phase 1：先做低风险 P0 小修

做什么：

- Chat 增加 source 编号和 citation parser。
- retrieval hits 和 citations 分开。
- BYOK/Agent prompt header 分开。
- `validate_manifest` 先加最小语义门槛：必须有 `sources`、`type`、source section。
- deep lint 增加 severity rubric 和更长证据。
- Codex deep lint 改成明确 read-only profile。
- Compile Bash profile 先加日志和风险标记。

为什么：

这些改动能快速修掉最明显的“假 citation”“弱 lint”“manifest 太松”问题，同时不立刻重构整个 compile 流程。

### Phase 2：重构 Compile prompt / DTO / validation

做什么：

- 加 `CompilePlan` DTO。
- 加共享 compile instruction builder。
- BYOK/Agent 都先 plan 后 manifest。
- 实现 `validate_plan` 和更强 `validate_manifest`。
- 默认 safe compile 改成 Rust 验证后写文件。

为什么：

这是 Compile 可信度的核心。只要没有 plan 阶段，后面再多 lint 都是在补救。

### Phase 3：重做 Chat citation / retrieval

做什么：

- 加 retrieval planner。
- index-first。
- 支持 graph expansion。
- 按 provider context window 控制预算。
- 新增 `wiki-query` Skill。

为什么：

Chat 要从“给模型几个 top-k 摘要”升级成“可解释、可预算、可引用的查询流程”。

### Phase 4：Lint/schema 本地化

做什么：

- 本地解析 frontmatter。
- 本地检查 `type`、`sources`、source path、required section。
- 把本地 lint baseline 传给 deep lint。

为什么：

确定性问题要由代码稳定检查，Agent 只负责需要判断和综合的部分。

### Phase 5：Agent Skill / API 后续路线

做什么：

- 完成 `wiki-query` Skill。
- 设计 read-only API：health/projects/search/read/graph/lint-summary。
- API 只绑定 localhost，token 走安全存储。
- MCP 等 API 稳定后再做。

为什么：

Agent 需要能力边界，不应该长期靠“你自己去读文件吧”的 prompt。

### Phase 6：文档和回归测试

做什么：

- 更新 CompilePlan、citation、lint layering 文档。
- 补旧 P0 的回归测试。
- 全量跑 test/lint。
- 做两轮 review。

为什么：

这类边界问题很容易以后又漂移，必须靠测试和文档钉住。

## 最适合优先交给后续 Agent 的任务

建议从这几个切入，收益最高：

1. Chat citation provenance：把 citations 从 retrieval hits 改成模型实际引用。
2. Compile 最小 semantic manifest gate：先拒掉没有 `sources`/`type` 的派生页。
3. BYOK/Agent prompt split：先别让两种能力完全不同的 route 共用同一段 prompt。
4. Deep lint severity rubric：先让红黄绿有规则。
5. Codex deep lint read-only profile：把 lint 操作的权限边界补明确。

这些任务都可以小步做、好测试、回滚清楚，而且不需要碰导入专项。
