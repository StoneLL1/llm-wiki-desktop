---
title: "AI + Vercel 一键部署网站实战"
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [engineering, deployment, tool, tutorial, open-source]
sources:
  - raw/articles/2026-05-26-ai-vercel-deploy-website-yupi.md
---

# AI + Vercel 一键部署网站实战

**作者**: 程序员鱼皮（liyupi） | **来源**: 微信公众号「程序员鱼皮」

本文介绍了如何利用 AI 编程工具 + Vercel 平台实现网站的一键自动部署，让「提一句话需求，写代码和部署都不用干」成为现实。

## 免费部署平台对比

| 平台 | 特点 | 适合场景 |
|------|------|----------|
| **Vercel** | Next.js 官方推荐，海外生态最成熟 | 前端/全栈项目 |
| **Netlify** | 老牌前端托管，内置表单/身份验证 | 静态站/表单需求 |
| **Cloudflare Pages** | 全球最大 CDN，每天 10 万请求免费 | 流量大的项目 |
| **EdgeOne Pages** | 腾讯云全球 CDN，国内速度快 | 国内用户为主的项目 |

所有平台都支持从 GitHub 仓库导入、一键部署、SSR 服务端渲染（Next.js/Nuxt/Astro 等）。

## Vercel 三种 AI 集成方式

Vercel 官方提供了 3 样东西让 AI 操作部署：

1. **Vercel CLI** — 命令行工具，`vercel` 命令完成部署，AI 直接调用
2. **Skills 技能包** — 标准化指令文件（`deploy-to-vercel`），装了之后 AI 知道完整操作流程
   ```bash
   npx skills add vercel-labs/agent-skills
   ```
3. **MCP 模型上下文协议** — AI 直接调用 Vercel API 管理项目

文章推荐 **CLI + Skills** 组合，相比 MCP 不需要额外配置服务端，且一次安装后 [[cursor|Cursor]]、[[claude-code|Claude Code]]、[[openai-codex|Codex]] 都能用。

## 实战流程

1. 注册 Vercel 账号（建议 GitHub 登录）
2. 安装 Vercel CLI：`npm i -g vercel@latest`
3. 安装 Skills：`npx skills add vercel-labs/agent-skills`（全局安装）
4. 在 AI 编程工具中输入 `/deploy-to-vercel` 命令
5. AI 自动检测项目类型、创建 Vercel 项目、构建部署、返回链接
6. 后续修改只需说「帮我重新部署」或推代码到 GitHub 自动触发

## 示例项目

- **编程宝典文档网站**：基于 VuePress 构建，代码开源
- 开源地址：https://github.com/liyupi/codefather（template 分支）

## 前后端分离部署

前端和静态站适合 Vercel 等托管平台，后端服务（Java/Python/WebSocket/定时任务/数据库）需要配合：
- 传统方式：阿里云/腾讯云服务器 + 宝塔/1Panel + Nginx
- 现代方式：Docker 容器 + Railway/Render 等 Serverless 容器平台

## 在 Agent 生态中的定位

Vercel 的 Skills 集成方式与 [[pinme|PinMe]]（一键部署 Skill）和 [[skills|Skills 体系]]理念一致——通过标准化的 Skill 文件让 AI Agent 获得部署能力。这种「给 AI 装技能」的模式是 [[vibe-coding|Vibe Coding]] 范式落地的关键基础设施。

三者对比：

| 工具 | 定位 | 特色 |
|------|------|------|
| Vercel Skills | 海外主流前端部署 | Next.js 生态、GitHub 深度集成 |
| [[pinme|PinMe]] | 全栈一键部署 | IPFS 存储、Serverless SQL、前后端一体 |
| 传统服务器 | 灵活全栈部署 | 最灵活、需运维 |
