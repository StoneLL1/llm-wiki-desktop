---
title: "Career-Ops"
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, agent, engineering, automation]
sources:
  - raw/GitHub/santifer-career-ops.md
---

# Career-Ops

**Career-Ops**（santifer/career-ops，45.2k ⭐，9.5k 🍴）是基于 AI CLI 代理的全自动求职系统。作者 Santiago 用它评估了 740+ 职位、生成 100+ 定制简历，最终拿到 Head of Applied AI 岗位。

> 核心理念：公司用 AI 过滤候选人，候选人也应该用 AI 来筛选公司。

## 关键特性

### A-F 评分系统
- 10 个加权维度评估职位匹配度（结构化打分，非关键词匹配）
- 包含差距分析、薪资调研、STAR 故事匹配

### ATS 优化 PDF 生成
- Space Grotesk + DM Sans 字体设计
- 针对 ATS（Applicant Tracking System）系统关键词注入
- 基于 Playwright + HTML 模板渲染

### 14 种 Skill 模式

| 命令 | 功能 |
|------|------|
| `/career-ops` | 全自动管道（评估 + PDF + 追踪） |
| `/career-ops scan` | 扫描门户新职位 |
| `/career-ops pdf` | 生成 ATS 简历 |
| `/career-ops batch` | 批量并行评估 |
| `/career-ops tracker` | 查看申请状态 |
| `/career-ops apply` | AI 填写申请表 |
| `/career-ops contacto` | LinkedIn outreach 消息 |
| `/career-ops deep` | 深度公司调研 |
| `/career-ops training` | 评估课程/证书 |
| `/career-ops project` | 评估作品集项目 |

### 其他亮点
- **45+ 公司门户预配置**：Anthropic、OpenAI、ElevenLabs、Retool、n8n 等
- **面试故事银行**：STAR+Reflection 故事，5-10 个核心故事覆盖所有行为面试题
- **薪资谈判脚本**：框架、地理折扣反驳、竞品 offer 杠杆
- **Human-in-the-Loop**：AI 评估推荐，人做最终决定，不自动提交申请

## 工作流程

```
粘贴职位 URL 或描述
        │
        ▼
  原型检测（LLMOps / Agentic / PM / SA / FDE / Transformation）
        │
        ▼
  A-F 评估（读取 cv.md）→ 匹配度、差距分析、薪资调研、STAR 故事
        │
    ┌───┼───┐
    ▼   ▼   ▼
 Report PDF Tracker
  .md  .pdf  .tsv
```

## 技术栈

| 组件 | 技术 |
|------|------|
| AI Agent | [[claude-code|Claude Code]] / OpenCode / Gemini CLI（多 CLI 支持） |
| PDF 生成 | Playwright + HTML 模板 |
| 职位扫描 | Playwright + Greenhouse API + WebSearch |
| Dashboard | Go + Bubble Tea + Lipgloss（Catppuccin Mocha 主题） |
| 数据存储 | Markdown tables + YAML config + TSV batch files |

## Go 语言 TUI Dashboard

- Bubble Tea + Lipgloss 终端 UI
- 6 个筛选标签、4 种排序模式
- 实时查看申请状态和评估结果

## 在 Agent 生态中的定位

Career-Ops 是 [[skills|Skills 体系]]在垂直领域（求职）的极致应用。它展示了 Skill 模式的威力：通过 14 个专用 Skill 模式覆盖求职全流程，将复杂的、需要领域知识的任务拆解为可复用的 Agent 能力单元。

与 [[pinme|PinMe]]（部署 Skill）和 [[md2pdf-skill|MD2PDF Skill]]（PDF 生成 Skill）类似，Career-Ops 证明了 Skill 工程化不仅适用于编程，还可以延伸到任何需要 AI 辅助的复杂工作流。

## 作者

**Santiago** — Head of Applied AI，前创始人（创办并出售了一家公司）。作品集：santifer.io

## 许可证

MIT License
