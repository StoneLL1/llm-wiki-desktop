---
title: "BestBlogs"
url: "https://github.com/ginobefun/BestBlogs"
source: "GitHub"
fetched: 2026-06-06
stars: 3766
forks: 333
language: "TypeScript"
topics: [ai, business, product, programming]
homepage: "https://bestblogs.dev"
---

# BestBlogs.dev

> AI 驱动的私人阅读助手，汇集顶级编程、人工智能、产品、科技文章，大语言模型摘要评分辅助阅读，发现真正适合你的高质量内容

**⭐ 3,766 stars | 🍴 333 forks | 📅 创建于 2024-01-05 | 👥 2 万+ 注册用户**

## 概述

BestBlogs.dev 是 AI 驱动的私人阅读助手，解决自管理 RSS 阅读器的三大疲劳：信息过载（200+ 篇/天读不完）、找不到精华（标题党/机翻/营销稿混杂）、没有评分（不知哪些值得深读）。产品结构分两端：公共策展层（所有人可免费访问精选内容）和我的空间（登录+Pro 用户拥有个性化阅读流）。

## 核心能力

### 公共策展层（无需登录）

| 入口 | 说明 |
|------|------|
| **每日早报** | 中英双语图文 + 10-15 分钟播客（Apple/Spotify/小宇宙）+ 海报/Telegram/X/RSS 多渠道 |
| **精选周刊** | 每周五，AI+编辑二次提纯本周高质量内容 |
| **主题解读** | 围绕事件/领域/人物/产品对比的编辑式深度解读（四类视角） |
| **内容广场** | 全部公开内容主入口，精选/最新双视角 |

### 我的空间（登录可见，Pro 解锁完整工作流）

| 入口 | 说明 |
|------|------|
| **我的早报** | 基于关注、兴趣标签与阅读行为的个性化早报（图文+邮件） |
| **我的关注** | 五种源（RSS/Newsletter/Twitter/YouTube/Podcast）三列工作台统一管理 |
| **我的阅读** | AI 伴读（摘要/翻译/提问/章节跳转）+ 图书馆沉淀（书签/划线/历史） |
| **我的回顾** | 每天晚间自动生成当日阅读小结 |

### 跨入口能力

- **AI 内容分析**：六维评分（选题/内容/深度/实用/创新/表达）+ 摘要 + 关键观点 + 金句
- **沉浸式翻译**：中英双向，文章/推文/播客/视频全类型
- **可解释画像**：兴趣偏好由显式行为驱动（关注/阅读/Domain 自定义篇数）

## Free vs Pro

Free 是独立可用的阅读产品。Pro 是完整私人阅读助手工作流的兑现：

- Pro 解锁：个性化早报、每日回顾、自定义视图、10× 关注源数量、10× AI 伴读和翻译次数（30次/天）
- 新用户 7 天、老用户 14 天免费试用

## RSS 订阅

灵活的 RSS 地址支持（全站/精选/分类/高分/周刊/早报），可导入 OPML 文件：

| 文件 | 数量 |
|------|------|
| 全部 | 400 个 |
| 文章 | 170 个 |
| 播客 | 30 个 |
| 视频 | 40 个 |
| Twitter | 160 个 |

### 微信公众号 RSS 源（375 个）

BestBlogs 团队正在系统整理超过 1,600 个优质订阅源，首批发布的 375 个仍在更新的微信公众号 RSS 源（通过 [wechat2rss](https://github.com/ttttmr/Wechat2RSS) 转换），覆盖人工智能、软件编程、商业分析等领域。OPML 文件：`opml/bestblogs_wechat2rss_opml_all.opml`

## 开放 API（v2）

Base URL：`https://api.bestblogs.dev/openapi/v2`，鉴权：Header `X-API-KEY`

典型路径：认证 → Intake 建立画像 → Discover 发现内容 → Read 深度阅读 → Capture 留存笔记

## CLI & Agent Skills

### CLI

```bash
npm install -g @bestblogs/cli
bestblogs auth login
bestblogs discover today --limit 20
bestblogs read deep <resourceId>
bestblogs capture bookmark add <resourceId> --note "值得反复读"
```

所有命令支持 `--json` 模式，可直接被 AI Agent 消费。

### Agent Skills

25 个稳定原语，一键安装到 Claude Code / Codex / Cursor：

```bash
npx @bestblogs/skills
```

安装后自然语言触发：今天有什么值得读的、深度阅读这篇、收藏这篇、为什么推这条给我。

## AI 内容处理流水线

```
RSS 爬取 → 初筛过滤 → AI 深度分析 → 多语言翻译 → 入库 → 个性化推荐
```

1. 内容爬取：基于 RSS + 无头浏览器提取全文
2. 初筛过滤：语言类型、内容质量特征初步评分
3. AI 深度分析：LLM 生成六维评分 + 摘要 + 关键观点 + 金句 + 标签
4. 多语言翻译：识别专业术语 → 初译 → 检查 → 意译优化
5. 个性化推荐：六维兴趣标签匹配 + 早报智能编排

早期基于 Dify Workflow 实现，文档和 DSL 开源在 `flows/Dify/` 目录。

## Build in Public

完全公开建造过程的独立产品，产品思考、Agent Native 设计理念、AI 助手工作流拆解等系列文章在 `posts/` 目录。

## 技术栈

- TypeScript 前端 + 后端
- Dify Workflow（早期）/ 自研流水线（当前）
- OpenAPI v2 RESTful API
- CLI（TypeScript，MIT 协议）
- Agent Skills（SKILL.md 格式，兼容 Claude Code/Codex/Cursor/OpenClaw）
