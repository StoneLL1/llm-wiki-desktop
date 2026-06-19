# Wiki 编译日志

## [2026-06-10] compile | 3 篇 raw 文章编译完成

**触发**: 自动编译（cron job）
**源文章**: 3 篇
- `raw/articles/2026-06-08-claude-code-parallel-agents-comparison.md`
- `raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md`
- `raw/articles/2026-06-09-openai-codex-best-practices-guide.md`

**新建页面**: 0

**更新页面**: 8
- `entities/agent-teams.md` — 新增「四种并行方案对比汇总」章节（四方案对比表、三问决策框架、worktrees/batch 澄清），修复无效 tag（tool→删除），新增来源，更新日期
- `entities/claude-code.md` — Subagent Pattern 段新增四种并行方案交叉引用，更新日期
- `concepts/context-compression-pipeline.md` — 新增「六家 Agent 横向对比」章节（Claude Code 五段流水线+服务端路径、Codex handoff、OpenCode 可逆隐藏、Cline 双模式、Cursor DCD、Amp 换线程、Letta RAM 模型、滑窗缓存陷阱 $64.8 实例、共识原则表），新增来源和 wikilinks，更新日期
- `concepts/context-rot.md` — 新增「压缩的本质：保护注意力而非省 token」洞察，新增来源，更新日期
- `entities/letta.md` — 新增「上下文压缩策略」章节（三层内存建模表、与六家对比的定位差异），新增来源，更新日期
- `entities/openai-codex.md` — 新增「官方最佳实践框架（六大支柱）」章节（六支柱表、四要素 prompt、推理级别、三种规划方法、AGENTS.md 维护策略、config 分层、Skill 存储、线程管理、成熟工作流三阶段表），新增 wikilinks，更新日期
- `entities/skills.md` — 新增「Codex Skills（OpenAI）」子章节（$skill-creator/$skill-installer、存储路径），新增来源，更新日期

**index.md**: 更新日期至 2026-06-10，Total pages 保持 191（无新增页面）

## [2026-06-09] ingest | OpenAI Codex官方最佳实践完整解读
- Source: 微信公众号 (字节笔记本)
- File: raw/articles/2026-06-09-openai-codex-best-practices-guide.md
- 字节笔记本逐条拆解 OpenAI 官方 Codex Best Practices 文档，围绕六个支柱展开：任务设计、上下文管理、工具使用、提示工程、安全性、性能优化，补充具体操作案例

## [2026-06-09] ingest | 横向拆解六大Agent上下文压缩策略
- Source: 微信公众号 (腾讯技术工程)
- File: raw/articles/2026-06-09-agent-context-compression-strategies-comparison.md
- 腾讯程序员 mervynyang 横向对比 Claude Code、Codex、Windsurf、Cursor、Gemini CLI、Aider 六大 Agent 的上下文压缩策略，提炼通用原则，并在 MUR AI 上落地四级水位线 + 增量摘要方案，面向云端多用户场景优化 token 消耗

## [2026-06-08] ingest | Claude Code并行Agents的四种方案对比
- Source: 微信公众号 (鲁工)
- File: raw/articles/2026-06-08-claude-code-parallel-agents-comparison.md
- 鲁工对比 Claude Code 四种并行 Agents 方案：Subagents（会话内临时工）、Agent view（分派多会话管理）、Agent teams（Claude 当 leader 带队协作）、Dynamic workflows（JS 脚本编排大规模调度）。核心区别是"谁来拿主意"，官方建议三问判断：谁协调、是否互通信、是否动同一批文件。

## [2026-06-08] compile | 1 篇 raw 文章编译完成

**触发**: 自动编译（cron job）
**源文章**: 1 篇
- `raw/articles/2026-06-07-anthropic-internal-skills-practices.md`

**新建页面**: 0（内容与已有 [[skills]] 高度重合，无独立实体提取）

**更新页面**: 2
- `entities/skills.md` — 新增 Anthropic 官方 2026-06-07 博客「Lessons from Building Claude Code: How We Use Skills」完整内容：九大类型详表、核心原则（聚焦>大而全、验证类价值最大、Gotchas 含金量最高）、5 个写作细节、Skill 演进三阶段（记忆/脚本/hooks）、分发与治理流程、Skill 组合机制
- `entities/anthropic.md` — 添加新 raw source 引用，更新日期

**index.md**: 更新日期至 2026-06-08，Total pages 保持 191（无新增页面）

## [2026-06-07] compile | 2 篇 raw 文章编译完成

**触发**: 自动编译（cron job）
**源文章**: 2 篇
- `raw/GitHub/alibaba-open-code-review.md`
- `raw/GitHub/ginobefun-BestBlogs.md`

**新建页面**: 2
- `entities/open-code-review.md` — 阿里巴巴开源 AI 代码审查 CLI，确定性管道+Agent 混合架构，行级精确评论，支持 [[claude-code]] 三种方式集成
- `entities/bestblogs.md` — AI 驱动私人阅读助手，六维评分+AI 伴读+沉浸式翻译，375 个微信公众号 RSS 源，25 个 Agent Skills 原语

**更新页面**: 0

**index.md**: 更新日期至 2026-06-07，Total pages: 189→191，新增 bestblogs 和 open-code-review 条目

## [2026-06-06] ingest | Open Code Review (阿里 AI 代码审查工具)
- Source: GitHub (alibaba/open-code-review)
- File: raw/GitHub/alibaba-open-code-review.md
- 阿里巴巴内部 2 年生产验证后开源的 AI 代码审查 CLI，混合架构（确定性工程管道 + LLM Agent），行级精确评论，内置 NPE/线程安全/XSS/SQL 注入规则集，支持 Claude Code 集成和 CI/CD，3144 stars

## [2026-06-06] ingest | BestBlogs (AI 驱动阅读助手 + 375 公众号 RSS 源)
- Source: GitHub (ginobefun/BestBlogs)
- File: raw/GitHub/ginobefun-BestBlogs.md
- AI 驱动的私人阅读助手，解决 RSS 信息过载/找不到精华/无评分三大疲劳，六维评分+AI 伴读+沉浸式翻译，已整理 375 个微信公众号 RSS 源（OPML 可导入），2 万+注册用户，3766 stars

## [2026-06-06] compile | 3 篇 raw 文章编译完成

**触发**: 自动编译（cron job）
**源文章**: 3 篇
- `raw/articles/2026-06-05-anthropic-95-percent-data-analytics-claude.md`
- `raw/articles/2026-06-05-codex-goal-command-guide.md`
- `raw/articles/2026-06-05-how-to-write-skills-ultimate-guide.md`

**更新页面**: 4
- `entities/anthropic.md` — 新增「内部数据分析自动化实践（2026-06）」章节（95% 数据分析自动化、三种 AI 分析模式、技术架构三件套、Skill 文件在数据分析中的核心作用），更新日期和来源
- `entities/codex-goals.md` — 新增「Kundel 的 7 条 Goal 原则（类比 OKR）」章节（验收量化/上下文/进度可衡量/真实环境/视觉警惕/过程跟踪/复盘清理），更新日期和来源
- `entities/skills.md` — 新增「Skill 写作最佳实践（腾讯工程师综合手册）」章节（Level 1-3 渐进式加载、Description 五要素、六大反模式、触发模式、MCP vs HTTP 决策树、Skill Creator、安全底线、Token 成本估算），更新日期和来源
- `concepts/skill-engineering.md` — 新增「腾讯工程师的 Skill 编写方法论补充」章节（指令编写原则深化、模块化拆分、调试方法论、与 Skillopt 自动化优化的衔接），更新日期和来源

**新建页面**: 0（3 篇文章内容与已有实体高度重叠，无新实体/概念达到创建阈值）

**index.md**: 更新日期至 2026-06-06，更新 anthropic/skills/codex-goals 条目摘要

## [2026-06-05] ingest | Anthropic 最新博客：95% 的数据分析，都已经交给了 Claude

- Source: 微信公众号（AGI Hunt）
- File: raw/articles/2026-06-05-anthropic-95-percent-data-analytics-claude.md
- AGI Hunt翻译解读Anthropic官方博客：内部95%数据分析查询由Claude自动完成（准确率95%），技术架构Text-to-SQL+RAG+Tool Use三管齐下，核心精髓是Skill文件（沉淀领域知识让AI可用），与Hermes Skill概念高度对应

## [2026-06-05] ingest | 如何写好 Skill：一份终极实战经验手册

- Source: 微信公众号（腾讯技术工程）
- File: raw/articles/2026-06-05-how-to-write-skills-ultimate-guide.md
- 腾讯程序员jackjchou撰写的Skill编写终极指南（73KB），覆盖15个章节：Skill定义与分类、结构设计（main/sub skill）、编写原则（可测试/可组合/单一职责）、Prompt工程技巧、反模式、MCP工具协议、质量评估、迭代维护。以Go为主兼顾Python/Java

## [2026-06-05] ingest | 用好/goal命令，Codex干活神器

- Source: 微信公众号（鲁工）
- File: raw/articles/2026-06-05-codex-goal-command-guide.md
- 鲁工翻译分享dominik kundel的Codex /goal使用指南，类比OKR方法论：7条原则——目标要能验收量化、多给上下文、进度可衡量、真实生产环境、视觉目标防跑偏、长任务过程跟踪、完成后复盘清理

## [2026-06-05] compile | Claude Code 作者Boris：我已经不写 prompt 了，我写 loop

**触发**: 自动编译（cron job）
**源文章**: 1 篇
- `raw/articles/2026-06-04-claude-code-boris-write-loops-not-prompts.md`

**更新页面**: 2
- `entities/boris-cherny.md` — 新增「写 Loop 不写 Prompt」章节（2026.6 演讲、Bun Zig→Rust 三模式组合、七大坑点、社区评价），更新日期和来源
- `entities/claude-code-dynamic-workflow.md` — 新增来源引用，更新日期

**新建页面**: 0（文章内容与已有实体高度重叠，无新实体/概念达到创建阈值）

**index.md**: 更新日期至 2026-06-05，更新 boris-cherny 条目摘要

**核心信息**:
- Boris 2026 年 6 月演讲核心哲学：「写 Loop 不写 Prompt」——角色从提示词作者转变为编排工程师
- 真实 workflow 通常组合 2-4 种编排模式，Bun Zig→Rust 用了 fan-out + 对抗验证 + loop until done
- 七大常见坑点（token 预算、self-preference、quarantine 等）
- 社区评价：「你不再跑任务，你在养管道」

## [2026-06-04] ingest | Claude Code 作者Boris：我已经不写 prompt 了，我写 loop

- Source: 微信公众号（AI工程化）
- File: raw/articles/2026-06-04-claude-code-boris-write-loops-not-prompts.md
- Boris 演讲：Claude Code + loops + Dynamic Workflow，六大编排模式（classify-and-act/fan-out/adversarial verification/generate-and-filter/tournament/loop-until-done），核心API三件套（agent/parallel/pipeline），常见坑点

## [2026-06-03] ingest | Claude Dynamic Workflow — 鲁工 Harness 设计模式分析

- Source: 微信公众号（鲁工）
- File: raw/articles/2026-06-03-claude-workflow-harness-design-patterns.md
- 鲁工工程视角：单上下文三大顽疾（偷懒/偏袒/漂移）+ 六种 Harness 模式详解（fan-out/对抗核查/分类路由/生成过滤/锦标赛/循环至终）+ 隔离区模式 + 实用技巧（/loop + /goal + token封顶）

## [2026-06-03] ingest | Claude Code Dynamic Workflow 官方长文解读

- Source: 微信公众号（AGI Hunt）
- File: raw/articles/2026-06-03-claude-code-dynamic-workflow-harness.md
- Anthropic 工程师 Thariq 官方长文：Dynamic Workflow 的三大顽疾（偷懒/自我偏好/目标漂移）、静态 vs 动态对比、六种编排模式（分类执行/扇出汇总/对抗验证/生成过滤/锦标赛/循环至终）、十种应用场景、上手建议

## [2026-06-03] ingest | BrowserAct 浏览器自动化 Skill（文章+GitHub+安装）

**触发**: 用户手动入库 + 安装
**来源**: 微信公众号「逛逛GitHub」+ GitHub browser-act/skills

- `raw/articles/2026-06-03-browseract-playwright-replacement.md` — 逛逛GitHub 介绍 BrowserAct：Stealth 反检测浏览器 + 三种浏览器模式 + Skill Forge
- `raw/GitHub/browser-act-skills.md` — GitHub 仓库入库：⭐ 1,573 stars，Python，面向 Agent 的浏览器自动化 CLI
- **已安装**: `browser-act` v0.1.25（via uv, Python 3.12），Hermes skill 已注册。`browser-act-skill-forge` 按用户要求未安装。
- Stealth 功能待 API Key 配置；Chrome/chrome-direct 模式无需认证即可使用。

## [2026-06-03] ingest | AI HOT 日报第22~26条原帖入库

**触发**: 用户手动入库
**来源**: AI HOT 日报 2026-06-03（第22~26条）

- `raw/articles/2026-06-03-khazix-mac-cleaner-skill.md` — 卡兹克：用 Codex 做 Mac 存储清理，开源三色分级 Skill，实测释放 120GB，远胜 CleanMyMac 的 15.8GB
- `raw/articles/2026-06-03-sensenova-skills-open-source.md` — 商汤 SenseNova-Skills 开源：信息图表/数据分析/PPT/深度研究四大功能，兼容 Hermes Agent
- `raw/articles/2026-06-03-karpathy-learning-methodology.md` — Karpathy 学习方法论（Rohan Paul 转发，5.6K 赞）
- `raw/articles/2026-06-03-claude-code-ai-native-engineering-org.md` — Fiona Fung 在 Code w/ Claude SF 2026 分享：agentic coding 如何重塑工程组织的规划/上下文/审查/团队构成
- `raw/articles/2026-06-03-claude-code-self-check-feedback-loop.md` — Claude Devs：如何编码人工检查让 Claude Code 形成自我反馈闭环（2.4K 赞）
- `raw/articles/2026-06-03-claude-code-self-check-deep-dive.md` — 第26条深度拆解：三层自检实现 + Learnings Loop + Dynamic Workflow 组合拳 + 评论区精华

## [2026-06-03] compile | garden-skills 编译为 entity 页面

**触发**: 自动编译（cron job）
**源文章**: 2 篇
- `raw/GitHub/ConardLi-garden-skills.md`
- `raw/articles/2026-06-02-garden-skills-7k-stars.md`

**新建页面**: 2
- `entities/conard-li.md` — ConardLi（花园老师），「code秘密花园」公众号主理人，garden-skills 创建者，提出 Skill「生产线」设计哲学
- `entities/garden-skills.md` — Agent Skills 合集（6,994 ⭐），4 个 production-ready Skill：web-video-presentation（23 套主题/可插拔 TTS）、web-design-engineer（25 套设计风格/反 AI 套路）、gpt-image-2（18 类 79 模板）、kb-retriever（本地知识库检索）

**更新页面**: 4
- `entities/huashu-skills.md` — 「生态定位」新增 garden-skills 互补定位对比
- `concepts/anti-slop-writing.md` — 「See Also」新增 garden-skills（视觉 AI 味反制）
- `entities/skills.md` — Notable skills 新增 garden-skills
- `index.md` — 新增 2 条 entity 条目，Total pages 179→181

**核心信息**:
- garden-skills 将 ConardLi 的 Skill 设计哲学落地为产品：明确工作流程+质量标准+迭代接口，把 Agent 从「接到任务」升级为「启动生产线」
- 四个 Skill 覆盖 AI Agent 视觉产出三大方向（视频/网页/图片）+ 知识检索，均达到 production-ready 水平
- 与 huashu-skills（内容创作全链路）形成「视觉产出 vs 内容产出」的互补

## [2026-06-02] ingest | garden-skills 7K Star + GitHub 仓库入库

**源文章**: `raw/articles/2026-06-02-garden-skills-7k-stars.md`
- ConardLi 介绍其开源的 Agent Skills 合集 garden-skills（近 7K Star）
- 包含 4 个 Skill：web-video-presentation（网页视频）、web-design-engineer（网页设计）、gpt-image-2（图片生成）、kb-retriever（知识检索）
- 核心理念：Skill 的价值在于把可重复稳定工作的方法交给 Agent

**关联 GitHub 仓库入库**: `raw/GitHub/ConardLi-garden-skills.md`
- ⭐ 6,994 stars | 🍴 956 forks | 语言: CSS
- Topics: agent, claude, gpt-image-2, rag, skills, web-design
- 在线体验: https://mmh1.top/

## [2026-06-02] ingest | Harness 研究反思 + 4 个低星 GitHub 项目

**触发**: 自动编译（cron job）
**源文章**: 2 篇
- `raw/articles/2026-05-30-harness-research-reflection.md`
- `raw/articles/2026-05-31-4-interesting-low-star-github-projects.md`

**新建页面**: 7
- `concepts/state-aware-runtime.md` — 陈希伟提出的 Agent 设计范式，位于 Harness Engineering 下一步：候选输出 vs 已提交状态、Trace-Native Evaluation、失败分类学
- `entities/chen-xiwei.md` — Datawhale 独立研究者，State-Aware Runtime 概念提出者
- `entities/peekdesktop.md` — 微软 VP Scott Hanselman 开发的 Windows 桌面 peek 工具（.NET 65MB→1.88MB）
- `entities/opentoonz.md` — DWANGO 开源的专业 2D 动画软件，吉卜力使用十多年
- `entities/recordly.md` — 开源录屏+自动编辑工具，对标 OpenScreen
- `entities/english-level-up-tips.md` — 程序员英语学习指南（4.8 万 Star）
- （harness-engineering.md 更新：新增 CMU/Yale 综述 + State-Aware Runtime 关联）

**更新页面**: 1
- `concepts/harness-engineering.md` — 新增"行业共识：CMU/Yale Harness 综述"章节，新增 State-Aware Runtime 交叉引用，追加 sources

**导航更新**: index.md（173→179 页）、log.md

**核心信息**:
- 陈希伟的 State-Aware Runtime 是 Harness Engineering 之后最有价值的独立研究方向：把 Agent 可靠性从"模型能力问题"重新定义为"运行时状态管理问题"
- 四个项目虽 Star 不高但各有特色：PeekDesktop 极致压缩（65MB→1.88MB）、OpenToonz 吉卜力背书、Recordly 自动出片、English-level-up-tips 4.8 万程序员收藏

## [2026-05-31] lint | Consistency check — 120 issues found, 12 fixed

**Mode**: CONSISTENCY_CHECK (no new raw files)

**Fixed (7 CRIT)**:
- `[[codex-openai]]` → `[[openai-codex]]` in 5 entities (ai-news-radar, computer-use-agent, everything-claude-code, typeui-design-md-extractor, vercel-ai-deploy)
- `[[langgraph]]` → `LangGraph` (plain text) in langchain.md (no page meets threshold for creation)

**Fixed (4 MED invalid tags)**:
- Added `automation`, `security`, `workflow` to Engineering taxonomy
- Added `research` to Meta taxonomy
- Updated SCHEMA.md taxonomy

**Remaining**: 102 source-drift warnings (MED), 1 no-sha256 (LOW), 6 oversized pages (LOW). Source drift needs investigation — likely caused by body extraction differences or actual file edits.

## [2026-05-30] ingest | Claude Opus 4.8 + Dynamic Workflow + OpenClaw/Hermes 架构剖析 + Agent 七大模块

**触发**: 自动编译（cron job）
**源文章**: 3 篇
- `raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md`
- `raw/articles/2026-05-29-openclawhermesai-agent.md`
- `raw/articles/2026-05-29-一文看懂-ai-agent-的7大核心模块skillragmcpharness.md`

**新建页面**: 4
- `entities/claude-opus-48.md` — Claude Opus 4.8 旗舰模型：诚实度 4× 提升，SWE-Bench Pro 69.2%，同步推出 Dynamic Workflow
- `entities/claude-code-dynamic-workflow.md` — Claude Code 动态工作流：JS 脚本编排 100+ 并行 subagents 交叉验证，Bun 11 天移植 Rust 案例
- `concepts/agent-seven-core-modules.md` — AI Agent 七大核心模块（Token/Skill/Prompt/RAG/MCP/SDD/Harness）三层架构解析
- （OpenClaw/Hermes 架构剖析合入已有实体页，未新建 comparison）

**更新页面**: 7
- `entities/claude-model-family.md` — 新增 Opus 4.8 章节（诚实度、基准、Dynamic Workflow）、更新 Context Window 表格、新增来源
- `entities/claude-code.md` — 新增 Dynamic Workflow 章节（与传统 Subagents 对比、两种触发方式、内置 /deep-research）
- `entities/anthropic.md` — 新增 Opus 4.8 + Mythos 预告章节，更新来源
- `entities/agent-teams.md` — 新增 Dynamic Workflow 关联（下一阶段演进），新增来源
- `entities/openclaw.md` — 大幅扩展：Gateway 微内核中枢（5 大角色）、Session Key 路由、Channel 25+ Adapter 契约、Auth Profile 智能容错、Agent 三层执行引擎、FailoverError 容错设计
- `entities/hermes-agent.md` — 大幅扩展：AIAgent 单体架构、Credential Pool vs Auth Profile 对比、Tool Registry 自注册、Session Search FTS5、Skill 渐进式披露、Context Compressor 四步算法
- `index.md` — 新增 4 条条目，Total pages 170→174

**核心信息**: 
- Claude Opus 4.8 + Dynamic Workflow 代表 Claude Code 从「单 Agent 逐一决策」到「脚本化百 Agent 并行交叉验证」的范式跃迁
- 腾讯技术工程 OpenClaw/Hermes 源码级架构对比揭示两种路线：微内核+插件化（OpenClaw）vs 单体+自我改进（Hermes）
- Agent 七大模块合集：竞争焦点已从模型转移到系统工程能力（Token/Skill/Prompt/RAG/MCP/SDD/Harness）

## [2026-05-29] ingest | Codex 入门最佳实践（OpenAI 官方）

**触发**: 自动编译（cron job）
**源文章**: 1 篇
- `raw/articles/2026-05-28-codex-best-practices-openai-official.md`

**更新页面**: 2
- `entities/openai-codex.md` — 大幅扩展：新增 Prompt 工程（四要素结构+推理强度+语音）、规划模式（Plan/采访/PLANS.md）、AGENTS.md 多层级配置、config.toml 配置（审批/沙箱模式）、自测自审循环（Diff 面板、/review、code_review.md、GitHub PR）、MCP 集成（codex mcp add、OAuth）、Codex Skills（/skill-creator、.agents/skills）、自动化（Skill+时间表）、线程管理（8 个命令+线程原则）、常见错误（8 个陷阱）。更新日期至 2026-05-29，新增来源
- `entities/skillopt.md` — 修正 [[codex-openai]] → [[openai-codex]] wikilink

**归档**: 
- `entities/codex-openai.md` → `_archive/entities/codex-openai.md`（已被 openai-codex.md 完全覆盖，重复页面）

**index.md**: 删除 codex-openai 条目，更新 openai-codex 描述，总页数 171→170

**核心信息**: OpenAI 官方 Codex 最佳实践九大原则——完整上下文→先规划→沉淀 AGENTS.md→配置保持一致性→自测自审→MCP 连接→Skill 打包→自动化→管理线程。核心理念：把 Codex 当成持续配置和改进的队友。

## [2026-05-27] ingest | Agent Harness Engineering 综述 + 论文项目

**触发**: 用户请求抓取公众号文章及其引用论文
**源文章**: 2 篇
- `raw/articles/2026-05-27-agent-harness-engineering-survey-datawhale.md` — Datawhale 公众号解读 CMU 等联合出品的 Agent Harness Engineering 综述，ETCLOVG 七层框架，170+ 开源项目梳理
- `raw/GitHub/Picrew-LLM-Harness.md` — 论文配套 GitHub 仓库，含 ETCLOVG 分类框架、OpenReview 论文链接、awesome-agent-harness 目录

## [2026-05-27] ingest | 微软 SkillOpt 论文 — 像训练神经网络一样训练 Skill

**触发**: cron 自动检测新文章
**源文章**: 1 篇
- `raw/articles/2026-05-26-skillopt-microsoft-train-skill-like-nn.md`

**新建页面**: 1
- `entities/skillopt.md` — 微软 Research 自动化 Skill 优化方法，将 Skill 文档视为可训练权重，rollout→reflection→edit→validation gating 循环，52/52 测试最优，平均 +23.5 分

**更新页面**: 1
- `concepts/skill-engineering.md` — 新增「自动化优化：SkillOpt 范式」章节，新增来源，更新日期，新增开放问题

**index.md**: 更新 total pages 170→171，新增 1 条 entity entry（skillopt）

## [2026-05-27] batch 8（最后一批）— 4 篇 raw 文章编译

**触发**: 用户请求处理 4 篇 raw 文章
**源文章**: 4 篇
- `raw/articles/2026-05-26-ai-vercel-deploy-website-yupi.md`
- `raw/GitHub/santifer-career-ops.md`
- `raw/GitHub/wps365-open-cli.md`
- `raw/GitHub/bergside-design-md-chrome.md`

**新建页面**: 2
- `entities/vercel-ai-deploy.md` — AI + Vercel 一键部署网站实战（鱼皮），CLI + Skills 组合，4 大部署平台对比，前后端分离方案
- `entities/career-ops.md` — AI 全自动求职系统（Santiago，45.2K ⭐），14 种 Skill 模式，A-F 评分，ATS 简历，Go TUI Dashboard

**更新页面**: 2
- `entities/wps365-cli.md` — 大幅扩展：认证命令表格、认证模式表格、环境变量表格、Dry Run 示例、更新日期
- `entities/typeui-design-md-extractor.md` — 扩展：操作表格、文件结构表格、相关链接节、更新日期

**index.md**: 更新 total pages 168→170，新增 2 条 entity entries（vercel-ai-deploy、career-ops）

## [2026-05-27] batch 6 — 5 篇 raw 文章编译

**触发**: 用户请求处理 5 篇 raw 文章
**源文章**: 5 篇
- `raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md`
- `raw/articles/2026-05-11-x-how-to-build-first-ai-agent-10k-plus.md`
- `raw/articles/2026-05-13-skill-engineering-design.md`
- `raw/articles/2026-05-17-8-github-open-source-projects.md`
- `raw/articles/2026-05-19-beautiful-practical-frontend-guide.md`

**跳过（已在之前的编译中处理）**:
- Effective Harnesses → 已编译为 `concepts/long-running-agent.md`（batch 8, 2026-05-23）
- AI Agent 构建教程 → 已编译为 `entities/claude-managed-agents.md` + `concepts/agent-building-tutorial.md`（batch 5, 2026-05-22）
- Skill 工程化设计 → 已编译为 `concepts/skill-engineering.md`（batch 大规模编译 #2, 2026-05-17）

**新建页面**: 7
- `entities/local-deep-research.md` — 本地运行深度研究工具（Qwen3.6-27B SimpleQA 95.7%，20+ 策略，MCP Server）
- `entities/agentmemory.md` — AI 编程助手长期记忆服务器（四层记忆架构，三流混合检索 R@5 95.2%，9k+ Star）
- `entities/ruflo.md` — Claude Code 编排平台（100+ Agent 集群，Raft/拜占庭共识，SONA 神经架构，5.1 万 Star）
- `entities/ai-to-earn.md` — 内容营销全链路工具（Monetize+Publish+Engage+Create 四模块，13 平台分发）
- `entities/ui-tars-desktop.md` — 字节跳动多模态桌面 Agent（Computer Use 开源替代，UI-TARS Desktop + Agent TARS 双产品）
- `entities/vibe-coding-course.md` — Datawhale Vibe Coding 渐进式教程（3+1 阶段，交互式 Vue 组件，1 万+ Star）
- `entities/frontend-design-workflow.md` — 前端设计工作流方法论（IA→Visual Schema→Stitch→动态设计四步流程）

**更新页面**: 1
- `entities/academic-research-skills.md` — 新增来源、详细规格（45 Agent、742 测试用例、四个核心 Skill 表格）

**index.md**: 更新 total pages 161→168，新增 7 条 entity entries


## [2026-05-27] batch 7 — 5 篇 raw 文章编译

**触发**: 用户请求处理 5 篇 raw 文章
**源文章**: 5 篇
- `raw/articles/2026-05-19-codex-goals-guide.md`
- `raw/articles/2026-05-21-datawhale-ai-agent-learning-roadmap.md`
- `raw/articles/2026-05-21-xhs-agent-projects-recommendation.md`
- `raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md`
- `raw/articles/2026-05-26-a-stock-data-agent-a-share.md`

**跳过（已在之前的编译中处理）**:
- Datawhale Agent 学习路线 → 已编译为 `concepts/agent-learning-roadmap.md`（batch 5, 2026-05-25）
- XHS Agent 项目推荐 → Aider/GPT Researcher/HolmesGPT/Letta 四个实体已创建（2026-05-21/22）
- TencentDB Agent Memory → 实体页已存在（2026-05-23），本次追加详细实验数据和架构分析

**新建页面**: 2
- `entities/codex-goals.md` — Codex Goals 持久目标机制完整指南（六要素模板、强/弱 Goal 对比、Deep Hedging 复现实战、内部架构）
- `entities/a-stock-data-agent.md` — A 股全场景数据 Agent 技能包（28 端点、零依赖、全免 Key、即插 Skill）

**更新页面**: 3
- `entities/openai-codex.md` — 新增 codex-goals-guide 来源，Goals 章节追加 [[codex-goals]] 链接，新增相关链接
- `entities/tencentdb-agent-memory.md` — 大幅扩展：新增压缩哲学章节（稀疏→稠密、符号压缩三原则）、上下文卸载与其他方案对比、无限画布核心含义、MMD 节点详细结构、分级找回机制、四组详细实验数据（含主模型/Offload 模型/具体数值）、[[manus]] 关联
- `index.md` — 新增 2 条 entity entries，total pages 158→161

## [2026-05-27] batch 4 — 5 篇合集文章编译

**触发**: 用户请求处理 5 篇 raw 合集文章
**源文章**: 5 篇
- `raw/articles/2026-04-18-11-hot-github-projects-this-week.md`
- `raw/articles/2026-04-18-58-website-styles-10days-40k-stars.md`
- `raw/articles/2026-04-20-5-low-key-awesome-github-projects.md`
- `raw/articles/2026-04-21-5-treasure-github-projects.md`
- `raw/articles/2026-04-21-github-top10-weekly-stars.md`

**跳过（非 AI 领域或已在之前批次处理）**:
- Bye-Mac-App, type4me, sidex, awesome-systematic-trading, developer-icons — 非 AI 领域通用工具（与 batch 11 一致）
- awesome-remote-job, TREK, katana — 非 AI 领域通用项目（与 batch 11 一致）
- finance-skills, bb-browser — 已在 batch 11 创建
- Kronos, ai-hedge-fund, andrej-karpathy-skills, markitdown, claude-mem, multica, agent-skills-addyosmani, VoxCPM, DeepTutor — 已在 batch 11 处理
- awesome-design-md — 已包含在 design-md.md entity 中

**新建页面**: 1
- `entities/taxhacker.md` — AI 驱动的记账算税工具（vas3k），支持 170+ 货币 + 本地 LLM 离线运行

**更新页面**: 10
- `entities/design-md.md` — 更新日期，确认已包含 article 2 来源
- `entities/ai-scientist-v2.md` — 更新日期
- `entities/vibevoice.md` — 更新日期
- `entities/onyx.md` — 更新日期
- `entities/claude-howto.md` — 更新日期
- `entities/oh-my-claudecode.md` — 更新日期
- `entities/oh-my-codex.md` — 更新日期
- `entities/last30days-skill.md` — 更新日期
- `entities/openscreen.md` — 更新日期
- `entities/timesfm.md` — 更新日期

**index.md**: 更新 total pages 158→159，新增 1 条 entity entry


## [2026-05-27] batch 5 — 合集文章 + 多 Agent 协作文章编译

**触发**: 用户请求处理 5 篇 raw 文章
**源文章**: 5 篇
- `raw/articles/2026-04-26-githubdaily-open-source-projects.md`
- `raw/articles/2026-04-21-hermes-multi-agent-collaboration-guide.md`
- `raw/articles/2026-04-18-multi-agent-collaboration-guide.md`
- `raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md`
- `raw/articles/2026-05-12-10-new-open-source-github-projects.md`

**跳过（已在之前的 log.md 中记录或不需编译）**:
- GitHubDaily 合集（已在 2026-04-26 log 中记录为数据源目录）
- yao-open-prompts（91 个中文提示词合集，非核心领域）
- HTML 幻灯片模板（无独立项目名）
- antirez（Redis 创造者，信息已包含在 ds4 页面中）

**新建页面**: 8
- `entities/ds4.md` — Mac 本地 DeepSeek V4 推理引擎（antirez，C/Metal，KV 缓存磁盘持久化，2-bit 量化）
- `entities/mirage.md` — AI Agent 统一虚拟文件系统（12+ 后端服务挂载到虚拟目录树）
- `entities/token-speed.md` — NVIDIA Blackwell 专精 LLM 推理引擎（LightSeek Foundation）
- `entities/dirty-frag.md` — Linux 内核本地提权漏洞链（确定性，通杀所有主流发行版）
- `entities/zero-native.md` — Vercel Labs 桌面应用框架（Zig 原生 + Web UI，Tauri 竞品）
- `entities/codex-plus-plus.md` — OpenAI Codex App 增强补丁（API Key 插件解锁 + 会话删除）
- `entities/cheat-on-content.md` — Claude Code 内容创作闭环评分 Skill（13 子 Skill）
- `entities/awesome-agentic-ai-zh.md` — AI Agent 中文学习路线图（7 阶段 + 2 轨道，三语对照）

**更新页面**: 7
- `concepts/multi-agent-collaboration.md` — 新增 SDD 多 AI 协同实践章节（Claude+Codex+Gemini 四步闭环）
- `entities/hermes-agent.md` — 新增 Hermes 多 Agent 实战踩坑详解（Profile 系统、Discord 三人小组、三大踩坑、delegate_task vs 真多 Agent）
- `entities/openspec.md` — 新增 SDD 多 AI 协同实战章节（六阶段工作流、SubAgent 架构、工具切换韧性）
- `concepts/spec-driven-development.md` — 新增企业实践案例（binxiong 团队跨境保险 SDD 全流程）
- `entities/deepseek.md` — 新增 ds4 本地推理引擎关联
- `concepts/agent-learning-roadmap.md` — 新增 awesome-agentic-ai-zh 互补路线
- `entities/mcp-ecosystem.md` — 新增 Mirage 详细信息（12+ 后端服务、SDK 适配层）

**index.md**: 更新 total pages 150→158，新增 8 条 entity entries


## [2026-05-25] lint | 自动日检 — 5 issues found, all fixed
- 触发: 新文章入库 2026-05-25-guangguang-github-claude-plugins-official.md（已在本次编译中处理）
- CRIT: 1 broken wikilink（jason-liu → [[instructor]]，已取消链接为纯文本）
- MED: 4 source-drift（sha256 不匹配，已重新计算修正）
- LOW: 3 oversized pages（claude-code 207行, hermes-agent 218行, multi-agent-collaboration 210行，暂不拆分）
- 修复后 lint: 0 CRIT/HIGH/MED, 仅剩 3 LOW oversized 警告
- 最终状态: 147 页面 | 98 raw articles | ✅ 健康

## [2026-05-25] ingest | Anthropic 官方 Claude Code 插件目录 + 公众号文章
- Sources:
  - raw/articles/2026-05-25-guangguang-github-claude-plugins-official.md（公众号「逛逛GitHub」文章）
  - raw/GitHub/anthropics-claude-plugins-official.md（GitHub 项目）
- 文章: 《让你的 Claude Code 满血复活，Anthropic 在 GitHub 上开源了个插件》
- GitHub 项目: anthropics/claude-plugins-official — Anthropic 官方 Claude Code 插件目录，27387 stars
- 核心插件: claude-code-setup（项目分析推荐）、feature-dev（7 阶段结构化开发）、hookify（自然语言 Hooks 配置）、code-modernization（遗留代码现代化）
- 关键词: claude-code, plugins, anthropic, mcp, skills, hooks

## [2026-05-25] ingest | Jason Liu Codex 官方指南（2 篇）
- Sources:
  - raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
  - raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
- 新建页面: 1
  - entities/jason-liu.md — OpenAI Codex 团队 Developer Experience Engineer，Instructor 作者
- 更新页面: 4
  - entities/openai-codex.md — 大幅扩展高级功能（Durable Threads、Steering/Queuing、Thread Automations、Goals、Side Panel、Shared Memory、移动端）
  - concepts/long-running-agent.md — 新增 Codex 长程 Agent 实践章节
  - concepts/agent-memory-systems.md — 新增 Codex Shared Memory 方案章节
  - entities/obsidian.md — 新增 Codex-Obsidian 集成关联
- index.md: 更新 total pages 145→146，新增 1 条 entry
- 主题: OpenAI Codex 高级功能、Agent 记忆系统、长程 Agent 设计模式

## [2026-05-21] ingest | 完备的 AI Agent 学习路线（Datawhale）
- Source: 微信公众号（Datawhale）
- File: raw/articles/2026-05-21-datawhale-ai-agent-learning-roadmap.md
- 作者: 陈思州（Datawhale成员）
- 内容: AI Agent 系统学习路线 TODO list，9 个阶段（Agent 基础→工具调用→Harness→多Agent→Skills/MCP/A2A→Browser Agent→评测→部署），收录 Claude Code/OpenClaw/Hermes 等项目，附开源仓库 Agent-Learning-Hub
- 关键词: agent-learning, agent-roadmap, hermes, claude-code, mcp, a2a

## 2026-05-24 — 批量编译 #11（GitHub 项目合集/OCR/知识库）

**触发**: 用户请求处理 5 篇文章
**源文章**: 5 篇
- `raw/articles/2026-04-20-5-low-key-awesome-github-projects.md`
- `raw/articles/2026-04-21-5-treasure-github-projects.md`
- `raw/articles/2026-04-21-chandra-ocr-handwriting-recognition.md`
- `raw/articles/2026-04-21-github-top10-weekly-stars.md`
- `raw/articles/2026-04-23-xhs-autowiki-paper-knowledge-base.md`

**跳过（超出领域/不重要）**:
- Bye-Mac-App, type4me, sidex, awesome-systematic-trading, developer-icons — 非 AI 领域通用工具
- awesome-remote-job, TREK, katana — 非 AI 领域通用项目
- dots.ocr — Chandra 文章中的次要对比提及，不够创建阈值

**新建页面**: 5
- `entities/chandra.md` — 开源 OCR 系统，手写体识别突出，40+ 语言，档案数字化首选
- `entities/kronos.md` — 金融市场语言基础模型，量化交易专用
- `entities/ai-hedge-fund.md` — AI 对冲基金模拟系统，多 Agent 投资决策
- `entities/finance-skills.md` — 金融分析 AI Agent 技能工具集
- `entities/bb-browser.md` — 浏览器登录态封装为 API，AI Agent 数据获取工具

**更新页面**: 9
- `entities/autowiki.md` — 新增 sources（github-top10-weekly-stars），更新日期，新增社区反馈
- `entities/hermes-agent.md` — 新增 source（github-top10-weekly-stars），一周涨星 51K
- `entities/andrej-karpathy-skills.md` — 新增 source（github-top10-weekly-stars），一周涨星 37.4K
- `entities/markitdown.md` — 新增 source（github-top10-weekly-stars），一周涨星 14.5K
- `entities/claude-mem.md` — 新增 source（github-top10-weekly-stars），一周涨星 12.4K
- `entities/multica.md` — 新增 source（github-top10-weekly-stars），一周涨星 10.6K
- `entities/agent-skills-addyosmani.md` — 新增 source（github-top10-weekly-stars），一周涨星 6.4K
- `entities/voxcpm.md` — 新增 source（github-top10-weekly-stars），一周涨星 6.3K
- `entities/deep-tutor.md` — 新增 source（github-top10-weekly-stars），一周涨星 4.5K

**index.md**: 更新 total pages 129→134，新增 5 条 entries

## 2026-05-23 — 批量编译 #10（Lovart / MinerU / PPT Master / stop-slop / 公众号排版 Skills）

**触发**: 用户请求处理 5 篇文章
**源文章**: 5 篇
- `raw/articles/2026-04-18-lovart-brand-design-features.md`
- `raw/articles/2026-04-18-mineru-pdf-conversion-tool.md`
- `raw/articles/2026-04-18-ppt-master-ai-editable-pptx.md`
- `raw/articles/2026-04-18-stop-slop-remove-ai-flavor-skill.md`
- `raw/articles/2026-04-18-wechat-article-unknown-title-1.md`

**新建页面**: 2
- `entities/lovart.md` — Lovart AI 原生品牌设计工具（Font Generator / Brand Kit / Create Skill / PSD 导出）
- `entities/wechat-article-skills.md` — 公众号自动化排版发布 Skills（样式提取 / 排版重构 / 草稿箱推送）

**更新页面**: 4
- `entities/ppt-master.md` — 从 19 行 stub 扩展为完整实体页（v2.3.0 详细功能、技术细节、创作者背景）
- `entities/mineru.md` — 添加新 source 引用，统一 MarkItDown 为 wikilink
- `entities/stop-slop.md` — 添加新 source，更新 tags 添加 skill
- `concepts/anti-slop-writing.md` — 添加新 source，更新 tags 添加 skill

**index.md**: 更新 total pages 124→126，新增 2 条 entries，更新 ppt-master 描述

## 2026-05-23 — 批量编译 #9（.claude 文件夹 / 横纵分析 / Figma vs Pencil / Harness 源码 / 卡兹克 Skill）

**触发**: 用户请求处理 5 篇文章
**源文章**: 5 篇
- `raw/articles/2026-04-18-claude-research-10x-better.md`
- `raw/articles/2026-04-18-deep-research-prompt.md`
- `raw/articles/2026-04-18-figma-vs-pencil-claude-code.md`
- `raw/articles/2026-04-18-harness-engineering-source-code.md`
- `raw/articles/2026-04-18-kazike-creative-skill-open-source.md`

**新建页面** (7):
- `entities/claude-code-hooks.md` — Claude Code Hooks 确定性控制系统
- `entities/pencil.md` — Pencil AI 原生设计工具
- `entities/hv-analysis.md` — 横纵分析法研究方法论
- `entities/khazix-writer.md` — 卡兹克风格创作 Skill
- `concepts/claude-code-folder-structure.md` — .claude 文件夹结构
- `concepts/context-compression-pipeline.md` — 上下文压缩管道

**更新页面** (4):
- `entities/claude-md.md` — 新增 CLAUDE.md 内容建议和 CLAUDE.local.md 说明
- `entities/figma.md` — 新增 MCP Server 详细信息、Code Connect、Pencil 对比
- `entities/skills.md` — 新增 Skill 集成方式、创作 Skill 迭代方法论
- `concepts/harness-engineering.md` — 新增六大工程支柱（来自源码分析）

**index.md**: 总页数 117 → 124

## 2026-05-23 — 批量编译 #8（Claude Design/Harness/长程 Agent）

**触发**: 用户请求处理 5 篇文章（Claude Design 系统提示词 + Anthropic Harness 指南）
**源文章**: 5 篇
- `raw/articles/2026-04-19-claude-design-system-prompt-leak-analysis.md`
- `raw/articles/2026-04-19-claude-design-system-prompt-bilingual.md`
- `raw/articles/2026-04-19-claude-design-impact-on-ai-design-vendors.md`
- `raw/articles/2026-05-07-anthropic-harness-guide-dead-weight.md`
- `raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md`

### 新建页面（4）
- `concepts/long-running-agent.md` — 跨 context window 长程 Agent 概念，Initializer + Coding Agent 模式
- `concepts/ai-slop-design.md` — AI 生成 UI 的视觉指纹（渐变、emoji、圆角+边框），Claude Design 负面清单
- `entities/lance-martin.md` — Anthropic 工程师，Harness 三原则 + Build to Delete + 宝可梦实验
- `entities/pliny-the-liberator.md` — 安全研究员，CL4R1T4S 仓库泄露 Claude Design 系统提示词

### 更新页面（4）
- `entities/claude-design.md` — 新增 Handoff to Claude Code、Target Users、与之前路线的区别
- `concepts/harness-engineering.md` — 新增长程 Agent Harness 详解、宝可梦实验、增量进度原则
- `entities/boris-cherny.md` — 新增产品方法论（先建原型不写 PRD、10 天做 Cowork）
- `entities/anthropic.md` — 新增 Harness 指南和长程 Agent 文章来源

### index.md 更新
- Total pages: 111 → 115
- 新增 4 个条目到对应分区

## 2026-05-23 — 批量编译 #7（论文写作/学术工具）

**触发**: 用户请求处理 5 篇新文章（论文写作/学术工具主题）
**源文章**: 5 篇

### 新建页面（5）
- `entities/academic-research-skills.md` — Cheng-I Wu 的 Claude Code 学术研究 Skill 套件，12-agent + 13-agent，完整性验证制度化
- `entities/aris.md` — Auto-Research-In-Sleep 全自动科研 Skill，17 个可组合 Skill，跨模型协作
- `entities/autofigure-edit.md` — 西湖大学张岳实验室 AI 论文绘图系统（ICLR 2026），SVG 矢量编辑
- `entities/overleaf.md` — 在线 LaTeX 协作编辑平台，Overleaf + Claude Code + GitHub 三件套
- `concepts/academic-paper-integrity.md` — 学术论文完整性验证概念，引用/数据/论断三维度核查

### 更新页面（4）
- `concepts/ai-research-workflow.md` — 添加 5 个新来源、论文绘图自动化节、五 Skill 组合节、完整性验证节、更新工具表和开放问题
- `entities/ai-scientist-v2.md` — 添加与 academic-research-skills 和 aris 的交叉引用、成本信息
- `entities/scientific-research-skills.md` — 添加 1 个新来源、3 个新相关链接
- `index.md` — 添加 5 个新页面条目，总数 103 → 108

### 来源文章
1. raw/articles/2026-04-18-academic-paper-auto-writing-skill.md
2. raw/articles/2026-04-18-five-skills-paper-writing.md
3. raw/articles/2026-04-18-overleaf-claude-code-latex-paper.md
4. raw/articles/2026-04-18-aris-auto-experiment-paper.md
5. raw/articles/2026-04-18-westlake-university-ai-paper-drawing.md

## 2026-05-22 — 批量编译 #6（Claude Code 深度使用）

**触发**: 用户请求处理 5 篇新文章
**源文章**: 5 篇（1 篇抓取失败，实际处理 4 篇）

### 新建页面（3）
- `entities/boris-cherny.md` — Claude Code 创造者，Anthropic 核心开发者
- `entities/nexus4cc.md` — 手机远程操控 Claude Code 的开源工具（WebSocket + tmux）
- `concepts/claude-code-session-management.md` — 会话管理五条岔路决策框架（Thariq Shihipar）

### 更新页面（6）
- `entities/claude-code.md` — 添加 4 个新来源、Remote Access & Mobile 节、隐藏功能节、Session Management 节；Boris Cherny → wikilink
- `entities/claude-code-slash-commands.md` — 添加 3 个新来源、远程访问/交互增强/并行批量/自动化Hook 4 个新命令分类、会话管理决策框架
- `entities/agent-teams.md` — 添加 1 个新来源、社区实践节（创建方式、协作模式、与 Subagent 区别）
- `entities/anthropic.md` — 添加 2 个新来源、Boris Cherny → wikilink、新增 Thariq Shihipar 信息
- `concepts/context-engineering.md` — 添加 2 个新来源、Session Management 策略节
- `index.md` — 添加 3 个新页面条目，总数 93 → 96

### 跳过
- `raw/articles/2026-04-18-claude-code-research-skills.md` — 小红书抓取失败，无内容可提取

### 来源文章
1. raw/articles/2026-04-18-claude-code-creator-15-hidden-features.md
2. raw/articles/2026-04-18-claude-code-hidden-commands.md
3. raw/articles/2026-04-18-claude-code-mobile-remote.md
4. raw/articles/2026-04-18-claude-code-session-management.md
5. raw/articles/2026-04-18-claude-code-research-skills.md (skip — fetch failed)

## 2026-05-22 — 批量编译 #5（Hermes/CUA/工具）

**触发**: 用户请求处理 5 篇新文章
**源文章**: 5 篇

### 新建页面（3）
- `entities/mano-p.md` — Mano-P 纯视觉驱动 CUA（明略科技）
- `entities/claude-managed-agents.md` — Claude Managed Agents 基础设施层
- `concepts/agent-building-tutorial.md` — Agent 构建实战方法论

### 更新页面（4）
- `entities/hermes-agent.md` — 添加 Skill 自我进化、三层联动记忆、Gateway 集成等详细内容；添加 turix-cua-agent-skill 来源
- `entities/computer-use-agent.md` — 添加 Mano-P 实现、更新来源和 wikilinks
- `entities/turix-cua.md` — 添加 Mano-P 对比表、应用场景扩展、更新 wikilinks
- `index.md` — 添加 3 个新页面条目，总数 69 → 72

### 来源文章
1. raw/articles/2026-04-18-hermes-agent-chinese-community-feishu.md
2. raw/articles/2026-04-18-hermes-agent-lobster-hermes.md
3. raw/articles/2026-04-18-github-open-source-control-computer-skill.md
4. raw/articles/2026-04-21-turix-cua-agent-skill.md
5. raw/articles/2026-05-11-x-how-to-build-first-ai-agent-10k-plus.md

## 2026-04-23 — 大规模编译 #1

**触发**: 用户请求修复并开始编译
**源文章**: 70 篇（69 有效，1 索引文件）
**编译耗时**: ~15 分钟（并行子代理）

### Phase 1: 实体和概念提取
- 扫描全部 68 篇有效文章（排除 OPEN-SOURCE-PROJECTS-INDEX.md）
- 提取 **180+ 实体**（公司、产品、工具、框架、人物）
- 提取 **70+ 概念**（范式、方法论、工作流）
- 输出: `raw/entities-concepts-extraction.md`（393 行）

### Phase 2: Entity 页面创建（20 个）
**高频实体（5+ 篇文章提及）**:
1. `entities/anthropic.md` — Anthropic 公司
2. `entities/claude-code.md` — Claude Code 编程 Agent（25+ 篇）
3. `entities/claude-design.md` — Claude Design 设计工具
4. `entities/claude-model-family.md` — Claude 模型家族（30+ 篇）
5. `entities/claude-md.md` — CLAUDE.md 配置文件（10+ 篇）
6. `entities/mcp.md` — Model Context Protocol（9 篇）
7. `entities/openclaw.md` — OpenClaw 多 Agent 平台（8 篇）
8. `entities/skills.md` — Agent Skills 体系（15+ 篇）

**中频实体（2-4 篇文章提及）**:
9. `entities/hermes-agent.md` — Hermes Agent
10. `entities/cursor.md` — Cursor IDE
11. `entities/figma.md` — Figma
12. `entities/knowledge-compilation.md` — 知识编译方法论
13. `entities/nousresearch.md` — NousResearch 实验室
14. `entities/video-use.md` — Video-Use 视频剪辑
15. `entities/stitch.md` — Google Stitch
16. `entities/andrej-karpathy.md` — Andrej Karpathy

**单篇但核心实体**:
17. `entities/autowiki.md` — AutoWiki 论文知识库
18. `entities/computer-use-agent.md` — Computer Use Agent
19. `entities/mineru.md` — MinerU PDF 转换
20. `entities/stop-slop.md` — Stop-Slop 反 AI 味

### Phase 3: Concept 页面创建（8 个）
1. `concepts/vibe-coding.md` — Vibe Coding 编程范式
2. `concepts/vibe-design.md` — Vibe Design 设计范式
3. `concepts/context-engineering.md` — Context Engineering
4. `concepts/multi-agent-collaboration.md` — 多 Agent 协作
5. `concepts/document-first-system.md` — Document-First 开发方法论
6. `concepts/ai-native-development.md` — AI Native 开发
7. `concepts/ai-research-workflow.md` — AI 研究工作流
8. `concepts/anti-slop-writing.md` — Anti-Slop 写作

### Phase 4: Comparison 页面创建（3 个）
1. `comparisons/claude-code-vs-openclaw-vs-hermes.md` — 三大 Agent 平台对比
2. `comparisons/vibe-coding-vs-ai-native.md` — 两种编程范式对比
3. `comparisons/claude-design-vs-traditional-tools.md` — AI 设计 vs 传统工具对比

### 统计
- 总页面: 31（20 entities + 8 concepts + 3 comparisons）
- 总行数: ~2,800 行
- Wikilinks: 200+ 条交叉引用
- Tags 覆盖: model, architecture, tool, agent, code, open-source, company, lab, person, data, rag, nlp, multimodal, prompt-engineering, optimization, comparison, tutorial

### 待办（下次编译）
- [ ] 为高频实体（Claude Code, OpenClaw, Hermes）创建更详细的专题页
- [ ] 创建 Query 页面（高频问题 + 跨页面综合回答）
- [ ] 补充更多人物页面（Boris Cherny, 鲁工, 袋鼠帝 等）
- [ ] 创建 Timeline 页面（AI Agent 发展时间线）
- [ ] 为 GitHub 项目合集创建分类 Entity 页面

## [2026-04-24] ingest | 开源一个 PPT Skill｜压进了我 10 年的设计经验
- Source: 微信公众号（歸藏的 AI 工具箱）
- File: raw/articles/2026-04-24-guizang-ppt-skill-10-year-design-experience.md
- 歸藏开源了 guizang-ppt-skill（github.com/op7418/guizang-ppt-skill），定义了"电子杂志 × 电子墨水"风格的 PPT 生成 Skill，包含 10 种布局、5 套主题色、完整翻页交互，产物为单文件 HTML

## [2026-04-26] ingest | GitHubDaily 开源项目推荐合集
- Source: GitHub (https://github.com/GitHubDaily/GitHubDaily)
- File: raw/articles/2026-04-26-githubdaily-open-source-projects.md
- 完整仓库克隆: raw/GitHubDaily/（2018-2025 年按年分类，共 10000+ 开源项目）
- 46.2k stars，涵盖 AI 工具、开发工具、学习教程、资料集合、实用工具等 11 个分类
- 以后查找 GitHub 开源项目可搜索此目录

## [2026-05-02] ingest | 舒服了！Claude Code + Gemma 4 一键理清Mac 30000+图片
- Source: 微信公众号（字节笔记本）
- File: raw/articles/2026-05-02-claude-code-gemma4-mac-image-manager.md
- 用 Claude Code 10 分钟开发的本地 AI 图片管家，核心是 Gemma 4 多模态能力（本地 Ollama 部署），支持自然语言检索图片、批量清理重复/无效截图、自动标签分类，类似离线版 Google Photos
- 提及 Vibe Coding 趋势：「软件开始日抛」，10-30 分钟就能做出够用的个人化工具

## [2026-05-07] ingest | Effective harnesses for long-running agents
- Source: Anthropic Engineering Blog
- File: raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md
- Anthropic 官方提出 Initializer Agent + Coding Agent 双阶段架构解决长时运行 Agent 跨上下文窗口问题：Initializer 搭建环境（init.sh、progress.txt、feature list JSON、初始 git commit），Coding Agent 每次会话读 progress+git log→启动 dev server→跑基础测试→做一个 feature→commit+更新 progress。feature list 用 JSON 非 Markdown，每次只做一个 feature，离开前必须干净状态

## [2026-05-07] ingest | Anthropic Harness 指南：到期清理、别帮倒忙
- Source: 微信公众号（AGI Hunt）
- File: raw/articles/2026-05-07-anthropic-harness-guide-dead-weight.md
- Lance Martin 提出 Harness Engineering 的「到期清理」原则：用 Claude 已会的（bash+文本编辑器）、问自己还能停掉什么（编排/上下文/记忆都该把控制权还给 Claude）、该设的边界还是要设（缓存策略/安全边界/可观测性）。核心概念 dead weight：模型变强后，为补偿弱点搭的基础设施反而拖累性能。Build to delete，护栏该装但 dead weight 该拆

## [2026-05-07] ingest | 再见了PowerPoint！以后的PPT都交给它了
- Source: 微信公众号（Draco正在VibeCoding）
- File: raw/articles/2026-05-07-open-slide-replace-powerpoint.md
- 介绍 Open-Slide 项目（github.com/1weiho/open-slide）：把仓库地址发给 Agent（Claude Code/Codex/Hermes 等）即可让 Agent 生成美观的 Web 版 PPT。同时提到 Kami（tw93/Kami，出版物级 formatting 项目）和 Open-Design（nexu-io/open-design，Claude Design 平替）。作者还封装了 skills 包含 11 套原生模板 + 38 套从 Open-Design 移植的模板。核心观点：Taste 是 Agent 时代的稀缺品，Agent 不需要视觉美感但人需要

## [2026-05-12] ingest | 分享5个Claude Code + 飞书的超实用Agent办公玩法
- Source: 微信公众号（数字生命卡兹克）
- File: raw/articles/2026-05-12-claude-code-feishu-agent-workflows.md
- 5个Claude Code + 飞书CLI的Agent办公玩法：(1)飞书CLI+Agent做个人知识管理，120GB+文件云存储+自然语言搜索；(2)飞书多维表格+Agent做数据看板，自动查询/写入/分析数据；(3)Agent自动阅读总结飞书长文档，15万字技术方案30秒掌握核心；(4)Agent自动生成飞书PPT汇报，5分钟替代2小时人工；(5)Agent辅助飞书多人协同编排，自动整合内容保持格式统一。核心观点：Agent+飞书组合才是AI办公未来

## [2026-05-08] ingest | 我用Obsidian 给 Coding Agent装了一块硬盘！它终于不再失忆了
- Source: 微信公众号（字节笔记本）
- File: raw/articles/2026-05-08-obsidian-coding-agent-long-term-memory.md
- 提出用 Obsidian 作为 Coding Agent 的长期记忆库，解决跨会话失忆问题。三层架构：AGENTS.md/CLAUDE.md（入口规则）→ Obsidian 项目笔记（decisions.md、errors.md、todo.md、overview.md）→ sessions/YYYY-MM-DD.md（会话日志）。配合 Obsidian CLI + Claude Code 自定义命令 /init-memory 和 /save-memory 实现自动化记忆读写工作流

## [2026-05-11] ingest | How to Build Your First AI Agent That Companies Will Pay $10K+ For (Full Course)
- Source: X/Twitter (@eng_khairallah1)
- File: raw/articles/2026-05-11-x-how-to-build-first-ai-agent-10k-plus.md
- Khairallah 的 AI Agent 构建完整教程，以 Claude Managed Agents 为核心工具，涵盖7步构建流程、工具链接入、失败模式处理、自动化调度和多Agent编排

## [2026-05-11] ingest | 开源「伯乐Skill」，让你和Agent同时进化成AI热点懂王！
- Source: 微信公众号（AI沃茨）
- File: raw/articles/2026-05-11-bole-skill-ai-news-radar.md
- 开源项目 ai-news-radar（github.com/LearnPrompt/ai-news-radar），伯乐Skill 能自动判断信息源最佳接入方式（RSS/API/网页）、7天观察期过滤重复源（差异度>65%才保留）、支持9类信源22个默认源。理念：千里马常有而伯乐不常有，Agent应学会判断信息源质量而非广撒网

## [2026-05-12] ingest | 盘点 10 个刚刚开源，但 Star 攀升很快的 GitHub 项目
- Source: 微信公众号（逛逛GitHub）
- File: raw/articles/2026-05-12-10-new-open-source-github-projects.md
- 盘点10个新开源GitHub项目：ds4（antirez, Mac本地跑DeepSeek V4, KV缓存磁盘持久化）、Mirage（AI Agent统一虚拟文件系统）、yao-open-prompts（91个中文提示词）、cheat-on-content（Claude Code内容创作闭环评分）、TokenSpeed（Blackwell专精LLM推理引擎）、32套HTML幻灯片模板、awesome-agentic-ai-zh（AI Agent中文学习路线图）、Codex++（OpenAI Codex App增强补丁）、Dirty Frag（Linux内核提权漏洞链）、zero-native（Vercel Labs Tauri竞品）

## [2026-05-11] ingest | Harness不是目的，知识才是护城河 —— 一个AI工程交付团队的知识沉淀实践
- Source: 微信公众号（腾讯技术工程）
- File: raw/articles/2026-05-11-harness-engineering-knowledge.md
- 腾讯 AI Team 团队分享 Harness Engineering 实践：五层知识存储（个人→项目）× 五种类型（model/decision/guideline/pitfall/process）× 三级成熟度（draft→verified→proven）+ 自动衰减。独立 Git 仓库作为团队知识库，工作流每个阶段与知识流动关联（INIT注入→按需查询→ARCHIVE提取），三级渐进式索引解决上下文膨胀问题。核心观点：Skill/Agent/工具链会随模型迭代更新，但领域知识是永恒的

## [2026-05-13] ingest | Kami：让 AI 生成的文档，终于有了值得一看的排版
- Source: 微信公众号（开源小聪明）
- File: raw/articles/2026-05-13-kami-ai-document-typography.md
- tw93 开源的 Kami，一个 Claude Code Skill 形式的 AI 文档设计系统。用自然语言描述需求，自动生成带高质量排版的 PDF（米色背景、深色衬线字体、统一视觉质感）。支持 serif/sans 字体切换、中英文版本、多种文档模板（Tesla 一页纸、Agent Slides、Musk Resume、Kaku Portfolio）。作者 tw93（Pake/Kaku/Waza 作者）的理念是"好内容值得好纸面"

## [2026-05-13] ingest | hue — AI 编码 Skill，从品牌 URL/截图学习并生成完整设计系统
- Source: GitHub（dominikmartn/hue）
- File: raw/GitHub/dominikmartn-hue.md
- 开源 Claude Code / Codex Skill，从 URL、名称或截图学习任意品牌，自动生成完整设计系统（颜色 token、排版、间距、组件、明暗模式）。安装一次后，AI 助手生成的所有 UI 自动匹配品牌风格。内置 17 个品牌示例（atlas/auris/drift/fizz/halcyon/kiln/ledger/meadow/orivion/oxide/prism/relay/ridge/solvent/stint/thrive/velvet），每个含 design-model.yaml + landing-page.html

## [2026-05-13] ingest | 当我把 AI 变成一个"算法"：Skill 工程化设计的心路历程
- Source: 微信公众号（腾讯技术工程，作者 peihanyu）
- File: raw/articles/2026-05-13-skill-engineering-design.md
- 腾讯工程师分享 Skill 工程化设计实践。核心理念：不改变 LLM 的概率本性，但通过确定性执行环境（CLI）包裹不确定性。三大支柱：①CLI 接管所有确定性操作（API 调用、字段校验、认证），Agent 只做决策；②热更新 discover 机制——工具列表实时同步、规则自动生成（IGNORE/ENUM）、三层信息分离（索引/元数据/规则），50 个工具 Agent 上下文只多一张表格；③Workflow 工作流引擎——步进式披露（Agent 永远只看当前步）、Gate 门禁 schema（开放题变填空题）、状态持久化到磁盘 JSON、模板变量实现步骤间数据流、三种步骤类型（interactive/automated/notification）协奏。Workflow 定义为文件系统上的 Markdown 文件，业务人员可复制粘贴创建新流程。还实现了 workflow-creator Skill（用 Skill 创造 Skill 的自举闭环）

## [2026-05-14] ingest | Anthropic 开源金融 Skills！华尔街分析师的活装进了插件包
- Source: 微信公众号（字节笔记本）
- File: raw/articles/2026-05-14-anthropic-financial-skills.md
- Anthropic 开源面向金融行业的 Claude 插件包（claude-for-financial-services），覆盖投研、财报分析、DCF/LBO模型、PE尽调、KYC审核等场景。包含 Pitch Agent、Meeting Prep Agent、Market Researcher、Earnings Reviewer、Model Builder 等岗位模块。展示了垂直行业 Agent 应如何组织：system prompt / skill / subagent / MCP 数据源 / 风险边界的设计范式。Claude Code 可直接安装使用。核心价值：企业级垂类 Agent 的完整样板

## [2026-05-17] ingest | 推荐 8 个本周 YYDS 的 GitHub 开源项目
- Source: 微信公众号（逛逛GitHub）
- File: raw/articles/2026-05-17-8-github-open-source-projects.md
- 本周 8 个热门开源项目推荐：①Local Deep Research（本地深度研究，Qwen3.6-27B+MCP Server）②Anthropic 金融 Agent 模板库（10 个预构建 Agent，2.2w Star）③agentmemory（AI 编程助手长期记忆，四层架构，9k+ Star）④Ruflo（Claude Code 编排平台，100+ Agent 集群，5.1w Star）⑤AiToEarn（内容营销全链路，13 平台分发）⑥UI-TARS Desktop（字节跳动多模态桌面 Agent）⑦Vibe Coding 渐进式教程（Datawhale，3+1 阶段）⑧Academic Research Skills（学术写作 Skills，45 Agent 协同）

---

## 2026-05-17 — 大规模编译 #2

**触发**: 用户请求编译 16 篇新 raw 文章（2026-04-24 至 2026-05-17）
**源文章**: 87 篇（新增 16 篇，含 1 篇 hue GitHub 项目）
**编译耗时**: ~25 分钟（并行子代理）

### Phase 1: 实体和概念提取
- 扫描 17 篇新文章（含原始请求的公众号文章）
- 提取实体和概念，分两批并行处理
- 输出: `raw/extracted_entities_concepts.json` + `raw/extracted_entities_concepts_batch2.json`

### Phase 2: 去重与决策
- 对比现有 31 个 wiki 页面
- 合并同类项：`Claude Agent SDK` + `Claude Managed Agents` → 更新 `claude-code.md`
- 删除重复：`harness-engineering` 同时存在于 entity 和 concept，删除 entity 版本
- 最终决策：新建 10 entity + 4 concept，更新 6 entity

### Phase 3: 页面创建和更新

**新建 Entity 页面（10 个）**:
1. `entities/agent-teams.md` — Agent Teams 多 Agent 并行协作
2. `entities/feishu.md` — 飞书企业协同办公平台
3. `entities/gemma-4.md` — Gemma 4 开源多模态模型
4. `entities/guizang-ppt-skill.md` — 歸藏杂志风 PPT Skill
5. `entities/hue.md` — 品牌设计 Skill
6. `entities/kami.md` — tw93 AI 文档设计系统
7. `entities/obsidian.md` — Obsidian 知识管理工具
8. `entities/open-slide.md` — Open-Slide 开源 PPT 工具
9. `entities/openai-codex.md` — OpenAI Codex 编程 Agent

**新建 Concept 页面（4 个）**:
1. `concepts/agent-memory-systems.md` — Agent 记忆系统
2. `concepts/harness-engineering.md` — Harness Engineering 脚手架工程方法论
3. `concepts/skill-engineering.md` — Skill 工程化设计
4. `concepts/vertical-industry-agents.md` — 垂直行业 Agent

**更新 Entity 页面（6 个）**:
1. `entities/claude-code.md` — 追加 Agent Teams / Managed Agents / Routines / SDK / 飞书集成
2. `entities/anthropic.md` — 追加 Managed Agents / 金融 Skills / Code with Claude
3. `entities/claude-model-family.md` — 追加 Opus 4.5 / 4.6 / Sonnet 4.5 性能数据
4. `entities/claude-design.md` — 追加 Open-Design 开源平替 / hue/Kami 生态
5. `entities/cursor.md` — 追加 Self-Driving / ds4 兼容性
6. `entities/mcp.md` — 追加 Mirage 虚拟文件系统 / 金融 Skills 数据源层

### Phase 4: Index 更新
- 更新计数：87 raw / 29 entities / 12 concepts / 3 comparisons
- 添加 16 篇新 raw 文章索引（按日期分组）
- 添加 10 个新 entity 索引 + 4 个新 concept 索引
- 更新已有 entity 的描述（Anthropic/Claude Code/Cursor/Claude 模型家族）

### 统计
- 总页面: 44（29 entities + 12 concepts + 3 comparisons）
- 较上次新增: +13 页面（+9 entities, +4 concepts）
- 更新已有页面: 6 个
- 新增 raw 文章: 16 篇
- Wikilinks: 所有新页面均包含 2-5 个出站 wikilinks

## [2026-05-18] ingest | Career-Ops — AI 求职系统
- Source: GitHub (santifer/career-ops)
- File: raw/GitHub/santifer-career-ops.md
- 45.2k stars，基于 Claude Code 的全自动求职管道：A-F 评分评估职位、ATS 优化 PDF 简历生成、45+ 公司门户扫描、Go TUI Dashboard、批量并行处理、面试故事银行、薪资谈判脚本


## [2026-05-19] ingest | 一文搞懂如何在Codex中使用goals
- Source: 微信公众号（AI寒武纪）
- File: raw/articles/2026-05-19-codex-goals-guide.md
- OpenAI Codex Goals 功能详解：Goals 作为 Codex 的结构化任务管理机制，支持并行执行、权限控制、benchmark 测量；与手动 prompt 对比 Goals 更高效；介绍安装使用、配置方法、实际示例（如 checkout 筛选匹配项）、管理命令（pause/resume/clear）、以及 Goals 在 CI/CD 中的应用场景

## [2026-05-19] lint | 218 issues found
- 🔴 CRIT: 39 broken wikilinks (targets don't exist as pages)
- ⚠️ HIGH: 44 pages missing from index.md + 11 orphan pages
- ⚠️ MED: 122 invalid tags (tag taxonomy in SCHEMA.md not aligned with actual usage)
- 💡 LOW: 1 oversized page (mcp: 201 lines) + 10 raw files without sha256
- log.md entries: 19 (OK, no rotation needed)

## 2026-05-19 — Wiki Lint 修复（218→0）

修复前：218 个 lint 问题（39 CRIT + 44 HIGH + 11 HIGH + 122 MED + 2 LOW）
修复后：**0 个问题，全部通过**

### 修复内容

| Phase | 操作 | 数量 |
|-------|------|------|
| 1 | SCHEMA.md 标签体系更新（+4 分组） | 10 新标签 |
| 2a | 创建缺失页面 | 15 新页面 |
| 2b | 修复断链映射 | 8 链接 |
| 3 | index.md 重建 | 60 页面 |
| 4 | 孤儿页面添加入站链接 | 11 页面 / 20 条链接 |
| 5a | 拆分 mcp 超长页 → mcp-ecosystem | 1 拆分 |
| 5b | raw 文件补 sha256 | 88 文件 |
| 6 | 最终 lint 验证 | 0 issues |

### 统计
- 总页面数：60（40 entities + 16 concepts + 3 comparisons）
- raw 文章：88 篇（全部含 sha256）
- 标签体系：41 个有效标签

## [2026-05-19] ingest | 如何做出美观且实用的前端，速成篇
- Source: 微信公众号
- Author: Mav高未央
- File: raw/articles/2026-05-19-beautiful-practical-frontend-guide.md
- 内容：AI 时代前端设计速成指南，涵盖 Antigravity（coding agent）、Stitch（UI 生成）、visual schema（信息架构可视化）、ux-designer（用户体验设计）、html in canvas 等工具/workflow 的使用方法


## [2026-05-19] ingest | Claude Code 斜杠命令完整指南
- Source: 微信公众号（程序员鱼皮）
- Author: 程序员鱼皮
- File: raw/articles/2026-05-19-claude-code-slash-commands-guide.md
- 内容：Claude Code 全部斜杠命令详解，涵盖 /clear、/compact、/resume、/branch、/fork、/rewind、/recap、/btw、/copy、/export、/exit、/usage、/context、/diff、/status、/help、/insights、/plan、/goal、/model、/effort、/fast、/config、/mcp、/skills、/plugin、/review、/simplify、/agents、/tasks、/background、/loop，以及高级用法（PR review、CI 集成等）

## [2026-05-20] ingest | 2 篇新文章编译

### raw/articles/2026-05-19-beautiful-practical-frontend-guide.md
- Author: Mav高未央
- AI 时代前端设计速成指南：四步工作流（IA→Visual Schema→Stitch→动态设计）
- 更新页面: concepts/vibe-design.md（+前端设计速成工作流章节）

### raw/articles/2026-05-19-claude-code-slash-commands-guide.md
- Author: 程序员鱼皮
- Claude Code 50+ 斜杠命令完整指南
- 更新页面: entities/claude-code.md（重写 Key Commands 章节，从 4 条命令扩充至摘要引用）
- 新建页面: entities/claude-code-slash-commands.md（从 claude-code.md 拆分出详细命令参考）
- 修复: 90 个 raw 文件 sha256 截断为 16 位（匹配 lint 脚本格式）


## [2026-05-20] ingest | 一键获取Nike/SpaceX的设计风格？试试DESIGNMD.sh
- Source: 微信公众号
- Author: DracoVibeCoding
- File: raw/articles/2026-05-20-designmd-sh-design-registry.md
- 内容：介绍 DESIGNMD.sh 网站，提供 20+ 知名品牌（Nike/SpaceX 等）的 DESIGN.md 设计规范文件，可用于指导 AI Agent 生成一致的 UI，支持 npx 命令一键安装到项目
- 更新: entities/design-md.md（+DESIGNMD.sh Registry 章节，补充来源引用）
- 补: raw/articles/2026-05-20-designmd-sh-design-registry.md sha256

## [2026-05-21] lint | 自动日检
- 新入库文章: 1 篇（2026-05-20-designmd-sh-design-registry.md）
- 修复: 1 个 raw 文件缺 sha256（已补）
- 更新: entities/design-md.md（+DESIGNMD.sh Registry 章节）
- 最终 lint 结果: 0 issues

## [2026-05-21] ingest | PinMe：一句话把网站发布到线上
- Source: 微信公众号（逛逛GitHub）
- Author: 逛逛
- File: raw/articles/2026-05-21-pinme-skill-one-click-deploy.md
- 内容：介绍 PinMe 2.0 开源项目，从静态页面部署工具升级为 AI Agent Skill，支持全栈应用一键部署（前端 SPA + Edge Runtime + Serverless SQL），可接入 Claude Code 等 AI Agent 实现自然语言驱动的开发-部署闭环。累计部署 100 万+网站。
- 项目地址：https://github.com/glittrernetwork/pinme

## [2026-05-21] ingest | Agent项目推荐：高质量开源项目
- Source: 小红书（摸鱼酱在coding）
- File: raw/articles/2026-05-21-xhs-agent-projects-recommendation.md
- 内容：推荐四个方向的 AI Agent 高质量开源项目——AI Coding（Aider, 44k stars, repo map 核心）、Deep Research（GPT-Researcher, 27k stars, planner/execution 分工）、AIOps（HolmesGPT, CNCF Sandbox, 只读 RBAC）、长期记忆（Letta/MemGPT, 22k stars, 虚拟内存管理 context window），每个项目附二改方向建议
- 互动：441 赞 / 850 收藏 / 6 评论

## [2026-05-21] 编译 | 5 篇 Agent 框架和多代理文章

**触发**: 用户请求编译 5 篇 raw 文章（Agent框架和多代理主题）
**源文章**:
1. raw/articles/2026-04-18-build-ai-agent-framework.md — 从零设计实现 AI Agent 框架
2. raw/articles/2026-04-18-multi-agent-collaboration-guide.md — Anthropic 多Agent协作模式指南
3. raw/articles/2026-04-18-multi-ai-sdd-coding-practice.md — 多AI协同 + SDD 编程实践
4. raw/articles/2026-04-18-openclaw-discord-ai-research-team.md — OpenClaw + Discord AI 科研团队
5. raw/articles/2026-04-18-openclaw-xiaohongshu-sop.md — 小红书全自动图文SOP

### 新建 Entity 页面（2 个）
1. `entities/manus.md` — Manus AI Agent 产品（Monica 公司，CodeAct 启发，文件系统作为上下文）
2. `entities/openspec.md` — OpenSpec SDD CLI 工具（Fission AI，规范驱动开发）

### 新建 Concept 页面（6 个）
1. `concepts/react-pattern.md` — ReAct 模式（推理+行动，Agent 最基础行为模式）
2. `concepts/plan-and-execute-pattern.md` — Plan-and-Execute 模式（先规划后执行）
3. `concepts/reflection-pattern.md` — Reflection 模式（自我反思改进，Reflexion/Self-Refine/CRITIC）
4. `concepts/agent-loop.md` — Agent Loop（While 循环核心运行机制）
5. `concepts/codeact.md` — CodeAct 模式（代码驱动执行）
6. `concepts/spec-driven-development.md` — SDD 规范驱动开发（GitHub 2025年提出）

### 更新已有页面（3 个）
1. `concepts/context-engineering.md` — +Agent 框架中的上下文工程章节，+Agent Loop 上下文管理，+文件系统作为上下文，+来源引用
2. `concepts/multi-agent-collaboration.md` — +Anthropic 5种协作架构模式（生成器-验证器/编排器-子智能体/智能体团队/消息总线/共享状态），+选型建议
3. `entities/openclaw.md` — +小红书 SOP 完整闭环（Nano Banana/OSS/飞书多维表格），+Discord 科研 5 Agent 实践，+安全注意事项

### Index 更新
- 总页面数：61 → 69（+8 新建）
- 新增 2 entity + 6 concept 条目
- 比较（comparisons）保持 3 个不变

## 2026-05-22 — 批量编译 #6（开源项目推荐合集）

**触发**: 用户请求处理 5 篇开源项目推荐文章
**源文章**: 5 篇
- raw/articles/2026-04-18-11-hot-github-projects-this-week.md
- raw/articles/2026-04-18-github-hot-10-open-source-projects.md
- raw/articles/2026-04-18-github-33k-knowledge-base-ai-brain.md
- raw/articles/2026-04-18-gsd2-auto-dev-tool.md
- raw/articles/2026-05-21-xhs-agent-projects-recommendation.md

### 新建页面（21）
- `entities/ai-scientist-v2.md` — Sakana AI 全自动科研系统
- `entities/vibevoice.md` — 微软开源语音 AI
- `entities/onyx.md` — 开源企业 AI 搜索
- `entities/claude-howto.md` — Claude Code 学习指南
- `entities/oh-my-claudecode.md` — Claude Code 多 Agent 编排
- `entities/oh-my-codex.md` — OpenAI Codex 多 Agent 编排
- `entities/last30days-skill.md` — 全网搜索 Skill
- `entities/openscreen.md` — 屏幕录制美化开源工具
- `entities/timesfm.md` — Google 时序预测模型
- `entities/rowboat.md` — 多 Agent 可视化 IDE
- `entities/multica.md` — Agent 团队任务管理
- `entities/agent-skills-addyosmani.md` — AI 编码工程纪律包
- `entities/archon.md` — AI 编码工作流引擎
- `entities/deep-tutor.md` — 港大 AI 学习助手
- `entities/andrej-karpathy-skills.md` — Karpathy 编码原则 CLAUDE.md
- `entities/claude-mem.md` — Claude Code 跨会话记忆
- `entities/markitdown.md` — 微软万物转 Markdown
- `entities/voxcpm.md` — 面壁智能 TTS 模型
- `entities/khoj.md` — 开源个人 AI 第二大脑
- `entities/gsd2.md` — 独立开发者 AI 编码工作流

### 更新页面（7）
1. `entities/hermes-agent.md` — +sources（文章1提及）
2. `entities/openclaw.md` — +sources（文章4提及 GSD2 对接）
3. `entities/aider.md` — +repo map 技术细节、+Star 数据
4. `entities/gpt-researcher.md` — +Planner/Execution 架构、+Star/成本数据
5. `entities/holmesgpt.md` — +CNCF Sandbox 信息、+RBAC 安全架构
6. `entities/letta.md` — +MemGPT 论文背景、+Star 数据
7. `entities/claude-md.md` — +sources（andrej-karpathy-skills 文章）

### Index 更新
- 总页面数：72 → 93（+21 新建）
- 新增 21 entity 条目

## 2026-05-22 — 批量编译 #7（Claude Code 进阶和插件）

**触发**: 用户请求处理 5 篇新文章
**源文章**: 5 篇

### 新建页面（3）
- `entities/everything-claude-code.md` — Claude Code 开源插件集合，132k Star，36 subagent + 150 skills
- `entities/superpowers.md` — Claude Code 多 agent 开发工作流 Skill，131k Star，TDD 强制方法论
- `concepts/context-rot.md` — 上下文腐烂概念，LLM 上下文变长后表现下滑

### 更新页面（4）
1. `entities/agent-teams.md` — 大幅扩充：前置条件、触发关键词、显示模式、Hooks 质量门、实测案例、与 Subagents 详细对比表
2. `concepts/claude-code-session-management.md` — +source、+context-rot wikilink、+1M Context 新建议、+Subagent 判断标准和 prompt
3. `entities/skills.md` — +10 个新 skill（frontend-design、firecrawl、web-interface-guidelines、mcp-builder、remotion-best-practices、pr-review、gws、/simplify、project-context、superpowers）
4. `entities/claude-code.md` — +5 个 sources、+8 条 best practices、+插件生态段落（superpowers、everything-claude-code、claude-code-best-practice）

### Index 更新
- 总页面数：96 → 99（+3 新建）
- 新增 2 entity 条目（everything-claude-code、superpowers）
- 新增 1 concept 条目（context-rot）

## 2026-05-22 — 批量编译 #8（设计/排版/PPT Skills）

**触发**: 用户请求处理 5 篇设计/排版/PPT 相关文章
**源文章**: 5 篇

### 新建页面（4）
- `entities/md2pdf-skill.md` — LovStudio 开源 PDF 排版 Skill，reportlab 纯 Python，CJK 双层混排，10 种主题
- `entities/excalidraw-diagram-skill.md` — 自然语言转 Excalidraw 手绘图表 Skill（coleam00）
- `entities/logo-generator-skill.md` — AI Logo 生成 Skill（op7418），SVG 三步工作流 + 12 种展示背景
- `entities/huashu-skills.md` — 花叔 20 个内容创作 Skills 合集，覆盖选题到发布全链路

### 更新页面（4）
1. `entities/design-md.md` — +source（vibe-design-frontend-ui）、+Vibe Design 解决痛点章节
2. `entities/stitch.md` — 修复 source 路径格式、bump updated date
3. `concepts/vibe-design.md` — 修复 source 路径格式、bump updated date
4. `entities/guizang-ppt-skill.md` — +wikilinks（huashu-skills、md2pdf-skill）

### Index 更新
- 总页面数：99 → 103（+4 新建）
- 新增 4 entity 条目

### 来源文章
1. raw/articles/2026-04-18-vibe-design-frontend-ui.md
2. raw/articles/2026-04-18-beautiful-pdf-typesetting-skill.md
3. raw/articles/2026-04-18-ai-skill-architecture-diagrams.md
4. raw/articles/2026-04-18-ai-logo-icon-generation-skill.md
5. raw/articles/2026-04-18-20-ai-creation-skills.md

## 2026-05-23 — 批量编译 #9（设计规范/约束先行/Skill 最佳实践/工具/Stata）

**触发**: 用户请求处理 5 篇文章
**源文章**: 5 篇
1. raw/articles/2026-04-18-58-website-styles-10days-40k-stars.md
2. raw/articles/2026-04-18-agent-skills-four-words.md
3. raw/articles/2026-04-18-anthropic-skill-best-practices.md
4. raw/articles/2026-04-18-asm-ai-coding-assistant-manager.md
5. raw/articles/2026-04-18-claude-code-stata-econometrics-guide.md

### 新建页面（2）
- `entities/asm.md` — asm（agent-skill-manager）统一管理 17+ AI 编程助手技能的 CLI 工具
- `entities/stata-skill.md` — Stata Skill 将 Claude Code 接入 Stata 计量经济学的完整指南

### 更新页面（4）
- `entities/design-md.md` — 新增 awesome-design-md 仓库章节（VoltAgent，58 品牌，4 万+ Star），更新 Star 数和来源
- `entities/skills.md` — 新增「Anthropic 内部 Skill 实践（2026）」章节（九大类型 + 9 个实战技巧）
- `entities/claude-md.md` — 新增「约束先行」实践案例章节（数字生命卡兹克的全局 CLAUDE.md 结构）
- `concepts/skill-engineering.md` — 新增来源引用

### index.md 更新
- Total pages: 115 → 117
- 新增 2 个 entity 条目
- 更新 design-md 描述

## [2026-05-23] ingest | 3 篇新文章编译（自动日检）

### raw/articles/2026-05-21-datawhale-ai-agent-learning-roadmap.md
- Datawhale AI Agent 学习路线，从入门到工程化 9 阶段系统学习路径
- 新建页面: concepts/agent-learning-roadmap.md

### raw/articles/2026-05-21-pinme-skill-one-click-deploy.md
- PinMe 2.0 一键部署工具和 AI Agent Skill
- 新建页面: entities/pinme.md

### raw/articles/2026-05-21-xhs-agent-projects-recommendation.md
- Agent 项目推荐（Aider/GPT-Researcher/HolmesGPT/Letta）
- 已在前期批量编译 #6 中处理，aider/gpt-researcher/holmesgpt/letta 页面已存在

### 修复
- 补 raw sha256: 1 个文件（2026-05-21-datawhale-ai-agent-learning-roadmap.md）

### index.md 更新
- Total pages: 141（+2 新建：pinme、agent-learning-roadmap）


## [2026-05-22] ingest | 腾讯云Agent Memory节省61% Token提升52%成功率的诀窍：Mermaid无限画布×上下文卸载
- Source: 微信公众号（腾讯技术工程）
- File: raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md
- 作者: 腾讯程序员（kentyhuang）
- 内容: TencentDB Agent Memory 团队提出的短期记忆压缩方案，通过上下文卸载+Mermaid结构化图表示，在超长Session中节省61% Token，任务通过率从33%提升到50%。涵盖JSONL摘要、MMD多状态图、Offload策略、Toolathon基准测试等核心技术，对比WideSearch/SWEBench/AA-LCR基线效果
- 关键词: agent-memory, mermaid, token-optimization, tencentdb, context-offloading, mmd, jsonl

## [2026-05-23] ingest | 腾讯云Agent Memory节省61% Token
- Source: 微信公众号（腾讯技术工程）
- File: raw/articles/2026-05-22-tencent-agent-memory-token-saving-mermaid.md
- 新建页面: 1
  - entities/tencentdb-agent-memory.md — 腾讯Agent记忆系统，四级折叠架构+Mermaid无限画布
- 更新页面: 4
  - concepts/agent-memory-systems.md — 新增TencentDB四级折叠+层次化注意力章节
  - concepts/context-rot.md — 补充80%阈值数据和tencentdb-agent-memory链接
  - concepts/context-compression-pipeline.md — 新增TencentDB方案对比+链接
  - index.md — 新增tencentdb-agent-memory条目，Total pages: 142

## [2026-05-24] ingest | OpenAI 官方分享：如何榨干 Codex
- Source: 微信公众号（AGI Hunt）
- File: raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
- 作者: Jason Liu（AGI Hunt 翻译）
- 内容: OpenAI Codex 团队 Jason Liu 官方指南，涵盖持久线程、语音输入、实时干预、任务排队、工具扩展、技能系统、自动化（定时+线程）、/goal 目标驱动、侧边栏、共享记忆等完整使用方法论
- 关键词: codex, openai, durable-threads, voice-input, steering, queuing, mcp, automation, goals, side-panel, shared-memory

## [2026-05-24] ingest | Getting the most out of Codex（英文原文）
- Source: X (Twitter) - @jxnlco
- File: raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
- 作者: Jason Liu (@jxnlco), OpenAI Codex Team
- 内容: 上述中文翻译版的英文原文，Jason Liu 在 X 上发布的长文，完整介绍 Codex 使用技巧
- 关键词: codex, openai, jason-liu, getting-most-out-of-codex

## [2026-05-25] ingest | Horizon — AI 驱动的个人新闻雷达
- Source: GitHub - Thysrael/Horizon
- File: raw/GitHub/Thysrael-Horizon.md
- 内容: AI 驱动的个人新闻雷达系统，从 HN/Reddit/Telegram/RSS/Twitter/GitHub/OpenBB 多源聚合，AI 打分+去重+背景丰富+评论摘要，生成中英双语每日简报，支持 GitHub Pages/邮件/飞书/钉钉/Slack/Discord/MCP 投递，4612 stars
- 关键词: news-aggregator, ai-news, mcp, multi-source, daily-briefing, feishu-bot, openclaw

## [2026-05-25] compile | Horizon — AI 新闻雷达
- New: entities/horizon.md
- Updated: index.md (+1 entry, total 143)
- Cross-references: [[mcp]], [[feishu]], [[claude-code]], [[openclaw]], [[deepseek]], [[claude-model-family]], [[react-pattern]], [[plan-and-execute-pattern]], [[gpt-researcher]]

## [2026-05-25] compile | TypeUI DESIGN.md Extractor + WPS365 CLI
- New: entities/typeui-design-md-extractor.md, entities/wps365-cli.md
- Updated: index.md (+2 entries, total 145)
- Cross-references (TypeUI): [[design-md]], [[claude-code]], [[codex-openai]], [[stitch]], [[hue]], [[kami]], [[vibe-design]]
- Cross-references (WPS365): [[skill-engineering]], [[feishu]], [[mcp]]


## [2026-05-26] ingest | AI News Radar — 24h AI 更新雷达
- Source: GitHub - LearnPrompt/ai-news-radar
- File: raw/GitHub/LearnPrompt-ai-news-radar.md
- 内容: 自动化 24 小时 AI/技术新闻聚合管线，GitHub Actions 驱动 + 实时 Web UI，核心创新为伯乐Skill（Scout Skill）信源评估系统，支持 OPML/RSS/AgentMail 多源接入，中英双语双视图输出，441 stars
- 关键词: news-aggregator, ai-news, github-actions, scout-skill, opml, rss, agentmail, github-pages, pipeline

## [2026-05-26] compile | AI News Radar — 24h AI 新闻雷达
- New: entities/ai-news-radar.md
- Updated: entities/horizon.md (对比段落扩展为三项目对比表)
- Updated: index.md (+1 entry, total 148)
- Cross-references: [[horizon]], [[gpt-researcher]], [[claude-code]], [[codex-openai]], [[openclaw]], [[hermes-agent]], [[skills]], [[skill-engineering]], [[feishu]], [[anthropic]]

## [2026-05-26] ingest | MiniCPM5-1B 正式发布并开源
- Source: 微信公众号 - OpenBMB开源社区
- File: raw/articles/2026-05-26-minicpm5-1b-openbmb.md
- 内容: 面壁智能发布端侧文本基座模型 MiniCPM5-1B，1B 参数超越所有 2B 以下模型（AA 榜单 17.9 分），INT4 量化仅 0.5GB，配套开源 ForgeTrain（AI 编写的训练框架，比 Megatron 快 10%）和 Ultra-FineWeb-L3 数据集
- 关键词: minicpm5, edge-model, small-model, forgetrain, openbmb, modelbest, density-law, RSI

## [2026-05-26] compile | MiniCPM5-1B — 端侧文本基座模型
- New: entities/minicpm5-1b.md
- Updated: index.md (+1 entry, total 149)
- Cross-references: [[deepseek]], [[gemma-4]], [[kimi-k25]], [[context-engineering]], [[anthropic]]

## [2026-05-26] lint | 自动日检 — 修复完成
- 触发: 新文章入库 2026-05-26-minicpm5-1b-openbmb.md（已在前置步骤编译）
- 修复项:
  - MED: 99 raw articles sha256 全部重算更新（含新增文章补充 sha256）
  - HIGH: 22 孤儿页面→0（为所有孤儿页面添加入站 [[wikilinks]]）
  - Updated 25 pages: deepseek, gemma-4, kimi-k25, agent-loop, skills, browser-use, claude-code, anthropic, claude-design, stitch, design-md, feishu, mineru, ai-scientist-v2, multica, archon, ppt-master, autofigure-edit, lovart, horizon, bb-browser, chandra, openai-codex, plus minicpm5-1b inbound links
- 修复后 lint: 0 CRIT / 0 HIGH / 0 MED / 3 LOW (oversized: claude-code, hermes-agent, multi-agent-collaboration)
- 最终状态: 149 页面 | 99 raw articles | ✅ 健康

## [2026-05-26] ingest | a-stock-data — A股全场景数据 Agent Skill
- Source: 微信公众号 - 开源AI项目落地
- File: raw/articles/2026-05-26-a-stock-data-agent-a-share.md
- 内容: A股全栈数据工具包，7层架构28端点13数据源，零第三方依赖，免Key，打包成 SKILL.md 直接注入 Claude Code/OpenClaw，2296 ⭐
- 关键词: a-stock, finance, skill, agent, china-stock, mootdx, eastmoney, claude-code-skill

## [2026-05-26] ingest | AI + Vercel 一键部署网站
- Source: 微信公众号 - 程序员鱼皮
- File: raw/articles/2026-05-26-ai-vercel-deploy-website-yupi.md
- 内容: 程序员鱼皮分享 AI 编程工具 + Vercel 自动部署网站全流程，含免费平台对比（EdgeOne Pages / Vercel / Netlify / Cloudflare Pages）、Skills + CLI 集成方案、操作步骤
- 关键词: vercel, deploy, ai-coding, skills, mcp, frontend, cloudflare, netlify, edgeone

## [2026-05-27] compile | Batch 1 Re-verification — 5 篇 raw 文章已编译验证

**触发**: 用户请求编译 batch 1（5 篇 raw 文章）
**源文章**: 5 篇
- `raw/articles/2026-04-18-ai-agent-stata-guide.md`
- `raw/articles/2026-04-18-ai-logo-icon-generation-skill.md`
- `raw/articles/2026-04-18-ai-skill-architecture-diagrams.md`
- `raw/articles/2026-04-18-beautiful-pdf-typesetting-skill.md`
- `raw/articles/2026-04-18-deep-research-prompt.md`

**状态**: 5 篇文章此前已全部编译为 entity 页面，本次为验证 + 补充更新。

**更新页面**: 1
- `entities/stata-skill.md` — 新增 Stata MCP 生态详解（4 种主流实现对比表、安装示例、DID 实战演示、安全守卫、实用技巧）

**已验证（无需更新）**: 4
- `entities/logo-generator-skill.md` — 内容已完整覆盖源文章（三步工作流、12 种背景、WebGL 动态背景、SVG 优势、使用场景）
- `entities/excalidraw-diagram-skill.md` — 内容已完整覆盖源文章（AI 图表生成、可视化验证、品牌风格定制、使用流程）
- `entities/md2pdf-skill.md` — 内容已完整覆盖源文章（CJK 双层混排、全链路文档结构、主题系统、12 竞品对比）
- `entities/hv-analysis.md` — 内容已完整覆盖源文章（横纵分析法方法论、两条轴详解、Prompt/Skill 版本、局限性）

**index.md**: 更新日期为 2026-05-27

**最终状态**: 149+ 页面 | 所有 5 篇文章均已编译 ✅

## [2026-05-27] compile | Batch 3 — 5 篇 raw 文章编译

**触发**: 用户请求编译 wiki batch 3
**源文章**: 5 篇
1. raw/articles/2026-04-18-westlake-university-ai-paper-drawing.md — 西湖大学 AutoFigure-Edit AI 论文绘图
2. raw/articles/2026-04-18-xhs-claude-no-compact-two-methods.md — Claude Code 不 Compact 的两种替代方法
3. raw/articles/2026-04-18-xhs-gpt-editable-publication-figures.md — GPT 生成可编辑顶刊绘图
4. raw/articles/2026-04-18-zero-human-coding-ai-native-dev-handbook.md — 0 人工 Coding AI Native 研发实战手册
5. raw/articles/2026-04-18-claude-research-10x-better.md — .claude 文件夹深度解析

### 跳过（已被现有 entity 覆盖）
- raw/articles/2026-04-18-xhs-gpt-editable-publication-figures.md → 已在 concepts/gpt-editable-figures.md 中覆盖，source 已引用
- raw/articles/2026-04-18-zero-human-coding-ai-native-dev-handbook.md → 已在 entities/codebuddy.md、entities/openspec.md、concepts/ai-native-development.md 中覆盖
- raw/articles/2026-04-18-claude-research-10x-better.md → 已在 concepts/claude-code-folder-structure.md 中覆盖

### 新建页面（1）
- `concepts/claude-context-continuity.md` — Claude Code 上下文续接方法（Handoff 文档 + Plan Mode 续接）

### 更新页面（4）
1. `entities/autofigure-edit.md` — 大幅扩展：SAM3 分割技术、FigureBench 基准测试对比数据、217 用户研究完整数据、风格迁移案例、张岳实验室团队信息、开源资源
2. `entities/codebuddy.md` — 大幅扩展：openspec-installer Skill 深度剖析（文件结构、version.json、skill-bundle.json、SKILL.md 设计原则）、Bridge Rule 迭代历史、团队协同规则、CLI/IDE 搭配策略、多项目联动
3. `concepts/claude-code-folder-structure.md` — 大幅扩展：CLAUDE.md 内容建议（写/不写）、多层级配置、rules/ 按路径生效、hooks 退出码语义与事件类型表、Stop Hook 防死循环、skills/agents 详解、settings.json 权限语义、入门 5 步配置流程
4. `entities/openspec.md` — 大幅扩展：项目目录结构、binxiong 团队案例（人角色重定义表格、六步工作流、关键规则）、扩展命令速查表

### index.md 更新
- Total pages: 149 → 150
- 更新 autofigure-edit、codebuddy、claude-code-folder-structure 描述
- 新增 claude-context-continuity concept 条目

### Wikilinks
- claude-context-continuity → [[claude-code-session-management]], [[context-rot]], [[context-compression-pipeline]], [[context-engineering]]
- autofigure-edit → [[chandra]], [[ppt-master]], [[gpt-editable-figures]], [[excalidraw-diagram-skill]]
- codebuddy → [[openspec]], [[skill-engineering]]
- claude-code-folder-structure → [[claude-md]], [[claude-code-hooks]], [[skills]], [[harness-engineering]]
- openspec → [[skill-engineering]], [[codebuddy]]

## [2026-05-27] compile | Batch 2: 5 篇 raw 文章编译

**源文章**: 5 篇
- raw/articles/2026-04-18-build-ai-agent-framework.md（从零实现 AI Agent 框架，yabohe/腾讯技术工程）
- raw/articles/2026-04-18-harness-engineering-source-code.md（Harness 工程项目源码拆解，charrli）
- raw/articles/2026-04-18-kazike-creative-skill-open-source.md（卡兹克风格创作 Skill 开源，数字生命卡兹克）
- raw/articles/2026-04-18-stop-vibe-coding-shit-mountain.md（Vibe Coding 完整指南，klöss）
- raw/articles/2026-04-18-vibe-design-frontend-ui.md（Vibe Design + DESIGN.md 实测，鲁工）

**跳过（已在 batch #9 编译）**:
- harness-engineering-source-code → 已在 2026-05-23 处理
- kazike-creative-skill-open-source → 已在 2026-05-23 处理（创建 entities/khazix-writer.md）

**新建页面**: 0（所有主题已有对应页面）

**更新页面**: 12
- concepts/agent-loop.md — 新增从零构建视角（279 行极简实现）
- concepts/react-pattern.md — 新增工程实现要点
- concepts/context-engineering.md — 新增 Agent 框架三大部分分析
- concepts/document-first-system.md — 新增 Vibe Coding 文档栈（6 规范 + 2 会话文件 + Interrogation 系统）
- concepts/vibe-coding.md — 新增失败模式分析和设计风格词汇表
- concepts/vibe-design.md — 新增 DESIGN.md 实践（9 模块 + awesome-design-md）
- entities/deepseek.md — 新增 Agent 框架使用场景
- entities/langchain.md — 新增框架对比定位
- entities/khazix-writer.md — 新增开源背景和创作哲学
- entities/claude-md.md — 新增自我改进循环（lessons.md）
- entities/cursor.md — 新增四种模式（Ask/Plan/Agent/Debug）
- entities/kimi-k25.md — 新增 Vibe Coding 工作流角色定位
- entities/design-md.md — 新增实测效果（鲁工）
- entities/stitch.md — 新增 Vibe Design 概念推广

**index.md**: total pages 不变（150），更新 last-updated

## [2026-05-27] ingest | 别再手写 Skill 了！微软最新研究：像神经网络一样训练 Skill
- Source: 微信公众号
- File: raw/articles/2026-05-26-skillopt-microsoft-train-skill-like-nn.md
- 微软 SkillOpt：把 Skill 文档当神经网络权重自动优化，52/52 测试全部最优，平均+23.5分，碾压人类手写 Skill。跨模型/跨环境迁移有效，MIT 开源。

## [2026-05-28] ingest | Codex 入门最佳实践「OpenAI官方」
- Source: 微信公众号（AI寒武纪）
- File: raw/articles/2026-05-28-codex-best-practices-openai-official.md
- OpenAI 官方 Codex 使用最佳实践九件事：完整上下文→先规划→AGENTS.md→配置一致性→测试审查→MCP→Skill→自动化→会话管理。核心：把 Codex 当持续改进的队友而非一次性助手。
## [2026-05-29] ingest | OpenClaw与Hermes：源码里的 AI Agent 架构知识大复盘
- Source: 微信公众号（腾讯技术工程）
- File: raw/articles/2026-05-29-openclawhermesai-agent.md
- OpenClaw 与 Hermes Agent 源码级对比分析：Gateway 架构、Context Engine、多 Agent 编排、Harness Engineering、安全机制等 22 个维度深度展开。含 QQ Bot 插件实战案例和 GAN-like 多智能体架构设计。

## [2026-05-29] ingest | 一文看懂 AI Agent 的7大核心模块：Skill、RAG、MCP、Harness……
- Source: 微信公众号（深蓝AI）
- File: raw/articles/2026-05-29-一文看懂-ai-agent-的7大核心模块skillragmcpharness.md
- AI Agent 七大核心模块详解：Skill 系统、RAG 检索增强、MCP 工具调用、Memory 记忆、Context Engine 上下文、Planning 规划、Harness 安全与稳定运行。

## [2026-05-29] ingest | Claude Opus 4.8 + Dynamic workflow，一次性并行上百个Subagents
- Source: 微信公众号
- File: raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md
- Claude Opus 4.8 搭配 Dynamic Workflow 实现百级 Subagent 并行编排，Subagent 角色分工（Planner/Generator/Evaluator）、Sprunt Contract 质量保证、上下文传递与断点恢复机制。

## [2026-06-01] ingest | 推荐 4 个 Star 数不高但挺有趣的 GitHub 项目
- Source: 微信公众号（逛逛GitHub）
- File: raw/articles/2026-05-31-4-interesting-low-star-github-projects.md
- 4 个低调但实用的开源项目：PeekDesktop（微软 VP 写的 Windows 桌面隐藏）、OpenToonz（吉卜力用了十年的 2D 动画软件）、Recordly（录屏自动后期工具）、English-level-up-tips（4.8 万 Star 的英语学习指南）。

## [2026-06-01] ingest | 一篇Harness研究后的思考！
- Source: 微信公众号（Datawhale）
- File: raw/articles/2026-05-30-harness-research-reflection.md
- 陈希伟对 Harness Engineering 的深度思考：Harness 解决静态组件问题，State-Aware Runtime 解决动态状态维护——候选输出 vs 已提交状态的严格区分、Trace-Native Evaluation、长上下文≠长期状态管理、级联传播与门控回滚机制。独立研究者的高壁垒方向。

## [2026-06-04] compile | Batch 1: BrowserAct + Claude Code self-check + Fiona Fung

**触发**: 自动编译（cron job）
**源文章**: 5 篇
- `raw/GitHub/browser-act-skills.md`
- `raw/articles/2026-06-03-browseract-playwright-replacement.md`
- `raw/articles/2026-06-03-claude-code-ai-native-engineering-org.md`
- `raw/articles/2026-06-03-claude-code-self-check-feedback-loop.md`
- `raw/articles/2026-06-03-claude-code-self-check-deep-dive.md`

**新建页面**: 3
- `entities/browser-act.md` — BrowserAct 开源浏览器自动化 CLI（1,573 ⭐），Playwright 替代方案，三层反检测（环境/执行/人机交互），31 个预置 Skill，Skill Forge 自动封装网站能力
- `entities/fiona-fung.md` — Fiona Fung，Anthropic Claude Code & Claude Cowork 工程总监，Code w/ Claude SF 2026 分享 AI 原生工程组织四大变革（JIT 路线图/问 Claude 不找人/信任但验证/角色模糊化）
- `concepts/claude-code-self-check.md` — Claude Code 自检反馈闭环方法论：传统开环 vs 闭环对比、三层实现（内联规则/Skill级/子代理并行审查）、Learnings Loop 飞轮效应、Dynamic Workflow 迷你 CI/CD 组合、社区技巧与局限性

**更新页面**: 2
- `entities/claude-code.md` — frontmatter 更新日期（2026-06-04）、新增来源、新增「AI-Native Engineering Organization」章节（Fiona Fung 四大流程变革 + Dogfooding 文化）
- `index.md` — 新增 3 条条目，Total pages 181→184

**核心信息**:
- BrowserAct 重新定义 Agent 浏览器自动化：环境层指纹伪装 + 执行层自动解验证码 + 人机层远程协助，三个模式（chrome/stealth/chrome-direct），自动剥离 90% 无效 HTML
- Fiona Fung 揭示工程瓶颈从「写代码」转向「验证/审查/安全」；人类审查聚焦法律/安全/品味，信任但验证
- Claude Code 自检闭环把验证左移：人工检查 → 编码为规则 → Claude 自动执行 → 自修 → 通过才交付；与 Dynamic Workflow 组合即迷你 CI/CD 流水线

## [2026-06-04] compile | Batch 2: Dynamic Workflow + Harness + Karpathy + Khazix + SenseNova

**触发**: 自动编译（cron job）
**源文章**: 5 篇
- `raw/articles/2026-06-03-claude-code-dynamic-workflow-harness.md`
- `raw/articles/2026-06-03-claude-workflow-harness-design-patterns.md`
- `raw/articles/2026-06-03-karpathy-learning-methodology.md`
- `raw/articles/2026-06-03-khazix-mac-cleaner-skill.md`
- `raw/articles/2026-06-03-sensenova-skills-open-source.md`

**新建页面**: 4
- `entities/thariq-shihipar.md` — Anthropic 工程师，Dynamic Workflow 官方博客作者，提出单上下文三大顽疾（偷懒/自我偏袒/目标漂移）和六种编排模式
- `entities/khazix-mac-cleaner.md` — 卡兹克开源的 AI Agent 存储清理 Skill，三色风险分级（🟢🟡🔴），实测释放 120GB（vs CleanMyMac 15.8GB），跨平台只读扫描+两步确认删除
- `entities/sensenova-skills.md` — 商汤科技开源 AI 办公技能套件，四大功能（信息图表/数据分析/PPT/深度研究），兼容 OpenClaw 和 Hermes Agent
- `entities/sensetime.md` — 商汤科技实体页，2026 年开源 SenseNova-Skills 进入 Agent 生态

**重写页面**: 1
- `entities/claude-code-dynamic-workflow.md` — 从 73 行大幅扩展为完整页面：三大单上下文顽疾、静态 vs 动态对比、核心 JS API、六种编排模式（含隔离区模式）、十种应用场景、使用/不使用指南、实用技巧、鲁工核心洞察（harness 成为新的竞争分水岭）、/deep-research 实测。新增 2 个来源，新增 5 个相关链接

**更新页面**: 4
- `concepts/harness-engineering.md` — 新增「Dynamic Workflow 六种 Harness 编排模式」章节（含表格模式详解+隔离区模式+核心洞察），更新日期至 2026-06-04，新增 2 个来源
- `entities/andrej-karpathy.md` — 新增「Learning Methodology」章节（深度参与不可替代论、学习需要摩擦、AI 不能替代深度思考），更新日期至 2026-06-04，新增来源
- `entities/khazix-writer.md` — tags 修正 tool→person，新增 [[khazix-mac-cleaner]] 交叉引用，更新日期至 2026-06-04，新增来源
- `index.md` — 新增 4 条 entity 条目（thariq-shihipar、khazix-mac-cleaner、sensenova-skills、sensetime），更新 claude-code-dynamic-workflow 摘要，Total pages 184→188

**核心信息**:
- Dynamic Workflow 六种编排模式是 Harness Engineering 在 Agent 编排层的直接落地，鲁工核心洞察「过去拼模型多聪明，往后拼会不会给任务写配得上的 harness」
- Karpathy 学习方法论与知识编译理念形成有趣张力：LLM wiki 用于组织知识，学习的摩擦用于内化知识
- 卡兹克 Mac Cleaner 体现 Agent 时代创新模式：发现需求→自然语言让 Agent 执行→封装为 Skill→开源分发；核心洞察「软件正从资产变成耗材」
- 商汤从模型/API 提供商向 Agent 生态参与者的战略延伸

## [2026-06-07] ingest | Anthropic 内部 Skills 经验公开
- Source: 微信公众号 (Datawhale)
- File: raw/articles/2026-06-07-anthropic-internal-skills-practices.md
- Anthropic 官方复盘 Claude Code Skills 内部用法：9 类 Skill 分类（library/reference → verification → data → business process → scaffolding → code review → CI/CD → runbooks → infra ops），核心原则（聚焦、验证最值得投入、gotchas 最有价值、progressive disclosure、description 服务触发），Skill 成熟后长出记忆/脚本/hooks，团队级分发和治理（check-in vs marketplace），原文链接 claude.com/blog

## [2026-06-04] lint-fix | Playwright 实体页创建
- 原因：browser-act 页面引用了 `[[playwright]]`，但该页面不存在（lint 报 broken-link）
- 创建 `entities/playwright.md`（微软开源浏览器自动化框架，browser-act/browser-use 的底层引擎）
- 更新 `index.md`：新增 entity 条目，Total pages 188→189
