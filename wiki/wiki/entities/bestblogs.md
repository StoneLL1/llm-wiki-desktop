---
title: BestBlogs
created: 2026-06-07
updated: 2026-06-07
type: entity
tags: [open-source, tool, agent, deployment]
sources:
  - raw/GitHub/ginobefun-BestBlogs.md
---

# BestBlogs

AI 驱动的私人阅读助手（3,766 ⭐，2 万+ 注册用户），解决 RSS 信息过载、找不到精华、无评分三大阅读疲劳。产品结构分公共策展层（免费）和我的空间（Pro）。

## 核心能力

### 公共策展层（无需登录）

- **每日早报** — 中英双语图文 + 播客 + 海报/Telegram/X/RSS 多渠道
- **精选周刊** — 每周五 AI + 编辑二次提纯
- **主题解读** — 围绕事件/领域/人物/产品的编辑式深度解读
- **内容广场** — 精选/最新双视角全内容入口

### 我的空间（Pro 解锁完整工作流）

- **我的早报** — 基于关注/兴趣/阅读行为的个性化早报
- **我的关注** — RSS/Newsletter/Twitter/YouTube/Podcast 五源三列工作台
- **我的阅读** — AI 伴读（摘要/翻译/提问/跳转）+ 图书馆沉淀
- **我的回顾** — 每晚自动生成当日阅读小结

### 跨入口能力

- **六维 AI 评分** — 选题/内容/深度/实用/创新/表达
- **沉浸式翻译** — 中英双向，覆盖文章/推文/播客/视频
- **可解释画像** — 兴趣偏好由显式行为驱动

## 微信公众号 RSS 源

团队系统整理 1,600+ 优质订阅源，首批发布 375 个仍在更新的微信公众号 RSS 源（通过 wechat2rss 转换），OPML 可导入。

## CLI & Agent Skills

```bash
npm install -g @bestblogs/cli
bestblogs discover today --limit 20
bestblogs read deep <resourceId>
```

25 个稳定原语，一键安装到 [[claude-code]] / [[openai-codex]] / [[cursor]]：`npx @bestblogs/skills`

## AI 内容处理流水线

```
RSS 爬取 → 初筛过滤 → AI 深度分析（六维评分+摘要+金句） → 多语言翻译 → 入库 → 个性化推荐
```

早期基于 Dify Workflow 实现，当前自研流水线。

## 技术栈

- TypeScript 前端 + 后端
- OpenAPI v2 RESTful API
- CLI（MIT 协议）
- Agent Skills（SKILL.md 格式，兼容 [[claude-code]]/Codex/Cursor/[[openclaw]]）

## 与同类工具对比

| 维度 | BestBlogs | [[horizon]] | [[ai-news-radar]] |
|------|-----------|------------|------------------|
| 定位 | AI 阅读助手 | 个人新闻雷达 | 24h 新闻聚合 |
| 核心价值 | 精读+沉淀 | 信息过滤+去重 | 信源预评估 |
| 用户规模 | 2 万+ | 个人工具 | 个人工具 |
| 中文公众号 | 375 个 RSS 源 | 无 | 无 |
| Agent Skills | 25 个原语 | 无 | 伯乐 Skill |
| 商业模式 | Free + Pro | 免费开源 | 免费开源 |
