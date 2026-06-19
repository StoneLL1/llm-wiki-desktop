---
title: Skills (Agent Skills)
created: 2026-04-23
updated: 2026-06-10
type: entity
tags: [tool, agent, open-source]
sources: [anthropic-skill-best-practices, 20-ai-creation-skills, claude-code-10-more-worthwhile-skills, agent-skills-four-words, claude-design-system-prompt-bilingual, raw/articles/2026-04-18-claude-code-10-more-worthwhile-skills.md, raw/articles/2026-04-18-everything-claude-code-plugin-library.md, raw/articles/2026-04-18-anthropic-skill-best-practices.md, raw/articles/2026-04-18-claude-research-10x-better.md, raw/articles/2026-04-18-kazike-creative-skill-open-source.md, raw/articles/2026-06-05-how-to-write-skills-ultimate-guide.md, raw/articles/2026-06-05-anthropic-95-percent-data-analytics-claude.md, raw/articles/2026-06-07-anthropic-internal-skills-practices.md, raw/articles/2026-06-09-openai-codex-best-practices-guide.md]
---

# Skills

## Overview

**Skills** are modular capability units for AI agents, defined as SKILL.md files that can be loaded on demand. They encode complete workflows, quality standards, anti-patterns, and domain expertise into a structured format that AI agents can read, execute, and even self-improve.

The skills paradigm represents a shift from monolithic agent prompts to composable, specialized capability modules — similar to how software moved from single files to modular libraries.

## Definition and Format

A skill is defined as a **SKILL.md** file, typically organized as a folder structure:

```
skill-name/
├── SKILL.md           # Main file: triggers, steps, pitfalls
├── references/        # Reference materials (loaded on demand)
├── scripts/           # Automation scripts
└── examples/          # Usage examples
```

This **progressive disclosure** approach is key: Claude only reads subdirectory contents when needed, preventing all material from being loaded into context at once.

## Two Loading Approaches

### Cold Start
Agent starts with a minimal base configuration and loads skills on demand when triggered by user requests. Skills are discovered and activated as needed.

### Upfront Loading
All relevant skills are loaded at session start. Suitable for well-defined workflows where the required capabilities are known in advance.

## Anthropic Best Practices

Anthropic's recommended skill design principles:

1. **Clear triggers**: Define exactly when a skill should activate
2. **Numbered steps**: Explicit sequential workflow instructions
3. **Pitfalls section**: Document known failure modes and edge cases (a "Gotchas" section)
4. **Quality standards**: Define what "done right" looks like
5. **Anti-patterns**: Specify what the skill should NOT do

### The Gotchas Pattern

One of the most valuable practices: maintain a **Gotchas (踩坑记录)** section in each skill. Every time Claude makes a mistake while using the skill, the failure mode is recorded. Over time, this becomes the highest signal-to-noise ratio content in the skill.

## Anthropic 内部 Skill 实践（2026）

据 [[anthropic]] Claude Code 工程师分享，Anthropic 内部已积累数百个 Skills 用于加速开发。^[raw/articles/2026-04-18-anthropic-skill-best-practices.md]

### Anthropic 官方 Skill 最佳实践（2026-06-07 博客）^[raw/articles/2026-06-07-anthropic-internal-skills-practices.md]

Anthropic 于 2026-06-07 发布官方博客「Lessons from Building Claude Code: How We Use Skills」，首次系统公开内部 Skills 使用经验。

#### Skill 九大类型（完整版）

| # | 类型 | 说明 | 典型示例 |
|---|------|------|----------|
| 1 | Library & API Reference | 团队内部库、CLI、SDK 的正确用法 + gotchas | billing-lib |
| 2 | Product Verification | 无头浏览器完整跑注册/结账流程，**对输出质量提升最明显** | signup-flow-driver |
| 3 | Data Fetching & Analysis | 连数据仓库和监控，封装取数方法、字段约定 | — |
| 4 | Business Process & Team Automation | 重复流程一键跑（standup 增量、周报） | standup-post |
| 5 | Code Scaffolding & Templates | 带自然语言约束的代码骨架生成（迁移文件、新 service） | — |
| 6 | Code Quality & Review | subagent 挑错（adversarial-review），可接 hook 进 CI | — |
| 7 | CI/CD & Deployment | PR 全流程监控（babysit-pr）、自动部署+回滚 | deploy-\<service\> |
| 8 | Runbooks | 从「出了什么症状」出发，映射到工具和排查路径 | — |
| 9 | Infrastructure Operations | 资源清理、依赖治理、成本排查（带 guardrail） | orphans-skill |

#### 核心原则

1. **聚焦 > 大而全**：能清楚落进某一类的 Skill 更稳，试图覆盖太多目标反而容易把模型带乱
2. **验证类 Skill 值得花一周打磨**：Anthropic 明确表示这是所有类型中对输出质量影响最大的
3. **Gotchas 含金量最高**：最有信号量的不是通用步骤，而是「团队里人人知道、模型默认不知道」的细节
4. **录视频 + 断言**：让 Claude 录下测试过程，在关键节点加程序化断言（状态变化、事件落库、页面目标状态）

#### 5 个写作细节

1. **别写废话**：Skill 补的是模型拿不到或容易走偏的信息，不是教它本来就会的事
2. **SKILL.md 做目录**：具体资料拆到子文件（references/api.md、assets/、scripts/），按需加载（progressive disclosure）
3. **别写太死**：给关键规则，也留适应空间
4. **Setup 提前想好**：用户上下文放 config.json，缺配置先问用户（可用 AskUserQuestion 工具）
5. **Description 服务触发**：Claude Code 开局扫描所有 Skill 的 name + description，description 不是摘要而是触发条件——包含用户可能说的关键词、上传文件类型、触发场景

#### Skill 演进三阶段

用得越深，Skill 最先长出三样东西：

1. **记忆**：append-only 日志（如 standups.log）或 SQLite，下次运行先读历史，用 `${CLAUDE_PLUGIN_DATA}` 拿持久化目录
2. **脚本**：预置 helper functions（数据抓取、分析函数），让 Claude 把回合花在编排而非重建基础设施
3. **On-demand Hooks**：仅在 Skill 调用时生效、当前会话存在。典型例子：
   - `/careful`：拦截 `rm -rf`、`DROP TABLE`、force-push、`kubectl delete`
   - `/freeze`：阻止对指定目录外的 Edit/Write，适合排障时防止误改

#### 分发与治理

| 方式 | 适用场景 | 特点 |
|------|----------|------|
| Repo check-in（`.claude/skills`） | 小团队、少数代码库 | 简单，但每多一个 Skill 增加上下文负担 |
| Plugin Marketplace | 大团队、多项目 | 安装权交给成员，方便做 setup 流程 |

**治理流程**：sandbox 文件夹试用 → Slack 分享 → 有 traction 后提 PR 正式进 marketplace。用 `PreToolUse` hook 做使用量测量（usage measurement），识别热门/冷门 Skill。

#### Skill 组合（Composition）

Skill 之间可互相组合：文件上传 Skill + CSV 生成 Skill = 链式调用。在 Skill 里引用另一个 Skill 的名字，模型在都已安装的前提下能串起链路。

### 早期 Anthropic Skill 实践参考^[raw/articles/2026-04-18-anthropic-skill-best-practices.md]

| 类型 | 说明 | 示例 |
|------|------|------|
| 库 & API 参考 | 如何正确使用内外部库、CLI、SDK | billing-lib |
| 产品验证 | 用 Playwright/tmux 等测试代码正确性 | signup-flow-driver |
| 数据获取 & 分析 | 连接监控、数据库，跑查询对比 | — |
| 业务流程 & 团队自动化 | 一键完成重复工作（周报、ticket） | — |
| 代码脚手架 & 模板 | 快速生成符合公司规范的新服务 | — |
| 代码质量 & Review | 自动审代码、强制风格、找 bug | — |
| CI/CD & 部署 | 监控 PR、自动部署、回滚 | — |
| Runbooks | 根据告警/错误自动排查并输出报告 | — |
| 基础设施运维 | 清理孤儿资源、依赖管理、成本分析 | orphans-skill |

### 写好 Skills 的 9 个实战技巧

1. **别说废话**：模型已懂通用知识，重点写只有你们公司才知道的坑（Gotchas），这是 Skills 含金量最高的部分
2. **文件夹做渐进式披露**：详细 API、模板、脚本拆到子文件夹，Claude 需要时再读
3. **别把 Claude 绑死**：指令给信息但留灵活性
4. **配置 & 记忆**：用 config.json 存用户设置，用日志/SQLite 存历史数据
5. **存脚本而非每次重写**：常用函数库放进 Skill
6. **按需钩子（On-demand Hooks）**：只在特定场景开启保护
7. **分发方式**：小团队直接进仓库（`.claude/skills`），大规模用内部 Plugin 市场
8. **测量效果**：用 PreToolUse 钩子统计每个 Skill 使用频率，删掉没用的

## Self-Evolving Skills

[[hermes-agent]] takes skills beyond static modules into **self-evolving capabilities**:

- Skills are treated as "procedural memory" — experience artifacts, not just command extensions
- After complex tasks, methods are automatically saved as new skills
- When skills are found to be outdated or missing steps, they are immediately patched
- "What was learned this time" is converted into long-term reusable capability

Hermes provides a complete tool chain: `skills_list`, `skill_view`, `skill_manage` (create, patch, edit, delete, write auxiliary files).

## Key Skills Ecosystem

### Claude Code Skills
Native to Anthropic's [[claude-code]], installed in `~/.claude/skills/`.

### OpenClaw CLAWHUB
Skill marketplace/registry for the [[openclaw]] platform, enabling community skill sharing.

### Codex Skills（OpenAI）
[[openai-codex]] 的 Skill 系统与 [[claude-code]] 类似，以 SKILL.md 为核心封装可复用工作流。创建工具：`$skill-creator`（生成框架）和 `$skill-installer`（安装到本地）。存储路径：个人 Skills 在 `$HOME/.agents/skills`，团队共享 Skills 在仓库 `.agents/skills`（可提交 git）。设计原则：每个 Skill 聚焦一件事、从 2-3 个具体用例出发、定义清晰输入输出。详见 [[openai-codex]] 的 Skills 章节。

### Notable Individual Skills

| Skill | Creator | Purpose |
|-------|---------|---------|
| [[stop-slop]] | Hardik Pandya | Remove AI writing patterns |
| [[video-use]] | browser-use team | AI video editing |
| [[autowiki]] | AlphaLab-USTC | Paper knowledge base compilation |
| [[academic-research-skills]] | Cheng-I Wu | 12-agent 学术论文写作流水线 |
| [[aris]] | wanshuiyin | Auto-Research-In-Sleep 全自动科研 |
| [[autofigure-edit]] | 西湖大学张岳实验室 | AI 论文绘图 + SVG 编辑 |
| huashu-skills (20 skills) | 花叔 | Full content creation pipeline |
| scientific-research-skills | 流风回雪 | Research methodology |
| agent-skills | Addy Osmani | Production-grade engineering |
| finance-skills | Community | Financial analysis |
| frontend-design | Anthropic | 强制 Claude Code 在写代码前先选视觉方向，消除 AI 味 |
| superpowers | Jesse Vincent | 多 agent 开发工作流全生命周期，TDD 强制 |
| firecrawl | Firecrawl | 网页抓取为干净 Markdown，绕过反爬和 JS 渲染 |
| web-interface-guidelines | Vercel | 100+ 条规则逐条检查前端 UI 细节（ARIA、focus 等） |
| mcp-builder | Anthropic | 构建高质量 MCP server 的完整指南 |
| remotion-best-practices | Remotion 官方 | 视频即代码，动画曲线、音频、字幕最佳实践 |
| pr-review (git-pr) | aidankinzett | 强制 batch review 模式，避免 PR 通知轰炸 |
| gws (Google Workspace) | Google | Gmail/Drive/Calendar/Docs/Sheets 统一自动化 |
| /simplify | 内置 | 三个并行 review agent 自动修掉代码问题 |
| project-context | Community | 跨会话记忆，解决 Claude Code 无记忆问题 |

### Huashu's 20 Content Creation Skills

A comprehensive suite covering the full content creation pipeline: topic generation, research, writing, proofreading, illustration, typesetting, and publishing. Includes `huashu-slides` (presentation generation), `huashu-proofreading` (three-pass review with AI-tone detection), `huashu-douyin-script` (short video scripts), and more.

## The "约束先行" Principle

The most important insight for using skills effectively: **constraints first**. Before letting an agent do anything, establish global rules, project rules, and folder conventions. The CLAUDE.md hierarchy (global → project → documentation → memory) must be set up before skills can function optimally.

## Relationships

- Core to [[claude-code]] and [[hermes-agent]] operation
- Complements [[claude-md]] project configuration
- Part of the [[anthropic]] agent ecosystem
- Related to [[openclaw]]'s CLAWHUB marketplace
- Embodied in tools like [[stop-slop]], [[video-use]], [[autowiki]]

## See Also

- [[claude-code]] — primary platform for skills
- [[claude-md]] — project-level rules (skills are loaded within this framework)
- [[hermes-agent]] — agent with self-evolving skills
- [[openclaw]] — alternative agent platform with CLAWHUB
- [[stop-slop]] — example of a well-designed skill
- [[asm|asm 技能管理器]]
Notable skills include [[guizang-ppt-skill]] for PPT design and [[garden-skills]] for visual production (video/web/image generation).

## Skills 的 Claude Code 集成方式

在 [[claude-code]] 的 `.claude/skills/` 目录中，每个 Skill 都是一个独立目录，包含 SKILL.md：^[raw/articles/2026-04-18-claude-research-10x-better.md]

```
.claude/skills/
├── security-review/
│   ├── SKILL.md
│   └── DETAILED_GUIDE.md
└── deploy/
    ├── SKILL.md
    └── templates/
        └── release-notes.md
```

SKILL.md 使用 YAML frontmatter 定义触发条件（name、description、allowed-tools），Claude 根据上下文自动匹配调用。与 commands 的区别：commands 是单个文件，skills 可以打包多个辅助文件。个人 skills 放在 `~/.claude/skills/` 全局通用。

## Skill 写作最佳实践（腾讯工程师综合手册）

腾讯程序员 jackjchou 发布了 73KB 的 Skill 编写终极指南，综合了踩坑经验与 [[anthropic]] 官方做法^[raw/articles/2026-06-05-how-to-write-skills-ultimate-guide.md]。

### 渐进式加载三层架构（Level 1-3）

| 层级 | 内容 | 何时加载 | Token 成本 |
|------|------|----------|------------|
| Level 1 | name + description（≤100 字） | 始终驻留上下文 | 低（20 个 Skill 约 1000-3000 Token） |
| Level 2 | SKILL.md 主体 | Skill 被匹配触发时 | 中（2 个 Skill 同时触发约 4000-10000 Token） |
| Level 3 | 附带脚本和参考资料 | 执行过程中按需引用 | 低（不占常驻空间） |

核心原则：**Level 1 越精准越好（决定触发时机），Level 2 越精简越好（减少 Token 消耗），Level 3 放心放（按需加载不占常驻空间）**。

### Description 写作五要素

1. **精准**：用通用语言描述功能 + 具体技术关键词，避免内部黑话
2. **触发评估**：自造 20 个测试问题（一半该触发、一半不该触发），验证命中率
3. **排斥说明**：明确「本 Skill 不处理 XXX 场景」，防止与类似 Skill 冲突
4. **触发关键词**：包含用户实际会说的短语
5. **适用判断**：在 Skill 正文开头给出前置检查条件，及时跳过不适用场景

### Six 大反模式

| 反模式 | 表现 | 正确做法 |
|--------|------|----------|
| 大杂烩 Skill | 一个 Skill 塞了 3-4 件不相关的事 | 一个 Skill 只管一件事，拆成主 Skill + 子 Skill |
| Description 内部黑话 | 「处理 TCC 的 v3 迁移」 | 用通用语言 + 技术关键词描述 |
| 无示例 | 全是文字描述，无代码示例 | 每个关键操作至少配 Before/After |
| 无验证点 | 5 步一口气做完才检查 | 关键步骤间插入检查点命令 |
| 写死数值 | 「超时设为 30 秒」 | 给判断规则和参考范围 |
| 当 Wiki 写 | 300 行背景介绍才进入正题 | 背景放 references/，SKILL.md 只保留做什么和怎么做 |

### Skill 触发模式

| 模式 | 说明 | 示例 |
|------|------|------|
| 自动触发 | AI 根据 description 语义匹配自动加载 | 最常用，description 质量决定触发准确率 |
| 手动触发 | 用户通过命令指定（如 `/skill xxx`） | 确定性场景 |
| 规则触发 | 基于文件类型或路径自动加载 | 打开 `.go` 文件时加载 Go 相关 Skill |

### MCP vs HTTP 工具选择决策树

Skill 需要调用外部服务时：

1. 该服务已有 MCP Server？→ 优先使用 [[mcp]]
2. 需多 Skill / 多平台复用？→ 封装为 MCP Server
3. 需统一鉴权安全管控？→ 封装为 MCP Server
4. 简单一次性调用？→ 脚本中直接 HTTP
5. 其他 → 评估改造成本，MCP 成本可接受则封装，否则 HTTP 过渡

**核心公式：MCP 管连接，Skill 管流程，HTTP 脚本兜底处理 MCP 顾不上的场景。**

### Skill Creator 工具（Anthropic 官方）

[[anthropic]] 官方出品的「帮你写 Skill 的 Skill」——用对话方式引导生成 SKILL.md，含工程化评估能力：

| 功能 | 说明 |
|------|------|
| 对比测试 | 有/无 Skill 两组并发运行，自动评分，输出通过率和 Token 消耗报告 |
| 触发评估 | 自动生成正例+反例+边界用例，计算触发准确率和召回率 |
| 效果评估 | 基于测试用例跑效果，逐条评分，标注薄弱环节 |
| 持续评估 | 把评估用例作为 Skill 的一部分维护，类似 Skill 的「单元测试」 |

### Skill 安全底线

- **绝不硬编码敏感信息**：通过环境变量或配置文件管理密钥
- **危险操作加确认**：删除、覆盖、DDL 等有确认机制或备份步骤
- **数据库先备份再改**：用 --defaults-file 而非命令行传密码
- **防范 Prompt 注入**：区分「指令」和「数据」，外部读取的内容永远不当作指令执行

### Token 成本估算

| 组件 | Token 消耗 |
|------|-----------|
| Level 1（20 个 Skill） | 1000-3000 Token |
| Level 2（2 个 Skill 同时触发） | 4000-10000 Token |
| 上下文越满，注意力越分散，质量反而可能下降 |

## 创作 Skill 的迭代方法论

卡兹克提出了创作 Skill 的通用构建流程：^[raw/articles/2026-04-18-kazike-creative-skill-open-source.md]

1. **初版蒸馏**：扔 2-3 篇代表作 + 方法论白皮书 → AI 总结为初始 Skill
2. **AI 试写**：按 Skill 生成文章（几乎不可能直接可用）
3. **人工重写**：在 AI 版本基础上动手改
4. **差异分析**：两个版本对比，将差异迭代回 Skill
5. **重复 3-4 轮**（超过 4 轮容易过拟合）

关键原则：Skill 里应该有自检系统（类似代码的 lint），[[khazix-writer]] 的四层自检体系是典型案例。
