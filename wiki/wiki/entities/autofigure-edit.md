---
title: AutoFigure-Edit
created: 2026-05-23
updated: 2026-05-27
type: entity
tags: [tool, open-source, design, cv, presentation]
sources:
  - raw/articles/2026-04-18-westlake-university-ai-paper-drawing.md
---

# AutoFigure-Edit

## 概述

**AutoFigure-Edit** 是西湖大学张岳实验室推出的 AI 论文绘图系统，在 AutoFigure（ICLR 2026）基础上新增 SVG 矢量编辑能力。作为首个能从长篇科学文本自动生成出版级学术插图的智能体框架，它解决了"生成不可编辑"的痛点。

- GitHub: https://github.com/ResearAI/AutoFigure-Edit
- AutoFigure GitHub: https://github.com/ResearAI/AutoFigure
- 论文: https://arxiv.org/abs/2603.06674
- AutoFigure 原始论文: https://arxiv.org/abs/2602.03828v1
- 在线体验: https://deepscientist.cc
- HuggingFace Daily Paper: https://huggingface.co/papers/2603.06674
- Star: 1.6k+

## 核心突破

### 从像素到矢量

生成的不再是静态 PNG，而是完全可编辑的 SVG 文件——每个模块、连线、文字标注都是独立对象，可在浏览器内拖拽、改字、换色。

### 内置交互式编辑器

生成完成后，立即进入可视化编辑画布。调整布局、修改标注、替换图标，所见即所得。支持本地部署和在线网站两种画布。

### 五阶段流水线

1. **风格条件生图**：使用文本和参考图生成初始栅格图像
2. **分割与结构索引**：利用 [[chandra|SAM3]] 识别视觉组件并构建结构骨架
3. **资产提取**：提取透明背景的视觉资产（RGBA）
4. **SVG 模板生成与精炼**：生成结构化的 SVG 布局模板
5. **资产注入**：将视觉资产注入模板，形成完全可编辑的 SVG

### AutoFigure 的"推理式渲染"范式

将"逻辑布局"和"美学渲染"彻底解耦：

- **Stage I — 概念锚定（Conceptual Grounding）**：AI 读入长篇方法描述（平均超过 10,000 tokens），提取核心实体和关系
- **Stage II — 评审-精炼闭环（Critique-and-Refine）**：AI Designer 负责修改布局，AI Critic 挑毛病（"箭头重叠""布局重心不稳""文字层级不清晰"），多轮迭代
- **Stage III — 美学渲染与"擦除-修正"**：OCR 识别模糊文字 → 抠掉 → 矢量文字重新覆盖，解决 AIGC 生图文字变形问题

### 参考图风格控制

不再依赖模糊的 Prompt 描述。上传一张风格参考图，系统自动学习配色方案、字体风格、图标类型、间距密度等。这使得实验室风格的统一、期刊风格的适配（Nature、ICLR 等）变得前所未有的简单。

## 实验结果

### FigureBench 基准测试

对比基线方法（包括 GPT-Image、SVG-Code、Diagram Agent 等），AutoFigure-Edit 在视觉设计、沟通效果、内容保真度三大维度均显著领先：

- 使用参考图后 **Win-Rate 从 76.0% 提升到 83.0%**
- 内容保真度全面提升：准确性 8.83、完整性 8.26、适应性 8.37
- 无参考图模式综合评分 8.29（美学 8.32、表达力 8.66）

### 用户研究（217 位用户，262 个插图）

**PNG 生成质量**：
- 科学语义正确性：4.04/5.0（48% 给满分）
- 信息完整性：4.11/5.0（51% 给满分）
- 视觉呈现质量：3.95/5.0
- 风格一致性：4.09/5.0（50% 给满分）

**实际可用性**：
- 126/262（48%）用户认为生成结果可直接用于论文发表，无需修改
- 系统已具备真实科研工作流的可用性

**SVG 转换质量**：
- 转换正确性：3.60/5.0
- 36% 的用户给满分，SVG 结构保持了高度准确性
- 低评分（1-2 分）在语义维度上非常罕见（通常低于 12%）

### 风格迁移案例

同一论文内容可在多种参考风格下生成不同视觉效果的插图，已展示 CycleResearcher、DeepReviewer、DeepScientist 等多种风格。

## 应用场景

- **赋能 AI 科学家**：打通科研可视化的"最后一公里"，AI 全流程自主研究
- **降低科研创作门槛**：一键生成算法流程图、系统架构图、教科书示意图
- **统一视觉风格**：参考图风格控制让实验室论文插图风格统一，期刊风格快速适配

## 团队

西湖大学自然语言处理实验室（张岳教授），成立于 2018 年 9 月。张岳教授毕业于牛津大学，获博士学位，现任西湖大学工程学院副院长，曾担任 EMNLP 2022 等多个顶级 NLP 会议的程序委员会主席。

## 开源资源

- 代码完全开源（GitHub 仓库包含完整代码库）
- FigureBench 数据集已在 HuggingFace 发布
- 提供在线网站一键使用 Web 界面
- 内置交互式编辑画布，支持实时调整

## Relationships

- [[ai-research-workflow]] — AI 研究工作流，AutoFigure 是论文写作环节的绘图工具
- [[skill-engineering]] — AutoFigure 体现了推理式渲染的工程设计理念
- [[manim]] — 另一种科学可视化工具，侧重动画而非静态插图
- [[gpt-editable-figures]] — 使用 GPT 生成可编辑科研绘图的方法，与 AutoFigure-Edit 异曲同工
- [[excalidraw-diagram-skill|Excalidraw 图表 Skill]] — 手绘风格图表生成
- [[ppt-master]] — 另一种 AI 生成可编辑文档的工具（PPTX）
