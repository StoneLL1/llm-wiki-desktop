---
title: HolmesGPT
created: 2026-05-21
updated: 2026-05-22
type: entity
tags: [tool, agent, deployment, open-source]
sources:
  - raw/articles/2026-05-21-xhs-agent-projects-recommendation.md
---

# HolmesGPT

## 概述

HolmesGPT 是一个开源的 AIOps 调查 Agent，22k+ Star，Apache 2.0 许可。2025 年 10 月成为 CNCF Sandbox 项目。专门用于运维场景中的问题诊断和根因分析。它能够连接到各种运维工具和数据源，自动调查系统异常。

## 核心特性

- **CNCF Sandbox 项目**：2025 年 10 月入选，表明其云原生社区认可度
- **只读权限和 RBAC**：安全架构内置，Agent 没有误操作生产的能力
- **运维诊断**：自动分析系统异常，定位根因
- **多数据源集成**：支持 Kubernetes、Prometheus、Grafana 等运维工具
- **自然语言交互**：用自然语言描述问题，Agent 自动排查
- **开源可自部署**：保护企业运维数据安全

## 在 Agent 生态中的定位

HolmesGPT 代表了垂直领域 Agent 的发展方向——不追求通用能力，而是深耕特定领域（AIOps）的专业 Agent。这种思路与 [[skill-engineering]] 的理念一致：让 Agent 在特定领域做到极致。

## 适用场景

- 生产环境故障排查
- 系统性能分析
- Kubernetes 集群诊断
- 告警根因分析

## 相关链接

- [[aider]] — AI 编码工具
- [[gpt-researcher]] — AI 研究工具
- [[multi-agent-collaboration]] — 运维场景的多 Agent 协作
