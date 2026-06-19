---
title: Nexus4CC
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, open-source, agent]
sources:
  - raw/articles/2026-04-18-claude-code-mobile-remote.md
---

# Nexus4CC

## Overview

**Nexus4CC** 是一个开源项目，解决的核心问题是：**在手机上操控电脑里的 [[claude-code]]**。由投资人 Librae（librae8226）开发，从 2026 年 3 月中旬启动，4 月 10 日完成 v1.0，共 247 个 commit，其中 225 个由 Claude 辅助完成（AI 辅助率 91%），约 9,271 行代码。

Nexus 本身就是在 Nexus 中开发出来的——作者在地铁上用手机让 Claude 实现新功能，在咖啡厅里调试代码，在会议间隙修 Bug。

## 核心架构

**把电脑上的 tmux 终端会话，通过 WebSocket 实时投射到手机浏览器。**

```
手机浏览器 (xterm.js)
    ↕  WebSocket 双向通信
Node.js 服务端
    ↕  stdin / stdout
node-pty (伪终端)
    ↕  tmux attach-session
电脑上的 tmux 会话
```

关键点：手机和电脑看到的是**同一个 tmux 会话**。手机本质上是电脑终端的一块可交互的「副屏」。

## 核心特性

- **发射后不管**：下指令后锁手机，AI 在后台持续工作，随时回来查看进度
- **自动重连**：网络中断恢复时自动重连，浏览器崩溃刷新即可
- **移动端优化**：左右滑动切换 tmux 窗口、双指缩放、底部快捷键工具栏、文件浏览器
- **IME 支持**：支持中文输入法等组合输入
- **语音输入**：配合 Typeless、豆包、微信输入法或系统语音识别

## 与远程桌面的对比

| 维度 | 远程桌面 (TeamViewer/向日葵) | Nexus4CC |
|------|------------------------------|----------|
| 功能范围 | 完整桌面，什么都能做 | 只有终端，但做到极致 |
| 手机体验 | 小屏操作灾难，鼠标定位困难 | 为移动端专门设计 |
| 延迟 | 较高 | 低（WebSocket 直连） |
| 适用场景 | 需要操作 GUI 应用 | [[claude-code]] 编程、看日志、跑脚本 |

## 部署要求

- 一台电脑（Linux / macOS / WSL2），Node.js 20+ 和 tmux
- 局域网：手机和电脑同一网络，内网 IP 直接访问
- 外网：需内网穿透（Cloudflare Tunnel、Tailscale、frp）或公网 IP + 域名
- 技术栈：Node.js + React + WebSocket + tmux + xterm.js

## 当前局限

- 不支持原生手机推送通知（无 APNs / FCM），需浏览器通知或 Telegram Bot
- 每个实例只能控制一台电脑，不支持多机器管理
- 多人协作（转交任务、结对编程）仍在讨论阶段

## 项目地址

https://github.com/librae8226/nexus4cc

## Relationships

- 用于远程操控 [[claude-code]]
- 与 [[claude-code]] 原生的 `/remote-control`、`--teleport` 互补
- 体现了 [[ai-native-development]] 的碎片时间利用理念
- 属于 [[context-engineering]] 工具链中的远程访问层

## See Also

- [[claude-code]] — 被远程操控的主体
- [[context-engineering]] — 远程会话中的上下文管理
