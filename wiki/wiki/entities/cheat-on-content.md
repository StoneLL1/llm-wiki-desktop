---
title: cheat-on-content
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, open-source, skill]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# cheat-on-content

## Overview

cheat-on-content 是一个装进 [[claude-code]] 的内容创作评分系统。通过"打分→盲预测→发布→复盘→进化评分公式"的闭环，把内容创作从凭感觉变成可校准的科学实验。

开源地址：https://github.com/XBuilderLAB/cheat-on-content

## 核心机制

### 防自欺机制
发布前要写预测，预测不可篡改（hook 强制执行）。T+3 天后复盘，对比实际数据和预测。

### 评分公式进化
每次循环评分公式都会进化，但升级必须全量重打加上跨模型独立审核。

### 13 个子 Skill
装好后在 [[claude-code]] 里自然语言触发：打分这篇、启动预测、复盘。

## 相关链接

- [[claude-code]] — 运行环境
- [[skills]] — Skill 体系
- [[khazix-writer]] — 另一个内容创作 Skill
