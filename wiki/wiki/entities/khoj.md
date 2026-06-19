---
title: Khoj
created: 2026-05-22
updated: 2026-05-22
type: entity
tags: [tool, rag, knowledge-management, open-source]
sources:
  - raw/articles/2026-04-18-github-33k-knowledge-base-ai-brain.md
---

# Khoj

## 概述

Khoj（发音同 "knowledge" 前两音节）是开源的个人 AI 第二大脑，将本地文档 + 在线大模型 + 语义搜索三者结合，让所有笔记、PDF、Markdown、Notion 都能被 AI 理解和对话。GitHub 33,642 Star。Python + TypeScript 技术栈。

## 核心特性

- **与文档对话**：上传 PDF、Markdown、Notion、Word、Org-mode，直接基于你的文档问答
- **自定义 Agent**：创建带知识库、人设、工具集的专属角色 AI
- **自动化研究**：定时任务自动抓取新闻、竞品动态，生成每日简报推送邮箱
- **全模型支持**：GPT、Claude、Gemini、DeepSeek、Llama3、Qwen、Mistral 随意切换混用
- **语义搜索**：AI 理解搜索意图，在海量文档中找到最相关内容
- **图像生成 + 语音**：可生成图片、朗读回答、语音提问
- **本地模型**：Llama3、Qwen、Gemma、Mistral，离线可用，Mac M 芯片无压力
- **私有化部署**：数据不上云，完全本地运行

## 多端支持

[[obsidian]] 插件、Emacs、Web、桌面客户端、手机 App、WhatsApp——在习惯的地方直接问。

## 快速上手

- **云端**：https://app.khoj.dev 零配置使用
- **Docker**：`docker compose up` 一键部署
- **Python**：`pip install khoj`

## 在知识管理生态中的定位

Khoj 是 [[rag]] 技术在个人知识管理领域的深度实践。与 [[onyx]] 的企业定位不同，Khoj 专注个人使用场景。它与 [[obsidian]] 深度集成，代表了 [[knowledge-compilation]] 的 AI 原生方向。

## 相关链接

- [[rag]] — 检索增强生成技术
- [[onyx]] — 企业级 AI 搜索平台
- [[obsidian]] — Khoj 支持的知识管理工具
- [[context-engineering]] — 上下文工程的系统方法论
