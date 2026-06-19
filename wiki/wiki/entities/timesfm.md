---
title: TimesFM
created: 2026-05-22
updated: 2026-05-27
type: entity
tags: [model, benchmark, open-source]
sources:
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
---

# TimesFM

## 概述

TimesFM 是 Google Research 开发的时序预测基础模型，在 1000 亿个真实世界时间点上预训练。最新 2.5 版本仅 200M 参数，但零样本预测准确率超过许多更大模型，在 GIFT-Eval 基准上全指标排名第一。

## 核心特性

- **200M 参数**：轻量级模型，消费级 GPU 即可运行
- **零样本预测**：无需针对特定数据集微调即可获得高准确率
- **超长上下文窗口**：支持 16384 个时间步，比上一代提升 8 倍
- **自动频率推断**：不需要指定数据频率，模型自动推断
- **BigQuery 集成**：已集成到 Google BigQuery，企业用户可通过 SQL 调用
- **HuggingFace 权重**：一行代码加载权重即可使用

## 在模型生态中的定位

TimesFM 代表了基础模型向时序预测领域的扩展。与 NLP、CV 领域的大模型不同，时序预测模型需要处理连续数值数据，TimesFM 证明了小参数量也能在该领域达到 SOTA 水平。

## 相关链接

- [[agent-skills-addyosmani]] — AI 编码规范
- [[multi-agent-collaboration]] — 多 Agent 协作范式
