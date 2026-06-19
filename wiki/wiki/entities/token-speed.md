---
title: TokenSpeed
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, architecture, open-source]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# TokenSpeed

## Overview

TokenSpeed 是一个专为 Agent 工作负载从零设计的 LLM 推理引擎，目标是在 NVIDIA Blackwell 上达到 TensorRT-LLM 级性能、vLLM 级易用性。

## 背景

- **主导方**：LightSeek Foundation（非营利组织）
- **协作方**：NVIDIA DevTech、AMD Triton、通义千问推理团队、Together AI 等

## 核心能力

- 构建了 NVIDIA Blackwell 上最快的 Multi-head Latent Attention (MLA) 实现之一
- MLA 已被 vLLM 项目采用
- 在 Kimi K2.5 实测中，最小延迟场景比 TensorRT-LLM 快约 9%，100 TPS/User 附近吞吐量高约 11%
- NVIDIA AI 官方 Twitter 转发，称其为"brand new inference engine purpose built for speed-of-light agentic workloads"

## 相关链接

- [[deepseek]] — DeepSeek V4 使用的推理引擎相关
- [[hermes-agent]] — 可配置不同推理后端的 Agent 平台
