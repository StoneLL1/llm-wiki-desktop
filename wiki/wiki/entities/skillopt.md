---
title: SkillOpt
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [methodology, training, engineering, open-source, agent]
sources:
  - raw/articles/2026-05-26-skillopt-microsoft-train-skill-like-nn.md
---

# SkillOpt

## Overview

**SkillOpt** 是 Microsoft Research 提出的自动化 Skill 文档优化方法。核心思路：将 Skill 文档视为神经网络的「权重」，用类似训练神经网络的方式（rollout → reflection → edit → validation gating）自动优化，而非依赖人工编写。

论文：arxiv.org/abs/2605.23904 | 开源：github.com/microsoft/SkillOpt（MIT 协议）

## 核心机制

SkillOpt 将深度学习训练循环一比一映射到文本编辑空间：

| 深度学习概念 | SkillOpt 对应 |
|-------------|--------------|
| 前向传播 (forward pass) | **Rollout**：Agent 带当前 Skill 做任务，收集完成情况 |
| 梯度计算 (gradient) | **Reflection**：优化器模型分析失败原因，提炼改进方向 |
| 权重更新 (weight update) | **Edit**：对 Skill 文档做 add/delete/replace 三种结构化编辑 |
| 学习率 (learning rate) | **Textual learning rate**：每轮最多改 L_t 条规则（默认 4），有 cosine decay |
| Checkpoint/验证集 | **Validation gating**：改完在验证集上验证，没涨分则拒绝修改 |

### 两个模型分工

- **Target model**：执行任务的 Agent（如 GPT-5.5、Claude），模型参数冻结
- **Optimizer model**：分析 target model 表现并提出修改建议的更强模型

关键优势：optimizer model 成本仅在训练阶段产生，部署时零额外开销。同级别优化器也能工作（恢复强优化器 56%-74% 增益）。

### 克制机制

- **每轮最多改 4 条规则**（L_t=4）：无限制重写反而差 2-3 分
- **Rejected-edit buffer**：被否决的修改存入缓冲区，供后续 reflection 参考
- **Slow/meta update**：类 momentum 的跨 epoch 纵向更新，受保护不可被 step 级编辑覆盖。去掉此机制 SpreadsheetBench 从 77.5 暴跌到 55.0（-22.5 分）

## 效果数据

6 个 benchmark 测试（GPT-5.5 直接对话）：

| Benchmark | 基线 → SkillOpt | 提升 |
|-----------|----------------|------|
| SearchQA | 77.7 → 87.3 | +9.6 |
| SpreadsheetBench | 41.8 → 80.7 | +39.0 |
| OfficeQA | 33.1 → 72.1 | +39.0 |
| DocVQA | 78.8 → 91.2 | +12.4 |
| LiveMath | 37.6 → 66.9 | +29.3 |
| ALFWorld | 83.6 → 95.5 | +11.9 |

**52 个测试格全部最优或并列最优，平均 +23.5 分。** Codex 环境 +24.8 分，Claude Code 环境 +19.1 分。

## 跨模型/跨环境迁移

优化出的 Skill 可跨模型、跨执行环境、跨任务迁移：

- GPT-5.4 → GPT-5.4-mini（SpreadsheetBench）：+9.4
- Codex → Claude Code（SpreadsheetBench）：+59.7
- OlympiadBench → Omni-MATH（GPT-5.4）：+3.7

所有规模模型（GPT-5.5 到 Qwen3.5-4B）均有一致提升，小模型配优化 Skill 可超越大模型裸跑。

## 训练成本

- 流程类 benchmark：每提升 1 分需 0.6-3.6M 训练 token
- 复杂轨迹类 benchmark：每提升 1 分需 37.9-46.4M token
- 训练一次性完成，部署零额外成本

## 使用方式

```bash
git clone https://github.com/microsoft/SkillOpt.git
pip install -e .
python scripts/train.py \
    --config configs/searchqa/default.yaml \
    --optimizer_model gpt-5.5 \
    --target_model gpt-5.5 \
    --num_epochs 4 --batch_size 40
```

输出 `best_skill.md` 即可直接使用。支持 WebUI 监控、断点续训。支持 OpenAI、Azure OpenAI、Anthropic Claude API。

## 局限性

需要任务有可自动评估的标准（exact match 或自动评分器），开放性任务暂不适用。

## 与其他方法的对比

SkillOpt 碾压所有基线：One-shot LLM 生成、Trace2Skill（轨迹蒸馏）、TextGrad（梯度风格优化）、GEPA（Pareto 反射演化）、EvoSkill（技能文件夹演化），以及人类手写 Skill。

## 相关链接

- 论文：https://arxiv.org/abs/2605.23904
- 项目主页：https://microsoft.github.io/SkillOpt/
- GitHub：https://github.com/microsoft/SkillOpt
- 相关项目 SkillLens：https://microsoft.github.io/SkillLens/

## Relationships

- 深化 [[skill-engineering]] 理论——从手写 Skill 进化到自动化优化 Skill
- 可优化 [[claude-code]]、[[openai-codex]] 等 Agent 的 [[claude-md|Skill 文档]]
- 补充 [[skills]] 的创建方法论（自动化 vs 手动迭代）
- 与 [[harness-engineering]] 同属 Agent 能力工程化方向
