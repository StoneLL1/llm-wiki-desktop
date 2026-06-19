---
title: Harness Engineering
created: 2026-05-17
updated: 2026-06-04
type: concept
tags:
  - methodology
  - agent
  - engineering
sources:
  - raw/articles/2026-05-07-anthropic-harness-guide-dead-weight.md
  - raw/articles/2026-05-07-effective-harnesses-for-long-running-agents.md
  - raw/articles/2026-05-11-harness-engineering-knowledge.md
  - raw/articles/2026-05-13-skill-engineering-design.md
  - raw/articles/2026-04-18-harness-engineering-source-code.md
  - raw/articles/2026-05-30-harness-research-reflection.md
  - raw/articles/2026-06-03-claude-code-dynamic-workflow-harness.md
  - raw/articles/2026-06-03-claude-workflow-harness-design-patterns.md
---

# Harness Engineering（脚手架工程）

## 定义

Harness Engineering 是构建引导和约束 AI 模型能力的脚手架工程的系统性方法论。它是 2026 年最热门的 AI 工程话题，关注如何通过结构化的约束和引导，让 AI 模型在实际工程场景中发挥最大价值。

## 核心理念

> **给 AI 搭的每一层脚手架都应有到期日。**

脚手架的本质是辅助——当模型能力足够强时，脚手架就应该被拆除。

## Dead Weight 现象

为解决旧模型问题而建设的基础设施，在模型能力提升后反而成为拖累。这种现象被称为 **Dead Weight**：

- 旧模型需要复杂的 prompt 模板来引导输出
- 新模型已经能自主完成这些任务
- 但旧的基础设施仍然存在，消耗维护成本和推理 token

## Build to Delete 原则

定期审视每一层 Harness，判断模型是否已能自主完成该 Harness 覆盖的任务：

1. 记录每层 Harness 的存在理由
2. 定期（如每次模型升级后）重新评估
3. 确认模型可自主完成时，果断拆除

## 三支柱框架

### 1. 上下文工程（Context Engineering）

管理输入给模型的信息的质量和数量：

- **Progressive Disclosure**：渐进式披露，按需提供上下文
- **Context Editing**：上下文编辑，移除无关信息
- **Compaction**：压缩，将冗长上下文压缩为精炼摘要

### 2. 架构约束（Architectural Constraints）

通过系统设计约束模型行为：

- **Memory Folder**：结构化的记忆存储
- **[[skills]]**：封装确定性操作为可复用 Skill
- **Subagent**：通过子代理隔离不同关注点

### 3. 持续治理（Continuous Governance）

持续监控和优化 Harness 的有效性：

- **Lint**：自动化检查输出质量
- **知识引用追踪**：确保模型输出可追溯到知识来源

## Sprint 分解案例

在 Opus 4.5 时代，将复杂任务分解为多个 Sprint 是有效的 Harness。但 Opus 4.6 后，模型已能处理更长的任务，Sprint 分解反而成为 Dead Weight。拆掉这层 Harness 可以节省约 **37%** 的成本。

## 腾讯技术工程实践

腾讯在 Harness Engineering 方面提出了系统化的实践方案：

### 五层知识存储

从底层基础设施到顶层应用的知识分层存储架构。

### 三级成熟度 + 自动衰减

- **draft** → **verified** → **proven**
- 知识条目随时间自动衰减成熟度，促使团队定期验证

### 三级渐进式索引

逐级增加索引粒度，平衡检索效率和存储成本。

## Skill 工程化设计（pangu-cli）

pangu-cli 体现了 Skill 工程化的设计理念：

- **CLI 接管确定性**：将确定性操作交给 CLI，模型只处理不确定性
- **Workflow 工作流引擎**：定义清晰的执行流程
- **步进式披露**：按步骤向模型提供所需信息
- **Gate 门禁**：在关键节点设置检查点

## 长程 Agent Harness

针对长时间运行的 Agent 任务（详见 [[long-running-agent]]）：

- **Initializer Agent + Coding Agent 模式**：一个 Agent 负责初始化和规划，另一个负责编码实现
- **Feature List 防提前宣布完成**：通过功能清单确保所有功能都已完成，防止 Agent 过早宣布任务结束
- **增量进度原则**：每次只做一个 feature，做完 git commit + 更新 progress file
- **端到端测试**：让 Agent 像人类用户一样测试，而非仅用单元测试

### 宝可梦实验：记忆能力的代际差异

Sonnet 3.5 把 memory 当记录仪用，14000 步攒 31 个文件还在第二个城镇。Opus 4.6 同样步数只有 10 个分类文件，已拿到 3 个道馆徽章，还写了"从失败中提炼教训"文件。

**模型在「选择记什么」上的进步，直接决定了长时任务能走多远。**

## Big Model vs Big Harness

一个核心的战略判断：

- **知识工程投入**是确定性回报——投入多少，收益多少
- **模型能力提升**是概率性回报——你无法控制模型何时变强

因此，在当前阶段，建设好 Harness 和知识工程基础设施，是最稳健的投资策略。

## 六大工程支柱（来自 Claude Code 源码分析）

综合 OpenAI、某头部 AI 实验室、Martin Fowler、LangChain、Latent Space 和 Cassie Kozyrkov 六方文献，结合 [[claude-code]] 512K 行源码分析：

### 支柱一：上下文架构 🗺️

精准设计进入模型上下文的信息。研究表明上下文窗口利用率超过 40% 时模型推理质量显著下滑。[[claude-code]] 构建了完整的四级压缩管道（[[context-compression-pipeline]]）。

### 支柱二：架构约束 ⛓️

用代码和工具强制执行规则，而非依赖 prompt 的「软约束」。`buildTool()` 工厂函数的 Fail-Closed 默认值是精髓：忘了设置就走最受限路径，遗漏不是漏洞。

### 支柱三：自验证循环 🔄

在执行流程中内置验证检查点。[[claude-code]] 的 `query()` 循环 16 个步骤中只有 1 个是「调用模型」，其余 15 个全是验证和修复逻辑。`transition` 字段让每次循环迭代的「为什么继续」变成可断言的数据。

### 支柱四：上下文隔离 🧊

多 Agent 协作时保持每个 Agent 上下文纯净。三层隔离：进程级隔离（子 Agent 独立上下文）、通信接口化（结构化消息而非共享原始上下文）、Coordinator 模式（控制面/数据面分离）。

### 支柱五：熵治理 ♻️

对抗系统状态的自然熵增。AutoDream 梦境系统是自动化熵治理引擎——借鉴认知科学的记忆巩固理论，让 AI 在「空闲时」整理和巩固记忆。

### 支柱六：可拆卸性 🔌

模块化设计使 Harness 能随模型迭代优雅适配。`QueryDeps` 依赖注入、Skills = Markdown、[[mcp]] 标准协议、模型降级容错——防止与特定模型深度耦合。

### 关键数据

在 [[claude-code]] 的 512K 行代码中，模型调用相关的代码不到 **5%**，剩下 95% 全部是 Harness。这是 Harness Engineering 核心论点的最强证据：**AI Agent 的瓶颈从来不在模型智能，而在基础设施。**

## Dynamic Workflow 六种 Harness 编排模式

2026 年 5 月，[[claude-code]] 推出的 [[claude-code-dynamic-workflow|Dynamic Workflow]] 功能是 Harness Engineering 思想在 Agent 编排层的直接落地。[[thariq-shihipar|Thariq Shihipar]]（Anthropic）在官方博客中系统阐述了六种编排模式，鲁工（九年 AI 算法老兵）从工程视角做了深度分析。

### 模式详解

| 模式 | 机制 | 解决什么问题 | 适用场景 |
|------|------|-------------|---------|
| **Fan-out-and-Synthesize**（扇出汇总） | 大任务拆成小步，各 Agent 并行执行，汇总 Agent 等所有分支完成后合并（栅栏 barrier） | 任务可分解但各子任务独立 | 最常用；深度研究、代码审查、规模化检查 |
| **Adversarial Verification**（对抗核查） | 每开一个干活 Agent，就再开一个挑刺 Agent 拿评分标准独立校验 | 自我偏袒（Self-preferential Bias） | **最该优先用**；代码审查、引用验证、安全审计 |
| **Classify-and-Act**（分类路由） | 分类 Agent 判断任务类型 → 路由到对应处理 Agent | 任务类型异构，需差异化处理 | 模型路由（简单→Sonnet，复杂→Opus）、工单分诊 |
| **Generate-and-Filter**（生成过滤） | 放开了生成一批候选 → 规则筛、去重 → 留下经得起验证的 | 需要广度再收敛的场景 | 创意、设计命名、方案探索 |
| **Tournament**（锦标赛） | N 个 Agent 不同思路干同一件事 → 裁判两两 PK → 淘汰到剩一个 | 模型不擅长打绝对分数，但擅长两两比较 | 品味判断、方案选优、排序 |
| **Loop Until Done**（循环至终） | 工作量不确定就不定死轮数，持续启动 Agent 直到连续几轮无新发现 | 工作量不确定的开放式任务 | 安全审计、全面代码审查、根因分析 |

### 隔离区模式（Isolation Zone）

鲁工特别强调：当 Agent 需要读取不可信内容时，将该 Agent 放入隔离区，**禁用高权限操作**（文件写入、网络调用）。读不可信内容的 Agent 不能有写权限——这是对抗供应链攻击的基础防线，也是架构约束（Architectural Constraints）在 Workflow 层的具体体现。

### 核心洞察

> 「过去大家拼的是模型单点多聪明，往后更拼你会不会给手头这个任务，现写一套配得上它的 harness。」——鲁工

Workflow 用 token 换可靠性、对抗性和并发规模。模型智能进化能替代一部分 harness，但 harness 本身对模型的加成仍然非常有效。这六种编排模式不是靠更强的模型，而是靠更聪明的编排让现有模型的能力发挥到极致。

## 行业共识：CMU/Yale Harness 综述

2026 年 5 月，CMU/Yale 等机构发布了 Harness Engineering 综述论文，标志着行业共识的正式转移——大模型 Agent 的可靠性，绝不能再只盯着模型本身。过去建立在"参数越大越聪明、上下文越长越能处理复杂任务"的线性外推已被证伪。

### Harness 之后：State-Aware Runtime

[[chen-xiwei|陈希伟]]（Datawhale 独立研究者）在综述基础上提出了 [[state-aware-runtime|State-Aware Runtime]] 框架，指出 Harness 解决了"组件构成"的静态问题，但更致命的是"组件如何共同维护运行状态"的动态问题。核心主张：**严格区分候选输出与已提交状态**、长上下文不等于长期状态管理、Trace-Native Evaluation、建立失败分类学。

## 相关链接

- [[claude-code]] — Harness Engineering 的主要实践平台
- [[anthropic]] — Anthropic 官方发布的 Harness 指南
- [[context-engineering]] — 上下文工程是 Harness Engineering 的核心支柱
- [[context-compression-pipeline]] — 上下文架构支柱的核心实现
- [[skills]] — Skill 是架构约束的重要手段
- [[mcp]] — MCP 协议为 Harness 提供工具调用基础设施
- [[claude-code-hooks]] — 架构约束的确定性实现
- [[state-aware-runtime]] — Harness Engineering 的下一阶段演进
- [[chen-xiwei]] — State-Aware Runtime 概念提出者
