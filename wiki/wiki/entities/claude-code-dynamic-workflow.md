---
title: Claude Code Dynamic Workflow
created: 2026-05-30
updated: 2026-06-05
type: entity
tags: [tool, agent, multi-agent, workflow, engineering]
sources:
  - raw/articles/2026-05-29-claude-opus-48-dynamic-workflow一次性并行上百个subagents.md
  - raw/articles/2026-06-03-claude-code-dynamic-workflow-harness.md
  - raw/articles/2026-06-03-claude-workflow-harness-design-patterns.md
  - raw/articles/2026-06-04-claude-code-boris-write-loops-not-prompts.md
---

# Claude Code Dynamic Workflow

## 概述

Claude Code Dynamic Workflow 是 [[anthropic]] 工程师 **Thariq Shihipar**（与 Sid）在 2026 年 5 月发布的研究预览功能（需 Claude Code v2.1.154+）。它让 Claude 针对具体任务实时编写 JavaScript 编排脚本，在后台并行启动几十到上百个独立 subagent，交叉验证后汇总结果。这是 [[claude-code]] 从「单 Agent 逐一决策」到「脚本化大规模 Agent 并行」的范式跃迁。

在 Claude Code 语境中，**harness（脚手架）就是编排框架**——决定了 Claude 怎么拆解任务、怎么调度子 Agent、怎么验证结果。Workflow 让 Claude 可以实时写出编排脚本，启动一整支子 Agent 舰队并行作战。

## 为什么要设计 Workflow：单 Context 的三大顽疾

Thariq 在官方博客中指出，默认单 Agent 循环在长时间/大规模/对抗性任务上存在三个系统性问题：

1. **Agentic Laziness（偷懒）** — 做到一半就宣布「完成了」。例如安全审计列了 50 项检查，做到 20 项就停；50 项安全审查做到 35 项就说搞定。
2. **Self-preferential Bias（自我偏袒）** — 让自己审自己的产出，像学生自己批卷子，分数总是偏高。模型对自己的代码下不了狠手。
3. **Goal Drift（目标漂移）** — 多轮对话和上下文压缩（compact）后，原始目标的细节逐渐丢失。压缩越多次，最初的约束越模糊。

**Workflow 的解法**：每个子任务单独启动一个 Claude，各自拥有干净的 context window 和聚焦的单一目标。编排逻辑由确定性的 JavaScript 脚本控制——plan 搬进了代码，循环、分支、每个 Agent 的产出全在脚本变量里，脚本收敛后只给主 Claude 汇总结果。

## 静态 vs 动态 Workflow

| 维度 | 静态 | 动态 |
|------|------|------|
| 编写方式 | 人手动事先写好 | Claude 现场生成 |
| 适用性 | 通用或专用，难以兼顾 | 针对具体任务量身定制，既专用又通用 |
| 案例 | 固定的「搜索→取结果→验证→总结」 | 先读代码库 → 并行检查每个功能 → 按交易量算价格 → 启动 devil's advocate 论证相反观点 |

## 核心 API（JavaScript）

- **`agent()`** — 启动子 Agent，可指定 schema（结构化 JSON 输出）、model、isolation（隔离 worktree）
- **`parallel()`** — 并行执行多个 agent，全部完成才返回（栅栏 barrier 机制）
- **`pipeline()`** — 流水线模式，每个 item 独立穿过所有阶段

## 与 Subagents / Agent Teams 的区别

| 维度 | Subagents / Agent Teams | Dynamic Workflow |
|------|------------------------|-------------------|
| 谁持有计划 | Claude 逐一决策每一轮 | 计划编码为 JS 脚本 |
| 规模 | 少数几个 Agent | 几十到上百个 Agent |
| 质量套路 | 单趟执行 | 多角度解题 + 专门挑刺 Agent 交叉验证 |
| 上下文压力 | 每个 Agent 产出回传 Claude | 脚本收敛后只给 Claude 汇总结果 |

## 触发方式

1. **Prompt 关键词**：在 prompt 中带上 `workflow` 一词（变色高亮），Claude 自动为任务编写工作流
2. **Ultracode 模式**：`/effort ultracode` 开启高强度推理，Claude 自动判断哪些任务值得开工作流。该模式下 Claude 拿到任务不再急着上手，先盘算能否拆成工作流、分几个阶段、如何编排。代价是 token 和时间消耗显著增加。

## 六种编排模式

### 1. Fan-out-and-Synthesize（扇出汇总）— 最常用

大任务切成小步，各 Agent 并行执行，汇总 Agent 等所有分支完成后再合并（栅栏 barrier 机制）。经典案例：`/deep-research` 一个请求跑 111 个 Agent（Scope 1 + Search 6 + Fetch 28 + Verify 75 + Synthesize 1），Verify 阶段每个结论由 3 个独立 Agent（v0/v1/v2）交叉验证+投票。

### 2. Adversarial Verification（对抗核查）— 最该优先用

每开一个干活 Agent，就再开一个挑刺 Agent 拿评分标准校验。直接解决自我偏袒问题。鲁工实测跑文献综述引用验证，假引用基本全揪出来。代码审查中尤其实用：扇出（bug/性能/安全三个维度）→ 每个发现对抗验证 → 汇总。

### 3. Classify-and-Act（分类路由）

分类 Agent 判断任务类型 → 路由到对应处理 Agent。适合模型路由场景：分类 Agent 判断复杂度 → 简单任务走 Sonnet，复杂任务走 Opus。

### 4. Generate-and-Filter（生成过滤）

放开了生成一批候选 → 按规则筛选、去重 → 留下经得起验证的。适合需要广度的创意/设计/命名场景。

### 5. Tournament（锦标赛）

N 个 Agent 从不同思路解决同一问题 → 裁判两两 PK → 层层淘汰到剩一个。**两两比较比让模型打绝对分靠谱得多**——模型不擅长打绝对分数，但擅长判断「A 是否比 B 好」。

### 6. Loop Until Done（循环至终）

工作量不确定就不定死轮数，持续启动 Agent 直到连续几轮无新发现才停。配合 `/loop` 定时驱动，适合安全审计、全面代码审查等「不确定要做多少」的脏活累活。

以上模式可自由组合。如代码审查场景：扇出三路（bug/性能/安全）→ 每路发现对抗验证 → 汇总 Agent 合并。

## 十种应用场景

| 场景 | 说明 |
|------|------|
| 迁移重构 | Bun 从 Zig→Rust 重写即用 Workflow 完成（75 万行，11 天，99.8% 测试通过） |
| 深度研究 | `/deep-research` 本身就是 Workflow 实现 |
| 深度验证 | 提取声明 → 独立核查 → 审计信源质量 |
| 排序 | 锦标赛模式：一对一比较，确定性循环控制 |
| 规则遵从 | 每条规则分配验证 Agent + 怀疑者过滤误报 |
| 根因分析 | 不同 Agent 从互不相关证据独立生成假设 |
| 规模化分诊 | 分类工单 → 去重 → 决定修复还是上报 |
| 探索品味 | 设计/命名等品味判断：广泛探索 → 按 rubric 评判 |
| 评估 | 不同变体丢独立 worktree 运行比较 |
| 模型路由 | 分类 Agent 判断复杂度 → 路由到 Sonnet 或 Opus |

## 隔离区模式（Isolation Zone）

鲁工特别强调一个关键安全模式：当 Agent 需要读取不可信内容（如用户提交的代码、外部传入的数据）时，将该 Agent 放入隔离区，禁用高权限操作（文件写入、网络调用等）。**读不可信内容的 Agent 不能有写权限**——这是对抗供应链攻击等场景的基础防线。

## 什么时候不要用

> 「常规写代码，动手前先问：这活真需要更多算力吗？大部分传统编码任务不需要五个 reviewer 组团。」

Dramic Workflow 非常消耗 token——鲁工深度使用时时不时触发五小时 limit。Workflow 的本质是**用 token 换可靠性、对抗性和并发规模**。常规编程任务单 Agent 循环足够。

## 实用技巧

- **Prompt 写详细**：用编排模式名称引导 Claude（如 "Use fan-out-and-synthesize"）。小事也适合——开个 quick workflow 做一次快速对抗复查。
- **配合 `/goal` 和 `/loop`**：定期执行 + 硬性完成标准，特别适合 Loop Until Done 模式。
- **控制 token 预算**：prompt 中直接说「用 10k token」封顶消耗。
- **保存分享**：满意的流程按 `s` 保存到 `~/.claude/workflows` 或作为 [[skills|Skill]] 分发。

## 鲁工核心洞察：Harness 成为新的竞争分水岭

> 「过去大家拼的是模型单点多聪明，往后更拼你会不会给手头这个任务，现写一套配得上它的 harness。」

Harness Engineering 是 2026 年的主旋律。模型智能进化能替代一部分 harness，但 harness 本身对模型的加成仍然非常有效。Dynamic Workflow 的六种编排模式是 Harness Engineering 思想在 Agent 编排层的直接落地——不是靠更强的模型，而是靠更聪明的编排，让现有模型的能力发挥到极致。

在 [[claude-code]] 的 512K 行代码中，模型调用相关的代码不到 5%，剩下 95% 全部是 Harness。这是 Harness Engineering 核心论点的最强证据。

## 内置 Workflow：/deep-research 实测

| 阶段 | Agent 数量 | 职责 |
|------|-----------|------|
| Scope（范围确定）| 1 | 确定研究范围 |
| Search（搜索）| 6 | 多角度并行搜索 |
| Fetch（抓取）| 28 | 抓取搜索结果 |
| Verify（核查）| 75 | 25 条结论 × 3 个独立 Agent（v0/v1/v2）交叉验证+投票 |
| Synthesize（汇总）| 1 | 汇总最终报告 |

**总计 111 个 Agent**。核查阶段大量 Agent 崩溃（StructuredOutput 工具调用失败），但 [[claude-opus-48]] 的诚实度改进发挥了作用：模型主动分析后将报告拆为三档——已核实的、未核实但大概率为真、被明确反驳。

## 当前限制

- **研究预览阶段**：功能尚不稳定，Agent 批量崩溃偶发
- **Token 消耗极高**：Max 20x 订阅随便跑几个 workflow 即触发 5 小时限额
- **建议**：适合长任务场景，不适合来回对话式抠细节

## 相关页面

- [[claude-code]] — Claude Code 编程 Agent，Dynamic Workflow 的运行平台
- [[claude-opus-48]] — 同步发布的旗舰模型，诚实度 4× 提升
- [[harness-engineering]] — Harness Engineering 方法论，Dynamic Workflow 是其编排层落地
- [[claude-code-hooks]] — 确定性行为控制，与 Workflow 互补
- [[agent-teams]] — Claude Code 多 Agent 团队功能，Dynamic Workflow 的演进前身
- [[lance-martin]] — Anthropic 工程师，Harness 三原则提出者
- [[thariq-shihipar]] — Dynamic Workflow 官方博客作者
- [[multi-agent-collaboration]] — 多 Agent 协作概念
- [[anthropic]] — Anthropic 公司
