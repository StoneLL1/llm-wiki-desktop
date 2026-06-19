---
title: "Claude Code Workflow 必读：面向所有任务的 harness"
url: "https://mp.weixin.qq.com/s/hxBkT-iJleQkaODzjWVC2A"
source: "微信公众号"
author: "AGI Hunt"
account: "AGI Hunt"
pub_date: 2026-06-03
fetched: 2026-06-03
category: "tip"
---

# Claude Code Workflow 必读：面向所有任务的 harness

**作者**: AGI Hunt | **公众号**: AGI Hunt

## 概述

Anthropic 工程师 Thariq（和 Sid）发布官方长文介绍 Dynamic Workflow 功能。在 Claude Code 语境中，harness 就是「编排框架」——决定了 Claude 怎么拆解任务、怎么调度子 Agent、怎么验证结果。

Workflow 让 Claude 可以实时写出编排脚本，启动一整支子 Agent 舰队并行作战。

## 三个顽疾（单 Context 的瓶颈）

Thariq 指出默认单 Agent 循环在长时间/大规模/对抗性任务上的三大问题：

1. **Agent 偷懒** — 做到一半就宣布「完成了」，比如安全审计查 50 条，做 20 条就停
2. **自我偏好** — 让自己的代码自己审，像学生自己批卷子，分数总是偏高
3. **目标漂移** — 多轮对话/上下文压缩后，原始目标的细节逐渐丢失

**Workflow 的解法**: 每个子任务单独启动一个 Claude，各自拥有干净的 context window 和聚焦的目标。编排逻辑由确定性的 JavaScript 脚本控制。

## 静态 vs 动态 Workflow

| 维度 | 静态 | 动态 |
|------|------|------|
| 编写方式 | 人事先写好 | Claude 现场生成 |
| 适用性 | 通用或专用，难以兼顾 | 针对具体任务量身定制，既专用又通用 |
| 案例 | 固定的「搜索→取结果→验证→总结」 | 先读你的代码库 → 并行检查每个功能 → 按交易量算价格 → 启动 devil's advocate 论证不迁移理由 |

## 核心 API（JavaScript）

- `agent()` — 启动子 Agent，可指定 schema（结构化 JSON）、model、isolation（worktree）
- `parallel()` — 并行执行，全部完成再返回
- `pipeline()` — 流水线，每个 item 独立穿过所有阶段

## 六种编排模式

1. **分类-执行**: 分类 Agent 判断类型 → 路由到不同处理 Agent
2. **扇出-汇总**: 拆成小步骤并行 → 汇总 Agent 合并结果
3. **对抗验证**: 执行 Agent 输出 → 独立 Agent 做对抗性审查
4. **生成-过滤**: 多 Agent 并行生成 → 按标准过滤去重
5. **锦标赛**: N 个 Agent 各自解同一问题 → 配对评审层层淘汰
6. **循环至终**: 持续启动 Agent 直到连续几轮无新发现

模式可自由组合。如代码审查：扇出（bug/性能/安全）→ 每个发现对抗验证 → 汇总。

## 十种应用场景

| 场景 | 说明 |
|------|------|
| 迁移重构 | Bun 从 Zig 到 Rust 重写即用 Workflow 完成 |
| 深度研究 | `/deep-research` 就是 Workflow 实现的 |
| 深度验证 | 提取声明 → 独立核查 → 审计信源质量 |
| 排序 | 锦标赛模式：一对一比较，确定性循环控制 |
| 规则遵从 | 每条规则分配验证 Agent + 怀疑者过滤误报 |
| 根因分析 | 不同 Agent 从互不相关证据独立生成假设 |
| 规模化分诊 | 分类工单 → 去重 → 决定修复还是上报 |
| 探索品味 | 设计/命名等品味判断：广泛探索 → 按 rubric 评判 |
| 评估 | 不同变体丢独立 worktree 运行比较 |
| 模型路由 | 分类 Agent 判断复杂度 → 路由到 Sonnet 或 Opus |

## 非技术任务的惊喜

Thariq 特别指出 Workflow 对非技术任务也许更有惊喜——方案选择涉及品味判断时跑锦标赛、优化 Skill/prompt 时跑 A/B 评估。

## 上手建议

- **Prompt 写详细**，用编排模式名称引导。小事也适合（对抗审查、小锦标赛选名字）
- **配合 /goal 和 /loop**：定期执行 + 硬性完成标准
- **控制 token 预算**：prompt 中直接说「用 10k token」
- **保存分享**：按 `s` 保存到 `~/.claude/workflows` 或 Skill 目录

## 克制使用

Thariq 强调：常规编程任务单 Agent 循环足够了。「这个任务真的需要更多算力吗？」——Workflow 是用 token 换可靠性、对抗性和并发规模。

## 相关链接

- Thariq 原文: https://claude.com/blog/a-harness-for-every-task-dynamic-workflows-in-claude-code
- Workflow 文档: https://code.claude.com/docs/en/workflows
- Cat Wu 介绍: https://x.com/_catwu/status/2060054180379689074
- ClaudeDevs 介绍: https://x.com/ClaudeDevs/status/2060044853279617150
