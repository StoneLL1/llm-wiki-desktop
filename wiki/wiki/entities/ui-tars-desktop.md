---
title: UI-TARS Desktop
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, agent, multimodal]
sources:
  - raw/articles/2026-05-17-8-github-open-source-projects.md
---

# UI-TARS Desktop

## 概述

**UI-TARS Desktop** 是字节跳动开源的多模态 AI Agent 技术栈，本质上是 [[anthropic]] Computer Use 的开源替代方案。用户用自然语言描述任务，Agent 自动操作电脑完成。

## 核心循环

```
截屏 → 视觉语言模型理解屏幕 → 推理预测下一步动作 → 执行（点击/输入/滚动） → 再截屏 → 循环
```

## 两个产品

### UI-TARS Desktop
- 原生桌面应用
- 使用 NutJS 控制鼠标键盘
- 支持 macOS 和 Windows

### Agent TARS
- 通用框架，通过 CLI 和 Web UI 使用
- 支持任意多模态 LLM（不限于字节自己的模型）
- Claude、GPT、豆包都能接
- 支持 [[mcp]] 集成

## Relationships

- 属于 [[computer-use-agent]] 范畴的开源实现
- 与 [[mano-p]]（明略科技 CUA）同类
- 与 [[turix-cua]]（双模型 CUA）同类
- 通过 [[mcp]] 支持工具服务器挂载

## See Also

- [[computer-use-agent]] — Computer Use Agent 概念
- [[mano-p]] — 明略科技纯视觉 CUA
- [[turix-cua]] — 双模型架构 CUA
- [[mcp]] — 工具连接协议
