---
title: Reflection Pattern
created: 2026-05-21
updated: 2026-05-21
type: concept
tags: [agent, architecture, methodology]
sources:
  - raw/articles/2026-04-18-build-ai-agent-framework.md
---

# Reflection Pattern

## 定义

Reflection（反思）模式是 AI Agent 行为模式中的一种增强策略，核心思想是让 Agent 对自身输出进行评估和改进。通过语言反馈而非权重更新来强化 Agent 表现。

## 三个里程碑工作

### 1. Reflexion（Shinn & Yao, 2023）

Noah Shinn 和 Shunyu Yao（[[react-pattern]] 作者）等发表《Reflexion: Language Agents with Verbal Reinforcement Learning》，提出 Reflexion 框架：
- Agent 对任务反馈信号进行口头反思
- 在情节记忆缓冲区中维护反思文本
- 后续试验中利用反思做出更好决策

### 2. Self-Refine（Madaan et al., 2023）

Aman Madaan 等在《Self-Refine: Iterative Refinement with Self-Feedback》中提出：
- LLM 先输出初稿
- 根据输出提供自我反馈
- 迭代改进，平均性能提升约 20%

### 3. CRITIC（清华 & 微软, 2023）

《CRITIC: Large Language Models Can Self-Correct with Tool-Interactive Critiquing》：
- 结合外部工具（搜索引擎、代码执行器）验证输出
- 基于验证结果进行自我修正
- 引入外部客观标准，而非纯自我评估

## 与其他模式的关系

Reflection 是对 [[react-pattern]] 的**增强**，而非替代：
- ReAct 解决"推理+行动"的闭环
- Reflection 在此基础上增加"评估+改进"的元认知层
- 可以与 [[plan-and-execute-pattern]] 结合：先规划，执行后反思，必要时重新规划

## 在多 Agent 系统中的应用

在 [[multi-agent-collaboration]] 中，Reflection 模式体现为：
- **生成器-验证器模式**：一个 Agent 生成，另一个验证，迭代直到质量达标
- **交叉审查**：多个 Agent 互相审查对方输出
- [[claude-code]] + Codex 协作中的"必须让 Codex review 代码改动"规则

## See Also

- [[react-pattern]] — Reflection 的基础行为模式
- [[plan-and-execute-pattern]] — 先规划后执行的替代模式
- [[agent-loop]] — Reflection 循环的工程载体
- [[multi-agent-collaboration]] — 多 Agent 中的反思与验证
- [[context-engineering]] — 管理反思过程中的上下文
- [[harness-engineering]] — 约束 Agent 行为的系统方法
