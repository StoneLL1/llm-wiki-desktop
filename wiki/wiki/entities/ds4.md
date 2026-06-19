---
title: ds4
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, model, open-source]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# ds4

## Overview

ds4 是 Redis 创造者 **antirez** 开发的本地推理引擎，专为 DeepSeek V4 Flash 设计。使用 C 语言编写，针对 Apple Metal 做了深度优化，可在 MacBook 上运行 284B 参数大模型。上线 4 天获得 7000+ Star。

## 核心特性

### KV 缓存磁盘持久化
将 KV 缓存视为磁盘的一等公民，利用现代 MacBook 的高速 SSD 将 KV 缓存持久化到磁盘，下次会话直接复用。对 [[claude-code]] 等编程 Agent 场景特别有效——Agent 反复发送长 prompt 时，无需每次重新 prefill，直接从磁盘恢复上下文。

### 2-bit 不对称量化
只对 MoE 路由专家做激进量化，共享专家和投影层保持不动。128GB 内存的 MacBook 也能跑起来，编码 Agent 场景下仍可靠调用工具。

### 性能数据
- **MacBook Pro M3 Max 128GB**（q2 量化）：长 prompt prefill 250 tokens/s，生成 21 tokens/s
- **Mac Studio M3 Ultra 512GB**：长 prompt prefill 468 tokens/s

### API 兼容
同时兼容 OpenAI 和 Anthropic 的 API 格式，[[claude-code]]、opencode 等编程 Agent 可直接对接。

## 相关链接

- [[deepseek]] — DeepSeek 公司和模型家族
- [[claude-code]] — 可直接使用 ds4 作为推理后端的编程 Agent
- [[openai-codex]] — 同样可对接 ds4 的编程 Agent
