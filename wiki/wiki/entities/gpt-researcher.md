---
title: GPT Researcher
created: 2026-05-21
updated: 2026-05-22
type: entity
tags: [tool, agent, methodology, open-source]
sources:
  - raw/articles/2026-05-21-xhs-agent-projects-recommendation.md
---

# GPT Researcher

## 概述

GPT Researcher 是一个开源的自主研究 Agent，27k+ Star，MIT 许可。能够对任何给定主题进行全面的在线研究。它自动搜索、抓取和整理来自多个来源的信息，生成结构化的研究报告。

## 核心架构

- **Planner Agent**：将研究问题拆成一组子问题
- **Execution Agents**：并行去各数据源抓取信息
- **Publisher**：聚合成带引用的报告
- **成本控制**：按需在 gpt-4o-mini 和 gpt-4o 之间切换，一次任务平均 2 分钟、几美分

## 核心特性

- **自主研究**：给定研究主题后，Agent 自动规划搜索策略
- **多源聚合**：从多个在线来源收集和交叉验证信息
- **结构化输出**：生成包含引用的研究报告
- **本地运行**：支持本地部署，保护研究隐私

## 在 Agent 生态中的定位

GPT Researcher 是 AI 深度研究领域的代表性开源项目。与 [[claude-code]] 的深度研究功能不同，GPT Researcher 是独立的、可自部署的研究 Agent，专注于信息检索和报告生成。

## 适用场景

- 学术文献调研
- 市场研究报告
- 竞品分析
- 技术趋势调研

## 相关链接

- [[aider]] — AI 编码工具
- [[holmesgpt]] — AIOps 调查 Agent
- [[multi-agent-collaboration]] — 研究任务的多 Agent 协作
