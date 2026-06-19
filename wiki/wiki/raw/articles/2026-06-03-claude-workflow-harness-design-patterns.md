---
title: "Claude的workflows功能，是一套顶级的harness设计"
url: "https://mp.weixin.qq.com/s/53ope9yoULtTEO7ROu_W7w"
source: "微信公众号"
author: "鲁工"
account: "鲁工"
pub_date: 2026-06-03
fetched: 2026-06-03
category: "tip"
---

# Claude 的 workflows 功能，是一套顶级的 harness 设计

**作者**: 鲁工（九年 AI 算法老兵，AI 全栈开发者）

## 概述

鲁工深度使用 Dynamic Workflow 后的体会，结合 Anthropic 官方博客文章《A harness for every task》的系统梳理。与之前 AGI Hunt 的解读互补——AGI Hunt 侧重功能介绍，鲁工更侧重工程视角的 harness 设计模式分析。

## 为什么要设计 Workflow

单上下文三个顽疾：

1. **Agentic Laziness（偷懒）** — 50 项安全审查做到 35 项就说搞定了
2. **Self-preferential Bias（自我偏袒）** — 让自己验自己的产出，下不了狠手
3. **Goal Drift（目标漂移）** — compact 压缩越多次，最初的约束越模糊

**解法**：拆给各自独立上下文的 subagent，每个只盯一小块，跑完合并。

## 六种 Harness 设计模式

### 1. Fan-out-and-Synthesize（扇出合并）— 最常用
大任务切成小步，各 agent 并行跑，汇总 agent 等所有分支完成再合并（栅栏 barrier 机制）。deep-research 跑 111 个 agent 就是这种。

### 2. Adversarial Verification（对抗核查）— 最该优先用
每开一个干活 agent，就再开一个挑刺 agent 拿评分标准校验。直接解决自我偏袒问题。作者实测：跑文献综述引用验证，假引用基本全揪出来。

### 3. Classify-and-Act（分类路由）
分类 agent 判断任务类型 → 路由到对应处理 agent。

### 4. Generate-and-Filter（生成过滤）
放开了生成一批 → 规则筛、去重 → 留下经得起验证的。

### 5. Tournament（锦标赛）
N 个 agent 不同思路干同一件事 → 裁判两两 PK → 淘汰到剩一个。两两比较比让模型打绝对分靠谱得多。

### 6. Loop Until Done（循环至终）
工作量不确定就不定死轮数，循环开 agent，直到没有新发现才停。配 `/loop` 干脏活累活。

## 适合 Workflow 的场景

| 场景 | 要点 |
|------|------|
| 大型迁移重构 | Bun Zig→Rust：几百 agent 并行，每个文件配俩 reviewer |
| 规模化分拣 | 工单分类→去重→自修 or 升级给人。**隔离区模式**：读不可信内容的 agent 禁高权限操作 |
| 排查问题 | 不同 agent 从互不相关证据提假设 → 验证反驳 → 审核。不止代码，任何复盘都适用 |

## 什么时候不要用

> 常规写代码，动手前先问：这活真需要更多算力吗？大部分传统编码任务不需要五个 reviewer 组团。

Workflow 非常废 token，作者深度用时不时触发五小时 limit。

## 实用技巧

- 不只服务大任务，可以开 quick workflow 做一次快速对抗复查
- 重复跑的活用 `/loop` 定时 + `/goal` 卡硬性标准
- prompt 里直接写「用 100k token」封顶消耗
- 满意的流程按 `s` 存到 `~/.claude/workflows` 或放 skill 里分发

## 核心观点

> 「过去大家拼的是模型单点多聪明，往后更拼你会不会给手头这个任务，现写一套配得上它的 harness。」

Harness Engineering 是今年的主旋律——模型智能进化能替代一部分 harness，但 harness 本身对模型的加成仍然非常有效。
