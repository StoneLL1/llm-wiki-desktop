---
title: Recordly
created: 2026-06-02
updated: 2026-06-02
type: entity
tags:
  - tool
  - open-source
sources:
  - raw/articles/2026-05-31-4-interesting-low-star-github-projects.md
---

# Recordly

## 概述

Recordly 是开源的桌面录屏 + 自动编辑工具，目标是让录屏后无需手动剪辑就能产出成品。对标 [[openscreen]]（Screen Studio 的开源替代），但更强调自动化后处理。

跨平台支持 macOS、Windows、Linux，完全免费开源。

## 核心功能

### 智能自动处理
- **自动缩放**：根据光标活动自动生成 zoom 建议，聚焦观众注意力
- **光标美化**：平滑移动、运动模糊、点击弹跳、摇摆效果，支持 macOS 风格光标素材
- **样式化输出**：内置壁纸、渐变背景、圆角、阴影、画面比例预设

### 时间线编辑
- **拖拽式编辑器**：支持裁剪、变速、添加标注、额外音轨
- **摄像头气泡**：摄像头画面叠加在录屏上，支持位置/大小/圆角/阴影自定义

### 扩展系统
- **插件市场**：社区驱动的插件系统，可安装点击音效、设备边框、浏览器 mockup 等
- **导出格式**：支持 MP4 和 GIF，质量可选

## 与 OpenScreen 的对比

| 维度 | Recordly | [[openscreen|OpenScreen]] |
|------|----------|--------------------------|
| 定位 | 录屏 + 自动编辑 | Screen Studio 开源替代 |
| 自动处理 | 自动缩放、光标美化 | 自动缩放平移、动态模糊 |
| 手动编辑 | 拖拽时间线、音轨 | 手动缩放编辑 |
| 社区扩展 | 插件市场 | — |
| 摄像头 | 摄像头气泡叠加 | — |

两者互补：Recordly 偏自动化流程（录完即出片），OpenScreen 偏手动精修。

## 相关链接

- [[openscreen]] — Screen Studio 的开源替代，同赛道工具
- [[video-use]] — AI 驱动的全流程视频制作 Skill
- [[remotion]] — 基于 React 的代码化视频框架
