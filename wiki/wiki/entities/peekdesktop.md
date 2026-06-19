---
title: PeekDesktop
created: 2026-06-02
updated: 2026-06-02
type: entity
tags:
  - tool
  - open-source
sources:
  - raw/articles/2026-05-31-4-interesting-low-star-github-projects.md
---

# PeekDesktop

## 概述

PeekDesktop 是由微软 VP **Scott Hanselman** 开发的 Windows 桌面工具，将 macOS Sonoma 的"点击桌面收起所有窗口"交互体验搬到 Windows 平台。

## 核心功能

- **点击桌面空白区域**：所有窗口自动收起，露出干净桌面
- **再次点击或点任意 App**：所有窗口恢复到原位置
- **Fly Away 动画模式**：窗口以飞行动画方式收起
- **免配置**：安装后无需任何设置即用

## 技术亮点

- **极致压缩**：.NET 程序从 65 MB 压缩到 1.88 MB（含 LZMA 压缩后可塞进一张软盘）
- **无需管理员权限**
- **空闲内存占用不到 5 MB**
- **无需安装 .NET 运行时**：自包含部署
- **自带自动更新**
- 解压即用（zip 格式分发）

## 开发者背景

Scott Hanselman 是微软 Developer Division 的 VP，知名技术布道者。他撰写了一篇专门的文章详细讲解如何将 .NET 程序体积压缩到极致，展示了 .NET Native AOT + LZMA 压缩的工程实践。

## 相关链接

- [[zero-native]] — Vercel Labs 的桌面应用框架（Zig 原生 Shell + Web UI）
