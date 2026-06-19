---
title: "这 2 个免费的开源 Skill 太给劲儿，直接替代 Playwright"
url: "https://mp.weixin.qq.com/s/c224HuJR5TWmI7AhzqvJvQ"
source: "微信公众号"
author: "逛逛"
account: "逛逛GitHub"
pub_date: 2026-06-03
fetched: 2026-06-03
category: "ai-products"
---

# 这 2 个免费的开源 Skill 太给劲儿，直接替代 Playwright

**作者**: 逛逛 | **公众号**: 逛逛GitHub

## 背景

Playwright 等基础浏览器自动化框架在真实互联网环境中存在短板：扫码登录、Session 保持、多账号、机器人验证弹窗等场景没有专门优化。缺少一层专门解决反检测、验证码、Session 管理、人机协作的基础设施。

## BrowserAct 简介

面向 AI Agent 的浏览器自动化 CLI，让 Agent 控制真实浏览器进入动态网页、登录态页面和受保护页面。

**开源地址**: github.com/browser-act/skills

### 两个产品 Skill

1. **browser-act CLI**：实时浏览器控制，适合一次性任务、即时操作
2. **browser-act-skill-forge**：把网站能力封装成可复用的 Skill，适合批量、定期、大规模任务

## 核心能力

### ① 三种浏览器模式

| 模式 | 说明 | 适用场景 |
|------|------|----------|
| **Stealth 浏览器** | 反检测浏览器，独立指纹+代理，需 API Key | 突破反爬保护、多账号并行采集 |
| **Chrome** | 独立 Chrome 实例，复用 Cookie/登录态 | 操作已登录后台或社交媒体 |
| **Chrome-Direct** | CDP 直连当前运行 Chrome | 快速调试、人机协同 |

### ② 突破反爬原理

- **环境层**：定制 Chromium 移除自动化痕迹，每次启动生成独立浏览器指纹，配合动态代理轮换和 Session 隔离
- **执行层**：内置 `solve-captcha` 自动解决 Cloudflare/reCAPTCHA/Datadome 等验证码；`stealth-extract` 提取受保护页面 JS 渲染后内容
- **人机交互层**：`remote-assist` 生成远程链接，让手机扫码/短信验证，完成后 Agent 继续原会话

### ③ 多任务处理

同一账号下可同时跑多个任务（检查消息、整理订单、生成日报等），每个任务独立 Session 工作区互不干扰。多账号场景下每个账号独立浏览器环境（Cookie、Session、代理、指纹）。

### ④ 自动剥离无效 HTML

自动剥离 90% 无效 HTML（广告、追踪代码、框架噪音），只把有意义内容喂给 LLM，省钱且信息更干净。

## Skill Forge：网站能力锻造器

`browser-act-skill-forge` 可以把任何网站的操作能力封装成可复用的 Skill：
- 自动发现网站背后的 API 端点、请求模式
- 探索完成后自动生成完整 `SKILL.md` + Python 脚本包
- 探索时踩的坑会沉淀下来，下次走最优路径
- 探索一次，后续大规模复用

## 开箱即用的 Skill 生态（31 个）

| 场景 | 数量 | 内容 |
|------|------|------|
| 电商 | 8 个 | Amazon ASIN 查询、热销产品、Buy Box 监控、竞品分析、评论抓取等 |
| 线索获取 | 7 个 | 商家联系方式、GitHub 贡献者查找、Google Maps 搜索、行业关键人雷达等 |
| 搜索研究 | 4 个 | Google 图片搜索、Google News、网页研究助手、网页搜索抓取 |
| 社交监听 | 3 个 | Reddit 竞品分析、微信公众号搜索、知乎搜索 |
| 视频平台 | 9 个 | YouTube 搜索、频道分析、评论提取、字幕提取、KOL 发现等 |
