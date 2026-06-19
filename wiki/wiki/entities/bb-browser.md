---
title: bb-browser
created: 2026-05-24
updated: 2026-05-24
type: entity
tags: [tool, open-source, agent, deployment]
sources: [raw/articles/2026-04-21-5-treasure-github-projects.md]
---

# bb-browser

## Overview

bb-browser（epiral/bb-browser）是一个将真实浏览器环境封装为 API 调用的工具，让 AI Agent 可以直接借助浏览器的登录状态访问网页内容，无需额外配置 API 密钥。它解决了 AI Agent 访问需要登录的平台时面临的认证难题。

GitHub: [epiral/bb-browser](https://github.com/epiral/bb-browser)

## Key Features

- **浏览器环境封装**：将真实浏览器（含 Cookie/Session）封装为 API
- **登录态继承**：AI Agent 利用已有浏览器登录状态直接访问网页
- **多平台支持**：知乎、B站、GitHub、东方财富等主流平台
- **多种数据获取**：
  - 平台热点查询
  - 股票信息获取
  - 视频字幕提取
  - 代码仓库内容检索
- **无需 API 密钥**：绕过常规 API 限制，模拟真实用户行为

## Comparison with Alternatives

| 维度 | bb-browser | [[browser-use]] | [[agent-browser]] |
|------|-----------|-----------------|-------------------|
| 核心思路 | 浏览器状态→API | LLM 驱动浏览器 | Vercel Labs 浏览器自动化 |
| 登录态 | 继承真实浏览器 | 需配置 | 需配置 |
| API 密钥 | 不需要 | 需要 LLM API | 需要 |
| 适用场景 | 数据获取 | 通用浏览器操控 | 网页抓取 |

## Use Cases

- AI Agent 数据获取：突破常规爬虫限制
- 自动化脚本：模拟真实用户行为访问网页
- 金融数据采集：通过东方财富等平台获取行情
- 内容分析：B站视频字幕提取、知乎热点追踪

## Relationships

- 替代方案：[[browser-use]] — LLM 驱动浏览器自动化
- 替代方案：[[agent-browser]] — Vercel 浏览器自动化
- 相关：[[computer-use-agent]] — CUA 范式
- 应用：[[multi-agent-collaboration]] — 多 Agent 数据获取场景

## See Also

- [[browser-use]] — LLM 驱动浏览器框架
- [[agent-browser]] — Vercel 浏览器自动化工具
- [[computer-use-agent]] — 计算机使用代理
