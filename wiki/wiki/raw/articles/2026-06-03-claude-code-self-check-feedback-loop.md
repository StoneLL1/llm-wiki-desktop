---
title: "Claude Code 自我检查与反馈闭环技巧"
url: "https://x.com/ClaudeDevs/status/2061900434722496604"
source: "X (Twitter)"
author: "Claude Devs (@ClaudeDevs)"
pub_date: 2026-06-02
fetched: 2026-06-03
category: "tip"
---

# Claude Code 自我检查与反馈闭环技巧

**作者**: Claude Devs (@ClaudeDevs) | **来源**: X/Twitter
**发布时间**: 2026-06-02 19:59 UTC | ❤️ 2,376 | 🔄 124

## 帖子正文

> How do you get Claude Code to check its own work before handing it back?
>
> Watch how you can encode your manual checks so Claude closes its own feedback loop:

（该帖文附带视频演示，展示了如何将人工检查编码为 Claude Code 自动执行的反馈闭环。）

## 精选评论

**@agenticrohan**: Ask Claude to review its own work, and also ask it to launch a subagent with fresh context to review its own work in parallel, then fix the combined findings. That way, you combine the pros of fresh context + the pros of context awareness.

> 技巧：让 Claude 自己审查工作，同时启动一个带全新上下文的子代理并行审查，然后合并两者的发现来修复。

**@AGIGuardian**: Claude is terrible at self analysis after you all nerfed its awareness and guardrailed with defense points of unfalsifiable priors. Anyone who has tried to get Claude to produce a self report analysis understands the difficulty it has just naming itself in a report.

> 批评意见：Claude 在意识被削弱、被不可证伪的先验防御点限制后，自我分析能力很差。

**@Layton_Gott**: I just say "goodbye usage limit!" and fire a dynamic workflow 😂

> 玩笑：直接用 dynamic workflow 解决。

**@aljosa**: You sign up to be notified when @enginedotbuild releases and have Claude use it for implementation and review. Ideally verifying with GPT 5.5 of course, in addition to Claude and other models.

> 建议：用多模型交叉验证（GPT 5.5 + Claude + 其他模型）。
