---
title: "Claude Code 作者Boris：我已经不写 prompt 了，我写 loop"
url: "https://mp.weixin.qq.com/s/-Mhg_EEibje5tkWlVazdMA"
source: "微信公众号"
author: "winkrun"
account: "AI工程化"
pub_date: 2026-06-04
fetched: 2026-06-04
---

# Claude Code 作者Boris：我已经不写 prompt 了，我写 loop

**作者**: winkrun | **公众号**: AI工程化

Claude Code 的作者 Boris 最近有一句话被传得挺广：

> 我现在不再给 Claude 写 prompt 了，我有一堆 loop 在跑。我的工作是写 loop。

他用一个 30 分钟的演讲，把自己日常写代码的 Claude 配置摊开讲了一遍：Claude Code + loops + Dynamic Workflow。

### Dynamic Workflow 是什么

Anthropic 在 2026 年 5 月 28 日把 Dynamic Workflows 塞进了 Claude Code。简单说：默认的 Claude Code 是把规划和执行塞进同一个上下文窗口，对大多数写代码的活够用，但碰到长任务、大规模并行、强结构化或者对抗性任务就会崩。

以前 Anthropic 内部要靠工程师手搓 harness（Research、Code Review、agent 团队都是这么来的）。Dynamic Workflows 让 Claude 自己临场写这个 harness——一个 JavaScript 文件，里面用几个特殊函数 spawn 子 agent，再用标准 JS 处理它们之间流动的数据。

相比默认 harness，多了三件事：

  * 每个子 agent 有独立上下文窗口，单一目标，不会互相污染。
  * 每个子 agent 可以指定模型：硬推理上 Opus，便宜的探索用 Haiku，中间走 Sonnet。
  * 每个子 agent 的隔离级别可选：worktree（独立 git checkout）或 remote（不 checkout）。

启动方式很简单，直接跟 Claude 说"给我做一个 workflow……"，或者用触发词 `ultracode`。中途被打断也能恢复。

### 它结构性地解决了三个老毛病

Anthropic 自己的发布文档点了三个失败模式：

  * **Agentic laziness** ：复杂多步任务做一半就宣布完成。50 项安全审计处理了 20 项，剩下的说"已处理"。
  * **Self-preferential bias** ：让 Claude 验证自己输出时，它倾向于偏袒自己。当裁判和当选手不能是同一个人。
  * **Goal drift** ：多轮之后逐渐偏离目标，每次压缩都会丢信息。第 47 轮时"不要做 X"这条约束悄悄消失。

Workflow 用结构来解：不同 Claude，各自上下文，目标聚焦，状态隔离。

### 静态 vs 动态

以前用 Claude Agent SDK 或者 `claude -p` 也能串多个 Claude Code，那是静态 workflow——写一次要兼顾所有边界情况，所以只能保守。

动态 workflow 是 Claude **为这次任务** 临场写的，量身定制。

动态版本赢的不是搜索，是它能贴着你的上下文塑形：读你的代码、对照新供应商真实文档、按你的数据定价、再用对抗 pass 攻击自己的初步答案。

### 核心 API 三件套

`agent()`、`parallel()`、`pipeline()`。

`parallel()` 是 barrier：扇出之后等所有结果回来才返回。`pipeline()` 是流式：每个元素独立流过每一阶段。

选哪个就看一个问题：下一步是不是必须等全部结果？是就 parallel，不是就 pipeline（更便宜更快）。

### Anthropic 工程师常用的 6 个模式

  1. **Classify-and-act** ：先让一个分类 agent 判断任务类型，再路由到不同的处理。便宜模型做分类，贵模型只用在需要的地方。
  2. **Fan-out-and-synthesize** ：拆成多个独立小任务并行跑，再用一个 agent 合并。50 个文件的安全审计这种最适合。
  3. **Adversarial verification** ：每个产出 agent 配一个独立的验证 agent，验证者从没看过原始工作，没法偏袒。专治 self-preferential bias。
  4. **Generate-and-filter** ：先生成 30 个想法，再用 rubric 筛掉一堆，只留前几名。和"直接给我最好的答案"相反，逼 Claude 晚做承诺。
  5. **Tournament** ：成对比较，淘汰赛。比绝对打分可靠得多，尤其是涉及品味的任务。要排 1000 个东西，别想着一次性排序，搞个 bracket。
  6. **Loop until done** ：不知道要跑几轮的任务，用循环 + 停止条件。配合 `/goal` 设硬性完成标准（比如"直到一个理论被证实才停"），配合 `/loop` 让整套 workflow 按时间表跑。

实战里这六个模式很少单独出现，一个真实 workflow 通常组合 2 到 4 个。Anthropic 把 Bun 从 Zig 重写到 Rust 用的就是"Fan-out（一个 agent 一个 callsite）→ 对抗验证 → loop until done"。

### 容易踩的坑

这部分基本是 Claude Code 团队自己的提醒：

  * 该用普通 session 解决的事别上 workflow。大多数日常编码任务不需要 5 个评审。
  * 不设 token 预算。一个野心大的 workflow 不封顶能烧到预期的 5 到 10 倍。直接在 prompt 里写"use 10k tokens"。
  * 让同一个 agent 既干活又验证。Self-preference 会让验证形同虚设。
  * 把 `parallel()` 和 `pipeline()` 当一回事。barrier 的差别很关键。
  * loop 模式不配 `/goal`。它会在第一个软完成点停下。
  * 让不可信内容直达执行 agent。处理用户提交内容必须 quarantine——读取 agent 只读，动作 agent 不接触原文。
  * 排序用绝对打分。换成 tournament 配对比较。
  * 跑通的 workflow 不保存。在 workflow 菜单按 `s` 存到 `~/.claude/workflows`，或者打包成 Skill 分发。

### 一句旁观者的话

有个网友总结挺贴切：Boris 不小心把 n8n 整套 UX 哲学说了一遍，一个 loop 看 webhook，一个 loop 看 schedule，一个 loop 盯队列。你不再跑任务，你在养管道。但编排层得先稳，再上动态路由，不然会被自己绕死。
