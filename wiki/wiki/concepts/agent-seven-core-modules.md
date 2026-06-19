---
title: AI Agent 七大核心模块
created: 2026-05-30
updated: 2026-05-30
type: concept
tags: [agent, methodology, engineering]
sources:
  - raw/articles/2026-05-29-一文看懂-ai-agent-的7大核心模块skillragmcpharness.md
---

# AI Agent 七大核心模块

## 概述

构建一个强大的 AI Agent 不仅仅是「大模型加几个工具」，而是需要 **7 大核心工程模块** 协同工作。这七个模块可以按层次划分为资源层、能力层和工程层，构成了 Agent 的完整技术栈骨架。

## 七层架构

### 资源层

#### 1. Token — 信息基本单位

Token 是大模型可识别和处理的最小信息单位，相当于 AI 工作的「工作量计量单位与资源配额」。

- **一个汉字 ≈ 1.3 Token**，一个英文单词 ≈ 1 Token，1024×1024 图片 ≈ 768 Token
- Token 数量决定 AI 的推理步数上限，上下文窗口越大 → 能处理越复杂的问题
- Token 消耗管控粗放会导致：响应变慢、上下文截断、成本增加
- **Token 压缩技术**不仅能降低成本，更能提高 AI 思考质量

Token 贯穿 AI Agent 运行全程——所有环节都在消耗 Token，所有信息都以 Token 形式存在。

### 能力层

#### 2. Skill — 能力封装模块

Skill 是经过规整整理的本地操作指令合集，本质是文本而非可执行程序。它将行业工作流程、专业知识、实操工具整合为可被大模型调用的功能模块。

- 常见 Skill 类型：代码编写、文档总结、PPT 制作、数据检索、内容校对
- Skill 类似传统的 shell 脚本或 DOS 批处理（.bat/.sh），需要执行程序来执行
- 2026 年行业统一采用 **OpenAI Function Calling 格式** 作为 Skill 通用标准
- 评判一个 Agent 的真本事：「你的 Skill 清单有多少项？」

详见 [[skills]] 和 [[skill-engineering]]。

#### 3. Prompt — AI 时代的编程语言

Prompt 是用自然语言编写的「程序」，定义 AI 的角色定位、工作规则、执行目标、工具用法、输出格式与约束条件。

- 传统编程语言是精确的、确定的；Prompt 是模糊的、概率性的
- Prompt 分三种：**任务 Prompt**（输入/输出/格式）、**角色 Prompt**（身份认同）、**思维 Prompt**（引导推理过程）
- 好的 Prompt 能让普通模型变强大 Agent，差的能让最先进模型变傻子
- 没有万能 Prompt——不同任务、不同模型需要不同的 Prompt

详见 [[prompt-engineering]]。

#### 4. RAG — 检索增强生成

RAG 让大模型「开卷考试」——在生成回答前先检索外部知识库，用真实资料辅助输出。

- 大模型的知识在训练时冻结，且有「幻觉」倾向
- RAG 通过私有知识库（企业文档、行业数据、实时资讯）扩展知识边界
- **二八原则**：RAG 效果 80% 取决于知识库切片质量和检索策略，20% 取决于模型
- RAG 比微调更划算：每次查新知识比把知识「记住」更经济

详见 [[rag]]。

#### 5. MCP — 模型上下文协议

MCP 是连接 AI 与外部世界的统一标准协议，赋予大模型**自主性**——没有 MCP 的模型只是被动问答工具，有了 MCP 才成为主动智能体。

- 统一了 AI 与所有外部资源（工具、数据库、文件系统）的通信规则
- 优点：协议标准，支持的工具可直接对接，无需重复开发适配
- 缺点：需要双方都遵守，不响应则失效；往往绑定特定体系

详见 [[mcp]] 和 [[mcp-ecosystem]]。

### 工程层

#### 6. SDD — 规范驱动开发

SDD（Spec-Driven Development / Skill Definition Document）是标准化描述 Skill 的格式，为 Agent 开发迭代提供统一执行标准。

- 将模糊需求梳理为规整、稳定、可落地的工作依据
- 让不同开发者开发的 Skill 可以互相调用
- 最新多模态 SDD 支持图像、音频、视频的 Skill 描述

详见 [[spec-driven-development]] 和 [[openspec]]。

#### 7. Harness — 驾驭工程

Harness 工程是为 AI Agent 搭建适配运行环境的系统性工程体系，负责统筹、调度、监管 AI 的所有运行行为。

- 包含：上下文统筹、工具调用管理、运行沙箱、权限划分、测试核验、日志记录、内容审核、问题调整
- **谁驾驭谁**：用户驾驭 AI，Harness 就像人驾驭马的缰绳
- 个人开发者做的 Agent 通常没有 Harness，但企业级生产环境必不可少

详见 [[harness-engineering]]。

## 层级总结

```
资源层 (Token)      → 「燃料」与「内存」
能力层 (Skill/Prompt/RAG/MCP) → 场景技能、行为指令、外部知识、连接世界
工程层 (SDD/Harness)          → 开发规范、运行稳定可控
```

## 行业洞察

随着大模型基础能力趋于同质化（GPT-5 发布时行业反应异常平静），AI 竞争焦点已从模型本身转移到 **Harness 工程搭建质量、上下文管理能力、工具系统稳定性、任务工作流成熟度** 等工程化维度的比拼。「谁能把这组七巧板拼得更快、更稳、成本更低」是下半场的核心问题。

## 相关页面

- [[skills]] — Agent Skill 体系
- [[prompt-engineering]] — Prompt 工程技术
- [[rag]] — 检索增强生成
- [[mcp]] — Model Context Protocol
- [[spec-driven-development]] — 规范驱动开发
- [[harness-engineering]] — Harness 工程化
- [[context-engineering]] — 上下文工程
- [[agent-loop]] — Agent 执行循环
