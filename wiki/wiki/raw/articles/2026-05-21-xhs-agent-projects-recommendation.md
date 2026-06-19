---
title: "Agent项目推荐：高质量开源项目"
url: "https://www.xiaohongshu.com/explore/6a0304ac0000000006031691?xsec_source=app_share&type=normal&xsec_token=CBzvqdzBliCABr5O8esTJJ4BnM8wmkIC70KNrQaw7BIrY="
source: "小红书"
author: "摸鱼酱在coding"
fetched: 2026-05-21
status: "success"
tool: "Spider_XHS"
likes: 441
collected: 850
comments: 6
tags: [AIAgent, agent项目, agent, agent开发, github开源项目推荐, 简历项目, 面试复盘, 找实习, 春招]
sha256: 19806feac0a730ba
---

# Agent项目推荐：高质量开源项目

**作者**: 摸鱼酱在coding | 👍 441 | ⭐ 850 收藏 | 💬 6 评论

上一篇发出来之后，私信和评论里问得最多的就是："有没有具体项目可以参考"，现在四个方向各挑了一个GitHub 高质量开源项目供大家参考。

AI Coding：Aider
44k+ stars，Apache 2.0
最值得学的部分是 repo map。前一篇里讲过"为什么不能直接把整个仓库塞给模型"，repo map 就是这个问题最早的开源答案之一：用tree-sitter解析代码，提取每个文件里的类、函数、关键定义，再用图算法算出哪些符号和当前任务最相关，只把这些塞进context。
二改：repo map 适配熟悉的语言生态（Aider对Python/JS 最好）

Deep Research：GPT-Researcher
27k+ stars，MIT
核心是planner和execution两类agent分工：planner把研究问题拆成一组子问题，execution agents并行去抓信息，最后由publisher聚合成带引用的报告。为了控制成本，会按需在 gpt-4o-mini 和 gpt-4o 之间切，一次任务平均 2 分钟、几美分。
二改：挑具体领域，比如医疗文献综述、行业财报对比、学术 survey，在被大部分项目忽略的环节上做深，评测体系、引用质量、矛盾信息的处理。

AIOps：HolmesGPT
2025 年 10 月成为 CNCF Sandbox 项目，Apache 2.0。
只读权限和 RBAC 是写在架构层的，agent 没有误操作生产的能力
二改方向：HolmesGPT 默认覆盖云原生场景，如果方向偏数据库、偏前端监控、偏业务告警，可以基于它的架构做垂直版本。

长期记忆：Letta（原 MemGPT）
22k+ stars，Apache 2.0
Letta 是 agent runtime，整个 agent 跑在 Letta 里，记忆系统是它的核心而不是附加层。核心设计来自 MemGPT 论文：把 LLM 的 context window 当成虚拟内存来管。
二改方向：挑一个很小但真实的场景，比如基于过去几个月聊天记录学写作风格助手，然后在 short-term/long-term怎么分、何时清理、怎么避免老信息污染上做扎实。
