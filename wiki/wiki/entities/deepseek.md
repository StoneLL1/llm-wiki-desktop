---
title: DeepSeek
created: 2026-05-21
updated: 2026-05-27
type: entity
tags: [model, company, open-source]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# DeepSeek

## 概述

DeepSeek（深度求索）是一家中国 AI 公司，提供高性能的 LLM 模型和 API 服务。在 AI Agent 构建领域，DeepSeek 模型因其高性价比和强推理能力，被广泛用作 Agent 的后端推理引擎。

## 在 Agent 开发中的应用

DeepSeek 的 API 可以作为 Agent 框架的核心推理引擎：

- **API 兼容性**：兼容 OpenAI API 格式，可无缝替换
- **高性价比**：相比同级别模型，推理成本显著更低
- **推理能力**：DeepSeek-R1 系列模型在复杂推理任务上表现突出

### 极简 Agent 框架集成

在构建 AI Agent 极简框架时，DeepSeek 的集成仅需配置 API Key 和 endpoint：

```python
client = OpenAI(
    api_key=os.environ.get("DEEPSEEK_API_KEY"),
    base_url="https://api.deepseek.com"
)
```

## 与其他 LLM 提供商的关系

DeepSeek 是 [[agent-loop]] 实现中的可选推理引擎之一，与 [[anthropic]]（Claude）、OpenAI（GPT）形成竞争。

## 小模型密度竞赛

随着「密度定律」的推进（大模型智能密度约每 3.5 个月翻一番），端侧小模型正在快速发展。[[minicpm5-1b|MiniCPM5-1B]]（1B 参数，AA 榜单 17.9 分）和 [[kimi-k25|Kimi K2.5]] 等端侧模型验证了小模型独立驱动真实应用的可行性。DeepSeek 在这一趋势中同样追求模型效率优化。

## 相关链接

- [[agent-loop]] — 使用 DeepSeek 作为推理引擎的 Agent 循环
- [[hermes-agent]] — 支持多种 LLM 后端的 Agent 框架
- [[openclaw]] — 可配置不同 LLM 的多 Agent 平台
- [[minicpm5-1b]] — 面壁智能端侧基座模型，密度定律的验证者
- [[kronos|Kronos 金融基础模型]]
- [[timesfm|TimesFM 时序预测]]

## ds4：Mac 本地推理引擎

Redis 创造者 antirez 开发的 ds4 是专为 DeepSeek V4 Flash 打造的本地推理引擎（C 语言，Metal 优化）。它让 284B 参数模型可以在 MacBook 上运行，核心创新是 KV 缓存磁盘持久化。详见 [[ds4]]。


## 在 Agent 框架中的使用

DeepSeek `deepseek-chat` 模型常被用于极简 Agent 框架教学，原因：
- 完全兼容 OpenAI SDK
- 原生支持 Tool Calls（Function Calling）
- API 成本低，适合实验和学习
- [[agent-loop]] 的极简实现（279 行 Python）即基于 DeepSeek

### Sources
- raw/articles/2026-04-18-build-ai-agent-framework.md
