---
title: Lovart
created: 2026-05-23
updated: 2026-05-23
type: entity
tags: [tool, design, typography]
sources: [raw/articles/2026-04-18-lovart-brand-design-features.md]
---

# Lovart

## Overview

**Lovart**（lovart.ai）是一款 AI 原生品牌设计工具，针对一人公司和小型团队的品牌视觉工作流进行深度优化。它不是通用的 AI 图像生成器，而是围绕品牌设计师的真实工作流打磨的产品——从字体创建、品牌规范管理到物料批量生成和专业交付，形成完整闭环。

官网：https://www.lovart.ai/

## 核心功能

### Font Generator（字体生成器）

Lovart 内置字体生成功能，用户可以：
- 上传字体参考图，AI 根据视觉特征生成新字体
- 用文字描述补充方向（如"笔画收尾利落、字形略窄、整体有纵向延伸感"）
- 多轮对话微调，直到满意
- 生成的字体存入「My Fonts」库，后续所有物料自动复用

这个功能解决了品牌设计中"找不到对的那款字体"的常见痛点。

### Brand Kit（品牌套件）

Brand Kit 是 Lovart 的品牌规范管理中枢：
- 统一管理 Logo、品牌色板、字体
- 一键挂载到项目，所有物料自动遵循品牌规范
- 颜色不会飘、字体不会换、Logo 版本不会用错

挂载 Brand Kit 后，项目内所有设计默认在品牌约束下运行，消除了"每次出图前重新交代规范"的重复劳动。

### Create Skill（技能创建）

Lovart 最具差异化的功能之一。用户走通一个设计流程后，可以在 Skill Book 中将其保存为可复用的 Skill：
- Skill 保存的是对话中的 Prompt 框架和设计决策
- 下次做同类物料直接调用，无需从头试错
- 随使用积累，Skill 库存储的是设计师的经验和判断力，而非单纯的 AI 能力

这与 [[skills]] 范式中的 SKILL.md 理念异曲同工——将隐性经验显性化、可复用。

### PSD 导出

Lovart 支持将画布上的素材直接导出为 PSD 文件：
- 多张图对应独立图层
- 位置结构完整还原
- 可直接在 Photoshop 中继续精修

这个功能打通了 AI 生成与专业精修之间的断层——Lovart 出图不是终点，而是交付链条的起点。

### 文字编辑

画布中的图片支持文字提取和编辑，点击图片后通过菜单栏的"编辑文字"即可修改图中文字内容。

## 实际应用场景

文章作者以"AI寒武纪"品牌为例，完整演示了一人公司品牌设计工作流：

1. **造字体**：Font Generator 生成品牌专属字体，3 分钟出初版，微调后存入 My Fonts
2. **建 Brand Kit**：将 Logo、品牌色、字体整合为统一套件
3. **批量出物料**：公众号封面×3、课程主视觉×2、活动 KV，品牌感一致
4. **保存 Skill**：将验证过的设计流程存为「公众号品牌 KV 视觉」Skill
5. **导出 PSD**：需要精修的物料导出为分层 PSD

整个过程一人完成，以前需要一个小团队配合的工作量。

## 设计哲学

Lovart 的核心定位是**顺着品牌设计师的真实工作流去打磨**，而非提供孤立的 AI 功能。关键设计原则：

- **品牌一致性优先**：Brand Kit 锁住规范，物料批量输出不跑偏
- **经验可沉淀**：Skill 功能让设计师的判断力成为可复用的资产
- **交付闭环**：PSD 导出确保 AI 生成物能进入专业设计工具继续加工
- **人在回路**：AI 不替代判断，设计师决定方向、构图、取舍

## Relationships

- 与 [[hue]] 互补——hue 是 CLI 端品牌设计 Skill，Lovart 是独立的 AI 设计平台
- 与 [[stitch]] 竞争——Google 的 AI 设计工具，但 Lovart 更聚焦品牌设计垂类
- 与 [[claude-design]] 不同定位——Claude Design 偏对话式 UI 原型，Lovart 偏品牌物料批量生产
- Brand Kit 概念与 [[design-md]] 的设计系统理念相通

## See Also

- [[hue]] — CLI 品牌设计 Skill
- [[stitch]] — Google AI 设计平台
- [[design-md]] — 设计系统 Markdown 规范
- [[vibe-design]] — 自然语言驱动设计的范式
- [[logo-generator-skill|Logo 生成 Skill]]
