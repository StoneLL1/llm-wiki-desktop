---
title: Stata Skill (Claude Code + Stata 计量经济学指南)
created: 2026-05-23
updated: 2026-05-27
type: entity
tags:
  - tool
  - methodology
  - skill
  - tutorial
sources:
  - raw/articles/2026-04-18-claude-code-stata-econometrics-guide.md
  - raw/articles/2026-04-18-ai-agent-stata-guide.md
  - raw/articles/2026-04-18-ai-agents-stata-skills-econometrics.md
---

# Stata Skill (Claude Code + Stata 计量经济学指南)

## 概述

Stata Skill 是将 [[claude-code]] 接入 Stata 统计软件的完整配置指南和方法论，由 **Aniket Panjwani**（西北大学经济学博士、前 Zelle 机器学习工程师）通过 ai-mba.io 平台发布。指南标题为《Using Claude Code with Stata — an Economist's Guide》，帮助经济学家通过三步系统性配置让 AI 理解 Stata 的运行逻辑、掌握 do-file 的书写规范。

## 三步配置流程

### 第一步：让 Claude Code 找到 Stata
Stata 默认不在系统 PATH 中，需将可执行文件路径写入 shell 配置文件。Claude Code 可自动扫描常见安装路径（`/usr/local/stata`、`/Applications/Stata` 等）。Windows 用户建议使用 WSL。

### 第二步：打造 Stata 专属技能
通过 Compound Engineering 插件创建用户级 Stata 技能，内容涵盖：
- Stata 的批处理模式与交互模式
- do-file 的标准结构与最佳实践
- 常用命令的语法规范
- 本机 Stata 安装路径的引用

社区已有 **dylantmoore/stata-skill** 开箱即用的高质量方案，包含 37 个参考文档和 20 个常用社区包（reghdfe、csdid、rdrobust、ivreg2、estout、coefplot 等），采用渐进式披露架构控制 token 消耗。

### 第三步：优化 PDF 文档处理
安装 pandoc（PDF→Markdown）、pdfgrep（PDF 全文搜索）、pdfplumber（PDF 结构分析）三个工具，让 Claude Code 按需查阅 Stata 手册而非一次性加载全部 PDF。

## MCP 闭环工作流

通过 [[mcp]] 协议接入 Stata MCP 服务器，实现提问→执行→读取结果→迭代的完整闭环：

```bash
claude mcp add stata-mcp --env STATA_MCP_CWD=$(pwd) --scope project -- uvx stata-mcp
```

闭环示例：数据探索→描述性统计→回归分析→结果输出→LaTeX 编译，Claude Code 自主完成所有步骤。

## CLAUDE.md 经济学研究配置

指南提供了经济学研究场景下的 [[claude-md|CLAUDE.md]] 最佳实践，包括数据路径规范、编码规范（版本声明、reghdfe 替代 areg、estout 统一导出）、研究设计说明、工作流规范（脚本命名约定）。

## 渐进式过渡策略

- **阶段一**：Stata 做所有分析，Claude Code 写 do-file、调试错误
- **阶段二**：大规模文本处理、网络爬虫等用 Python，其余仍用 Stata
- **阶段三**：计算密集型任务迁移到 Python，Stata 保留用于快速探索

## 进阶 MCP 扩展

通过 MCP 可接入 Zotero/Mendeley（文献检索）、FRED/World Bank/IPUMS（数据获取）、Web 搜索（文献综述）、SQL 数据库等外部工具。

## 典型应用场景

1. **论文数据复现**：理解复现包结构，追踪依赖关系
2. **快速假设检验**：Local Projection、事件研究图等
3. **代码重构**：统一命名规范、生成 master.do
4. **结果解读**：将回归系数转化为论文语言


## Stata MCP 生态（2026 年主流实现）

基于 [[mcp]] 协议，多个开源项目将 AI Agent（Claude Code、Cursor 等）与 Stata 打通，实现「AI 写代码，Stata 直接跑，输出实时返回」的闭环。

| 工具 | 开发者 | 特色 | 适合人群 |
|------|--------|------|----------|
| hanlulong/stata-mcp | DeepEcon | VS Code/Cursor 完整扩展，最多 100 并行会话，WebView 实时图形 | 日常编辑器用户 |
| SepineTam/stata-mcp | 社区 | 内置安全守卫（拦截 `rm`/`erase`/`shell` 等危险命令）+ 因果推断工作流 + 自动日志 | 计量经济学家 |
| tmonk/mcp-stata | LSE 经济学家 | 轻量极简，图形直接嵌入聊天界面 | 快速执行党 |
| statamcp.com | 通用 | 不绑定 IDE，配置最简单 | 零基础入门 |

### 安装示例（SepineTam/stata-mcp）

```bash
# 安装
uvx stata-mcp --help  # 或 pip install stata-mcp

# macOS
claude mcp add stata-mcp -- uvx stata-mcp \
  --stata-path "/Applications/Stata/StataMP.app/Contents/MacOS/stata-mp"

# Windows
claude mcp add stata-mcp -- uvx stata-mcp ^
  --stata-path "C:\Program Files\Stata18\StataSE-64.exe"
```

### 安装示例（hanlulong/stata-mcp，VS Code/Cursor 扩展）

1. VS Code 扩展市场搜索并安装 `hanlulong.stata-mcp`
2. 设置 `stata-mcp.stataPath` 填入 Stata 路径
3. MCP 配置文件添加相应配置
4. `Ctrl+Shift+Enter` 运行选中代码

### 实战：Claude Code 完整 DID 分析

在 Claude Code 中用自然语言描述需求后，AI 自主完成全流程：数据探索 → `describe` + `summarize` → 安装 `reghdfe` + `ftools` → DID 估计（`reghdfe lnrev c.treat#c.post size, absorb(firmid year) cluster(firmid)`）→ 结果解读 → 平行趋势检验 + `coefplot` + 图形导出。

### 安全守卫（SepineTam 版）

自动拦截危险命令（`!rm -rf`、`erase`、`shell`），所有执行记录自动写入日志，方便复现和审计。适合服务器或共享环境。

### 实用技巧

1. 写 `CLAUDE.md` 文件：在项目根目录放数据路径、变量说明、分析目标
2. 开启输出精简模式（hanlulong 版）：大数据集只返回摘要，省 token
3. 让 AI 做识别诊断：回归后直接问模型潜在识别问题
4. 多会话并行（hanlulong 版）：同时跑多个稳健性检验规格
## 相关链接

- [[claude-code]] — 接入 Stata 的 AI 编程助手
- [[claude-md]] — 项目级配置最佳实践
- [[mcp]] — Stata MCP 服务器连接协议
- [[skills]] — Stata Skill 是 Agent Skills 的具体应用
- [[ai-research-workflow]] — AI 辅助学术研究工作流
