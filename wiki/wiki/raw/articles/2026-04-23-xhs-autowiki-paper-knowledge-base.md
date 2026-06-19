---
title: "AutoWiki-paper knowledge base"
url: "https://www.xiaohongshu.com/explore/69d76c6a000000001f0062fd?app_platform=ios&app_version=9.26.1&share_from_user_hidden=true&xsec_source=app_share&type=normal&xsec_token=CBi6XG4wQBR9Z-CzMHPR_UFP1ZjVpfKA-um9NvDx7jv_A=&author_share=1"
source: "小红书"
author: "雾灯"
fetched: 2026-04-23
status: "success"
tool: "Spider_XHS"
likes: 79
collected: 119
comments: 4
tags: [科研学习, wiki, llm, 科研, 文献管理]
note_type: 图集
sha256: a898ca31e4ac9a5e
---

# AutoWiki-paper knowledge base

**作者**: 雾灯 | 👍 79 | ⭐ 119 收藏 | 💬 4 评论

宣传一下AutoWiki的工作～
论文看了就忘、笔记散落一地？让LLM帮你"编译"一本活的wiki。

🚨 问题背景
做研究最痛的不是读论文，是管论文：
1. 笔记碎片化：Notion 里一堆孤立 summary，互相不链接，三个月后跟没读一样。
2. 综述级理解太奢侈：真正有价值的是"这篇工作在领域时间线上处于什么位置"，但这种时序推理人工做一次就累瘫。

🔥 AutoWiki：让 LLM 把论文"编译"成一本结构化 Wiki
核心思路：你只管扔 PDF，LLM 负责读、分析、归类、写、链接、维护。
- Milestone-Centric 组织：不按文件夹分类，按"概念突破点"聚类——就像好综述不列 reference list，而是讲 idea 怎么演化的。
- Temporal Graph 自动构建：每篇论文自动定位到领域时间线上，标注 extends / complements / contrasts_with，演化链路一目了然。
- Deep Analysis, Not Summary：不做摘要搬运工。用 prior/update 对比结构提取真正的 novelty delta——"领域已经有什么 → 这篇到底新在哪"。

💡 关键设计
- Skill = Architecture：整个系统就是一个 390 行的 SKILL.md，编码了质量标准、反模式和工作流规则。

📈 实战效果
- 80 篇 Agent Self-Evolution 论文，2 h完成 ingest，产出 13 个 milestone 节点 + 三层分类体系
- 每篇论文自动生成：本质提炼、CRGP 因子分析、时序关系图、批判性分析（prior → update 对比）
- Topic 页面读起来就像一篇小综述：milestone 定义、演化脉络、开放问题、跨领域联系，全部自动生成。

AutoWiki 把 Karpathy 的愿景落了地：你扔论文，LLM 编译 wiki，知识自动复利。

欢迎交流反馈
🔗 GitHub：AlphaLab-USTC/AutoWiki-skill
🔗 Showcase（80 篇论文 wiki 实例）：在线 Demo 见 repo

#科研学习 #wiki #llm #科研 #文献管理
