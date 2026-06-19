---
title: "MiniCPM5-1B"
created: 2026-05-26
updated: 2026-05-26
type: entity
tags: [model, open-source, deployment]
sources:
  - raw/articles/2026-05-26-minicpm5-1b-openbmb.md
---

# MiniCPM5-1B

**MiniCPM5-1B** 是面壁智能（ModelBest）联合清华大学、OpenBMB 开源社区发布的端侧文本基座大模型。仅 1B 参数规模，在 Artificial Analysis（AA）榜单上以 17.9 分超越所有 2B 参数以下模型，成为全球 2B 以下最强开源基座模型。

## 核心定位

端侧文本小钢炮——**不是把云端大模型能力打折塞进小设备，而是让小尺寸模型本身就足够强，强到能独立驱动真实应用。**

## 关键性能

| 指标 | 数据 |
|------|------|
| 参数规模 | 1B |
| AA 榜单得分 | 17.9（2B 以下第一） |
| 对比 Qwen3.5-2B | 17.9 vs 16.3，参数量仅一半 |
| INT4 量化体积 | 0.5GB |
| 设备兼容率 | 90%+ |

- **综合知识、数学推理、代码推理、工具调用**四个维度全面超越同尺寸模型（Qwen3.5-0.8B、LFM2.5-1.2B-Thinking 等）
- 相比 3 个月前的 Qwen3.5-2B，效果更优但参数量减半

## 密度定律

MiniCPM5-1B 验证了一个持续观察：**大模型的智能密度约每 3.5 个月翻一番**。更小的模型正在承载更高的智能密度。这一趋势与 [[deepseek|DeepSeek]] 等团队追求的模型效率优化方向一致。

## 端侧部署

与 [[gemma-4|Gemma 4]] 类似，MiniCPM5-1B 定位本地/端侧部署，但侧重点不同：

| 维度 | MiniCPM5-1B | [[gemma-4|Gemma 4]] | [[kimi-k25|Kimi K2.5]] |
|------|-------------|---------------------|----------------------|
| 参数量 | 1B | 多尺寸 | 未知 |
| 调制 | 纯文本基座 | 多模态（图片理解） | 多模态（视觉→代码） |
| 部署门槛 | 极低（0.5GB INT4） | 低（Ollama 本地） | API / 集成 |
| 核心场景 | 通用端侧推理 | 图片语义分析 | 前端代码生成 |
| 定位 | 端侧通用基座 | 本地多模态辅助 | 视觉编码专精 |

**部署要求极低**：不需要 GPU 集群或云端 API，普通笔记本、手机、浏览器标签页即可运行。配套「桌宠」应用 [MiniCPM-Desk-Pet](https://github.com/OpenBMB/MiniCPM-Desk-Pet) 展示了端侧模型的独立应用能力。

## 训练数据：分级数据治理

面壁智能构建了**分级数据治理体系**（L0-L4 五级），核心理念：

> 与其用海量低质数据灌出一个模型，不如用精选高密度数据养出一个模型。

三个关键语料方向：高知识密度中文网页、高知识密度英文网页、高质量数学合成语料。配套开源数据集 **Ultra-FineWeb-L3**。

这与 [[context-engineering|Context Engineering]] 中「垃圾进垃圾出」的原则异曲同工——模型性能取决于输入数据质量。

技术报告：[arxiv.org/pdf/2602.09003](https://arxiv.org/pdf/2602.09003)

## ForgeTrain：AI 制造 AI

MiniCPM5-1B 的 Base Model 由 **ForgeTrain** 预训练完成——全球首个完全由 AI 编写的生产级大模型训练框架：

- **全部代码由 AI 生成**，人类工程师零代码介入
- 在英伟达 H100 上训练速度比 Megatron **快 10%**
- 在华为昇腾上完成预训练

验证了「AI 制造 AI」的递回归智能（RSI）路径可行性。面壁智能认为这可能比 [[anthropic|Anthropic]] CEO Dario Amodei 预言的 2028 年更早实现。

## 开源资源

| 资源 | 链接 |
|------|------|
| HuggingFace | [openbmb/MiniCPM5-1B_](https://huggingface.co/openbmb/MiniCPM5-1B_) |
| GitHub | [OpenBMB/MiniCPM](https://github.com/OpenBMB/MiniCPM) |
| ModelScope | [OpenBMB/MiniCPM5-1B](https://modelscope.cn/models/OpenBMB/MiniCPM5-1B) |
| 桌宠项目 | [OpenBMB/MiniCPM-Desk-Pet](https://github.com/OpenBMB/MiniCPM-Desk-Pet) |
