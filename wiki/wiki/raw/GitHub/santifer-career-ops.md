---
title: "Career-Ops"
url: "https://github.com/santifer/career-ops"
source: "GitHub"
fetched: 2026-05-18
stars: 45200
forks: 9500
language: "JavaScript/Go"
topics: [claude-code, job-search, ai, pdf-generation, playwright, dashboard, automation, ats]
---

# Career-Ops

> AI-powered job search system built on Claude Code. 14 skill modes, Go dashboard, PDF generation, batch processing.

**⭐ 45.2k stars | 🍴 9.5k forks | 📅 198 commits**

## 概述

Career-Ops 是一个基于 AI CLI 代理（Claude Code / OpenCode / Gemini CLI）的全自动求职系统。作者 Santiago 用它评估了 740+ 职位、生成了 100+ 定制简历，最终拿到 Head of Applied AI 岗位。

核心理念：公司用 AI 过滤候选人，候选人也应该用 AI 来筛选公司。

## 关键特性

- **A-F 评分系统**：10 个加权维度评估职位匹配度（结构化打分，非关键词匹配）
- **ATS 优化 PDF 生成**：Space Grotesk + DM Sans 字体设计，针对 ATS 系统关键词注入
- **45+ 公司门户预配置**：Anthropic、OpenAI、ElevenLabs、Retool、n8n 等，覆盖 Ashby/Greenhouse/Lever/Wellfound 等招聘平台
- **14 种 Skill 模式**：
  - `/career-ops` — 全自动管道（评估 + PDF + 追踪）
  - `/career-ops scan` — 扫描门户新职位
  - `/career-ops pdf` — 生成 ATS 简历
  - `/career-ops batch` — 批量并行评估
  - `/career-ops tracker` — 查看申请状态
  - `/career-ops apply` — AI 填写申请表
  - `/career-ops contacto` — LinkedIn outreach 消息
  - `/career-ops deep` — 深度公司调研
  - `/career-ops training` — 评估课程/证书
  - `/career-ops project` — 评估作品集项目
  - 等等
- **面试故事银行**：跨评估积累 STAR+Reflection 故事，5-10 个核心故事覆盖所有行为面试题
- **薪资谈判脚本**：框架、地理折扣反驳、竞品 offer 杠杆
- **Go 语言 TUI Dashboard**：Bubble Tea + Lipgloss 终端 UI，6 个筛选标签、4 种排序模式
- **Human-in-the-Loop**：AI 评估推荐，人做最终决定，系统不会自动提交申请
- **Pipeline 完整性**：自动合并、去重、状态标准化、健康检查

## 工作流程

```
粘贴职位 URL 或描述
        │
        ▼
┌──────────────────┐
│  原型检测         │  分类：LLMOps / Agentic / PM / SA / FDE / Transformation
└────────┬─────────┘
         │
┌────────▼─────────┐
│  A-F 评估        │  匹配度、差距分析、薪资调研、STAR 故事
│  (读取 cv.md)    │
└────────┬─────────┘
         │
    ┌────┼────┐
    ▼    ▼    ▼
 Report  PDF  Tracker
  .md   .pdf   .tsv
```

## 技术栈

- **Agent**: Claude Code / OpenCode / Gemini CLI（多 CLI 支持，统一 AGENTS.md 指令）
- **PDF**: Playwright + HTML 模板
- **Scanner**: Playwright + Greenhouse API + WebSearch
- **Dashboard**: Go + Bubble Tea + Lipgloss（Catppuccin Mocha 主题）
- **Data**: Markdown tables + YAML config + TSV batch files

## 快速开始

```bash
git clone https://github.com/santifer/career-ops.git
cd career-ops && npm install
npx playwright install chromium
npm run doctor
cp config/profile.example.yml config/profile.yml  # 编辑个人信息
cp templates/portals.example.yml portals.yml       # 自定义公司
# 创建 cv.md 放入简历
claude  # 启动 Claude Code
```

## 作者

Santiago — Head of Applied AI，前创始人（创办并出售了一家公司）。作品集：santifer.io

## 许可证

MIT License
