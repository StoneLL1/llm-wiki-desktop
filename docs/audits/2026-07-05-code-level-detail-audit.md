# LLM Wiki Desktop 代码级对抗式审查

日期：2026-07-05
分支：`task1-backend-contracts`
方法：5 个对抗式子代理并行审查（提示词/Lint/Chat-skill、UI 微观、导入管线、性能/屎山、Karpathy 对照）+ WebSearch 校验，所有发现要求带 `file:line` 证据。
与 [2026-07-05-adversarial-product-audit.md](./2026-07-05-adversarial-product-audit.md) 互补：前者偏产品策略与第一性原理，本次聚焦**代码细节**——提示词硬度、Lint 覆盖、Chat 是否该走 Skill、UI 滚动/对齐/溢出、导入管线缺陷、性能与屎山。既有审计的 12 条产品策略发现本次不重复。

## 严重度定义（沿用前次）

- **P0**：阻断用户闭环、破坏信任或可能导致数据/隐私风险。
- **P1**：明显降低核心体验，用户会误解产品或放弃继续使用。
- **P2**：细节不顺、维护风险或规模增长后会放大。

## 执行摘要

### 各板块成熟度评分

| 板块 | 评分 | 关键短板 |
|---|---|---|
| 编译提示词 / Lint / Chat Skill | **5/10** | BYOK 与 Agent 提示词严重不对等；无 few-shot/schema 校验；citations 是检索透传 |
| 导入与解析管线 | **6/10** | URL Readability 在前端违反 spec；charset 缺失；单文件失败整批回滚 |
| 性能 | **4/10** | 无内存索引、每次全量读盘+SHA-256；同步 IO 命令阻塞 IPC 线程 |
| 代码健康 | **7/10** | 架构清晰、错误模型统一、测试覆盖好；7 处生产 panic + 11 处静默 catch 拉低 |
| UI 完成度 | **7.5/10** | token/i18n/a11y 扎实；graph 硬编码颜色、几处列表/详情溢出结构没想清楚 |

### 三大系统级问题（跨板块）

1. **BYOK 与 Agent 路径在提示词、能力、UI 表达上严重不对等**
   编译：`compile_prompt`（Agent 路径）有 6 步骤详细指令，`provider_prompt`（BYOK 路径）只有 1 行 JSON 约束（[compile_service.rs:24-48](../../src-tauri/src/services/compile_service.rs)）。Chat：Agent 能 `Read Grep Glob` 整个 `wiki/`，BYOK 只能看 6 条 × 1200 字符检索片段，但提示词对两者都说"current working directory is the project root: you may read Markdown files under wiki/"——对 BYOK 模型是事实错误（[chat_service.rs:258-265](../../src-tauri/src/services/chat_service.rs)）。用户对同一份资料在不同机器上得到完全不同结构的 wiki，且无法归因。

2. **后端无内存索引 + 同步 IO 阻塞 IPC**
   `scan_wiki` / `search` / `get_graph` / `compile` 每个入口都对全部 wiki 文件做全量 `fs::read` + SHA-256 + frontmatter 解析（[search_service.rs:32-56](../../src-tauri/src/services/search_service.rs)、[file_store.rs:151,238](../../src-tauri/src/services/file_store.rs)），且这些 Tauri 命令是同步的、跑在 IPC 主线程，没 `spawn_blocking`（[git_commands.rs:33-70](../../src-tauri/src/commands/git_commands.rs)、[graph_commands.rs:13](../../src-tauri/src/commands/graph_commands.rs)）。500 页样本库每次打开项目/图谱/搜索/编译都长时间无响应。

3. **Chat 引用是检索 Top-N 透传，无 claim 级对齐核验**
   `citations` 在调用模型**之前**就被克隆为最终 assistant message 的 citations（[chat_service.rs:188-229](../../src-tauri/src/services/chat_service.rs)、[chat_commands.rs:159-167](../../src-tauri/src/commands/chat_commands.rs)）。无论模型答案里实际引用了什么，UI 永远显示检索 Top-6。这违背 Karpathy LLM Wiki 模式的"可信度"核心。

### P0/P1 数量

P0 共 **11 条**（提示词 4 + 导入 4 + 性能 2 + 屎山 1），P1 共 **20+ 条**。详见下文。

---

## 一、编译提示词 / Lint / Chat Skill（5/10，最严重板块）

### P0-1. 编译 BYOK 提示词与 Agent 提示词完全不对等

**现状**：两条路径构造的提示词深度差异巨大。

- Agent 路径 `CompileService::compile_prompt`（[compile_service.rs:325-343](../../src-tauri/src/services/compile_service.rs)）：6 步骤自然语言指令（命名约定、cascade 更新、log 追加、"do NOT write one page per source"、wikilinks 规范、workspace 边界警告）。
- BYOK 路径 `CompileService::provider_prompt`（[compile_service.rs:24-48](../../src-tauri/src/services/compile_service.rs)）：开头一行 JSON 格式约束 + "Create real DERIVED content pages..." 一句话，**完全省略**了 6 步骤说明。然后把 `purpose.md` / `schema.md` / 所有 `raw/extracted/*.md` / 整个 `wiki/**/*.md` 一次性塞进 user message。

```rust
// compile_service.rs:25 (BYOK)
let mut prompt = String::from("Return only JSON matching {files:[{path,content}],deletions:[],summary}.\nCreate real DERIVED content pages under wiki/ ... Do NOT return only the index files.");
```

**用户风险**：BYOK 用户编译的 wiki 极可能：(a) 每个源生成一页而不是跨源综合页；(b) 漏掉 `wiki/index.md` / `overview.md` / `log.md` 更新；(c) 不做 cascade 更新。用户会判定"产品不稳定"，实际只是提示词漏写。

**建议**：把 6 步骤说明提取成共享常量 `COMPILE_INSTRUCTIONS`，两条路径都注入；差异只在"workspace 文件如何获取"。加集成测试断言 BYOK prompt `contains("Do NOT write one page per source")` / `contains("Cascade")`。

### P0-2. Chat Agent 与 BYOK 能力差一个数量级，但 UI 不告知且提示词事实错误

**现状**：`ChatService::assemble_prompt`（[chat_service.rs:249-314](../../src-tauri/src/services/chat_service.rs)）对 Agent 与 BYOK 用同一份提示词字符串：

```rust
// chat_service.rs:258-265
"...When this prompt is executed by an Agent, the current working directory is the project root: you may read Markdown files under wiki/ to answer. If the keyword Sources section is empty or insufficient, inspect wiki/ before saying the context is insufficient. If you cannot access the filesystem, answer using only the provided context..."
```

- Agent 路径（[chat_commands.rs:191-221](../../src-tauri/src/commands/chat_commands.rs)）：Claude/Codex 真有 `--allowedTools=Read Grep Glob`，cwd 是项目根。
- BYOK 路径（[chat_commands.rs:223-270](../../src-tauri/src/commands/chat_commands.rs)）：同一提示词发给 `LlmService::complete_streaming`，模型**没有任何工具调用能力**，只能看 `RETRIEVAL_LIMIT=6` × `EXCERPT_CHARS=1200` 字符（[chat_service.rs:17-19](../../src-tauri/src/services/chat_service.rs)）。

**用户风险**：用户问"所有提到 transformer 架构的页面有哪些共同点？"，Agent 路径 grep 全库给出完整答案，BYOK 路径基于 7200 字符片段硬答或说"上下文不足"。UI 把两者都标成"Chat 回答"。第 2-3 句对 BYOK 模型是事实错误——模型会以为自己有 fs 访问。

**建议**（推荐度排序）：
1. 把 Chat 也封装成 `wiki-chat` skill（参考 `wiki-ingest` / `wiki-lint`），Agent 路径执行 skill。BYOK 路径在提示词里**明确声明**"你只能看下方 Sources 区段，无文件系统访问"，并删除"current working directory is the project root"那句。
2. 至少在 BYOK 提示词顶部加 `CAPABILITIES: You have NO filesystem or tool access. Answer ONLY from the Sources section below.`，"current working directory..."只在 Agent 路径注入。
3. UI 层标注"BYOK 模式仅基于检索片段回答，可能不如 Agent 模式全面"。

### P0-3. 编译提示词无 few-shot / 输出 schema 校验 / 自检，依赖"一锤子"生成

**现状**：
- 无 few-shot：没有"好的 derived page 长什么样"的 markdown 样本。
- 无 frontmatter schema 强约束：只说"a frontmatter `sources: [...]` array"，没给 `type` / `title` / `tags` 枚举。`schema.md` 被原文塞进 prompt，但用户写的 schema 质量参差，应用层无兜底。
- 无 chain-of-thought / 分阶段：提取 → 分类 → 综合 → 写入 → 更新 index 全压在一次调用。
- 输出格式强制力弱：`parse_manifest`（[compile_service.rs:186-209](../../src-tauri/src/services/compile_service.rs)）容忍 ```json fence 也容忍裸 `{...}`；`validate_manifest`（[compile_service.rs:345-385](../../src-tauri/src/services/compile_service.rs)）只查路径安全和三个核心页存在，**不查** frontmatter `sources` 字段、**不查** `> Sources:` 行。
- 温度不一致：BYOK 路径 OpenAI/Custom 用 `temperature: 0`（[llm_service.rs:96](../../src-tauri/src/services/llm_service.rs)），但 Anthropic（`:106-110`）和 Google（`:113-118`）**没有** temperature 字段。Anthropic 默认 1.0，对"严格输出 JSON"偏高。

**用户风险**：模型生成"看起来像 wiki 但 frontmatter 缺 sources"的页面 → 图谱 join key 失效（`sources` 是 machine join key），节点孤立；Anthropic BYOK 用户偶尔得到带 fence 的 JSON，`parse_manifest` 容错切片若遇 `content` 字段含 `}` 会截断在错误位置，产生**静默数据损坏**。

**建议**：
1. BYOK 提示词加 1-2 个 few-shot：一个 `wiki/concepts/X.md`（正确 frontmatter + `> Sources:` 行 + wikilink），一个 `wiki/index.md` 片段。
2. `validate_manifest` 加语义校验：对每个 `wiki/concepts|entities|synthesis|comparisons/*.md`，解析 frontmatter 检查 `sources` 非空数组，检查正文含 `> Sources:` 行；不通过返回 `COMPILE_OUTPUT_SCHEMA_INVALID` 带 path。
3. 给 Anthropic/Google 路径设 `temperature: 0`。
4. 把编译拆两阶段：先输出"页面计划"（path + one-line summary + source citations），再逐页生成；失败可只重试某一页。

### P0-4. Chat citations 是检索 Top-N 透传，无 claim 级对齐核验

**现状**：`build_retrieval_context`（[chat_service.rs:188-229](../../src-tauri/src/services/chat_service.rs)）：

```rust
let citations: Vec<ChatCitation> = hits.iter().map(|hit| ChatCitation { ... }).collect();
```

`citations` 在调用模型**之前**就被克隆下来作为最终 assistant message 的 citations（[chat_commands.rs:159-167, 282-292](../../src-tauri/src/commands/chat_commands.rs)）。无论模型答案实际引用了什么，UI 显示的引用永远是"检索 Top-6 命中"。

**用户风险**：模型幻觉"根据 wiki/concepts/agent.md..."但 agent.md 不在检索结果——点开发现引用页里没这段话；模型答"上下文不足"但 UI 仍显示 6 个引用，用户以为模型偷懒；检索到正确页面但模型没用，UI 还是显示这条引用。这正是 Karpathy 模式最致命的失败模式。

**建议**：
1. 短期：`assemble_prompt` 给每条 hit 编号（`[1] wiki/concepts/agent.md`），要求模型用 `[n]` 标注每段 claim 来源；模型返回后解析 `[n]`，只把**实际引用**的页面作为 `citations`。
2. 中期：prompt 加"如果 claim 无法从 Sources 找到依据，标注 [unverified]"，UI 渲染警告标签。
3. `build_answer_markdown`（[chat_service.rs:319-367](../../src-tauri/src/services/chat_service.rs)）保存到 `wiki/queries/` 时，frontmatter `sources` 应来自模型实际引用而非检索 Top-6。

### P1-1. wiki-lint SKILL 对 severity 分级无标准

**现状**：[wiki-lint/SKILL.md:21](../../src-tauri/templates/skills/wiki-lint/SKILL.md) 只写 `severity — error, warning, or info`，无判定标准。`lint_service.rs:547-555` 的 `build_deep_lint_prompt` 重复这段。`parse_agent_issues`（`:609-641`）直接透传模型 severity。对比本地 lint 有明确分级（dead link / index drift = Error，missing frontmatter / duplicate filename = Warning，orphan = Info，测试 `severity_grading_marks_dead_link_and_index_drift_as_error` `:1609-1664`），Agent deep lint 没有对应规则。

**风险**：同一"重复主题"这次标 error 下次标 warning，summary card 计数来回跳，用户失去信任；模型倾向把所有问题标 error（"安全"选择）。

**建议**：SKILL.md 和 `build_deep_lint_prompt` 加分级标准，例如：`error`=事实错误/矛盾/与 schema.md 严重不符；`warning`=结构问题（重复主题、弱关联、缺源）；`info`=风格建议。每个 issue 在 message 里解释为什么选这个 severity。

### P1-2. deep lint 每页摘要只给 240 字符，漏判率高

**现状**：`DEEP_LINT_EXCERPT_CHARS = 240`（[lint_service.rs:21](../../src-tauri/src/services/lint_service.rs)），`build_deep_lint_prompt`（`:583-589`）对每页只取 body 前 240 字符。SKILL.md 同时要求"Do not invent page paths that were not provided"。

**风险**：240 字符 ≈ 一段摘要。模型要判断"两页是否重复主题"或"是否矛盾"只能基于摘要片段，漏判率极高；UI 显示"deep lint 通过"让用户以为整库无重复主题。

**建议**：`DEEP_LINT_EXCERPT_CHARS` 提到 800-1200（与 chat 检索的 1200 对齐）；对 `duplicate_topic` / `contradiction` 类判断，把"潜在冲突页面组"全文一起塞进上下文（用本地 lint 的 `duplicate_filename` 结果作 hint）；或 deep lint 走 Agent 路径时让 Agent 自己 `cat` 文件（`lint_invocation` Claude profile 当前没 allowedTools 限制，prompt 鼓励它主动读）。

### P1-3. 本地 Lint 漏检 PRD 承诺的"schema 不一致"

**现状**：PRD-LINT-002 把 schema 校验划到 Agent deep lint（SKILL.md `schema_mismatch`），但**本地 lint 完全不校验 frontmatter `type` 字段是否符合 schema.md 约定**。`wiki/concepts/foo.md` 写 `type: entity` 本地 lint 不报；`wiki/sources/bar.md` 缺 `sources:` 字段本地不报（`MissingResource` 只检查已有 `sources` 字段的目标存在性，[lint_service.rs:44-279](../../src-tauri/src/services/lint_service.rs)）。

**风险**：用户从 Obsidian 手改了 type 字段，本地 lint 全绿，图谱分类错乱，导出 html-concept-map 时把 entity 当 concept 渲染。

**建议**：`run_local_lint` 加 `SchemaTypeMismatch` 规则：解析 schema.md（或硬编码 enum: concept/entity/source/synthesis/comparison/query）；对每页 frontmatter `type` 做 enum 校验；对 `wiki/sources/*.md` 检查 `type: source` 且有非空 `sources:` 数组。

### P1-4. 编译/Chat 提示词无"源文本-综合页混淆"硬约束

**现状**：[wiki-ingest/SKILL.md:17-19](../../src-tauri/templates/skills/wiki-ingest/SKILL.md) 与 `compile_prompt`（[compile_service.rs:330](../../src-tauri/src/services/compile_service.rs)）都说"Do NOT write one page per source"。但 `validate_manifest` **不校验**生成页是否与 `wiki/sources/*.md` 内容高度相似；SKILL.md 没定义"什么是 derived content"的反例。

**风险**：用户导入 10 个源，编译出 10 个"概念页"，每个 90% 是原文 + 一句"总结"——违反 Karpathy 核心模式，但产品无感知。Lint 也不查（`duplicate_topic` 是查"两页讲同一主题"，不是查"综合页是源的拷贝"）。

**建议**：`validate_manifest` 加软校验：对每个生成的 `wiki/concepts|entities|synthesis|comparisons/*.md`，计算其与每个 `wiki/sources/*.md` 的 Jaccard/shingling 相似度，>0.7 返回 `COMPILE_OUTPUT_SOURCE_MIRROR` 警告（不阻断，写 `wiki/log.md` 并 UI 提示）。SKILL.md 加反例：BAD=`wiki/concepts/source-1-summary.md` 是源的逐段翻译；GOOD=`wiki/concepts/transformer-architecture.md` 综合了 3 个源的对比。

### P1-5. deep lint 提示词不传本地 baseline，Agent 重复劳动

**现状**：`build_deep_lint_prompt`（[lint_service.rs:532-592](../../src-tauri/src/services/lint_service.rs)）完全独立于 `run_local_lint` 输出。本地 lint 报告的死链、孤立页、缺 frontmatter 不传给 Agent。

**风险**：用户修了死链再跑 deep lint，Agent 又报告一遍衍生问题；Agent 不知道已知 orphan 页，无法聚焦语义问题。

**建议**：prompt 追加 `--- Local lint baseline ---` 列出本地 issue `(path, issueType)`，指示 Agent "these are already known; focus on semantic issues not covered by local lint"。

### P1-6. Chat history 取最后 8 turns，无 token 预算裁剪

**现状**：`HISTORY_TURNS = 8`（[chat_service.rs:19](../../src-tauri/src/services/chat_service.rs)），`assemble_prompt`（`:290-309`）直接拼 8 turns 进 prompt，无 token 计数、无按 `LlmProviderConfig.context_window`（[llm_service.rs:107](../../src-tauri/src/services/llm_service.rs)，仅用于 Anthropic `max_tokens`）裁剪。

**风险**：长答案（2-3k tokens）× 8 turns + 6 个 1200 字符摘录 ≈ 20k+ tokens，遇到 8k context window 的本地 Ollama 直接 422/截断；BYOK 无重试（`complete` 一次 POST，[llm_service.rs:215-269](../../src-tauri/src/services/llm_service.rs)），用户看到"provider request failed"完全不知道是上下文太长。

**建议**：`assemble_prompt` 加 token 估算（粗略 字符数/4），按 `provider.context_window` 60% 预算从最旧开始丢 history；`SendChatMessageRequest` 带上 `provider.context_window`。

### P2（提示词板块）

- **P2-1** `wiki-ingest/SKILL.md` 没对 PDF/DOCX/URL/Markdown 等格式差异化指导（PDF 丢表格结构、URL 含广告残留），模型可能把噪声当正文。
- **P2-2** Chat prompt 说"do not modify files"（[chat_service.rs:264](../../src-tauri/src/services/chat_service.rs)）与 `chat_convenience_invocation` 给 Agent `Edit Write Bash`（[agent_service.rs:304-347](../../src-tauri/src/services/agent_service.rs)）矛盾，convenience 模式下模型可能拒绝编辑。convenience 路径应移除该句或用独立 `assemble_convenience_prompt`。
- **P2-3** `extract_json_block` 裸 `[...]` fallback 在弱模型（Ollama）下静默漏报（[lint_service.rs:1372-1393](../../src-tauri/src/services/lint_service.rs)）；返回 None 但原文含 issueType 关键词时应回 warning。
- **P2-4** `compile_prompt` 第 6 步"Never delete pages"与 manifest `deletions` 字段矛盾，应改"Do not delete unless schema.md explicitly requires consolidation; record rationale in wiki/log.md"。
- **P2-5** dead-link fix 只能删链接，无"创建占位页"/"改 alias"选项（[lint_service.rs:710-736, 1264-1282](../../src-tauri/src/services/lint_service.rs)）。
- **P2-6** `classify_chat_intent` 关键词匹配对英文长句易误判（[chat_convenience_service.rs:89-120](../../src-tauri/src/services/chat_convenience_service.rs)），"Could you save this?" 以 Could 开头判 ReadOnly。

---

## 二、导入与解析管线（6/10）

### 管线图

```
React ImportView → invoke("preview_import"|"fetch_import_url"|"preview_text_import")
  → import_commands.rs (Tauri 薄层)
    → collect_source_paths (递归遍历，跳 symlink)
    → extraction_service.extract_batch (PDF/DOCX/PPTX/XLSX/CSV/HTML/MD/TXT/Image)
    → import_service.preview_import (SHA-256 去重、命名碰撞)
    → 写 .app/import-previews/{task_id}.json
  → confirm_import_preview → import_service.confirm_import:
      validate → fs::copy 到 raw/sources/ → promote_extracted_to_sources
      → record_confirmed_sources → 删 staged extracted → 写 import-conflicts.json（覆盖式）
      → 可选 create_import_checkpoint
```

### P0-1. URL Readability 在前端运行 + 完全不处理 charset（违反 spec）

**现状**：
- `AppShell.tsx:31` 导入 `articleToMarkdown, extractArticleFromHtml` from `../../lib/readability`。
- `AppShell.tsx:400-402` 在 `requestTextImportPreview("url", …)` 执行 `extractArticleFromHtml(fetched.html, fetched.url)` → `articleToMarkdown(...)`。
- `src/lib/readability.ts:1` `import { Readability } from "@mozilla/readability"`。
- 后端 `fetch_import_url`（[import_commands.rs:489-593](../../src-tauri/src/commands/import_commands.rs)）**只返回原始 HTML**，且 `String::from_utf8`（`:581-588`）—— 任何 GBK/Shift-JIS/Big5 编码页面直接 `IMPORT_URL_ENCODING_UNSUPPORTED` 拒绝。`Cargo.toml` 无 `encoding_rs`。

**风险**：违反 SPEC"URL/HTML 提取应集中到后端"。Tauri WebView 的 DOMParser 在 Windows 上对 CJK/非 UTF-8 解码可能错乱；国内大量站点（百度、知乎部分页、政府站、博客园老内容）根本无法导入。

**建议**：把 `readability.ts` 逻辑（或 Rust 等价物如 `readability` crate / `lol_html`）迁到 `fetch_import_url` 内部，用 `encoding_rs` 按 `Content-Type` charset 解码，输出已规范化的 Markdown。

### P0-2. 100 MB / 64 MB 文件上限不一致 + 超大文件无 archive-only 退化

**现状**：
- `file_hash_fast`（[import_service.rs:1174-1184](../../src-tauri/src/services/import_service.rs)）：> 100 MiB → `FILE_TOO_LARGE`。
- `MAX_SOURCE_FILE_BYTES = 64 MiB`（[extraction_service.rs:911](../../src-tauri/src/services/extraction_service.rs)）：超过 → `EXTRACT_SOURCE_TOO_LARGE`，但仍进预览（status=Failed）。
- 用户若确认导入，`confirm_import` → `validate_confirm_entry`（`:808`）重新 `file_hash_fast`——64-100 MB 区间文件预览用 `!nohash:` 蒙混，确认时因 100 MB 上限 `FILE_TOO_LARGE` 中止整批。

**风险**：60 页以上扫描 PDF、大型 PPTX、含图 DOCX 全部无法归档；两个上限矛盾，错误信息互掐。

**建议**：统一上限（建议 256 MB 或可配置）；超大文件提供"archive-only/跳过文本提取"退化路径；`confirm_import` 不要在已 `!nohash:` 后又硬性重哈希。

### P0-3. URL 抓取完全屏蔽重定向

**现状**：[import_commands.rs:535](../../src-tauri/src/commands/import_commands.rs) `.redirect(reqwest::redirect::Policy::none())`，只接受 `is_success()`。

**风险**：Wikipedia（http→https）、GitHub blob、`t.co`/`bit.ly` 短链、大量博客 301/302 都返回 `IMPORT_URL_HTTP_ERROR`，错误文案"The URL returned HTTP 301."——极差体验。

**建议**：`Policy::limited(5)`，每次跳转用已有 `is_public_ip` 工具检查防 SSRF。

### P0-4. `confirm_import` 单文件失败做整批原子回滚

**现状**：[import_service.rs:700-712](../../src-tauri/src/services/import_service.rs)（source 不存在）、`:829-840`（hash 漂移）、`:842-854`（archive 路径无效）任一失败整批返回错误，预览中其他可成功文件**也不归档**。与预览阶段 `extract_batch` 的"继续处理其他"（[extraction_service.rs:180-207](../../src-tauri/src/services/extraction_service.rs)）行为相反。

**风险**：选 50 个文件，第 47 个被外部删除/修改，整批回滚，前 46 个明明能成功却不归档。违反 CLAUDE.md"失败要标记具体文件和原因"。

**建议**：`confirm_import` 改逐项尝试 + 失败项标记 + 部分成功，类似 `extract_batch`；只对 `archived_path` 篡改（安全敏感）保留全局失败。

### P1（导入板块）

- **P1-1** `fetch_import_url` 仅按 UTF-8 硬解码，丢弃 `Content-Type` 的 charset（[import_commands.rs:570-588](../../src-tauri/src/commands/import_commands.rs)）；HTTP 缺省应按 ISO-8859-1（HTML 规范）。
- **P1-2** `fetch_import_url` 不验证 `Content-Type`，PNG/PDF/zip 被当前端 HTML 解析输出乱码 Markdown；白名单 `text/html`/`application/xhtml+xml`/`text/plain`。
- **P1-3** 失败/取消的预览任务不清理已写入的 `raw/extracted/` 与 `.app/import-staging/` 文件，磁盘和 Git 仓库被无声污染（[import_commands.rs:327-411](../../src-tauri/src/commands/import_commands.rs)）。
- **P1-4** `.app/import-conflicts.json` 是**整文件覆盖**写入（[import_commands.rs:607-612](../../src-tauri/src/commands/import_commands.rs)），多批次导入丢失前批冲突记录。改为 read-merge-write 或目录化。
- **P1-5** CSV `has_headers(false)` + `first_row_is_header=true` 硬编码（[extraction_service.rs:251-267](../../src-tauri/src/services/extraction_service.rs)）；无表头 CSV 把第一行数据误标表头；多行单元格用 `<br>` 在多数 Markdown 渲染器非法；超大 CSV 无行数上限全量入内存。
- **P1-6** PDF 用纯 Rust `pdf-extract = "0.10"`（[extraction_service.rs:344-389](../../src-tauri/src/services/extraction_service.rs)），不处理表格/多列/CJK ToUnicode CMap 缺失——后者返回空字符串落入 `no_text_layer_result`，错误信息"No extractable text layer"误导（文本层有，字体映射缺）。
- **P1-7** `collect_source_files` 跳过 symlink 但**不报错**（[import_service.rs:36-38](../../src-tauri/src/services/import_service.rs)），macOS `~/Documents` 默认 symlink、NAS 用户素材库会被静默丢弃。记录到 conflicts / warnings。
- **P1-8** 0 字节文件成功归档但 `text_preview=""`, `promote_extracted_to_sources` 跳过（[import_service.rs:320](../../src-tauri/src/services/import_service.rs)），用户以为没导入。预览明确标"empty file (archived only)"。

### P2（导入板块）

- **P2-1** URL 不设 `Accept`/`Accept-Language`，Wikipedia 等返回英文版/简略版，部分 CDN 因 UA 403。
- **P2-2** CSV `flexible(true)` 列数错位时用空字符串 `resize` 填充（[extraction_service.rs:284-287](../../src-tauri/src/services/extraction_service.rs)），视觉与语义错位。
- **P2-3** `take_preview` 字节边界回退丢字符（[extraction_service.rs:220-223](../../src-tauri/src/services/extraction_service.rs)），CJK 3 字节场景明显，应用 `char_indices`。
- **P2-4** Windows MAX_PATH 260 字符未处理，CJK 长名文件 `IMPORT_ARCHIVE_COPY_FAILED`；启用 `\\?\` 长路径或预校验。
- **P2-5** `file_hash_fast` 全量读入内存再 SHA-256（[import_service.rs:1186-1191](../../src-tauri/src/services/import_service.rs)），100 MB = 100 MB 堆分配；改流式 `BufReader`。
- **P2-6** 预览不显示 parser 名 / 失败原因，UI 只显示状态徽章（[ImportView.tsx:62-77](../../src/features/import/ImportView.tsx)），用户看到 Failed 不知道为什么；表格加"原因"列展示 `extractionError`。
- **P2-7** `request_replace_source` 不对"类型同但内容差异极大"做 warning（[import_commands.rs:207](../../src-tauri/src/commands/import_commands.rs)），PendingAction summary 加"+/-"大小差提示。

### 元数据与无损性核查

| 维度 | 状态 |
|---|---|
| 原文件不动 | ✓ `fs::copy(source, target)` |
| 来源 URL 保留 | ✗ 仅 staged 文件，**未在 source-index.json 记录 source URL** |
| 作者 / 创建时间 | ✗ 后端从不读 fs `created`/`modified`，DOCX core.xml / PDF /Info / XLSX core.xml **完全没解析** |
| 原始路径 | ✗ 不记录 |
| 哈希 | ✓ SHA-256 |

### 路径与安全核查

| 维度 | 状态 |
|---|---|
| 路径经 ProjectContext | ✓ |
| Path traversal 防护 | ✓ 拒 `..`/绝对/UNC |
| Symlink escape | ⚠ 仅 Unix 可检测，Windows 默认需管理员才能建 symlink，风险低 |
| 内部正斜杠 | ✓ |
| CJK 文件名 | ✓ 但 `sanitize_filename` 只清 stem，Windows 非法字符 `:` `<` `>` 在 `概念:1.pdf` 仍 `IMPORT_ARCHIVE_COPY_FAILED` |

---

## 三、性能（4/10）

### P0-1. `scan_wiki` / `search` 每次全量读盘 + 全文件 SHA-256 + frontmatter 解析，无内存索引

**现状**：[search_service.rs:32-56](../../src-tauri/src/services/search_service.rs)（scan_wiki 循环 load_page）、`:481-513`（search 同样遍历）、[file_store.rs:151,238](../../src-tauri/src/services/file_store.rs)（每次 `fs::read` 后 SHA-256）。`graph_commands.rs:19`、`wiki_commands.rs:29`、`compile_commands.rs:357` 三处入口都触发完整 scan。

**风险**：500 页样本库，每次打开项目/图谱/搜索/编译都对 500 文件做全量 IO+hash+解析。机械盘或数千页大库下 UI 长时间无响应。

**建议**：维护内存 `PageIndex`（path→meta+hash+mtime），按 mtime 增量重算；backend 启动/项目切换时构建一次索引，scan/search/graph/compile 共享。

### P0-2. 同步 Tauri 命令在 IPC 线程跑长 IO，缺 `spawn_blocking`

**现状**：[git_commands.rs:33-70](../../src-tauri/src/commands/git_commands.rs)（git_status/diff/checkpoint 用 `process::Command` 同步）、[graph_commands.rs:13](../../src-tauri/src/commands/graph_commands.rs)（get_graph 同步 scan_wiki+resolve）、[wiki_commands.rs:23](../../src-tauri/src/commands/wiki_commands.rs)（scan_wiki）。Tauri v2 非 async 命令跑在主事件循环，阻塞所有 IPC。

**风险**：git checkpoint 跑 `git add --all`+commit（数百 ms～数秒）期间，整个后端 IPC 不可响应；scan_wiki 大库同理。

**建议**：所有 IO/子进程密集命令改 `async fn`，内部 `tokio::task::spawn_blocking` 包 service 调用。

### P1-1. 图谱无 LOD/虚拟化，50ms 强制 refresh 风险

**现状**：[GraphView.tsx:487-494](../../src/features/graph/GraphView.tsx) 仅设 `labelDensity:0.07 / labelGridCellSize:80 / labelRenderedSizeThreshold:6`，无节点上限阈值；`startBackgroundLayout:600` 期间 `setInterval(refresh, 50)` 强制每 50ms 全量 refresh。

**风险**：500+ 节点 + 数千边时 50ms refresh 掉帧；`nodeReducer`/`edgeReducer`（`createRenderer:496-523`）每次 refresh 都 O(n) 重建 options。

**建议**：refresh 节流到 ~16fps（60-65ms）并 `skipIndexation:true`（除非布局变更）；超大图降级到只渲染度数 >k 的节点。

### P1-2. 图谱 reducer 每帧重复构造 Map 与 Set

**现状**：[GraphView.tsx:466-485](../../src/features/graph/GraphView.tsx) `currentRenderOptions()` 每次 reducer 调用都 `new Map/Object.entries` 重建 `communityByNodeId`；`hiddenNodeIds(options)` 每次 edge reducer 都 `new Set + filter`。

**风险**：sigma 每次 refresh 对每节点/每边调一次 reducer，n=500 时 = O(n²) 的 Set/Map 构造。

**建议**：`communityByNodeId`、`hiddenNodeIds` 在 buildGraph 时或 data 变更时计算一次存 ref，reducer 直接读。

---

## 四、代码屎山（7/10）

### P0-1. 服务层 7 处 `panic!` / `unreachable!` 散落在生产路径

**现状**：[agent_service.rs:299](../../src-tauri/src/services/agent_service.rs)、[compile_service.rs:539](../../src-tauri/src/services/compile_service.rs)、[export_service.rs:1089](../../src-tauri/src/services/export_service.rs)（`panic!("missing template")`）等。

**风险**：生产代码 panic 让 Tauri 后端线程崩溃，用户丢未保存状态。模板缺失属可恢复用户错误。

**建议**：全部换 `BackendError`。

### P1-1. AppShell.tsx 单文件 713 行聚合 8 个 store + 27 处 hook

**现状**：[AppShell.tsx:51-142](../../src/components/app/AppShell.tsx) 单组件订阅 navigation/project/task/toast/wiki/import/settings/... 8 个 store；27 处 useState/useEffect/useCallback/useMemo。

**风险**：任一订阅 state 变更都重渲整棵 shell；测试边界巨大；CLAUDE.md 已自我确认"AppShell 聚合太多流程"。

**建议**：拆 `usePendingAction`、`useTaskToasts`、`useProjectBootstrap` 等自定义 hook，组件只负责布局。

### P1-2. 前端 11 处 `catch { }` 静默吞错，无用户反馈

**现状**：[TopBar.tsx:179](../../src/components/app/TopBar.tsx)、[wikiStore.ts:242](../../src/stores/wikiStore.ts)、[LintView.tsx:213,259](../../src/features/lint/LintView.tsx)、[ProjectStartView.tsx:85,509](../../src/features/project/ProjectStartView.tsx) 等 11 处。

**风险**：BYOK/导入/项目加载失败时用户只看到"没反应"，难排查。

**建议**：catch 至少 `pushToast("error", ...)` 或注释说明预期静默原因（gotchas 2026-06-24 已立"空 catch 吞错是反模式"规矩）。

### P2（屎山）

- **P2-1** `extraction_service.rs`、`import_service.rs` 单文件 116/142 个 unwrap（多数在 `#[cfg(test)]` 内，但应抽 helper 降噪）。
- **P2-2** [graph_service.rs:63-73](../../src-tauri/src/services/graph_service.rs) 标签共现边对 ≤64 的组做 O(k²) 配对，无上界预警（`MAX_TAG_GROUP_FOR_EDGES` 阈值硬编码）。
- **P2-3** `get_graph` 与 `build_graph`（后台任务）都跑完整 `scan_wiki`，未复用扫描结果（与 P0-1 同根）。

---

## 五、UI 微观细节（7.5/10）

整体：CSS token 系统、i18n、a11y 基础（焦点环、Esc/箭头键、skip link、role/aria）做得相当扎实；没有 `text-sm/base/lg/xl` 相对字号，没有 Tailwind 调色板色。问题集中在 graph 硬编码颜色、几处容器溢出/截断缺失、个别图标/字号漂移。

### P1-1. Anthropic provider 徽标在暗色主题下几乎不可见

**现状**：[LlmProviderSettings.tsx:163](../../src/features/settings/LlmProviderSettings.tsx)

```tsx
style={kind === "anthropic" ? { background: "#0d0d0d", color: "#fff" } : undefined}
```

**风险**：暗色主题 `--surface` ≈ `#151918`，徽标背景 `#0d0d0d` 几乎同色，徽标"消失"。

**建议**：`{ background: "var(--foreground)", color: "var(--text-inverse)" }`。

### P1-2. Exports 列表 `outputPath` 无溢出/截断

**现状**：[ExportsView.tsx:319](../../src/features/exports/ExportsView.tsx)

```tsx
<div className="secondary font-mono">{record.outputPath}</div>
```

`.secondary`（[styles.css:1408](../../src/styles.css)）只有 font-size/color/margin。同行的 `record.title` 用了 `.truncate`（`.table .truncate` 限 340px），`outputPath` 没有。

**风险**：CJK 长路径无空格不换行，`.table-wrap` 出现整表横向滚动而非单元格截断。

**建议**：`<div className="secondary truncate font-mono">`，父容器 `min-w-0` 已就位。

### P1-3. Lint 列表面板高度/滚动结构错误

**现状**：[styles.css:2290-2296](../../src/styles.css) `.lint-view__list-pane { display:flex; flex-direction:column; ... }`（无 overflow/height 控制）；[LintView.tsx:313-405](../../src/features/lint/LintView.tsx) 在该面板堆叠 toolbar、notice、error、`LintHistoryList`、`LintSummaryCards`、`LintIssueList`、`LintPassedSection`、batch banner。`LintIssueList`（`LintIssueList.tsx:77`）自己 `flex h-full flex-col overflow-y-auto`，`h-full` 与兄弟元素抢高度。

**风险**：issues 多 + history 展开 + summary cards 同屏时，面板总高超过视口；`LintIssueList` 内部滚动区可能为 0 或负，issues 看不全；与右侧详情底部对不齐。

**建议**：`lint-view__list-pane` 设 `overflow:hidden`；history+summary+issue-list 包进同一 `min-h-0 flex-1 overflow-y-auto`；issue list 内部不要再 `h-full`。

### P1-4. Lint evidence/before/after code 块缺 `overflow-wrap`

**现状**：[LintIssueDetails.tsx:131, 221, 229](../../src/features/lint/LintIssueDetails.tsx)

```tsx
<code className="block whitespace-pre-wrap rounded-... ">...</code>
```

**风险**：`whitespace-pre-wrap` 对超长 URL、base64、CJK 无空格串不生效，撑破详情面板触发整页横向滚动。

**建议**：追加 `break-anywhere` 或 `overflow-wrap:anywhere`。

### P1-5. GraphView 大量硬编码颜色，暗色主题对比度失衡

**现状**：[GraphView.tsx:28-31](../../src/features/graph/GraphView.tsx) 与 `:493`

```ts
const EDGE_COLOR = "#d4d4d4";
const PLAIN_COLOR = "#9b9b9b";
const DIM_COLOR = "#ececec";
labelColor: { color: "#6b7280" },
```

`graphRenderStyle.ts:44, 46-47` 同样硬编码 `GRAPH_SELECTED_COLOR="#0d9488"`、`PLAIN_COLOR`、`DEFAULT_EDGE_COLOR`。`GraphLegend.tsx:40`、`GraphInspector.tsx:93` 也是 `#9b9b9b` 兜底。

**风险**：暗色主题下整体观感偏浅、与"节点用 page-type 色、边用 `--border-strong`"的设计意图不符；`labelColor "#6b7280"` 在暗色下可读性低于 `--text-muted`。

**建议**：定义独立 graph CSS 变量 `--graph-edge`、`--graph-node-plain`、`--graph-label` 在 `:root` 与暗色覆盖层分别赋值；`GRAPH_SELECTED_COLOR` 统一为 `var(--accent)` 色值。

### P2（UI）

- **P2-1** `text-[10px]` / `text-[9.5px]` 不在 token 字号表（CLAUDE.md 锁 10.5px）。TopBar.tsx:252,290、RightContextPanel.tsx:129、TaskLogDrawer.tsx:67,68,100,327、ChatView.tsx:333 用 10px；LeftSidebar.tsx:119、GraphInspector.tsx:130 用 9.5px。统一 10.5px。
- **P2-2** TopBar 项目下拉钉死 360px（[TopBar.tsx:225](../../src/components/app/TopBar.tsx)），路径用 ellipsis 吃掉有用信息；改 `min(360px, calc(100vw - 32px))` + compactPath。
- **P2-3** TopBar 搜索结果项缺 focus-visible 环（[TopBar.tsx:297](../../src/components/app/TopBar.tsx)）。
- **P2-4** WikiTree 过滤框 `outline-none` 无替代焦点环（[WikiTree.tsx:89-94](../../src/features/wiki/WikiTree.tsx)），父 div 加 `focus-within:` 样式。
- **P2-5** ChatSessionList 操作按钮用 Unicode `✎`/`×` 而非 Lucide（[ChatSessionList.tsx:103, 111](../../src/features/chat/ChatSessionList.tsx)），违反 CLAUDE.md 图标规范。
- **P2-6** `ImportRightPanel` `ARCHIVE_RULES` 混用中英文且未走 i18n（[ImportRightPanel.tsx:46-54](../../src/features/import/ImportRightPanel.tsx)），英文界面出现"图片"。
- **P2-7** LeftSidebar lint 徽标 aria-label 含硬编码英文（[LeftSidebar.tsx:119](../../src/components/app/LeftSidebar.tsx)）。
- **P2-8** Wiki 面包屑过长不截断挤压工具栏（[WikiView.tsx:302-322](../../src/features/wiki/WikiView.tsx)），中间段做省略。
- **P2-9** Settings project root path 直接显示无截断（[SettingsView.tsx:179](../../src/features/settings/SettingsView.tsx)），触发整面板横向滚动。
- **P2-10** GraphLegend / GraphInspector "其它类型"兜底色 `#9b9b9b` 与 `graphRenderStyle.ts` `PLAIN_COLOR` 同值但未共享常量，export 复用。
- **P2-11** 暗色主题下 `.html-preview__iframe` 硬编码 `background: white`（[styles.css:594](../../src/styles.css)），加载前白色闪烁；改 `var(--surface-raised)`。
- **P2-12** `.view-toolbar` min-height:44px 与 CLAUDE.md "主区头 52px" 不一致；`.workspace-header`（52px）似乎未被任何 view 实际使用，删掉或迁移。

---

## 六、外部对照（Karpathy / nashsu / Astro-Han 等）

### 关键模式对比

| 维度 | Karpathy 原始 gist | nashsu/llm_wiki + skill | 本项目 |
|---|---|---|---|
| 三层模型 | raw immutable / wiki LLM-owned / schema rules | 同 | ✓ 实现到位 |
| Ingest 流程 | 抽象描述 | **读源 → 与用户讨论 takeaways → 写 source 摘要 → 更新 10-15 相关页 → 更新 index → 追加 log** | 抽象（"create real DERIVED content pages"），无"讨论"环节，无"10-15 页"具体目标 |
| Agent 查询入口 | 直接 LLM 读 wiki 目录 | **本地 HTTP API（127.0.0.1:19828）让 Agent 查询 wiki** | Tauri IPC（无 HTTP API；BYOK 完全看不到 fs） |
| Lint | 笼统 | Ingest/Query/Lint 三操作并列 | 本地 lint 扎实，Agent lint 提示词脆弱（见 P1-1/P1-2） |
| Citation | 未明确 | 未严格 | 检索透传，无对齐核验（P0-4） |
| Edge 解释 | 未提 | 页面级图谱 | 仅"相关"，无依据/关系类型/置信度（前次审计 #7 已记） |

### 启发与借鉴（5 条）

1. **Ingest 加"讨论 takeaways"环节与"10-15 页"具体目标**（nashsu）：参考 [nashsu/llm_wiki_skill SKILL.md](https://github.com/nashsu/llm_wiki_skill/blob/main/SKILL.md)。本项目编译提示词过抽象，弱模型（GLM-5.2 等）会只生成 3 个脚手架页（gotchas 2026-06-24 已踩过此坑）。给模型"每个源更新 10-15 个相关页"的具体数字目标，能直接对治 P0-3 的"一锤子"问题。
2. **暴露本地 HTTP API 给 Agent 查询**（nashsu `127.0.0.1:19828`）：本项目 Agent 走 Tauri IPC，BYOK 完全看不到 fs。可在后端起一个**只读、绑定 127.0.0.1、token 鉴权**的 HTTP 端点让 BYOK 模型（通过 tool calling）查询 wiki。这能直接缓解 P0-2 的能力鸿沟，且不破坏 local-only / 无云数据库 边界。需新增 capability，属较大改动，建议进 roadmap。
3. **三操作并列的 SKILL 结构**（[alirezarezvani/claude-skills llm-wiki/SKILL.md](https://github.com/alirezarezvani/claude-skills/blob/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md)）：Ingest/Query/Lint 三个 SKILL 共享 index.md/log.md 契约。本项目 `wiki-ingest`/`wiki-lint` 已有，但**缺 `wiki-query` skill**——这正是 P0-2 建议的"Chat 封装成 skill"的外部先例。
4. **schema.md 作为纪律源**（[infranodus/skills skill-llm-wiki](https://github.com/infranodus/skills/blob/master/skill-llm-wiki/SKILL.md)）：把 schema 当作"编译期校验 + lint 期校验"的双重源。本项目 P1-3（本地 lint 不校验 frontmatter type）正是缺这一层。
5. **不要为了追赶功能放弃 local-only**（[lucasastorian/llmwiki](https://github.com/lucasastorian/llmwiki) 走 Web/MCP/Claude 路线引入云依赖）：本项目坚持 local-first、无数据库、OS secret storage 是差异化优势，前次审计已记。

参考链接：
- [Karpathy LLM Wiki 原始 gist](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki)
- [nashsu/llm_wiki_skill](https://github.com/nashsu/llm_wiki_skill)
- [alirezarezvani/claude-skills llm-wiki](https://github.com/alirezarezvani/claude-skills/blob/main/engineering/llm-wiki/skills/llm-wiki/SKILL.md)
- [Astro-Han/karpathy-llm-wiki](https://github.com/Astro-Han/karpathy-llm-wiki)
- [lucasastorian/llmwiki](https://github.com/lucasastorian/llmwiki)
- [HN 讨论](https://news.ycombinator.com/item?id=47963913)

---

## 七、推荐优先级

### 本周必修（P0，按 ROI 排序）

1. **统一 BYOK / Agent 提示词内容深度**（一/ P0-1、P0-2）：把 `compile_prompt` 6 步骤抽成共享常量 BYOK 也注入；Chat 路径在 BYOK 模式明确声明"无 fs 访问"并删"current working directory..."误导句。**最低投入、最大体验提升**——这是"产品看起来稳不稳"的分水岭。
2. **后端加 PageIndex 内存索引**（三/ P0-1）：path→(hash,mtime,meta,wikilinks)，scan/search/graph/compile 共享，按 mtime 增量。一举消掉全量扫描痛点。
3. **IO/子进程命令改 `async + spawn_blocking`**（三/ P0-2）：git_commands、get_graph、scan_wiki、search。消除主线程阻塞。
4. **Chat citations 加 claim 级对齐核验**（一/ P0-4）：prompt 编号 sources，要求模型 `[n]` 标注，post-hoc 解析实际引用。Karpathy 模式的可信度核心。
5. **URL Readability + charset 解码迁到后端**（二/ P0-1 + P1-1）：用 `encoding_rs`，对 GBK 等中文站点从"完全不可用"变"可用"。违反 SPEC 硬约束。
6. **`confirm_import` 改逐项失败容忍 + 失败/取消清理 staged 文件**（二/ P0-4 + P1-3）：违反 CLAUDE.md"失败标记具体文件"原则 + 污染磁盘。
7. **替换 7 处生产 panic 为 BackendError**（四/ P0-1）：防 Tauri 后端崩溃丢用户状态。
8. **URL 重定向 `Policy::limited(5)`**（二/ P0-3）：Wikipedia/短链/HTTPS 跳转从"全部失败"变"可用"。
9. **统一文件大小上限 + 流式哈希 + 元数据提取**（二/ P0-2 + P2-5）：100/64 MB 矛盾、`SourceMetadata` 几乎全 None。

### 下一阶段（P1 重点）

1. **编译加 few-shot + frontmatter schema 校验 + Anthropic/Google 设温度 0**（一/ P0-3、P1-4）：把编译从"碰运气"变"可重现"。`validate_manifest` 校验 `sources` 字段和 `> Sources:` 行。
2. **本地 lint 加 `SchemaTypeMismatch` + deep lint 提交本地 baseline + 摘要提到 800-1200 字符**（一/ P1-2、P1-3、P1-5）。
3. **wiki-lint SKILL 加 severity 分级标准**（一/ P1-1）。
4. **Chat history 加 token 预算裁剪**（一/ P1-6）。
5. **Chat 封装成 `wiki-query` / `wiki-chat` skill**（一/ P0-2 推荐 1）：参考 nashsu 三操作模式。
6. **UI 一波收口**（五/ P1-1～P1-5）：硬编码颜色接 token、Exports outputPath 截断、Lint 列表滚动结构、Lint code 块 break-anywhere、Anthropic 徽标暗色修复。一次性把暗色主题观感和容器溢出拉齐。
7. **AppShell 拆 hook**（四/ P1-1）：`usePendingAction`、`useTaskBootstrap`、`useTaskToasts`，降低后续改 UX 的回归风险。
8. **11 处 `catch {}` 加 toast 或显式注释**（四/ P1-2）：恢复可观测性。

### 稍后但必须跟踪（P2 / 战略）

1. **暴露只读本地 HTTP API 给 BYOK 模型 tool-calling**（六/ 启发 2）：缓解 Agent/BYOK 能力鸿沟的战略级方案。
2. **导入格式覆盖深化**（PDF 表格/多列、CJK ToUnicode、CSV 表头检测、symlink 提示）。
3. **CJK/Unicode/Windows 路径全链路 smoke test**（前次审计 #11，本审计二/ P2-3/P2-4 呼应）。
4. **UI 字号/图标细节统一**（五/ P2-1、P2-5、P2-10）。
5. **Edge 解释 / Graph provenance**（前次审计 #7）。

---

## 八、验收矩阵（代码级闭环对照）

| 用户闭环 | spec 锚点 | 当前状态 | 关键缺口（本审计） |
|---|---|---|---|
| 编译出可信 wiki | PRD-WIKI-001/002/004 | BYOK 提示词弱（P0-1）、无 schema 校验（P0-3）、无源/综合混淆检测（P1-4） |
| Chat 引用可追溯 | PRD-CHAT-003 | citations 检索透传无对齐（P0-4）、Agent/BYOK 能力不对等且 UI 不告知（P0-2）、history 无 token 预算（P1-6） |
| Lint 发现真问题 | PRD-LINT-001/002 | 本地缺 schema 校验（P1-3）、deep lint 摘要太短（P1-2）、severity 无标准（P1-1）、Agent 不知本地 baseline（P1-5） |
| 导入无损可追溯 | PRD-IMP-001/004/005 | URL charset 缺失（P0-1）、整批回滚（P0-4）、元数据全 None、import-conflicts 覆盖式（P1-4） |
| 大库流畅 | PRD §11.1 | 无内存索引（P0-1）、同步 IO 阻塞（P0-2）、图谱无 LOD（P1-1） |
| 暗色主题可用 | PRD-SET-004 | graph 硬编码颜色（P1-5）、Anthropic 徽标暗色不可见（P1-1）、html-preview iframe 白底（P2-11） |

---

## 九、方法论与限制

- **执行方式**：5 个对抗式子代理并行（提示词/Lint/Chat、UI、导入、性能、Karpathy），要求每条发现带 `file:line` 证据。Karpathy 对照代理因连接中断未返回完整结果，由 WebSearch + 既有审计的"外部项目对照"章节补足。
- **未深度覆盖**：后端错误模型（`BackendError`）、Secret 服务、Task 服务、Settings 服务的细粒度审查——这些在 [00-codebase-audit.md](../fixes/00-codebase-audit.md) 与 [2026-07-05-adversarial-product-audit.md](./2026-07-05-adversarial-product-audit.md) 已部分覆盖，且不是本次用户提问的重点。
- **未跑运行验证**：本次为只读审查，未起 `npm run tauri dev` 复现具体 UI 缺陷；行号基于 `task1-backend-contracts` 分支当前工作树，若后续修复推进需以实际文件为准。
- **既知环境限制**：`cargo test --lib` 在本机受 WebView2 DLL 入口点 0xc0000139 阻塞（gotchas 2026-06-29），团队以 `cargo check --lib --tests` 为代码正确性闸门——本审计的性能结论基于代码分析，未做运行期 profile。

---

## 结论

项目骨架与硬边界（local-first、raw 不可变、Git checkpoint、ProjectContext 路径安全、PendingAction、Skill 模板）已经相当扎实，工程纪律（progress/gotchas 持续记录、双子代理审查、批次修复闭环）也很成熟。**真正拉低产品感的是三个跨板块的代码级问题**：(1) BYOK 与 Agent 提示词/能力严重不对等让产品"看运气"；(2) 后端无内存索引 + 同步 IO 让中大库卡顿；(3) Chat 引用是检索透传而非 claim 级核验，伤了 Karpathy 模式最核心的"可信度"。

修这三件事 + 编译加 few-shot/schema 校验 + 导入元数据与失败容忍，是当前最高 ROI 的投入方向。UI 与 Lint 的细节问题单体不致命，但累积起来影响"成熟产品"的观感，建议打包一波收口。
