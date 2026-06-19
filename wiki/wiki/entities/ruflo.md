---
title: Ruflo
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, multi-agent, agent]
sources:
  - raw/articles/2026-05-17-8-github-open-source-projects.md
---

# Ruflo

## 概述

**Ruflo** 是将 [[claude-code]] 扩展为可协调 Agent 集群系统的编排平台。5.1 万 Star，是目前 Claude 生态最大的编排平台。

## 核心架构

### 100+ 专用 Agent
内置 100+ 个专用 Agent，涵盖：
- 编码
- 测试
- 安全
- 文档
- 架构
- 等多种开发角色

### 集群拓扑
支持多种集群组织拓扑：
- **层级式** — 上下级管理关系
- **网状** — 平等协作关系
- **自适应** — 动态调整结构

集群内部通过 **Raft 共识算法** 和 **拜占庭容错** 协调。

### SONA 神经架构
自学习记忆系统，从每个任务中学习，跨会话记忆。向量搜索使用 HNSW 算法，比暴力搜索快 150–12500 倍。

### Agent 联邦
支持跨机器协作：
- 零信任架构
- mTLS 加密
- ed25519 认证
- PII 自动脱敏

## 使用方式

安装后正常使用 [[claude-code]] 即可。Hooks 系统会自动：
- 路由任务
- 检索记忆
- 协调后台 Agent

## Relationships

- 基于 [[claude-code]] 构建
- 实现 [[multi-agent-collaboration]] 的编排层
- 与 [[oh-my-claudecode]]、[[everything-claude-code]] 同类但规模更大
- 与 [[claude-managed-agents]] 的 [[anthropic]] 官方方案互补

## See Also

- [[claude-code]] — 基础平台
- [[multi-agent-collaboration]] — 多 Agent 协作概念
- [[oh-my-claudecode]] — 另一个 Claude Code 多 Agent 编排
- [[claude-managed-agents]] — Anthropic 官方 Agent 基础设施
