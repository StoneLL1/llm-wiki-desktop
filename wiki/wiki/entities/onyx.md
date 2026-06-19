---
title: Onyx
created: 2026-05-22
updated: 2026-05-27
type: entity
tags: [tool, rag, enterprise, open-source]
sources:
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
  - raw/articles/2026-04-18-onyx-self-hosted-chatgpt-guide.md
---

# Onyx

## 概述

Onyx（原名 Danswer）是开源的企业 AI 搜索平台 + 知识库管理 + 企业 ChatGPT 三合一平台。最早是 YC W24 批次项目，2025 年获得 Khosla Ventures 和 First Round Capital 联合领投的 1000 万美元种子轮。Netflix、Ramp 等公司在使用。目前 2.3 万 Star。

如果说 ChatGPT 是一个通用的超级大脑，Onyx 的目标就是把这个大脑装进公司的身体里，让它读懂所有的内部文档。

## 核心功能

### 1. 智能聊天

- **文件/URL 对话**：支持上传 PDF、DOCX、TXT 等文档，直接粘贴 URL 自动抓取内容
- **4 大动作选择器**：内部搜索（企业知识库）、网络搜索（实时互联网）、代码执行（Python 沙箱）、图像生成（AI 生图）
- **Deep Research**：多轮思考+研究+行动，适合复杂问题整合多来源（成本为普通推理 10 倍+）
- **模型选择器**：支持所有主流 LLM 提供商及自部署模型（Ollama、VLLM 等）

### 2. 自定义 Agent

Agent = 指令 + 知识 + 动作，可视为针对特定任务优化的 AI 团队成员：
- **指令（Instructions）**：自定义 System Prompt，支持变量如 `CURRENT_DATETIME`
- **知识（Knowledge）**：来自 Connector 自动同步或文件上传，最佳实践是范围越窄性能越可靠
- **动作（Actions）**：通过 API 与外部应用交互（更新工单、查询 CRM 等）

### 3. 内部搜索

- **混合搜索**：语义搜索 + 关键词搜索
- **上下文检索**：智能理解查询意图
- **AI 生成知识图谱**：发现知识关联
- **高级 RAG**：减少幻觉，提高准确性

### 4. MCP 支持

支持 [[mcp]]（Model Context Protocol）标准，允许 Agent 通过 OpenAPI 和 MCP 配置更多外部动作，连接企业内部系统、调用 REST/GraphQL API。

## 技术架构

| 层级 | 技术 |
|------|------|
| 前端 | Next.js、React、TypeScript |
| 后端 | Python、FastAPI |
| 数据库 | PostgreSQL |
| 向量库 | Qdrant / Weaviate / Milvus |
| 搜索引擎 | Vespa / Elasticsearch |
| 部署 | Docker、Docker Compose、Kubernetes |

## 部署模式

| 模式 | CPU | 内存 | 存储 | 适用场景 |
|------|-----|------|------|----------|
| Lite | 2 核 | 4GB | 20GB | 个人/小团队 |
| Standard | 4 核 | 8GB | 50GB | 生产环境 |

## 已支持的 Connector

Google Drive、Confluence、Slack、GitHub、GitLab、Jira、SharePoint、Notion、Salesforce、自定义 API、数据库、文件服务器。

## 安全与合规

- AES-256 数据加密存储
- RBAC 基于角色的访问控制
- 完整审计日志
- SSO（SAML、OIDC）
- VPC 部署支持
- SOC 2 Type II / GDPR / HIPAA / ISO 27001

## 在 RAG 生态中的定位

Onyx 是 [[rag]] 技术在企业场景的典型落地。与通用 RAG 框架不同，Onyx 专注于企业数据源的深度集成和权限管理，让 [[context-engineering]] 在组织级知识管理中发挥价值。

## 竞品对比

| 特性 | Onyx | ChatGPT Enterprise | Microsoft Copilot | Glean |
|------|------|--------------------|--------------------|-------|
| 自部署 | ✅ | ❌ | ❌ | ❌ |
| 开源 | ✅ | ❌ | ❌ | ❌ |
| LLM 选择 | ✅ 任意 | ⚠️ 仅 OpenAI | ⚠️ 仅 Azure | ⚠️ 有限 |
| 数据隐私 | ✅ 完全可控 | ⚠️ 依赖厂商 | ⚠️ 依赖厂商 | ⚠️ 依赖厂商 |
| 成本 | 免费自托管 | 💰💰💰 | 💰💰💰 | 💰💰💰 |

## See Also

- [[rag]] — 检索增强生成的技术架构
- [[khoj]] — 开源的个人 AI 知识库
- [[context-engineering]] — 上下文工程的系统方法论
- [[mcp]] — Model Context Protocol
