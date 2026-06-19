---
title: ConardLi（花园老师）
created: 2026-06-03
updated: 2026-06-03
type: entity
tags: [person, open-source, skill, design]
sources:
  - raw/GitHub/ConardLi-garden-skills.md
  - raw/articles/2026-06-02-garden-skills-7k-stars.md
---

# ConardLi（花园老师）

ConardLi 是「code秘密花园」微信公众号的主理人，[[garden-skills]] 开源项目的创建者。他专注于将 AI Agent 工作流沉淀为可复用的 [[skills]]，核心理念是**把一套可重复稳定工作的方法交给 Agent**。

## 核心贡献

### garden-skills
开源 Agent Skills 合集，包含 4 个 production-ready 的 Skill，上线后迅速获得 7K+ Star。仓库地址：https://github.com/ConardLi/garden-skills

### Skill 设计哲学

ConardLi 认为 Skill 的真正价值不在提示词写得多漂亮，而在于三点：

1. **明确的工作流程** — 什么时候该问、什么时候该做、什么时候该停下来让用户审查
2. **明确的质量标准** — 什么算好、什么算"AI 味太重"
3. **明确的迭代接口** — 不满意时该反馈什么、Agent 知道该改哪一层

他把 Agent 从"接到一个任务"升级为"启动一条生产线"——复杂产物需要的是生产线，不是单次指令。

### 最佳模型实践

ConardLi 实测指出，视频制作和网页设计类 Skill 对模型能力要求较高，**Opus 4.7** 是当前效果最好的选择。弱模型在审美判断、章节规划、代码实现和返工决策上差距明显。

## 关联项目

- [[garden-skills]] — 开源 Skills 合集
- 在线体验平台：https://mmh1.top/（部署在 Easy AI 上，含视频主题预览、设计风格预览、Image2 Playground）

## 相关领域

- [[skills]] — Agent Skills 体系
- [[skill-engineering]] — Skill 工程化设计方法论
- [[huashu-skills]] — 花叔 20 个内容创作 Skills 合集（同类型的 Skills 合集项目）
- [[skillopt]] — 微软 Skill 文档自动化优化方法
- [[claude-code]] — garden-skills 主要目标平台
