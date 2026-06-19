---
title: AI Research Workflow
created: 2026-04-23
updated: 2026-05-23
type: concept
tags: [data, rag, tutorial, methodology, agent]
sources:
  - raw/articles/karpathy-knowledge-compilation.md
  - raw/articles/claude-research-10x-better.md
  - raw/articles/overleaf-claude-code-latex-paper.md
  - raw/articles/deep-research-prompt.md
  - raw/articles/five-skills-paper-writing.md
  - raw/articles/academic-paper-auto-writing-skill.md
  - raw/articles/aris-auto-experiment-paper.md
  - raw/articles/xhs-scientific-research-skills-open-source.md
  - raw/articles/2026-04-18-academic-paper-auto-writing-skill.md
  - raw/articles/2026-04-18-five-skills-paper-writing.md
  - raw/articles/2026-04-18-overleaf-claude-code-latex-paper.md
  - raw/articles/2026-04-18-aris-auto-experiment-paper.md
  - raw/articles/2026-04-18-westlake-university-ai-paper-drawing.md
---

# AI Research Workflow

## Definition

AI Research Workflow refers to end-to-end automation of academic research processes using AI agents. This encompasses literature survey, experimental design, data analysis, paper writing, and knowledge compilation — transforming what traditionally requires months of human effort into workflows that can be completed in days or even hours.

## Three Key Workflow Patterns

### 1. Auto Research Pipeline

The end-to-end pipeline: **Idea → Experiment → Record → Paper**

Popularized by Andrej Karpathy's autoresearch concept, this pattern uses AI agents to automate the full research cycle. Karpathy's [[claude-md]] file for research coding encodes insights about how LLMs can effectively conduct research when given proper context and structured workflows.

### 2. Deep Research Methodology

Comprehensive AI-driven investigation that goes beyond surface-level analysis. The Deep Research approach uses structured prompts and multi-step reasoning to produce thorough, well-sourced investigations. Key techniques:

- **Decomposition**: breaking complex questions into sub-questions
- **Multi-source synthesis**: gathering information from diverse sources
- **Iterative deepening**: starting broad and progressively focusing
- **Verification**: cross-checking claims across sources

### 3. Pipeline-Driven Research

Organized as sequential stages with handoff between AI agents:

```
Literature Survey → Data Preparation → Algorithm Development → Paper Writing
```

Each stage can use different AI agents or models, with structured outputs feeding into the next stage.

## Literature Search and Reading

### Multi-Engine Adaptive Retrieval

The scientific-research-skills open-source methodology by 流风回雪 (Richard) implements adaptive literature search across multiple engines:

- Semantic Scholar
- arXiv
- Google Scholar
- PubMed (for biomedical research)
- DBLP (for computer science)

The system adaptively selects search engines based on query type and dynamically adjusts search strategies based on result quality.

### Three-Level Reading Method

1. **Skim**: Read title, abstract, introduction, conclusion (5 minutes per paper)
2. **Standard read**: Full read of main sections, skip proofs (30 minutes)
3. **Deep analysis**: Detailed study including methodology, equations, reproducibility (2-4 hours)

AI agents can perform level 1 and 2 reading at scale, flagging papers that merit human deep analysis.

### Related Work Survey Methodology

Systematic survey approach:
1. Define survey dimensions and scope
2. Multi-axis search across dimensions
3. Build taxonomy of the field
4. Identify gaps and opportunities

## Paper Writing

### Five Skills for a Paper

The five-skills-paper-writing framework breaks academic paper creation into:

1. **Idea formulation** — research question, hypothesis, novelty claim
2. **Literature positioning** — related work, gap identification
3. **Methodology design** — approach, experiments, baselines
4. **Result presentation** — figures, tables, statistical analysis
5. **Writing craft** — structure, clarity, academic style

### Academic Paper Auto-Writing

The academic-paper-auto-writing-skill automates paper drafting through structured templates and AI generation. Key capabilities include:

- Abstract generation from experimental results
- Related work section synthesis from literature notes
- Methodology description from code documentation
- Result interpretation from statistical outputs
- LaTeX formatting and compilation

### Overleaf + Claude Code + GitHub Pipeline

An integrated workflow for paper writing:

1. **GitHub** — version control for LaTeX source, figures, and data
2. **Claude Code** — AI agent for writing, editing, and debugging LaTeX
3. **Overleaf** — collaborative online LaTeX editor for real-time co-authoring

Claude Code can read and modify LaTeX files directly, generate BibTeX entries, compile documents, and debug compilation errors — all from the command line.

## Automated Experiments

The **Aris** framework automates the experiment cycle:

1. Define experimental parameters
2. Run experiments automatically
3. Collect and organize results
4. Generate figures and tables
5. Write experiment sections

This connects to the broader Auto Research pipeline by producing the experimental record that feeds into paper writing.

## Knowledge Compilation

[[knowledge-compilation]] is the practice of using LLMs to compile raw research outputs (papers, notes, experimental logs) into structured, linked knowledge bases. The AutoWiki skill implements this for academic paper management, organizing papers by conceptual breakthroughs rather than folders, and auto-constructing temporal graphs showing paper relationships.

## 论文绘图自动化

[[autofigure-edit]]（西湖大学张岳实验室，ICLR 2026）实现了从长篇科学文本自动生成出版级学术插图，并支持 SVG 矢量编辑。五阶段流水线（风格条件生图 → 分割与结构索引 → 资产提取 → SVG 模板生成 → 资产注入）将"文本→可编辑 SVG"自动化。

在 [[aris]] 的工作流 3 中，`/paper-figure` Skill 负责自动生成数据驱动的图表（训练曲线、柱状图、对比表），但约 40% 的图表（架构图、流程图）仍需手动制作。

## 学术论文五 Skill 组合

五个核心 Skill 覆盖"题目—正文—交付"三节点：

1. **research-proposal**（luwill）— Nature Reviews 风格开题模板
2. **strategist**（lishix520）— 7 个评审节点 + 证据缺口标注
3. **composer + scientific-writer**（K-Dense-AI）— 骨架生成 + 学术润色
4. **statistical-analysis**（K-Dense-AI）— pandas + statsmodels 工作流
5. **latex-document**（ndpvt-web）+ 官方 docx/pptx/pdf — 排版交付

## 完整性验证

[[academic-paper-integrity]] 是 AI 辅助论文写作中不可跳过的质量门。包括引用核查（作者、标题、DOI 是否真实存在）、数据核查（统计量是否一致）和论断核查（claim 是否有证据支撑）。[[academic-research-skills]] 将其制度化为 Stage 2.5 和 Stage 4.5 的强制步骤。

## Tools

| Tool | Purpose |
|------|---------|
| Claude Code | Writing, editing, debugging LaTeX; running experiments |
| Overleaf | Collaborative LaTeX editing |
| GitHub | Version control |
| Aris | Automated experiment execution |
| academic-research-skills | 12-agent paper writing workflow |
| AutoFigure-Edit | AI 论文绘图 + SVG 编辑 |
| scientific-research-skills | Open-source research methodology |
| AutoWiki | Knowledge base compilation |
| MinERU | PDF-to-markdown for paper processing |
| Claude-mem | Session memory for cross-session research continuity |

## Open Questions

- Can AI agents conduct truly novel research, or only synthesize existing knowledge?
- How do you validate AI-generated research claims?
- What is the role of human researchers in AI-augmented research workflows?
- How do AI research workflows change peer review and academic publishing?
- Can automated experiments replace human intuition in experimental design?
- 当论文生产成本降到 $15，学术共同体如何应对"论文洪水"？
- AI 拥有写代码+执行代码的权限时，该给多少自由度？

## See Also

- [[claude-code]] — primary tool for research automation
- [[knowledge-compilation]] — compiling raw research into structured knowledge bases
- [[claude-md]] — project configuration for research workflows
- [[context-engineering]] — managing research context across long sessions
- [[multi-agent-collaboration]] — multiple agents handling different research stages
- [[academic-research-skills]] — 12-agent 学术论文写作套件
- [[aris]] — Auto-Research-In-Sleep 全自动科研 Skill
- [[autofigure-edit]] — AI 论文绘图工具
- [[overleaf]] — 在线 LaTeX 协作编辑器
- [[academic-paper-integrity]] — 学术论文完整性验证
- [[scientific-research-skills]] — 科学研究 Skills 开源合集
