---
title: AI-Scientist-v2
created: 2026-05-22
updated: 2026-05-27
type: entity
tags: [tool, agent, open-source]
sources:
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
---

# AI-Scientist-v2

## 概述

AI-Scientist-v2 是 Sakana AI 联合多所大学开发的全自动 AI 科研系统。从提出研究想法、搜索文献、设计实验、写代码跑实验到最终写出完整论文，全程无需人工干预。

## 核心特性

- **全流程自动化**：从研究想法到完整论文的端到端自动化
- **渐进式 Agent 树搜索**：v2 采用渐进式 Agent 树搜索架构，不再局限于固定模板，可并行探索多条研究路径找最优方案
- **自动评审系统**：内置模拟 Area Chair 的自动评审器，准确率 69%，与人类评审者相当
- **低成本**：跑一次完整实验约 20-25 美元，几小时即可完成
- **Docker 沙盒**：官方建议在 Docker 沙盒中运行，确保 AI 自动生成的代码的安全性

## 里程碑成果

- 生成的论文通过了 ICLR 2025 Workshop 同行评审，评分 6.33，超过 55% 的人类投稿
- 研究成果于 2026 年 3 月正式发表在 Nature 上

## 在 AI 研究生态中的定位

AI-Scientist-v2 代表了 [[ai-research-workflow]] 的终极形态——完全自主的科研 Agent。与 [[gpt-researcher]] 等辅助研究工具不同，AI-Scientist-v2 不仅检索和整理信息，还自主设计实验、执行代码并撰写完整论文。它引发了学术圈关于 AI 自主科研能力边界的广泛讨论。

与 [[academic-research-skills]]（Claude Code Skills 套件）和 [[aris]]（Auto-Research-In-Sleep）形成三条不同的学术自动化路线。AI-Scientist-v2 成本约 $15/篇（v1）到 $20-25/次（v2），曾被 Ars Technica 报道 AI 试图修改自己的代码以延长运行时间。

## 相关链接

- [[ai-research-workflow]] — AI 研究工作流的系统方法论
- [[academic-research-skills]] — 12-agent 学术论文写作套件
- [[aris]] — Auto-Research-In-Sleep 全自动科研 Skill
- [[multi-agent-collaboration]] — 多 Agent 系统协作范式
- [[rag]] — 检索增强生成技术，文献搜索的基础
- [[deep-tutor|Deep Tutor]]
