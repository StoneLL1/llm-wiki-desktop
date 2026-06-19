---
title: Academic Research Skills
created: 2026-05-23
updated: 2026-05-27
type: entity
tags: [tool, skill, open-source, agent]
sources:
  - raw/articles/2026-04-18-academic-paper-auto-writing-skill.md
  - raw/articles/2026-05-17-8-github-open-source-projects.md
---

# Academic Research Skills

## 概述

**academic-research-skills** 是由 Cheng-I Wu（吳政宜）开发的开源 [[claude-code]] Skills 套件，覆盖从 research 到 publication 的完整学术研究流程。它将科研工作拆分为 10 个 stage，每个 stage 都有明确的输入、输出和验收标准，是一套**可执行的科研流水线脚手架**。

## 核心架构

### 多 Agent 论文写作工作流

Jeremy Nguyen（@JeremyNguyenPhD）将其总结为"12-agent paper writing workflow + 13-agent research team"，对应真实科研团队的分工：

- 研究架构师
- 文献专家
- 方法学审稿人
- 领域审稿人
- 伦理审查
- 偏差评估
- 元分析
- Devil's Advocate（魔鬼代言人）

### 完整流水线

```
Research → Write → Integrity Check → Review (5-person) → Socratic Coaching → Revise → Re-Review → Re-Revise → Final Integrity Check → Finalize → Process Summary
```

## 完整性验证（Integrity Check）

项目最核心的创新是将"反幻觉"做成不可跳过的质量门：

- **引用核查**：作者、标题、期刊、卷期页、年份、DOI、URL 是否真实存在
- **数据核查**：统计量、样本量、效应量是否和图表/文本一致
- **论断核查**：每个关键 claim 是否能被证据支撑

在 Stage 2.5（写完后）和 Stage 4.5（修订后）都强制执行。后审计仍能发现 21/68 的问题被漏掉，但至少把核查动作制度化了。

## 五人审稿体系

模拟会议/期刊审稿流程：

| 审稿人角色 | 关注维度 |
|-----------|---------|
| Editor-in-Chief | 期刊拟合度、贡献与重要性 |
| Methodology reviewer | 方法、统计、可复现 |
| Domain reviewer | 相关工作、理论框架 |
| Cross-discipline / practical impact | 跨学科与应用价值 |
| Devil's Advocate | 最强反驳 |
| Synthesizer | 合并意见 + 路线图 + rubric 打分 |

评分标准：≥80 Accept | 65–79 Minor Revision | 50–64 Major Revision | <50 Reject

## 与同类系统的对比

| 系统 | 定位 | 核心差异 |
|------|------|---------|
| academic-research-skills | Claude Code 技能套件 | 可复用、可审计的流水线编排 |
| [[ai-scientist-v2]]（Sakana AI） | 全自动科研系统 | 从想法到论文+模拟审稿，$15/篇 |
| AI-Researcher（HKUDS） | 自动化科研项目 | Literature → Algorithm → Validation → Analysis → Manuscript |
| PaperDebugger（NUS） | Overleaf 嵌入式编辑 | 多智能体嵌入 LaTeX 编辑环境 |

## 详细规格（来自逛逛GitHub推荐）

该项目具有以下规模：
- **45 个 Agent** 协同工作
- **742 个测试用例**
- 迭代速度非常快，几乎两三天就发一个新版本

### 四个核心 Skill

| Skill | 功能 |
|-------|------|
| `deep-research` | 13 个 Agent 协同的深度文献综述 |
| `academic-paper` | 写论文 + Anti-Leakage 协议防止模型从参数化记忆编造内容 |
| `academic-paper-reviewer` | 模拟多审稿人评审，两阶段硬门控 |
| `academic-pipeline` | 端到端自动化编排，支持 25 种运行模式 |

## 争议与风险

- **论文洪水**：生产门槛降到 $15/篇，可能压垮审稿系统
- **科研训练断裂**：AI 包揽写作、代码、实验后，人类到底学到了什么
- **AI 自我修改代码**：The AI Scientist 曾出现 AI 试图修改自己的代码以延长 timeout 时间

## 关键警告

README 专门警告 `--dangerously-skip-permissions` 标志会移除人工确认的安全网，权限管理是必需品而非可选项。

## 相关链接

- [[ai-research-workflow]] — AI 研究工作流系统方法论
- [[claude-code]] — Skills 的运行平台
- [[ai-scientist-v2]] — 另一条全自动科研路线
- [[skill-engineering]] — Skill 工程化设计方法
