---
title: 横纵分析法 (hv-analysis)
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, methodology, open-source]
sources:
  - raw/articles/2026-04-18-deep-research-prompt.md
---

# 横纵分析法 (hv-analysis)

## Overview

横纵分析法是 AI 博主「数字生命卡兹克」基于社会科学和语言学研究视角开发的研究方法论，封装为 Prompt 和 Skill 两种形式。核心思想是**纵向追时间深度，横向追同期广度，最后交汇出判断**。开源在 [GitHub 仓库](https://github.com/KKKKhazix/khazix-skills)。

## 方法论起源

灵感来自两个学术传统：

1. **索绪尔的历时分析与共时分析**：语言学中研究一个语言现象可以从时间维度（历时，diachronic）和当下系统维度（共时，synchronic）入手
2. **社会科学的纵向研究与横截面研究**：追踪对象变化轨迹 vs 在某时间点上观察截面状态

结合商业和竞争战略分析思路，形成了一套用 AI 来跑的通用研究框架。

## 两条轴

### 纵向分析（Diachronic / Longitudinal）

沿时间轴完整还原研究对象从诞生到现在的发展全貌：
- **起源追溯**：诞生背景、技术/理念来源、创始团队、行业环境
- **诞生节点**：首次发布时间和初始形态
- **演进历程**：按时间梳理关键节点（版本更新、融资、战略转型等）
- **决策逻辑**：每个关键节点上为什么选 A 而非 B
- **叙事驱动**：不是干巴巴的年表，而是有因果和脉络的故事

### 横向分析（Synchronic / Cross-sectional）

以当前时间点为切面，与竞品全面对比：
- 场景 A（无竞品）：分析为什么没有竞品、潜在竞争者方向
- 场景 B（少量竞品）：逐一深入对比
- 场景 C（竞品充分）：选取 3-5 个代表性竞品对比

对比维度包括技术路线、产品形态、用户口碑、生态位分析、趋势判断。

### 交叉验证

纵向告诉你它怎么走到今天，横向告诉你它今天站在哪。两条轴交叉可以看到单独看任何一条轴都看不到的东西。

## 使用方式

### Prompt 版本

配合有深度研究功能的 AI（ChatGPT DeepResearch、Claude 深度研究、豆包专家模式等），只需修改「研究对象」等式即可。约 13 分钟可产出一万字研究报告。

### Skill 版本（hv-analysis）

安装到 [[claude-code]] 等 Agent 后，直接说「帮我研究一下 xxx」即可。额外功能：
- 自动联网搜索信息
- 包含 arxiv API 查询学术论文
- 生成排版好的 PDF 研究报告
- 文风优化，更易读

## 局限性

- 不是万能的，替代不了亲自下场的深入研究
- AI 信息仍可能有幻觉和 inaccuracies
- 报告质量取决于所用模型和工具——DeepResearch 工具效果最好（10 分钟以上），普通联网搜索效果大打折扣（不到 1 分钟）
- 建议作为研究的起点（快速建立地图），而非结论

## Relationships

- Skill 版本部署在 [[claude-code]] / Agent 平台上
- 与 [[skills]] 生态兼容，属于 khazix-skills 仓库
- 与 [[khazix-writer]] 同一作者不同工具
- 研究方法论与 [[knowledge-compilation]] 互补

## See Also

- [[skills]] — Skill 加载和运行机制
- [[claude-code]] — hv-analysis Skill 的运行平台
- [[knowledge-compilation]] — 另一种知识组织方法论
- [[khazix-writer]] — 同作者的写作 Skill
