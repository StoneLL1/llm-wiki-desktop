---
title: A-Stock-Data Agent
created: 2026-05-27
updated: 2026-05-27
type: entity
tags:
  - agent
  - tool
  - open-source
sources:
  - raw/articles/2026-05-26-a-stock-data-agent-a-share.md
---

# A-Stock-Data Agent

## 概述

**a-stock-data** 是一个 A 股全场景数据 Agent 技能注入包，在 30 秒内闭环完成基于客观大厂数据的自动化调研。全方位覆盖从实时五档盘口、主力资金分钟级流向，到券商研报免密 PDF 下载、巨潮公告精准匹配、F10 核心基本面快照等 **28 个高频数据端点**。

GitHub: https://github.com/simonlin1212/a-stock-data

## 核心特性

### 零依赖直连稳定不挂

移除了二次封装库，原生 Python 直连 **13 个大厂底层 HTTP API** 和通达信 TCP 行情源，中间商最少，不因第三方库不兼容崩溃。

### 7 层全栈架构，28 个端点全覆盖

一站式打通 A 股投研闭环，包含：

- 实时行情
- 情绪信号
- 深度筹码
- 券商研报
- 财联社快讯
- 全量交易所公告

### 极致节省 Token

对大模型上下文进行文本清洗和截断优化，过滤掉 F10 和基础资料中 **70% 的无用废话**，帮 AI 极大降低推理成本，防止长文本迷失。

### 全免 Key 架构

全库除问财外全部免 Key 免费，可高频调用。

### 即插去用

全套代码和逻辑打包成一个 [[skills|skill]]，直接丢进 [[claude-code]] 或 [[openclaw]]，内置单票估值、360° 漏斗扫描等流程，让 AI 查数据层层递进不胡言乱语。

## 在 Agent 金融生态中的定位

a-stock-data 与 [[kronos]]（金融市场语言基础模型）、[[ai-hedge-fund]]（AI 对冲基金模拟）和 [[finance-skills]]（金融分析 Skills）形成互补——kronos 提供模型层，ai-hedge-fund 提供策略层，finance-skills 提供分析 Skill，而 a-stock-data 专注于**数据接入层**，解决 Agent 分析股票时数据来源不可靠的核心痛点。

## 相关链接

- [[skills]] — Agent 技能打包系统
- [[claude-code]] — 支持 a-stock-data Skill 的编码 Agent
- [[openclaw]] — 支持 a-stock-data Skill 的多 Agent 平台
- [[kronos]] — 金融市场语言基础模型
- [[finance-skills]] — 金融分析 Agent 技能工具集
- [[ai-hedge-fund]] — AI 对冲基金模拟系统
