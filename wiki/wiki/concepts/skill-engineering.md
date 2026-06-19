---
title: Skill 工程化设计
created: 2026-05-17
updated: 2026-06-06
type: concept
tags: [methodology, agent, engineering]
sources:
  - raw/articles/2026-05-13-skill-engineering-design.md
  - raw/articles/2026-04-18-anthropic-skill-best-practices.md
  - raw/articles/2026-05-26-skillopt-microsoft-train-skill-like-nn.md
  - raw/articles/2026-06-05-how-to-write-skills-ultimate-guide.md
---

# Skill 工程化设计

## 定义

Skill 工程化设计是一种把 Agent 当算法用的设计哲学——给定输入，得到指定格式的输出。其核心原则是：**确定性事务交给 CLI，Agent 只做决策引擎**。这种方法将 LLM 的非确定性限制在最小范围内，最大化输出的可靠性和可预测性。

相关：[[claude-code]]、[[skills]]、[[harness-engineering]]、[[mcp]]

## CLI 接管确定性

将精确格式要求、固定执行流程的事务交给 bash 脚本处理，而非让 LLM 自由生成。这样做的好处：

- **消除格式漂移**：LLM 不擅长严格遵循输出格式，CLI 脚本天然精确
- **降低 token 消耗**：确定性操作不需要 LLM 推理
- **提高可审计性**：脚本行为可预测、可复现
- **分离关注点**：LLM 负责判断"做什么"，脚本负责"怎么做"

## 工具信息三层分离

Skill 的工具信息采用三层架构，逐层按需加载：

| 层级 | 内容 | 大小 | 加载时机 |
|------|------|------|----------|
| **索引层** | 50 行工具摘要表格 | ~50 行 | Skill 激活时 |
| **元数据层** | 单个工具的完整描述 | 按需 | Agent 调用该工具时 |
| **规则层** | IGNORE/ENUM 自动生成规则 | 自动 | 基于元数据自动推断 |

这种设计避免了将所有工具信息一次性注入上下文，有效缓解 LLM 注意力衰减问题。

## discover 热更新

当 Skill 被激活时，系统自动同步后端工具列表（discover 机制）。这意味着：

- 工具列表始终保持最新状态
- 无需手动维护工具注册表
- Skill 之间可以动态发现彼此的能力
- 支持运行时新增/移除工具

## Workflow 工作流引擎

Skill 工程化设计引入了基于 Markdown 文件的工作流引擎，核心特性包括：

### 步进式披露（Progressive Disclosure）
- 每一步只展示当前步骤所需的信息
- 避免一次性加载过多上下文
- Agent 可以按需深入获取细节

### Gate 门禁机制
- 工作流中的关键节点设置检查点
- Agent 必须满足前置条件才能推进到下一步
- 确保输出质量和流程合规性

### 状态持久化
- 工作流状态可以保存到文件系统
- 支持中断恢复和长期运行的任务
- 状态文件可用于审计和调试

### 模板变量数据流
- 步骤之间通过模板变量传递数据
- 上一步的输出自动成为下一步的输入
- 支持条件分支和循环结构

## 自举（Bootstrapping）

Skill 工程化设计支持**自举**——用 Skill 创造 Skill。这意味着：

- 可以编写一个 Meta-Skill 来生成新的 Skill 模板
- Skill 的设计规范本身可以作为 Skill 来执行
- 降低创建新 Skill 的门槛，实现规模化的 Skill 生产

## LLM 注意力衰减

Skill 工程化设计直面一个核心挑战：**规则越多，AI 遵守越差**。

LLM 在处理大量规则时存在注意力衰减现象：
- 早期规则比晚期规则更容易被遵守
- 规则之间存在冲突时，LLM 倾向于忽略复杂的规则
- 超过一定阈值后，新增规则的边际效用急剧下降

应对策略：
1. **精简规则**：只保留最重要的规则（参考三层分离架构）
2. **按需加载**：不同场景加载不同规则集
3. **自动化检查**：用 CLI 脚本代替规则约束
4. **Gate 门禁**：在关键节点验证规则遵守情况

## 与其他概念的关系

- **[[claude-code]]**：Skill 工程化的主要实践平台，CLAUDE.md 和 SKILL.md 是其核心载体
- **[[skills]]**：Skill 工程化的产出物，模块化的能力定义
- **[[harness-engineering]]**：更高层次的 Agent 管理方法论，Skill 工程化是其中的关键实现手段
- **[[mcp]]**：为 Skill 提供外部工具和数据源的连接层

## 自动化优化：SkillOpt 范式

2026 年 5 月，Microsoft Research 提出 [[skillopt]]，将 Skill 优化从手工迭代推向自动化。核心思路是将 Skill 文档视为可训练的「权重」，用 rollout → reflection → edit → validation gating 的循环自动优化。在 52 个测试组合中全部取得最优，平均提升 23.5 分，碾压人类手写 Skill。

这标志着 Skill 工程化从「经验驱动的手动调优」进入「数据驱动的自动训练」阶段。关键启示：**Skill 文档不应是一次性写完就不管的东西，它应该像模型权重一样，持续被优化。**

## 开放问题

- 三层分离架构在极端工具数量（100+ 工具）下的性能表现
- 自举过程中如何保证生成 Skill 的质量
- 注意力衰减的量化研究和最优规则数量上限
- Workflow 引擎与现有 CI/CD 系统的集成模式
- 开放性任务（无可自动评估标准）的 Skill 优化方法

## 腾讯工程师的 Skill 编写方法论补充（2026-06）

腾讯程序员 jackjchou 发布了 73KB 的综合指南，从工程实践角度补充了 [[skill-engineering]] 的多个维度^[raw/articles/2026-06-05-how-to-write-skills-ultimate-guide.md]：

### 指令编写原则深化

| 原则 | 说明 |
|------|------|
| 祈使句下指令 | 「检查 Go 版本，根据版本号选择方案」而非「你应该检查 Go 版本」 |
| 解释原因 | 与其堆 MUST，不如讲清楚为什么。「字符串拼接会导致 SQL 注入——攻击者可以通过输入 `'; DROP TABLE` 删除整张表」 |
| Before/After 对比 | 注释标注（简单）、完整文件对比（复杂）、Diff 格式（推荐最直观）三种方式 |
| Few-Shot 示例 | 3-5 个高质量输入/输出示例，覆盖典型场景、边界情况、错误情况。示例间有差异，先放最典型的 |

### 模块化拆分

当 Skill 内容超过 500 行时拆分：

```
project-migration/          # 主 Skill：流程总览与编排
├── SKILL.md
└── steps/                  # 拆分出的子步骤文档
    ├── 00-environment-setup.md
    ├── 01-dependency-update.md
    └── 02-api-migration.md
```

主 SKILL.md 按顺序引用子步骤，每个步骤完成后运行验证命令确认无误再继续。**每个子 Skill 都能独立使用**，脱离主流程也能跑。

### 调试方法论

Skill 问题排查的五步顺序：① 确认已加载（路径和格式）→ ② 确认触发正确（description 匹配）→ ③ 确认指令清晰（对比 AI 输出与预期）→ ④ 确认脚本可执行（权限、依赖、平台）→ ⑤ 加检查点缩小范围。**70% 以上问题出在前两步。**

### 与 [[skillopt]] 自动化优化的衔接

腾讯指南强调 Skill 需要持续迭代优化——「从具体反馈中总结规律」「越精简越好」「解释原因让 AI 理解 why 而非死记 how」。这与 Microsoft Research 的 [[skillopt]] 自动优化范式形成互补：腾讯提供人工迭代的系统方法论，[[skillopt]] 提供数据驱动的自动训练管道。两者共同构成了 Skill 工程化的完整闭环。

Example: [[guizang-ppt-skill]] — a design skill for magazine-style PPT generation.
