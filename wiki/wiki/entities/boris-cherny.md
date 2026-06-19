---
title: Boris Cherny
created: 2026-05-22
updated: 2026-06-05
type: entity
tags: [person, company]
sources:
  - raw/articles/2026-04-18-claude-code-creator-15-hidden-features.md
  - raw/articles/2026-04-19-claude-design-system-prompt-leak-analysis.md
  - raw/articles/2026-06-04-claude-code-boris-write-loops-not-prompts.md
---

# Boris Cherny

## Overview

**Boris Cherny**（鲍里斯·切尼）是 [[claude-code]] 的创造者，[[anthropic]] 核心开发者。他在 2026 年 4 月发布了一条长帖，分享了 15 个 Claude Code 隐藏功能，引发社区广泛关注。

## 核心观点

Boris 在分享中强调 Claude Code 的使用哲学：

> "用 Claude Code 最重要的一个技巧是，给 Claude 一种验证产出的方式。一旦有了验证手段，Claude 就会自己迭代到满意为止。"

这体现了一个核心原则：**给 AI 反馈闭环**。无论是 Chrome 插件让 Claude 看到页面渲染结果，还是桌面端内置浏览器自动测试，都是为了让 AI 能自我验证和迭代。

## 使用习惯

Boris 本人的 Claude Code 使用方式极具代表性：

- **手机编程**：大量代码在 iOS App 上完成，利用多端会话流转
- **语音编程**：大部分代码是**说出来**的，不是打字
- **并行作战**：经常同时跑几十个 Claude，靠 git worktree 互不干扰
- **定时巡逻**：设了多个本地循环（`/loop 5m /babysit`），自动处理 code review、rebase、CI
- **Dispatch 派活**：不写代码时用 Dispatch 刷 Slack、处理邮件、管理文件

## 推荐的关键功能

1. **多端流转** — `--teleport` 和 `/remote-control` 在手机/桌面/终端之间无缝切换
2. **`/loop` 定时巡逻** — 按固定间隔自动执行任务，最长跑一整周
3. **Hooks** — 在 Agent 生命周期插入自定义逻辑（SessionStart、PreToolUse 等）
4. **`/batch`** — 把任务拆成若干份分发到 worktree Agent 并行执行
5. **`--bare`** — 非交互调用时跳过配置加载，启动速度提升 10 倍
6. **`--agent`** — 在 `.claude/agents` 定义自定义 Agent（如只读 Agent）
7. **`/voice`** — 语音输入编程
8. **Chrome 插件 / 内置浏览器** — 给 Claude 验证产出的能力

## 产品方法论

Boris 在 Lenny's Newsletter 播客中透露了 Anthropic 的产品开发方法：

- **不先写 PRD，而是先建几百个可运行的原型**，然后挑值得发布的
- 他个人每天合并 20-30 个 PR，同时跑 5 个 Claude 实例
- 整个 Cowork 产品大约 **10 天就做出来了**
- 他会故意给团队**不足的资金和无限的 token**，逼迫他们用 AI 来放大产出

这种方法论代表了 [[ai-native-development]] 的极致实践：用 AI 将原型迭代速度压缩到传统流程的 1/10。

## "写 Loop，不写 Prompt" — 2026 年 6 月演讲

Boris 在 2026 年 6 月的一次 30 分钟演讲中提出了更具概括性的工作哲学：

> "我现在不再给 Claude 写 prompt 了，我有一堆 loop 在跑。我的工作是写 loop。"

他展示了自己的日常编码配置：**Claude Code + loops + [[claude-code-dynamic-workflow|Dynamic Workflow]]**。核心观点是将自己的角色从「提示词作者」转变为「编排工程师」——不再一条一条给指令，而是写循环让 Agent 自动运行、自动验证、自动迭代。

### 实战模式组合

Boris 强调 Anthropic 工程师常用的六种 [[claude-code-dynamic-workflow|Dynamic Workflow]] 编排模式很少单独使用，真实 workflow 通常组合 2-4 个。他举例：**Bun 从 Zig 重写到 Rust** 使用了"Fan-out（一个 agent 一个 callsite）→ 对抗验证 → loop until done"三模式组合。

### 容易踩的坑

Boris 在演讲中还总结了 Claude Code 团队自己的提醒：

- 该用普通 session 解决的事别上 workflow
- 不设 token 预算——野心大的 workflow 不封顶能烧到预期 5-10 倍
- 让同一个 agent 既干活又验证（self-preference 会让验证形同虚设）
- loop 模式不配 `/goal` 会在第一个软完成点停下
- 让不可信内容直达执行 agent（必须 quarantine）
- 排序用绝对打分（应换用 tournament 配对比较）
- 跑通的 workflow 不保存（应存到 `~/.claude/workflows` 或打包成 [[skills|Skill]] 分发）

### 社区评价

一位网友总结：Boris 不小心把 n8n 整套 UX 哲学说了一遍——一个 loop 看 webhook，一个 loop 看 schedule，一个 loop 盯队列。**你不再跑任务，你在养管道**。但编排层得先稳，再上动态路由。

## Relationships

- [[claude-code]] 的创造者
- [[anthropic]] 核心开发者
- 其「验证闭环」理念与 [[context-engineering]] 和 [[skill-engineering]] 一脉相承
- 多端流转能力体现了 [[multi-agent-collaboration]] 的基础设施需求
- 其产品方法论体现了 [[vibe-design]] 和 [[vibe-coding]] 的闭环

## See Also

- [[claude-code]] — 他创造的产品
- [[anthropic]] — 他所在的公司
- [[claude-code-slash-commands]] — 他推荐的命令参考
