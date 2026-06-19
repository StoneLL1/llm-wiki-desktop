---
title: "Agent Harness Engineering: A Survey"
url: "https://github.com/Picrew/LLM-Harness"
source: "GitHub"
fetched: 2026-05-27
stars: 0
language: "HTML/CSS"
license: "MIT (code) / CC BY-SA 4.0 (docs)"
topics: [agent, harness, survey, llm]
paper_url: "https://openreview.net/forum?id=eONq7FdiHa"
catalog_url: "https://github.com/Picrew/awesome-agent-harness"
dataset_url: "https://huggingface.co/datasets/ChenLiu1996/Agent-Harness-Engineering"
---

# Agent Harness Engineering: A Survey

> Companion page for A Survey of Harness Engineering: the ETCLOVG seven-layer taxonomy of agent harness engineering, with pointers to a public catalog of open-source projects.

**作者**: Li Junjie, Xiao Xi, Zhang Yunbei, Liu Chen, Zhao Lin, Liao Xiaoying, Ji Yingrui, Wang Janet, Gu Jianyang, Ge Yingqiang, Xu Weijie, Fang Xi, Xu Xiang, Zhao Tianchen, Kim Youngeun, Wang Tianyang, Hamm Jihun, Krishnaswamy Smita, Huan Jun, Reddy Chandan

**机构**: CMU, Yale, JHU, Virginia Tech, Amazon 等

**论文地址**: https://openreview.net/forum?id=eONq7FdiHa

## 概述

这篇 71 页的综述论文系统梳理了 Agent Harness Engineering 领域，提出了 **ETCLOVG 七层分类框架**，用于拆解 Agent 模型外部的工程系统。论文覆盖了 170+ 个开源 Agent Harness 项目，定义了从 Prompt Engineering → Context Engineering → Harness Engineering 的工程演进路径。

## ETCLOVG 七层框架

| 层级 | 名称 | 职责 |
|------|------|------|
| E | Execution | 执行环境（本地/容器/浏览器/沙箱） |
| T | Tooling | 工具接口（描述、发现、调用、防误选） |
| C | Context | 上下文和记忆（短期/会话/长期） |
| L | Lifecycle | 生命周期和编排（单轮/多轮/多 Agent 分工） |
| O | Observability | 可观测性（trace、token 成本、延迟） |
| V | Verification | 验证和评估（结果正确性、路径合理性） |
| G | Governance | 治理和安全（权限、审批、审计） |

## 关键观点

1. **模型已够强，外壳太弱**：Agent 失败往往不是模型不够聪明，而是 harness 系统没管好。仅优化 harness（不改模型）在编码 benchmark 上可带来 10 倍提升
2. **GPT-5.2-Codex 实证**：通过重构 system prompt + 中间件上下文注入 + 自验证 hooks，Terminal-Bench 2.0 从 52.8% → 66.5%
3. **Meta-Harness**：自动优化 harness 在 Terminal-Bench-2 上达到 76.4%，超过手工设计
4. **评估要 trace-native**：不能只看最终成功率，要把完整执行轨迹作为评估对象
5. **好 Harness 要会删控制**：随着模型变强，某些 context reset 对更强模型已不必要，去掉后成本下降质量不变

## 核心矛盾

- **成本-质量-速度三角**：更安全 = 更强沙箱 + 更细权限 + 更完整 trace → 更高成本和延迟
- **能力-控制矛盾**：更多工具/记忆/权限 = 更有用但失控半径更大
- **外壳耦合（harness coupling）**：改任何一层都可能改变整个系统行为

## 从 Framework 到 Platform

- **Framework**：解决局部抽象（agent、tool、memory、loop）
- **Platform**：解决完整生产系统（durable workspace、managed sandbox、identity、billing、observability、evaluation、governance、human handoff）

## 引用

```bibtex
@misc{li2026agentharness,
  title={Agent Harness Engineering: A Survey},
  author={Li, Junjie and Xiao, Xi and Zhang, Yunbei and Liu, Chen and
          Zhao, Lin and Liao, Xiaoying and Ji, Yingrui and Wang, Janet and
          Gu, Jianyang and Ge, Yingqiang and Xu, Weijie and Fang, Xi and
          Xu, Xiang and Zhao, Tianchen and Kim, Youngeun and
          Wang, Tianyang and Hamm, Jihun and Krishnaswamy, Smita and
          Huan, Jun and Reddy, Chandan},
  url={https://openreview.net/pdf?id=eONq7FdiHa},
  year={2026}
}
```

## 相关资源

- **项目仓库**: https://github.com/Picrew/LLM-Harness
- **论文主页**: https://picrew.github.io/LLM-Harness/
- **开源目录**: https://github.com/Picrew/awesome-agent-harness （170+ Agent Harness 项目）
- **数据集**: https://huggingface.co/datasets/ChenLiu1996/Agent-Harness-Engineering
- **论文 PDF**: https://openreview.net/pdf?id=eONq7FdiHa
