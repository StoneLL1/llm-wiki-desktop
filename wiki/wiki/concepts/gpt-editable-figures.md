---
title: GPT 生成可编辑科研绘图
created: 2026-05-23
updated: 2026-05-23
type: concept
tags: [methodology, tool, cv, tutorial]
sources:
  - raw/articles/2026-04-18-xhs-gpt-editable-publication-figures.md
---

# GPT 生成可编辑科研绘图

## Definition

使用 GPT 等大模型生成**真正可编辑**的学术出版级图表的工作流方法。核心理念是利用 LLM 的代码生成能力，将图像转化为可编辑格式（如 PPT/SVG），而非生成静态图片。

来源为小红书用户李东东的教程帖（419 赞、607 收藏），标签：#chatgpt #科研日常 #科研绘图。

## 三步工作流

1. **生成图像**：按照你的思路让 GPT 生成初始图像
2. **转换为代码**：让 GPT 把图像转成 JS 代码，然后生成 PPT 文件
3. **手动调整**：在 PPT 中自行调整排版细节

## 与相关方法的对比

| 维度 | GPT+PPT 方法 | [[autofigure-edit]] | 传统绘图 |
|------|-------------|---------------------|---------|
| 输入 | 自然语言描述 | 科学文本 + 参考图 | 手动操作 |
| 输出格式 | PPT（可编辑） | SVG（可编辑） | 各异 |
| 自动化程度 | 半自动 | 全自动 | 手动 |
| 适用场景 | 快速原型 | 正式论文 | 精确控制 |

## 设计洞察

这种方法的关键洞察是：**不要求 AI 直接生成完美图像，而是利用 AI 生成「可编辑的中间产物」**。这与 [[skill-engineering]] 的理念一致——让 AI 做擅长的事（代码生成），人类做擅长的事（视觉调整）。

与 [[autofigure-edit]] 的"从像素到矢量"思路异曲同工，但实现路径不同：
- AutoFigure-Edit：科学文本 → SVG 矢量图（全自动五阶段流水线）
- GPT+PPT：自然语言 → JS 代码 → PPT（半自动三步工作流）

## Relationships

- 与 [[autofigure-edit]] 同属"AI 生成可编辑图表"方向
- 体现 [[skill-engineering]] 中"确定性事务交给工具"的理念
- 是 [[ai-research-workflow]] 中论文绘图环节的实践方法

## See Also

- [[autofigure-edit]] — 西湖大学张岳实验室的全自动论文绘图系统
- [[ai-research-workflow]] — AI 辅助科研的完整工作流
- [[skill-engineering]] — 让 AI 生成中间产物的设计哲学
