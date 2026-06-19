---
title: PinMe
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, deployment, skill, open-source]
sources:
  - raw/articles/2026-05-21-pinme-skill-one-click-deploy.md
---

# PinMe

## 概述

PinMe 是一个开源的一键部署工具和 AI Agent Skill，由 glittrernetwork 开发。从静态页面部署工具（1.0）升级为全栈应用一键部署平台（2.0）。累计部署超过 100 万个网站。支持通过 Claude Code 等 AI Agent 实现自然语言驱动的开发-部署闭环。

- **项目地址**: https://github.com/glitternetwork/pinme
- **官网**: https://pinme.dev/
- **安装**: `npx skills add glitternetwork/pinme`

## 核心能力

### 静态页面部署
- 30 秒内将网页、图片等资源转成公网链接
- 静态资源走 IPFS 分布式存储
- 支持拖拽上传，零配置

### 全栈应用部署
- 前端 SPA + Edge Runtime + Serverless SQL
- 一条命令完成全链路部署（前端 + 后端 + 数据库）
- 支持邮件推送和 LLM 调用能力

### AI Agent Skill 集成
- 安装 Skill 后，AI Agent 自动获得部署能力
- 工作流：自然语言描述 → AI 写代码 → PinMe 自动部署 → 返回链接
- 后续修改自动重新部署，无需手动操作
- 与 [[claude-code]] 深度集成，`npx skills add glitternetwork/pinme` 即可安装

## 典型用例

- 个人记账本（含数据库，多人共享）
- 共享像素画板（实时协作，状态持久化）
- MVP 验证和 Demo 展示
- AI 生成页面的即时发布

## 技术架构

| 层 | 技术 |
|---|---|
| 前端 | 现代 SPA 框架 |
| 后端 | Edge Runtime |
| 数据库 | Serverless SQL |
| 静态存储 | IPFS 分布式 |

## 在 Agent 生态中的定位

PinMe 代表了 AI Agent 工具链中「最后一公里」的解决方案——代码写完后如何上线。它补全了 [[skill-engineering]] 中部署环节的空白，让 Agent 的能力从"写代码"延伸到"代码上线"。与 [[openclaw]]、[[claude-code]] 等 Agent 平台配合使用。

## 相关链接

- [[claude-code]] — 主要集成的 AI Agent 平台
- [[skills]] — PinMe 以 Skill 形式分发
- [[skill-engineering]] — Skill 工程化设计方法论
- deployment — 部署基础设施（标签）
