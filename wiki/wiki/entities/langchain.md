---
title: LangChain
created: 2026-05-21
updated: 2026-05-21
type: entity
tags: [tool, agent, engineering, open-source]
sources:
  - raw/articles/2026-05-11-x-how-to-build-first-ai-agent-10k-plus.md
---

# LangChain

## 概述

LangChain 是一个用于构建 LLM 驱动应用的开源框架，是 AI Agent 生态中最早且最具影响力的框架之一。它提供了丰富的抽象层和工具链，支持 [[agent-loop]]、[[multi-agent-collaboration]]、RAG 等多种 AI 应用模式。

## 核心组件

### LangChain
- **Chains**：将多个 LLM 调用和工具操作串联成工作流
- **Agents**：支持 ReAct、Plan-and-Execute 等多种 Agent 模式
- **Tools**：丰富的工具集成生态
- **Memory**：对话历史和状态管理

### LangGraph
LangChain 推出的图结构 Agent 编排框架，特别适合：
- 多 Agent 工作流编排
- 复杂状态机实现
- 循环和条件分支

## 在 Agent 生态中的定位

LangChain 是 Agent 框架的"瑞士军刀"——功能全面但复杂度较高。在 2026 年的 Agent 生态中，它面临来自 [[hermes-agent]]、[[openclaw]] 等更轻量级框架的竞争。

## 生态项目

- **LangGraph** — 图结构 Agent 编排
- **LangSmith** — Agent 追踪和调试平台
- **LangServe** — 将 Chain 部署为 API

## 相关链接

- [[agent-loop]] — LangChain Agent 的基础循环
- [[react-pattern]] — LangChain 支持的核心 Agent 模式
- [[multi-agent-collaboration]] — LangGraph 的多 Agent 编排
- [[plan-and-execute-pattern]] — LangChain 提出的 Agent 模式


## 框架对比中的定位

主流 Agent 框架选型建议：
- **快速原型** → LangChain（丰富工具链和集成）
- **RAG 应用** → LlamaIndex（专注数据索引和检索）
- **多 Agent 协作** → AutoGen / CrewAI（专为多智能体设计）
- **复杂流程控制** → LangGraph（基于状态管理的 workflow）
- **.NET 生态** → Semantic Kernel（轻量级、插件化）

### Sources
- raw/articles/2026-04-18-build-ai-agent-framework.md
