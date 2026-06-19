---
title: Jason Liu
created: 2026-05-25
updated: 2026-05-25
type: entity
tags:
  - person
  - company
  - open-source
sources:
  - raw/articles/2026-05-24-jason-liu-getting-most-out-of-codex.md
  - raw/articles/2026-05-24-openai-codex-tips-jason-liu.md
---

# Jason Liu

## 概述

Jason Liu（@jxnlco）是 OpenAI Codex 团队的开发者体验工程师（Developer Experience Engineer），2026 年 5 月撰写了官方指南《Getting the Most out of Codex》，系统介绍 Codex 高级用法。他也是开源项目 **Instructor** 的作者。

## 背景

- 出生于中国北方，靠近蒙古草原的边境地带，在加拿大长大
- 安大略省公立艺术学校（数字动画和设计）→ 滑铁卢大学（计算数学和统计）
- 早期学数学物理，后转向计算机

## 职业轨迹

| 阶段 | 组织 | 角色/贡献 |
|------|------|----------|
| Meta | Meta | 内容审核算法 |
| Stitch Fix | Stitch Fix | Staff Engineer，5 年 ML 工程，构建多模态嵌入系统（ResNet-50、CLIP+GPT-3），开发内部框架 Flight（日处理 3.5 亿请求，内部采用率 80%） |
| 567 Studios | 独立咨询 | 客户包括 Zapier、HubSpot、Weights & Biases、Pydantic；在 Maven 教授 RAG 和 AI Agent 课程（学员来自 OpenAI、Anthropic、Google 等 50+ 公司） |
| OpenAI | OpenAI | Codex 团队 Developer Experience Engineer |

## Instructor

Jason Liu 最广为人知的开源项目。核心功能：用 Pydantic 从 LLM 输出中提取结构化数据。

- **GitHub Star**: 1.3 万+
- **月下载量**: 600 万+
- **影响**: OpenAI 官方推出的 Structured Outputs 功能明确表示受 Instructor 启发

## Codex 官方指南

2026 年 5 月，Jason 发表了《Getting the Most out of Codex》，这是 OpenAI 首次以内部视角系统介绍 Codex 的高阶用法。核心概念包括：

- **Durable Threads（持久线程）**：将线程作为持久化工作空间，而非一次性对话
- **Steering + Queuing**：实时干预（纠偏）+ 任务排队（追加），不需等 Agent 完成
- **Thread Automations（线程自动化）**：定时唤醒同一线程继续工作，带完整上下文
- **Goals（目标驱动）**：设定可验证的终点线，配套测试套件/Benchmark 等验证机制
- **Shared Memory（共享记忆）**：用 Obsidian 知识库存储跨线程持久上下文
- **Side Panel（侧边栏）**：代码、文档、幻灯片等产物的实时审查和标注

## 相关链接

- [[openai-codex]] — Jason 所在的团队产品
- Instructor（1.3 万 Star 开源项目，用 Pydantic 从 LLM 输出提取结构化数据）
- [[claude-code]] — Codex 的主要竞争者
- [[obsidian]] — Jason 推荐的 Codex 共享记忆方案
- [[agent-memory-systems]] — Codex Shared Memory 与之呼应
- [[long-running-agent]] — Codex Durable Threads 是长程 Agent 的实践方案
