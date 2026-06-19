---
title: Khazix Mac Cleaner Skill
created: 2026-06-04
updated: 2026-06-04
type: entity
tags: [skill, tool, open-source, automation]
sources:
  - raw/articles/2026-06-03-khazix-mac-cleaner-skill.md
---

# Khazix Mac Cleaner Skill

## 概述

Khazix Mac Cleaner 是 AI 博主「数字生命卡兹克」（[[khazix-writer|@Khazix0918]]）开源的一款基于 AI Agent 的存储清理 [[skills|Skill]]，同时支持 macOS 和 Windows。它利用 Codex（或其他 Agent）对磁盘做只读扫描，以三色风险分级系统帮助用户安全地释放存储空间。开源仓库：[github.com/KKKKhazix/khazix-skills](https://github.com/KKKKhazix/khazix-skills)。

## 背景与动机

作者看到一条推文——有人用 Codex 提示词对 MacBook 做只读存储分析，发现了 116GB 的 `codex-tui.log` 文件和 500GB 可回收空间。作者对自己积攒 2 年的 MacBook Air 运行同样提示词后，发现约 100GB 的 B 站离线视频藏在 `Containers` 目录深处，以及 Chrome、开发环境、Claude 等产生的大量文件。Codex 评估保守可释放约 **120GB**，激进方案可超 140GB。

作者想把这个能力做成任何人都能用的工具，替代收费的 CleanMyMac（约 $40/年）。触发词为任意自然语言请求（如「帮我看看存储」）。

## Skill 工作流程

### 1. 仪表盘概览
- 总容量、已用空间、可用空间，带颜色进度条
- 系统环境摘要

### 2. Top 5 空间杀手
- 按空间排序，含颜色标签、类型、完整路径、人话说明

### 3. 🟢🟡🔴 三色风险分级（核心）

| 颜色 | 含义 | 包含内容 | 操作 |
|------|------|---------|------|
| 🟢 绿色 | 安全可清理 | 纯缓存、临时文件、安装器残留（无功能影响） | 「移到废纸篓」（可恢复）或「直接删除」（不可逆），均需确认弹窗，支持批量清理 |
| 🟡 黄色 | 需人工判断 | B 站视频、下载文件夹安装器、项目文件夹等 | Agent 解释影响，无直接删除按钮，仅「在访达中打开」和针对已验证安全的子目录的「移到废纸篓」 |
| 🔴 红色 | 禁止触碰 | 系统文件、活跃应用核心数据、sleepimage 等 | 解释跳过原因，仅「在访达中打开」供高级用户自行处理 |

### 4. 长期优化建议
超越即时清理的可操作建议

### 安全原则
- 扫描全程**只读**，无用户主动发起则不写入
- 删除始终需要**两步确认**（操作按钮 + 确认弹窗）
- AI 幻觉在此用例中极低，但仍建议谨慎

## 实测结果

- 从 MacBook Air 释放近 **120GB**（CleanMyMac 仅发现 15.8GB）
- 跨平台验证：在同事的 Windows PC 上用 Codex 成功测试，同一 Skill 跨平台、跨 Agent 工具可用

## 与 CleanMyMac 对比

| 维度 | CleanMyMac | Khazix Mac Cleaner (Agent Skill) |
|------|-----------|-------------|
| 扫描时间 | ~30 分钟 | 几分钟 |
| 发现空间 | 15.8GB 通用垃圾 | 120GB+（含非传统冗余如 B 站离线视频） |
| 透明度 | 「用户缓存文件」等模糊标签，无详情 | 完整路径、大小、说明、影响分析 |
| 可定制性 | 固定规则 | 自然语言驱动，可按需查找特定类型文件 |
| 决策信心 | 用户必须盲信软件 | 颜色编码+描述赋能知情决策 |
| 灵活性 | 静态软件 | 可适配任何基于规则的任务 |

## 深层洞察：Agent vs 传统软件

卡兹克提出核心洞察：

> **「软件正在从资产变成耗材。软件的本质就是人和机器之间的翻译层，而 Agent 正在填平这道鸿沟。」**

传统软件是固化的产品——功能写死、规则固定、无法适应个性化需求。Agent Skill 是可被自然语言驱动、按需定制的「软件耗材」——不需要时就消失，需要时即时生成。这个 Skill 的诞生体现了 Agent 时代的典型创新模式：**发现需求 → 用自然语言让 Agent 执行 → 将有效提示词封装为 Skill → 开源分发**。

## 相关页面

- [[khazix-writer]] — 同作者的风格创作 Skill，同一 GitHub 仓库
- [[skills]] — Skill 的概念和生态
- [[garden-skills]] — 另一个 Agent Skills 合集
- [[hermes-agent]] — 支持 Skills 的 Agent 平台
