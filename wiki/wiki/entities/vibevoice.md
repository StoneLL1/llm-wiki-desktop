---
title: VibeVoice
created: 2026-05-22
updated: 2026-05-27
type: entity
tags: [tool, speech, open-source]
sources:
  - raw/articles/2026-04-18-11-hot-github-projects-this-week.md
---

# VibeVoice

## 概述

VibeVoice 是微软开源的语音 AI 模型家族，涵盖语音合成（TTS）和语音识别（ASR）两大方向。最初因深度伪造风险被下架，后重新上架并迅速获得 3.5 万+ Star。

## 核心特性

- **超长音频处理**：TTS 模型单次生成 90 分钟多说话人对话音频；ASR 模型单次处理 60 分钟音频无需切片
- **智能 ASR 输出**：一次推理完成说话人识别、时间戳标注和内容转录
- **轻量实时 TTS**：0.5B 参数的实时 TTS 模型，首音频延迟约 300ms，消费级 GPU 可运行
- **多模态**：同时支持语音合成和语音识别

## 在语音 AI 生态中的定位

VibeVoice 是开源语音 AI 领域的领先项目。与 [[voxcpm]] 类似，VibeVoice 提供了端侧可部署的语音 AI 能力，但 VibeVoice 侧重于超长音频处理，而 VoxCPM 侧重于声音设计和克隆。

## 相关链接

- [[voxcpm]] — 面壁智能的开源 TTS 大模型
- [[multi-agent-collaboration]] — 语音是 Agent 多模态交互的重要方向
