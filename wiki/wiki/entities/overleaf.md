---
title: Overleaf
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, collaboration, open-source]
sources:
  - raw/articles/2026-04-18-overleaf-claude-code-latex-paper.md
  - raw/articles/2026-04-18-academic-paper-auto-writing-skill.md
---

# Overleaf

## 概述

Overleaf 是主流的在线 LaTeX 协作编辑平台，广泛用于学术论文写作。在 AI 辅助研究工作流中，Overleaf 扮演着**实时协作编辑器**和**GitHub 同步枢纽**的角色。

## Overleaf + Claude Code + GitHub 工作流

小红书用户 Lxxzzz_ 分享的集成工作流（1842 赞、3017 收藏）：

### 基础版流程

1. **Overleaf 建模板** → 同步到 GitHub（需 Overleaf 会员）
2. **本地 clone** → VSCode + 本地 LaTeX 环境
3. **Claude Code / Codex 辅助写作** → git push 同步到 GitHub → Overleaf 自动更新

### 优势

- 🤝 多人协作 + AI 协作：导师、合作者、AI 围绕同一个仓库
- 🧾 修改记录清晰：GitHub 版本管理
- 🛟 防止 Overleaf 意外翻车：GitHub 备份

### 实用 Tips

- **Writing from 文档**：用实验记录、周报、思路草稿喂给 AI，质量更高
- **俄罗斯套娃式写作**：先让 AI 帮你写文档，再从文档写论文

## PaperDebugger 路线

PaperDebugger（NUS, Robert Youssef）代表另一条路线：**把多智能体嵌入 Overleaf 写作环境**，直接在 LaTeX 编辑器里做 agentic editing，并行跑 Reviewer / Enhancer / Scoring / Researcher。

## 在 AI 研究生态中的定位

Overleaf 是 [[ai-research-workflow]] 中论文写作阶段的核心协作工具，与 [[claude-code]] 形成互补：Claude Code 在本地做 AI 辅助编辑，Overleaf 处理实时协作和格式预览。

## 相关链接

- [[claude-code]] — 本地 AI 辅助写作的核心工具
- [[ai-research-workflow]] — AI 研究工作流系统方法论
- [[academic-research-skills]] — 包含 LaTeX 排版交付的学术研究 Skill 套件
- [[aris]] — 全自动科研 Skill，论文写作阶段使用本地 LaTeX
