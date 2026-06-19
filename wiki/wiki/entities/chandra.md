---
title: Chandra OCR
created: 2026-05-24
updated: 2026-05-24
type: entity
tags: [tool, open-source, data, cv]
sources: [raw/articles/2026-04-21-chandra-ocr-handwriting-recognition.md]
---

# Chandra OCR

## Overview

Chandra 是一个开源 OCR 系统，由 **datalab-to** 团队开发，专注于高精度文档版面理解和结构化识别。其最强能力在于**手写体识别**，尤其擅长手写文档的数字化处理。相比 [[mineru]] 的文档转换能力和其他 OCR 工具，Chandra 在手写体、复杂版面和老旧文档方面有显著优势。

GitHub: [datalab-to/chandra](https://github.com/datalab-to/chandra)

## Core Capabilities

- **手写体识别**：核心优势，能准确识别连笔潦草的手写文字（包括医生处方等极端样本）
- **40+ 种语言支持**：覆盖主流语言，满足多语言 OCR 需求
- **复杂版面布局**：精准识别多栏布局、报纸排版、表格结构
- **表格与表单**：准确重建复杂表格、表单、复选框
- **数学公式识别**：支持手写和印刷数学公式自动识别
- **档案数字化**：最适合的 use case —— 将封存多年的老文档数字化

## Comparison with dots.ocr

| 维度 | Chandra | dots.ocr |
|------|---------|----------|
| 手写体识别 | ⭐⭐⭐ 极强 | ⭐ 一般 |
| 表格识别 | 强 | 强 |
| 数学符号 | 强 | 强 |
| 版面布局 | 强 | 强 |
| 页面布局识别 | 强 | 更突出 |
| 定位 | 档案数字化首选 | 表格/符号首选 |

> 两者各有优势，实际项目建议都测试对比后再选型。

## Use Cases

1. **档案数字化**：封存多年的老文档、手写信件数字化
2. **手写表格提取**：表单数据自动化录入
3. **报纸扫描件处理**：艺术字体、多栏复杂布局
4. **数学作业/试卷**：手写数学公式自动识别

## Technical Notes

- 2000+ GitHub Stars（在 OCR 开源项目中属于较高水平）
- 官方 benchmark 数据优于 dots.ocr（但为自测数据，仅供参考）
- 建议重要文档数字化时仍需人工复核

## Relationships

- 竞品：dots.ocr（表格/符号见长，手写体弱）
- 与 [[mineru]] 互补：MinerU 处理 PDF→Markdown 转换，Chandra 擅长手写体识别
- 可配合 [[markitdown]] 构建文档处理管道
- 属于 [[ai-research-workflow]] 中的文档预处理环节

## See Also

- [[mineru]] — PDF 转 Markdown 工具
- [[markitdown]] — 微软万物转 Markdown
- [[ai-research-workflow]] — AI 研究工作流
