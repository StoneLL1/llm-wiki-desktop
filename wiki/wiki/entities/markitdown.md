---
title: MarkItDown
created: 2026-05-22
updated: 2026-05-24
type: entity
tags: [tool, data, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
---

# MarkItDown

## 概述

MarkItDown 是微软官方的 Python 工具，将各种格式文件转换为 Markdown。目前 11.1 万 Star，是 GitHub 上 Star 最多的文档处理工具之一。

## 核心特性

- **广泛格式支持**：PDF、Word、PPT、Excel、图片、音频、HTML，甚至 YouTube 链接
- **LLM 原生友好**：Markdown 是 LLM 利用率最高、结构保留最完整的格式
- **内置 LLM 集成**：图片可调 OpenAI 模型描述，音频可转录
- **Azure 文档智能对接**：企业级文档处理能力
- **插件扩展**：如 markitdown-ocr 等第三方插件
- **双模式使用**：命令行和 Python API

```bash
pip install 'markitdown[all]'
markitdown path-to-file.pdf -o document.md
```

## 在文档处理生态中的定位

MarkItDown 是 LLM 文档处理管道的关键前置工具。与 [[mineru]]（专注 PDF）不同，MarkItDown 覆盖几乎所有文件格式。它在 [[context-engineering]] 中扮演"格式标准化"角色——将非结构化文档统一转为 LLM 友好的 Markdown。

## 相关链接

- [[mineru]] — 专注 PDF 转 Markdown 的工具
- [[context-engineering]] — 上下文工程的系统方法论
- [[rag]] — 文档处理后用于检索增强生成
