# Karpathy LLM Wiki 开源生态对照深潜（2026-07-07）

本文只做外部生态调研和本仓库差距对照，不改产品决策和实现代码。调研目标是回答一个问题：如果把 Karpathy 原始模式、nashsu 产品化实现、若干 Skill/Agent 变体放在一起看，我们的 `skill / prompt / lint / 导入` 下一步最该补什么。

## 0. 结论先行

我们的方向是对的：`SPEC/PRD.md:16-20` 已明确本地优先、Raw Sources → Wiki → Schema；`SPEC/PRD.md:55-61` 把导入、编译、图谱、Chat、Lint、导出闭环列为 MVP；`src-tauri/templates/skills/wiki-ingest/SKILL.md:12-14` 和 `src-tauri/src/services/compile_service.rs:25` 已经把 `wiki/sources/` 原文保护、派生页、双来源引用写进 Skill/BYOK prompt。相比 2026-07-05 audit，当天指出的“BYOK prompt 很薄”已经部分补上。

但和外部最强实现相比，主要差距不在“有没有 LLM Wiki 框架”，而在四个细节层：

1. **编译不是两阶段。** nashsu 把 ingest 明确做成先分析再生成的 `Two-Step Chain-of-Thought Ingest`（[README#L33](https://github.com/nashsu/llm_wiki/blob/main/README.md#L33)），Astro Skill 也要求抓取 raw 与编译 wiki `Always both steps, no exceptions`（[SKILL.md#L43-L44](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L43-L44)）。我们目前 Agent prompt 较强，但 BYOK/Agent 仍是一次性生成 manifest，缺少可校验的 plan 阶段、few-shot、语义级 schema/source 验证。
2. **Chat 引用可信度不足。** Karpathy 的 query 是搜索页面后“带 citations 综合回答”（[gist#L98](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f#file-llm-wiki-md-L98)）。nashsu 的 Skill 让 Agent 先 search/read，再 `Cite paths`（[SKILL.md#L194](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L194)）。我们在 `src-tauri/src/commands/chat_commands.rs:167` 先 clone 检索命中的 citations，再调用模型，`src-tauri/src/services/chat_service.rs:333-363` 保存的来源也是这批检索页，而不是模型实际使用的证据。
3. **Lint 深度不够。** Karpathy 原文把 contradictions、stale claims、orphan pages、missing crossrefs、data gaps 都纳入 lint（[gist#L100](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f#file-llm-wiki-md-L100)）。Astro 区分 deterministic auto-fix 与 heuristic report（[SKILL.md#L137-L165](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L137-L165)）。我们本地 Lint 已有死链、孤立、索引漂移等规则，但 Agent 深度 prompt 只有 240 字 excerpt（`src-tauri/src/services/lint_service.rs:21`, `:584`），且没有 severity rubric。
4. **导入管线还不像“产品化 ingest”。** nashsu 有 `Persistent ingest queue`（[README#L40](https://github.com/nashsu/llm_wiki/blob/main/README.md#L40)）和崩溃恢复/取消/重试；lucasastorian 强调本地模式中文件 `files stay where they are`（[README#L50](https://github.com/lucasastorian/llmwiki/blob/master/README.md#L50)），MCP/Chrome clipper 让高亮和备注也进入 ingest。我们 URL 获取仍是后端拉 HTML、前端 Readability（`src/components/app/AppShell.tsx:416-428`），只接受 UTF-8（`src-tauri/src/commands/import_commands.rs:581`），确认导入仍是单文件失败导致整批失败（`src-tauri/src/services/import_service.rs:808`）。

一句话：我们已经有正确的桌面壳、文件边界和本地索引基础；下一步应优先补“可验证的编译计划、真实引用闭环、深度 Lint 输入质量、导入韧性、Agent 只读 API/Skill”。

## 1. 调研对象和取舍

| 对象 | 本次阅读重点 | 对我们最有价值的点 |
|---|---|---|
| Karpathy 原始 gist | 三层模式、ingest/query/lint/index/log 的最小模式 | 北极星：wiki 是 `persistent, compounding artifact`（[gist#L77-L80](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f#file-llm-wiki-md-L77-L80)） |
| `nashsu/llm_wiki` | README 的 ingest、graph、retrieval、queue、API/MCP | 产品化最完整：两阶段 ingest、检索预算、图谱多信号、后台队列 |
| `nashsu/llm_wiki_skill` | Skill 的本地 HTTP API、读路径、引用规范 | 最值得借鉴的 Agent-facing 只读接口 |
| `Astro-Han/karpathy-llm-wiki` | 单 Skill 的 ingest/query/lint 操作纪律 | prompt 细则最清晰：合并/新建/冲突/cascade |
| `alirezarezvani/claude-skills` llm-wiki | Skill + slash commands + subagents + scripts | 把 query、lint、graph 分成可执行工作流 |
| `infranodus/skills` skill-llm-wiki | 多阶段 scaffold、ontology/gap analysis | 借鉴 gap analysis，不宜照搬额外目录体系 |
| `lucasastorian/llmwiki` | MCP 工具面、Chrome clipper、nightly routine | 借鉴 MCP/tool surface；SQLite/Postgres 存储不适合直接采用 |
| HN 讨论 | Karpathy 自述运行方式和 skill 触发边界 | 后台循环和“少污染上下文”的 Skill 触发策略 |

没有深挖每个 fork 的全部代码；原因是本任务关注本仓库下一步差距，`nashsu`、Astro、alirez 三类已覆盖产品化 app、精细 Skill、Agent 工作流三个主要方向。

## 2. 本仓库当前基线

### 2.1 产品和架构基线

- 产品层：`SPEC/PRD.md:18` 明确遵循 Karpathy 模式，不是传统临时 RAG；`SPEC/PRD.md:120` 要求 Chat 回答展示引用页面并可保存到 `wiki/queries/`。
- 存储层：`SPEC/PRD.md:400` 和 `SPEC/TECH_STACK.md:33-37` 都明确内容是 Markdown/JSON/本地文件，不引入数据库。
- 搜索层：`SPEC/TECH_STACK.md:320` 要求自然语言问答进入 Chat/Agent/BYOK，普通搜索是本地关键词/筛选。
- 导入层：`SPEC/TECH_STACK.md:217-219` 记录 URL 正文提取使用 Readability.js，OCR/视觉交给后续 Agent/Skill。
- Agent/BYOK：`SPEC/TECH_STACK.md:260-262` 要求 Agent 默认优先，但未配置 Agent 时 BYOK 必须跑通核心流程。

### 2.2 已经补强的地方

- `src-tauri/src/services/compile_service.rs:25` 的 BYOK prompt 已经要求派生页、跨源综合、不能只返回 index、保护 `wiki/sources/`、双来源引用、必须包含 `wiki/index.md`/`overview.md`/`log.md`。
- `src-tauri/src/services/compile_service.rs:330-333` 的 Agent prompt 更进一步要求不要一源一页、不要复制摘要、按概念命名、cascade 更新。
- `src-tauri/templates/skills/wiki-ingest/SKILL.md:12-19` 已经把 `wiki/sources/` 作为权威原文、只能生成派生页写清楚。
- `src-tauri/src/services/wiki_index.rs:12-16` 明确 `WikiIndex` 是内存索引、无数据库、不写项目文件；`src-tauri/src/services/search_service.rs:22-30` 说明本地搜索不会调用 LLM/Agent，并按 mtime+size 缓存刷新。

这意味着后续不应把“加向量库/数据库”作为第一反应。更有价值的是在现有 Markdown + JSON + 内存索引边界内，把 prompt、验证和引用证据做实。

## 3. 编译 Skill / Prompt：差在“计划可验证”和“操作细则”

### 外部最佳实践

Karpathy 的 ingest 描述不是“一次生成几页”，而是读 source、提炼 takeaways、写 summary、更新 index/entity/concept/log，且一个源可能触达 10-15 页（[gist#L96](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f#file-llm-wiki-md-L96)）。这背后的关键是“新资料会改写既有 wiki”，而不是把 source 转成页面。

nashsu 进一步产品化：README 把 ingest 称为 `Two-Step Chain-of-Thought Ingest`，并列出 source traceability、incremental cache（[README#L33-L36](https://github.com/nashsu/llm_wiki/blob/main/README.md#L33-L36)）。后面还写到每个生成页包含 `sources: []`，且有 source summary fallback（[README#L125-L127](https://github.com/nashsu/llm_wiki/blob/main/README.md#L125-L127)）。

Astro-Han 的 Skill 更像“prompt spec”：先取源再编译 `Always both steps, no exceptions`（[SKILL.md#L43-L44](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L43-L44)）；遇到 `Same core thesis` 就合并到已有文章，新概念才新建，跨主题则加 See Also（[SKILL.md#L64-L66](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L64-L66)）；冲突必须带来源标注（[SKILL.md#L68](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L68)）；cascade 更新单独成节（[SKILL.md#L75-L87](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L75-L87)）。

alirez 的 Skill 也把 ingest 明确为更新 10-15 relevant pages、index、log（[SKILL.md#L55-L58](https://github.com/alirezarezvani/claude-skills/blob/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md#L55-L58)），并提供 slash commands、sub-agents、`wiki_search.py`/`graph_analyzer.py` 等辅助脚本（[SKILL.md#L80-L110](https://github.com/alirezarezvani/claude-skills/blob/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md#L80-L110)）。

### 我们的差距

- `src-tauri/templates/skills/wiki-ingest/SKILL.md:14-19` 已经有“不改 sources / 不一源一页 / 只写派生页”，但没有 Astro 那种“同 thesis 合并、全新概念新建、跨主题 See Also、冲突如何标注”的分流规则。
- `src-tauri/src/services/compile_service.rs:24-45` 的 BYOK prompt 和 `:326-333` 的 Agent prompt 不是同一个 instruction source；长期会出现 Agent/BYOK 质量漂移。
- `src-tauri/src/services/compile_service.rs:345-385` 的 `validate_manifest` 主要做路径保护、重复路径、必需文件检查；没有解析每个派生页 frontmatter 的 `sources`、`type`、`> Sources:`，也没有检查 page 是否像 source mirror。
- 目前 manifest 是“模型直接返回 files”，没有 plan 阶段。nashsu 的两阶段 ingest 本质上给了可审查中间物：先决定哪些页面该 merge/create/update，再生成内容。

### 对照结论

编译侧下一步不是再堆一句 prompt，而是把“计划”变成 DTO：每个待触达页面要有 `action: create|update|merge`、`pageType`、`sourceIds`、`affectedExistingPages`、`reason`。UI 可以展示这个 plan，后端可以先验证，再进入生成/应用 manifest。

## 4. Chat / Query：差在“模型实际引用”和“Agent 可读接口”

### 外部最佳实践

Karpathy 原文的 query 是 LLM 搜索页面后综合回答并带引用（[gist#L98](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f#file-llm-wiki-md-L98)），高质量答案还能归档回 wiki。这个闭环的重点是：引用证明回答来自 wiki，而不是搜索结果装饰。

nashsu README 的检索链路比普通 top-k 更完整：关键词、可选向量、graph expansion、budget、context assembly，并把来源组织成 `Numbered pages with full content`（[README#L194-L220](https://github.com/nashsu/llm_wiki/blob/main/README.md#L194-L220)）。同时它的 app 暴露 `Local HTTP API + MCP Server + AI Agent Skill`（[README#L413-L415](https://github.com/nashsu/llm_wiki/blob/main/README.md#L413-L415)）。

nashsu 的 Skill 是最值得我们借鉴的 Agent 边界：API 是 `localhost-only`（[SKILL.md#L75](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L75)），不要泄露 token（[SKILL.md#L91](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L91)），search/read 分步，支持 `includeContent` 避免 N+1（[SKILL.md#L164-L198](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L164-L198)），并要求 `Stay read-only by default`（[SKILL.md#L195](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L195)）。

Astro 和 alirez 的 Skill 都要求 query 先读 index，再 drill into pages（Astro [SKILL.md#L110-L126](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L110-L126)，alirez [SKILL.md#L57-L58](https://github.com/alirezarezvani/claude-skills/blob/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md#L57-L58)）。lucasastorian 则把 Claude/Codex 通过 MCP 连接到一组 deliberate tools（[README#L123-L136](https://github.com/lucasastorian/llmwiki/blob/master/README.md#L123-L136)）。

### 我们的差距

- `src-tauri/src/services/chat_service.rs:17-19` 固定 `RETRIEVAL_LIMIT = 6`、`EXCERPT_CHARS = 1200`、`HISTORY_TURNS = 8`，还没有 provider context-window aware 的 token budget。
- `src-tauri/src/services/chat_service.rs:259-264` 同一个 prompt 同时服务 Agent 和 BYOK：“Agent 可读文件系统；如果不能访问文件系统则只用 context”。这对 BYOK 来说太含糊；对 Agent 来说也没有 index-first/read-more 的专用策略。
- `src-tauri/src/commands/chat_commands.rs:167` 在调用模型前 clone `retrieval.citations`；`src-tauri/src/services/chat_service.rs:333-363` 保存 query markdown 时使用 answer.citations。模型是否真的用了这些页面，目前没有结构化证据。
- 当前没有 `wiki-chat` / `wiki-query` Skill。已有 `wiki-ingest` 和 `wiki-lint`，但 Chat 的 Agent 行为只靠服务层 prompt。
- 当前没有本地只读 API/MCP surface 给外部 Agent 用。nashsu/lucas 的一个共同经验是：Agent 不一定要直接读裸文件；它可以通过受控工具 search/read/graph/rescan。

### 对照结论

Chat 的优先级最高问题是 citation provenance。应把 sources 编号传给模型，要求答案中使用 `[S1]`/`[S2]` 或 JSON citations，再解析模型实际引用的 source ids；保存到 `wiki/queries/` 的 sources 应来自解析结果，而不是检索 top-k。

## 5. Lint / Schema：差在“深度输入质量”和“规则分层”

### 外部最佳实践

Karpathy 的 lint 包括 contradictions、stale claims、orphan pages、missing crossrefs、data gaps（[gist#L100](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f#file-llm-wiki-md-L100)）。Astro-Han 把 lint 明确拆成 deterministic checks 和 heuristic checks：前者可 auto-fix，后者 report only（[SKILL.md#L137-L165](https://github.com/Astro-Han/karpathy-llm-wiki/blob/main/SKILL.md#L137-L165)）。这和我们“本地快速 Lint + Agent 深度 Lint”的产品方向一致。

InfraNodus 的 Skill 走得更重：它要求 ontology/gap analysis，且 ontology 文件 `NEVER regenerate ontology files from scratch`（[SKILL.md#L179-L231](https://github.com/infranodus/skills/blob/master/skill-llm-wiki/SKILL.md#L179-L231)）。这套方法不宜直接照搬，但“累积型图谱/本体 + gap todo”可作为后续高级 Lint 的设计参考。

lucasastorian 的 MCP 工具里，`search` 可以查 citation graph、stale/uncited pages，`lint` 做 deterministic hygiene checks（[README#L123-L136](https://github.com/lucasastorian/llmwiki/blob/master/README.md#L123-L136)）。这说明 Lint 不一定只在“修复模式”里出现，也可以成为日常检索/维护的一部分。

### 我们的差距

- `src-tauri/templates/skills/wiki-lint/SKILL.md:12-17` 已列出 duplicate_topic、missing_source、schema_mismatch、outdated_content、contradiction，但 `:21` 只说 severity 是 error/warning/info，没有分级标准。
- `src-tauri/src/services/lint_service.rs:81-230` 本地规则已有 DeadLink、MissingFrontmatter、MissingResource、OrphanPage、DuplicateFilename、PathCase；`:514-515` 还有 IndexDrift error。这块不错。
- `src-tauri/src/services/lint_service.rs:547-554` 深度 Lint prompt 也列了 issue types，但 `:584` 每页只给 240 字 excerpt。对 contradiction/outdated/schema_mismatch 来说，这几乎不够。
- `src-tauri/src/services/lint_service.rs:628` 直接信任 Agent 返回 severity；只有本地规则有 `:1609-1664` 的 severity 测试。
- `src-tauri/src/models/lint.rs:29-46` 里 SchemaMismatch 等主要属于 Agent deep-lint，当前没有本地解析 `schema.md` 后检查 `type`、必需 section、sources frontmatter 的规则。

### 对照结论

Lint 已有“形”，缺的是“证据输入”和“分级契约”。短期应把 deep lint excerpt 提高到 800-1200 或按页预算提供关键 sections；中期把 `schema.md` 解析成本地可检查规则，至少验证 frontmatter/type/sources/index entry。

## 6. 导入管线：差在“网页提取、队列韧性、部分成功”

### 外部最佳实践

nashsu 在产品层补齐了 ingest 工程能力：队列可持久化、崩溃恢复、取消、重试、进度可视（[README#L120-L127](https://github.com/nashsu/llm_wiki/blob/main/README.md#L120-L127)）。这不是炫技，而是 LLM Wiki 导入会长期运行且失败源很多。

lucasastorian 强调 local mode 不移动/上传原文件，`.llmwiki/` 是可重建 derived layer，`safe to delete`（[README#L111-L117](https://github.com/lucasastorian/llmwiki/blob/master/README.md#L111-L117)）。它还通过 Chrome extension 捕获网页/PDF、高亮和评论，再交给 nightly routine（[README#L72-L80](https://github.com/lucasastorian/llmwiki/blob/master/README.md#L72-L80)）。

InfraNodus 的 Skill 把 acquire 和 process 严格分开：Phase 8/9 是 `TWO DIFFERENT OPERATIONS`，一个只碰 raw，一个只读 raw 写 wiki（[SKILL.md#L49-L52](https://github.com/infranodus/skills/blob/master/skill-llm-wiki/SKILL.md#L49-L52)）。这和我们 raw immutable 的安全边界一致。

HN 里 Karpathy 提到自己的运行方式是长会话 `once an hour` 触发 subagent，且没有把 skill 常驻以保持 `context clean and scoped`（[HN item](https://news.ycombinator.com/item?id=47963913)）。这对我们的后台任务和 Skill 触发策略有启发：长任务必须可取消、可见、可恢复，但 prompt 上下文要窄。

### 我们的差距

- URL 导入：`src-tauri/src/commands/import_commands.rs:489-581` 负责 fetch HTML，但 `:535` 禁用 redirect，`:581` 只用 `String::from_utf8`，遇到 GBK/Shift-JIS 等站点会失败。
- Readability 运行在 React：`src/components/app/AppShell.tsx:416-428` 在 URL import path 动态导入 `src/lib/readability.ts`。这和 `SPEC/TECH_STACK.md:59` “React 不承担大量文件系统/后端逻辑”方向不完全一致；解析和元数据归档更适合移到 Rust command/service。
- `src-tauri/src/services/import_service.rs:682-808` 的 `confirm_import` 在验证每个 entry 时调用 `file_hash_fast(source)?`；源文件变动/缺失会让整批失败。测试 `:1753-1754` 甚至固定了“batch must fail before copying any source”的行为。对真实导入队列来说，这会把一个坏源扩大成整批失败。
- `.app/import-conflicts.json` 在 `src-tauri/src/commands/import_commands.rs:609` 由本次确认结果整体写入；长期可能覆盖旧冲突上下文。

### 对照结论

导入侧不应先追求 OCR/视觉，而应先做网页编码、redirect、Content-Type、部分成功、冲突日志合并和后台队列恢复。外部项目证明，ingest 的“可靠工程”会直接决定 wiki 质量。

## 7. Graph / Search / Index：我们不应盲目追向量库

nashsu 有 4-signal graph：direct links、source overlap、Adamic-Adar、type affinity（[README#L139-L146](https://github.com/nashsu/llm_wiki/blob/main/README.md#L139-L146)），也支持可选 LanceDB vector search（[README#L194-L225](https://github.com/nashsu/llm_wiki/blob/main/README.md#L194-L225)）。这对 recall 有价值，但它引入向量索引/数据库心智。

本仓库的边界是 `SPEC/PRD.md:400` 不引入数据库，`src-tauri/src/services/wiki_index.rs:12-16` 明确无数据库、内存索引、不写项目文件。这个选择是合理的。当前更该补的是：

- search/retrieval 的 token budget 和 graph expansion，而不是先引入持久向量库。
- graph edge 的 evidence/provenance，让 UI 能解释“为什么这两个页面相关”，而不是只画线。
- 如果未来做 embeddings，应作为可删除、可重建的 `.app/` derived cache，并且需要用户同意和清晰禁用路径。

## 8. Agent API / Skill 生态：我们缺一个只读工具面

nashsu 和 lucas 的共同点不是“用了 MCP”本身，而是把 Agent 能力收进小工具面：health、search、read、graph、lint、rescan；默认只读，写操作需要更强语义或另一路径。nashsu Skill 明确 search 返回 ranked hits（[SKILL.md#L98](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L98)），graph 有 endpoint（[SKILL.md#L149](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L149)），而 `/chat` 在该版本未实现，Skill 反而要求 Agent 自己基于 search/read 综合（[SKILL.md#L167](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md#L167)）。

我们的 Agent 路径目前是 CLI 直接进 workspace：

- Compile: `src-tauri/src/services/agent_service.rs:150-218` 通过 Claude/Codex 执行，写入受 temp workspace 和 manifest validation 约束。
- Chat: `src-tauri/src/services/agent_service.rs:257-296` 是只读工具/沙箱方向，Claude 只允许 Read/Grep/Glob，Codex 用 read-only sandbox。这是好基础。

但外部 Agent 如果想“问这个 app 的 wiki”，目前没有受控 API/Skill，只能读裸文件或依赖我们内置 Chat。下一步可考虑一个 token-protected localhost read-only API，先提供 health/projects/search/read/graph/lint summary，后续再映射成 MCP。

## 9. 具体差距表

| 模块 | 外部最佳实践 | 本仓库当前 file:line | 差距 | 优先级 |
|---|---|---|---|---|
| Compile prompt | 两阶段 ingest，先 plan 后写页；merge/create/update 有规则 | `src-tauri/src/services/compile_service.rs:24-45`, `:326-333` | Prompt 已强，但仍一次性 manifest，缺少可校验 plan | P0 |
| Compile validation | 每页 source traceability、schema/type、index/log 一致 | `src-tauri/src/services/compile_service.rs:345-385` | 只做路径/必需文件/重复路径，没做语义验证 | P0 |
| Ingest Skill | 合并同 thesis、新概念新建、冲突来源标注、cascade | `src-tauri/templates/skills/wiki-ingest/SKILL.md:14-38` | 有硬边界，缺操作分流和冲突规则 | P1 |
| Chat citations | 模型实际引用 source ids，保存实际 citations | `src-tauri/src/commands/chat_commands.rs:167`, `src-tauri/src/services/chat_service.rs:333-363` | 现在保存检索命中，不是模型证据 | P0 |
| Chat retrieval | index-first、read-more、graph expansion、token budget | `src-tauri/src/services/chat_service.rs:17-19`, `:259-294` | 固定 top-6 excerpt，同 prompt 混 Agent/BYOK | P0 |
| Query Skill/API | localhost read-only API/MCP，Agent search/read 后回答 | 无对应 API/MCP；Chat 走内部 Tauri command | 外部 Agent 不能通过受控工具访问 wiki | P1 |
| Deep Lint | deterministic vs heuristic 分层，Agent 看足够上下文 | `src-tauri/src/services/lint_service.rs:21`, `:547-584` | issue types 有，但 excerpt 太短、无 severity rubric | P0 |
| Schema Lint | 本地解析 schema/frontmatter/type/sources | `src-tauri/src/models/lint.rs:29-46`, `src-tauri/templates/skills/wiki-lint/SKILL.md:15` | schema_mismatch 主要依赖 Agent 判断 | P1 |
| URL import | 编码、redirect、metadata、可恢复队列 | `src-tauri/src/commands/import_commands.rs:535`, `:581`, `src/components/app/AppShell.tsx:416-428` | UTF-8 only、无 redirect、Readability 在前端 | P0 |
| Confirm import | 单源失败不应扩大成整批失败 | `src-tauri/src/services/import_service.rs:682-808` | 当前整批失败；测试固化了该行为 | P1 |

## 10. 可落地建议（按优先级）

1. **把编译指令抽成单一来源。** 新建共享的 compile instruction builder，让 `wiki-ingest`、Agent prompt、BYOK prompt 覆盖同一组关键条款：不写 sources、不一源一页、派生页、双来源引用、cascade、merge/create/update 分流、冲突标注。给关键句加单元测试，避免后续漂移。对应：`src-tauri/src/services/compile_service.rs:24-45`, `:326-333`, `src-tauri/templates/skills/wiki-ingest/SKILL.md:14-38`。
2. **引入 CompilePlan 阶段。** 在生成 manifest 前先返回结构化 plan：`action`、`path`、`pageType`、`sourceIds`、`affectedExistingPages`、`reason`。UI 可展示，后端可验证，失败时不进入写入阶段。对应：`src-tauri/src/services/compile_service.rs:24-45`, `:345-385`。
3. **增强 `validate_manifest` 为语义验证。** 解析每个派生页 frontmatter：`sources` 非空且引用存在；`type` 符合 schema；必须有 `> Sources:` 或等价来源段；禁止 source mirror；`index.md`/`overview.md` 必须链接新增/更新页。对应：`src-tauri/src/services/compile_service.rs:345-385`。
4. **拆分 Chat 的 Agent/BYOK prompt，并建立实际引用解析。** BYOK prompt 明确“只能用给定 sources”；Agent prompt 明确“先读 index，sources 不足时 read wiki”。sources 编号传入，答案必须引用编号或返回 JSON citations；保存 query 时只保存模型实际引用。对应：`src-tauri/src/services/chat_service.rs:259-294`, `:333-363`, `src-tauri/src/commands/chat_commands.rs:167`。
5. **升级 Chat retrieval。** 保持本地关键词搜索边界，但增加 index-first、top pages full-content-with-budget、graph expansion、provider context window 预算；不要只固定 top-6/1200 字。对应：`src-tauri/src/services/chat_service.rs:17-19`, `src-tauri/src/services/search_service.rs:632-657`。
6. **给 Chat 补一个 `wiki-query` Skill。** 参考 `wiki-ingest` / `wiki-lint` 写一个只读 Skill：何时触发、先读 index、如何引用 path、何时保存到 `wiki/queries/`、不改 sources。短期服务内置 Agent；长期可映射到 localhost API/MCP。对应：`src-tauri/templates/skills/wiki-ingest/SKILL.md`, `src-tauri/templates/skills/wiki-lint/SKILL.md`。
7. **把 deep lint 输入做厚，并补 severity rubric。** excerpt 提到 800-1200 或按页面/section 动态预算；prompt/Skill 定义 error/warning/info 标准；本地先产出 deterministic baseline，再让 Agent 只做 heuristic。对应：`src-tauri/src/services/lint_service.rs:21`, `:547-584`, `src-tauri/templates/skills/wiki-lint/SKILL.md:21`。
8. **把基础 schema/source lint 本地化。** 先不用完整 DSL，先解析 `schema.md` 的 page type/必需 section 约定，检查 `sources` frontmatter、来源文件存在、index entry 漂移。对应：`src-tauri/src/models/lint.rs:29-46`, `src-tauri/src/services/lint_service.rs:81-230`。
9. **修 URL 导入的工程韧性。** 后端支持有限 redirect、Content-Type/meta charset、`encoding_rs` 解码；Readability 或等价 Markdown 化尽量移到 Tauri service/command 侧，前端只展示预览/确认。对应：`src-tauri/src/commands/import_commands.rs:489-581`, `src/components/app/AppShell.tsx:416-428`, `src/lib/readability.ts:31-90`。
10. **把确认导入从 all-or-nothing 改成 partial success。** 每个 entry 独立校验/复制/记录结果；失败项留在 preview/conflicts，成功项不回滚；`.app/import-conflicts.json` 合并写入而不是整批替换。对应：`src-tauri/src/services/import_service.rs:682-808`, `src-tauri/src/commands/import_commands.rs:597-609`。

## 11. 暂不建议照搬的点

- **不要现在引入 LanceDB/SQLite 作为 wiki 内容依赖。** nashsu/lucas 的索引能力强，但本项目硬边界是 Markdown + JSON + local files；如果未来做 embeddings，应是 `.app/` 下可删可重建缓存。
- **不要照搬 lucas 的 hosted Postgres/S3 或 VaultFS 架构。** 它适合 remote/local 双模式，本项目目前重点是 Tauri 本地桌面。
- **不要照搬 InfraNodus 的 `infranodus/` ontology 目录。** 可借鉴 gap analysis，但新增长期工件需要产品确认，避免污染 Karpathy 三层模型。
- **不要让普通 Search 变成自然语言问答。** `SPEC/TECH_STACK.md:320` 已经规定语义问答进入 Chat/Agent/BYOK，这条边界应保留。

## 12. 源链接

- Karpathy LLM Wiki gist: https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f
- HN 讨论: https://news.ycombinator.com/item?id=47963913
- nashsu/llm_wiki: https://github.com/nashsu/llm_wiki
- nashsu/llm_wiki_skill: https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md
- Astro-Han/karpathy-llm-wiki: https://github.com/Astro-Han/karpathy-llm-wiki
- alirezarezvani/claude-skills llm-wiki: https://github.com/alirezarezvani/claude-skills/blob/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md
- InfraNodus skill-llm-wiki: https://github.com/infranodus/skills/blob/master/skill-llm-wiki/SKILL.md
- lucasastorian/llmwiki: https://github.com/lucasastorian/llmwiki
