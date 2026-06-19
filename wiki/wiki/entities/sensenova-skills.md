---
title: SenseNova-Skills
created: 2026-06-04
updated: 2026-06-04
type: entity
tags: [skill, open-source, tool, multimodal]
sources:
  - raw/articles/2026-06-03-sensenova-skills-open-source.md
---

# SenseNova-Skills

## 概述

SenseNova-Skills 是 [[sensetime|商汤科技（SenseTime）]] 于 2026 年 6 月开源的 AI 办公技能套件，面向任何 skills-compatible Agent 平台，兼容 **[[openclaw|OpenClaw]]** 和 **[[hermes-agent|Hermes Agent]]**。它通过 [[skills|Skill]] 形式将 AI 办公能力模块化，用户安装后即可在 Agent 对话中调用。

## 四大核心功能

### 🖼️ 信息图表生成
- 图片生成与图形设计
- 可镜像参考风格（mirror visual style from a reference）
- Demo：3D 天气预报信息图

### 📊 数据分析
- 多表解析（multi-sheet parsing）
- 数据清洗
- 基于图片的数据提取与可视化

### 📑 PPT 创建
- 大纲与内容生成
- 智能排版设计
- 输出可编辑的 PPT 文件

### 🔍 深度研究
- 跨学术、技术、社交平台等多源搜索
- 综合洞察并生成报告
- 可与信息图表生成组合使用

## 安装方式

- **GitHub**：开源仓库直接安装
- **快速安装**：与 [[hermes-agent|Hermes Agent]] 和 [[openclaw|OpenClaw]] 捆绑安装（agent 平台内置支持）

## 兼容性

| 平台 | 支持 |
|------|------|
| [[openclaw|OpenClaw]] | ✅ |
| [[hermes-agent|Hermes Agent]] | ✅ |
| 其他 skills-compatible Agent | ✅（任何支持 SKILL.md 规范的平台） |

## 相关页面

- [[skills]] — Skill 的定义和生态
- [[hermes-agent]] — 兼容的 Agent 平台
- [[openclaw]] — 兼容的 Agent 平台
- [[sensetime]] — 开发商汤科技
