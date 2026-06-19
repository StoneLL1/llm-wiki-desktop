---
title: andrej-karpathy-skills
created: 2026-05-22
updated: 2026-05-24
type: entity
tags: [tool, engineering, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
---

# andrej-karpathy-skills

## 概述

andrej-karpathy-skills 是由 forrestchang 开发的 [[claude-md|CLAUDE.md]] 配置包，灵感来自 [[andrej-karpathy]] 对大模型写代码常见问题的吐槽。目前狂揽 5 万 Star，是 Claude Code 生态中最受欢迎的配置方案之一。

## 核心问题与原则

Karpathy 吐槽的问题：
- 模型做了错误假设不澄清就硬往下写
- 代码和 API 过度设计，搞一堆没用的抽象
- 顺手改了没完全理解的代码
- 困惑了也不说就装懂继续

转化为 4 条原则：
1. **思考再写**：明确假设，必要时推回（push back）
2. **简洁优先**：只写必要代码，不搞推测性功能
3. **手术式修改**：只碰必要的代码，匹配现有风格
4. **目标驱动执行**：先定验证标准再跑

## 安装方式

```bash
/plugin marketplace add forrestchang/andrej-karpathy-skills
/plugin install andrej-karpathy-skills@karpathy-skills
```

也可直接将 CLAUDE.md 下载到项目根目录。

## 在 CLAUDE.md 生态中的定位

andrej-karpathy-skills 是 [[claude-md]] 最佳实践的具体化——将 [[andrej-karpathy]] 的编码观察转化为可执行的 [[skills|Skill]] 规则。与 [[agent-skills-addyosmani]] 的全流程覆盖不同，它更侧重于编码行为的约束和修正。

## 相关链接

- [[claude-md]] — CLAUDE.md 配置最佳实践
- [[skills]] — Skill 模块化能力系统
- [[agent-skills-addyosmani]] — Addy Osmani 的 AI 编码工程纪律包
