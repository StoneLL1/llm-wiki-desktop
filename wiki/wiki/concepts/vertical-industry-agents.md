---
title: 垂直行业 Agent
created: 2026-05-17
updated: 2026-05-17
type: concept
tags: [agent, methodology, enterprise]
sources:
  - raw/articles/2026-05-14-anthropic-financial-skills.md
  - raw/articles/2026-05-12-claude-code-feishu-agent-workflows.md
---

# 垂直行业 Agent

## 定义

垂直行业 Agent 是将 AI Agent 能力按具体行业岗位拆成**可调用、可审计、可替换**的工作单元的设计范式。与通用 Agent 不同，垂直行业 Agent 针对特定行业的业务流程、合规要求和专业知识进行深度定制，使其在特定领域内的表现远超通用方案。

相关：[[claude-code]]、[[anthropic]]、[[skills]]、[[mcp]]、[[feishu]]

## Anthropic 金融 Skills 案例

[[anthropic]] 发布的 claude-for-financial-services 是垂直行业 Agent 的标杆实践，包含 10 个预构建 Agent：

| Agent | 功能 | 应用场景 |
|-------|------|----------|
| **Pitch Agent** | 投资推介材料生成 | 创业融资、IPO 路演 |
| **Model Builder** | 财务模型构建 | 估值分析、DCF 建模 |
| **KYC Screener** | 客户尽职调查 | 合规审查、反洗钱 |
| **Research Agent** | 行业研究报告 | 投研分析、市场洞察 |
| **Compliance Agent** | 合规检查 | 监管报送、政策审查 |
| **Risk Agent** | 风险评估 | 信用风险、市场风险 |
| **Client Comm Agent** | 客户沟通 | 财富管理、投后服务 |
| **Data Agent** | 数据分析 | 量化研究、绩效归因 |
| **Report Agent** | 报告生成 | 定期报告、监管披露 |
| **Workflow Agent** | 流程编排 | 跨部门协作、审批流 |

这套金融 Skills 体现了"行业知识 + Agent 能力"的融合——不仅理解金融术语和流程，还能直接产出符合行业标准的交付物。

## Agent-Skill-SubAgent-MCP 层次架构

垂直行业 Agent 采用清晰的分层架构：

```
┌─────────────────────────┐
│   Agent Layer           │  ← 行业 Agent（如 Pitch Agent）
├─────────────────────────┤
│   Skill Layer           │  ← 技能定义（SKILL.md + YAML）
├─────────────────────────┤
│   SubAgent Layer        │  ← 子任务委托（多 Agent 协作）
├─────────────────────────┤
│   MCP Layer             │  ← 数据源与工具连接
└─────────────────────────┘
```

- **Agent Layer**：面向业务用户的入口，每个 Agent 对应一个行业岗位或职能
- **Skill Layer**：模块化的能力定义，使用 Markdown + YAML 标准化描述
- **SubAgent Layer**：复杂任务的拆分与委托，支持多 Agent 并行协作
- **MCP Layer**：通过 [[mcp]] 连接行业数据库、API、文档系统等外部资源

## 企业级部署模式

垂直行业 Agent 支持三种递进的部署模式：

### 1. Markdown + YAML 定义
- 使用标准化的 Markdown 文件描述 Agent 的能力、输入输出和约束
- YAML 配置定义参数、数据源映射和权限控制
- 版本化管理，支持代码审查和变更追踪

### 2. [[claude-code]] 插件一键安装
- 将 Agent 定义打包为 Claude Code 插件
- 企业内部可以通过私有插件市场分发
- 支持权限控制和用量监控

### 3. Managed Agents API 无头部署
- 通过 [[anthropic]] 的 Managed Agents API 实现无头（headless）部署
- Agent 作为后端服务运行，无需人工交互
- 支持定时触发、事件驱动和 API 调用

## 迁移复制模式

金融行业的垂直 Agent 方案可以迁移复制到其他行业：

| 目标行业 | 核心岗位 Agent | 迁移要点 |
|----------|---------------|----------|
| **医疗** | 诊断助手、病历生成、药物审查 | 合规要求更高，需 HIPAA 适配 |
| **政务** | 公文处理、政策分析、信访处理 | 安全等级要求高，数据隔离严格 |
| **教育** | 课程设计、作业批改、学情分析 | 需适配教育评估标准和课程体系 |
| **跨境电商** | 选品分析、Listing 优化、客服自动化 | 多语言支持，平台规则适配 |

迁移的关键在于：
1. 替换行业知识库和术语体系
2. 调整合规和审计规则
3. 接入行业专属数据源（通过 [[mcp]]）
4. 定制输出格式和审批流程

## 飞书 Agent 工作流

[[feishu]] 平台上的 Agent 工作流展示了垂直行业 Agent 在办公场景的 5 个核心玩法：

### 1. 知识管理
- 自动归档会议纪要和决策记录
- 从对话中提取行动项和待办事项
- 构建团队知识图谱

### 2. 数据分析
- 自然语言查询业务数据
- 自动生成数据报告和可视化图表
- 异常数据预警

### 3. 文档协同
- 多人协作文档的智能建议
- 文档版本对比和变更摘要
- 自动生成文档模板

### 4. 流程自动化
- 审批流程的智能路由
- 跨系统数据同步
- 定时报告推送

### 5. 客户服务
- 智能客服对话
- 工单自动分类和派发
- 客户反馈分析

## 开放问题

- 垂直行业 Agent 的标准化程度与定制化的平衡点在哪里？
- 如何建立行业 Agent 的质量评估标准？
- 跨行业迁移中的数据隐私和合规挑战如何系统化解决？
- Managed Agents API 的 SLA 保障和容错机制
- 行业 Agent 的持续学习和知识更新机制
