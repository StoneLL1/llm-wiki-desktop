---
title: "Anthropic 最新博客：95% 的数据分析，都已经交给了 Claude"
url: "https://mp.weixin.qq.com/s/DHMgoVa9YykVYWIRc0GATw"
source: "微信公众号"
author: "Anthropic"
account: "AGI Hunt"
pub_date: 2026-06-05
fetched: 2026-06-05
---

# Anthropic 最新博客：95% 的数据分析，都已经交给了 Claude

**作者**: Anthropic | **公众号**: AGI Hunt | **发布日期**: 2026-06-05

Anthropic 的数据科学团队昨天发了篇博客：他们内部 95% 的业务数据分析查询，现在都已经由 Claude 自动完成了，准确率大约是 95%。

![Claude 数据分析](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkvyxO3xuNA2jhdN30wqx8owiacf9kv2p1Pf46ZNBjcqMAyyKkG5LyaMA087hjBStRrITW6yNu6RQyyaTaekORPAyn81vqnAPuc/640?from=appmsg)

数据团队也因此能腾出了手来，专注做因果建模、预测和机器学习这些更有价值的工作。

在这篇博客里，Anthropic 把他们做这件事的整个过程、踩过的坑、以及最终沉淀下来的那些方法论，都毫无保留地分享了。

在这里给大家做一个全面的中文翻译和解读。

## 01/ 核心要点速览

*   **95% 的查询自动化**：Anthropic 内部的数据分析查询有 95% 已经由 Claude 自动完成，不需要人工介入。
*   **95% 的准确率**：这些自动化查询的准确率大约在 95%。
*   **关键技术**：Text-to-SQL + RAG + 自定义 Tool Use 三管齐下。
*   **核心理念**：让数据分析从"专家专属"变成"人人可用"的自助服务。
*   **关键经验**：好的数据分析系统不是技术有多牛，而是能把领域知识沉淀下来，变成 AI 也能用的资源。
*   **重要提醒**：不是所有查询都能自动化，复杂查询仍需要人工介入。

## 02/ 三种 AI 分析模式

Anthropic 采用了三种不同的 AI 分析模式来应对不同类型的查询需求：

### 2.1 低复杂度：自动模式

适用场景：简单、直接的数据查询。

特点：用户只需要用自然语言描述需求，系统自动选择合适的分析方法和工具。

这个模式覆盖了 Anthropic 内部大约 60% 的数据分析需求。

### 2.2 中等复杂度：指导模式

适用场景：有一定复杂度的分析任务。

特点：AI 会先展示一个分析计划，用户确认后再执行。这种模式兼顾了自动化和可控性。

### 2.3 高复杂度：人工模式

适用场景：复杂、敏感或需要深度领域知识的分析。

特点：AI 提供辅助，但由人工主导分析过程。大约 5% 的查询需要完全人工处理。

![三种模式](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkvyxO3xuNA2jhdN30wqx8oqgm7XZrPYnKgpxMpZODM3j9JWmx8MmmbvydxOqEQpbBFVRt2H8aNEaGECV7wJW2S/640?from=appmsg)

## 03/ 技术架构详解

Anthropic 的技术架构由三个核心组件构成：

### 3.1 Text-to-SQL

这是整个系统的基础。用户用自然语言提问，系统将其转换为 SQL 查询。

关键技术点：

*   **Schema 描述**：为每张表编写清晰、详细的 schema 描述，包括表的用途、关键字段的含义、表与表之间的关系等。
*   **示例查询**：为常见查询类型提供示例 SQL，帮助模型理解复杂的业务逻辑。
*   **查询验证**：在执行 SQL 之前，先让 Claude 检查生成的 SQL 是否正确。

### 3.2 RAG（检索增强生成）

用于处理需要上下文信息的查询。

关键技术点：

*   **知识库**：将之前的分析报告、业务术语表、数据字典等文档存入知识库。
*   **语义检索**：当用户的查询涉及到业务概念时，先从知识库中检索相关的背景信息，再生成回答。
*   **信息溯源**：在回答中注明信息来源，方便用户验证。

### 3.3 自定义 Tool Use

为特定的分析任务创建专用工具。

关键技术点：

*   **封装复用逻辑**：将常用的分析流程（如漏斗分析、留存分析等）封装成可复用的工具。
*   **标准化输出**：确保工具的输出格式一致，方便后续处理。
*   **权限控制**：不同工具设置不同的权限，确保数据安全。

## 04/ Skill 文件——这个才是精髓

Anthropic 博客中提到的 Skill 文件，跟我们在 Hermes 中使用的 skill 概念高度一致。

Skill 文件本质上就是：

> 把资深分析师脑子里的"知道怎么做"沉淀成结构化的文本，让 Claude 也能"照着做"。

Anthropic 的 Skill 文件包含：

### 4.1 上下文信息

*   数据库的概况
*   各表的用途和含义
*   关键业务指标的定义
*   常见的分析模式

### 4.2 编码规范

*   SQL 编写风格指南
*   命名规范
*   注释要求
*   性能优化建议

### 4.3 质量标准

*   查询结果的验证方法
*   常见的错误类型及检查方法
*   结果呈现的格式要求

Anthropic 强调了一个关键点：

> **Skill 文件不是一次写完就万事大吉的。它需要根据实际使用情况不断迭代优化。**

每次 Claude 给出错误或不够好的分析结果，都需要反思：是 Skill 文件中缺少了什么信息？然后把它补上。

## 05/ 实施经验与教训

Anthropic 分享了几个重要的经验教训：

### 5.1 领域知识比技术能力更重要

一个对业务非常了解的普通分析师 + Claude，远比一个不懂业务的技术大牛 + Claude 要有效得多。

技术架构大家都差不多，真正拉开差距的是**领域知识的沉淀质量**。

### 5.2 不要追求一步到位

Anthropic 的系统也不是一开始就能处理 95% 的查询。他们经历了一个渐进的过程：

1.  先从最简单的查询开始自动化
2.  收集错误案例，分析失败原因
3.  把新的知识补充到 Skill 文件中
4.  逐步扩展到更复杂的查询

### 5.3 数据质量是基础

不管 AI 多强大，如果底层数据质量有问题，分析结果也不会可靠。

Anthropic 特别强调了几点：

*   **数据治理**：确保数据的一致性和准确性
*   **文档完善**：每张表、每个字段都有清晰的文档
*   **命名规范**：表名和字段名要能自解释

### 5.4 人的因素不可忽视

技术解决方案只是工具，真正让这个系统运作起来的，是人的参与：

*   **数据团队需要持续维护 Skill 文件**
*   **业务用户需要积极使用并提供反馈**
*   **管理层需要支持这一转型**

### 5.5 安全和隐私

Anthropic 在这方面非常谨慎：

*   **数据脱敏**：敏感信息在送入 AI 之前先做脱敏处理
*   **权限管理**：不同级别的用户只能查询对应权限范围内的数据
*   **审计日志**：所有 AI 生成的查询都有完整的审计日志
*   **人工审核**：敏感查询仍然需要人工审核

## 06/ 成果数据

![成果数据](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkvyxO3xuNA2jhdN30wqx8om6wWFdFGHNdXGMR7HmFno6JdDqelFrHECJgfN2TsPuTCiqHmRGFnDkwPHRHxG1H/640?from=appmsg)

## 07/ Anthropic 做法和 Hermes Skill 的对应关系

Anthropic 的 Skill 文件和 Hermes 的 Skill 概念高度一致：

| Anthropic 做法 | Hermes 对应 |
| --- | --- |
| Schema 描述 + 示例查询 | skill 中的 context 部分 |
| 编码规范 | skill 中的 conventions |
| 质量标准 | skill 中的 pitfalls |
| 上下文信息 | skill 中的 background |
| 工具封装 | skill + tool 调用 |

以前这些知识都在资深分析师的脑子里，Anthropic 的做法是，是把脑子里的东西变成 Skill 文件，让 Claude 也能用上。

而现在，Skill 也给出来了，就等你抄作业了！

◇ ◆ ◇

相关链接：

https://claude.com/blog/how-anthropic-enables-self-service-data-analytics-with-claude
