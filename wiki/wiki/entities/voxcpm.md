---
title: VoxCPM
created: 2026-05-22
updated: 2026-05-24
type: entity
tags: [model, speech, open-source]
sources:
  - raw/articles/2026-04-18-github-hot-10-open-source-projects.md
  - raw/articles/2026-04-21-github-top10-weekly-stars.md
---

# VoxCPM

## 概述

VoxCPM 是面壁智能（OpenBMB）开源的语音合成大模型，2B 参数，训练数据超过 200 万小时多语种语音。目前 1.38 万 Star。

## 核心特性

- **2B 参数 TTS**：端侧可部署的语音合成大模型
- **30 种语言**：自动识别和切换语言
- **48kHz 录音棚音质**：输出接近专业录音水平
- **Voice Design**：文字描述直接生成符合描述的音色，无需参考音频
- **可控声音克隆**：克隆后可加风格引导，让同一声音表现不同情绪
- **实时流式推理**：RTX 4090 上 RTF 约 0.3，端侧可用

## 在语音 AI 生态中的定位

VoxCPM 与 [[vibevoice]] 是开源语音 AI 领域的两个重要项目。VoxCPM 侧重于声音设计和克隆能力，而 VibeVoice 侧重于超长音频处理。两者共同推动了端侧语音 AI 的可用性。

## 相关链接

- [[vibevoice]] — 微软的语音 AI 模型家族
- [[multi-agent-collaboration]] — 语音是 Agent 多模态交互方向
