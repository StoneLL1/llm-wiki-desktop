---
title: Gemma 4
created: 2026-05-17
updated: 2026-05-17
type: entity
tags:
  - model
  - multimodal
  - open-source
sources:
  - raw/articles/2026-05-02-claude-code-gemma4-mac-image-manager.md
---

# Gemma 4

## 概述

Gemma 4 是 Google 发布的开源多模态模型，属于 Gemma 系列的最新版本。作为一款本地可部署的多模态模型，Gemma 4 在 AI 编程工作流中扮演着重要的补充角色，特别是在图片语义分析和标签生成等视觉理解任务中。

## 核心特性

### 开源与本地部署

Gemma 4 作为开源模型，可以在本地机器上完全离线运行。搭配 Ollama 等推理框架，用户可以实现**零成本离线图片分析**，无需调用任何付费 API。这对于隐私敏感场景和成本控制都具有重要意义。

### 多模态图片理解

Gemma 4 的多模态能力超越了传统的 OCR（光学字符识别），实现了多维度图片理解：

- **内容识别**：识别图片中的物体、人物和场景
- **颜色分析**：提取图片的配色方案和视觉风格
- **文字提取**：识别图片中的文字内容
- **场景理解**：理解图片的整体语义和上下文

这种多维度理解使得 Gemma 4 可以生成高质量的语义标签，支持自然语言检索图片。

## 在 Vibe Coding 中的应用

Gemma 4 是本地 [[vibe-coding]] 的首选多模态模型。在 Vibe Coding 工作流中，开发者可以通过自然语言描述来管理和检索图片资源：

1. **语义标签生成**：Gemma 4 自动为图片生成多维度语义标签
2. **自然语言检索**：通过自然语言描述查找匹配的图片
3. **批量处理**：自动化处理大量图片的分类和标注

## 搭配 Ollama 的部署

通过 Ollama 部署 Gemma 4 非常简单：

```bash
# 拉取模型
ollama pull gemma4

# 运行多模态推理
ollama run gemma4 "描述这张图片的内容" --image photo.jpg
```

在 [[claude-code]] 的工作流中，可以通过 MCP 工具或 Shell 命令调用本地 Gemma 4，实现图片分析能力的无缝集成。

## 实践案例：Mac 图片管理器

一个典型的实践案例是使用 Gemma 4 + [[claude-code]] 构建本地图片管理器：

1. Gemma 4 对图片进行语义分析，生成多维度标签
2. 标签存储在本地数据库中
3. 用户通过自然语言查询图片
4. 系统返回语义匹配的图片结果

整个过程完全离线、零成本，展示了开源多模态模型在实际工作流中的价值。

## 端侧模型横向对比

[[minicpm5-1b|MiniCPM5-1B]]（1B 纯文本基座）和 [[kimi-k25|Kimi K2.5]]（视觉编码专精）代表了端侧模型的两种路线：Gemma 4 介于两者之间，提供本地多模态能力但侧重图片理解而非文本推理。

## 相关概念

- [[claude-code]] — Anthropic 的 CLI 编程 Agent，可与 Gemma 4 协同工作
- [[vibe-coding]] — Vibe Coding 方法论，Gemma 4 是其本地多模态首选模型
- [[minicpm5-1b]] — 面壁智能 1B 端侧文本基座，AA 榜单 2B 以下最强
