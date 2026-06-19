---
title: WeChat Article Skills（公众号排版发布 Skills）
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, skill, open-source, collaboration]
sources: [raw/articles/2026-04-18-wechat-article-unknown-title-1.md]
---

# WeChat Article Skills

## Overview

**WeChat Article Skills** 是由 inhail-wiki 开源的一套微信公众号自动化排版发布工具集，包含 3 个可独立复用的 [[skills]]，覆盖了从样式提取、排版重构到草稿箱推送的完整工作流。

项目开源地址：
- 完整网页项目：https://github.com/inhai-wiki/wechat-typesetting
- 3 个 Skills：https://github.com/inhai-wiki/wechat-article-skills

## 三个核心 Skills

### Skill 1：样式提取

- **输入**：公众号文章链接
- **功能**：自动解析文章的排版结构，提取段落类型、图片位置、引用区块等样式信息
- **输出**：结构化的样式描述，供后续排版复用

这个 Skill 的核心思路是**将排版能力本身抽象出来**——看到一篇排版好的文章，不再需要手动模仿，而是让模型学习其结构和样式。

### Skill 2：排版重构

- **输入**：自己的 Markdown 文章 + 提取的样式结构
- **功能**：根据目标样式对文章进行重新排版
- **本质**：结构化样式的嵌套映射问题

### Skill 3：草稿箱推送

- **功能 1**：一键复制到公众号编辑器
- **功能 2**：通过微信公众号开发者 API 直接推送到草稿箱
- **前置条件**：在微信公众平台获取 AppID 和 AppSecret

## 工作流

完整的使用流程：

1. **写作**：用 [[openclaw]] 或其他 Agent 完成文章写作（Markdown 格式）
2. **样式提取**：丢入参考文章链接，提取排版样式
3. **排版重构**：将文章按目标样式重新排版
4. **推送**：一键推送到公众号草稿箱
5. **确认发布**：在微信公众号后台确认后发布

## 技术细节

- 使用 [[claude-code]] 作为开发 IDE
- 模型接入通过七牛云大模型 API 服务平台（base_url + API Key 替换即可）
- 实测使用 GLM-5 模型，约消耗两百万 token
- 技能可部署到 [[openclaw]] 环境中，通过微信对话直接调用

## 设计理念

作者的关键洞察是：**一旦能力变成 Skill，就不再依赖某一个项目，是可以被复用到任何 Agent 体系里。**

这体现了 [[skill-engineering]] 的核心哲学——将工作流能力封装为可复用的模块化单元，无论是 OpenClaw 还是自建的 Agent 框架，只要接入这几个 Skill 就能直接使用。

## Relationships

- 基于 [[skills]] 范式构建
- 可部署到 [[openclaw]] 环境中
- 使用 [[claude-code]] 开发
- 与 [[huashu-skills]] 定位相似——都是内容创作领域的 Skill 集合
- 解决了 [[feishu]] 和公众号等内容平台的自动化发布需求

## See Also

- [[openclaw]] — 开源多 Agent 平台，这些 Skills 的运行环境
- [[skills]] — SKILL.md 模块化能力框架
- [[claude-code]] — 开发和运行环境
- [[huashu-skills]] — 另一套内容创作 Skills 合集
