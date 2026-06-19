---
title: Mano-P
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, agent, open-source]
sources:
  - raw/articles/2026-04-18-github-open-source-control-computer-skill.md
---

# Mano-P

## Overview

**Mano-P** 是明略科技（Mininglamp AI）开源的 GUI-VLA 智能体模型，是一个纯视觉驱动的桌面应用自动化 AI。它能看懂电脑屏幕并像人一样操作任何桌面软件，不依赖 CDP 协议、HTML 解析或 API 接口。

GitHub: [Mininglamp-AI/Mano-P](https://github.com/Mininglamp-AI/Mano-P)

名称源自西班牙语 "mano"（手），P 代表 Person 和 Party，意味着无论个人还是组织都能用它创建个性化 AI。

## Benchmark

- OSWorld 专项模型榜单排名第一，72B 模型成功率 58.2%，领先第二名 13.2 个百分点
- 全球 13 个多模态基准榜单 SOTA

## Core Features

### 纯视觉驱动
- 模型直接看屏幕截图，像人一样理解界面内容并执行操作
- 覆盖桌面软件、网页、3D 应用、专业工具——只要有图形界面就能操作
- 不依赖 CDP 协议、Accessibility API 或云端模型

### 本地隐私优先
- 所有截图和任务数据完全不出设备
- 不需要联网，断网也能跑
- 4B 量化模型在 Apple M4 Pro 上：预填充 476 tokens/s，解码 76 tokens/s，峰值内存 4.3GB
- 本地模式需要 M4 芯片 Mac + 32GB 内存，或通过 USB 4.0 算力棒运行

### Think → Act → Verify 闭环推理
- 先思考当前画面该做什么 → 执行操作 → 验证结果是否正确
- 发现不对会自动纠错重来
- 保证复杂长任务的稳定性

## 使用方式

### CLI 工具
```bash
brew tap HanningWang/tap
brew install mano-cua

# 操作微信发消息
mano-cua run "打开微信并告诉XX会议延期"
# 停止当前任务
mano-cua stop
```

### Agent Skill 集成
可通过 mano-skill 作为 Skill 安装到 [[claude-code]]、[[openclaw]] 等 Agent 中。

### 硬件要求
- 本地：M4 Mac + 32GB 内存
- 云端模式可用，但敏感数据（本地文件、剪贴板、凭证）不上传

## 开源阶段

目前处于第一阶段，开放 Mano-CUA Skills 部分。Mano-CUA 本地模型和 SDK 组件后续开源。

## Relationships

- 属于 [[computer-use-agent]] 范式下的具体实现
- 与 [[turix-cua]] 同属 CUA 方向，但 Mano-P 侧重本地隐私和视觉 VLA 模型
- 可通过 [[skills]] 机制集成到 [[claude-code]] 和 [[openclaw]]
- 开发者是明略科技（Mininglamp AI）

## See Also

- [[computer-use-agent]] — CUA 通用概念
- [[turix-cua]] — 另一个开源 CUA 实现
- [[skills]] — Agent 技能系统
- [[claude-code]] — 支持 Skill 集成的 Agent
