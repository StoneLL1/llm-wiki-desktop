---
title: AI Slop Design
created: 2026-05-23
updated: 2026-05-23
type: concept
tags: [design, prompt-engineering, methodology]
sources:
  - raw/articles/2026-04-19-claude-design-system-prompt-leak-analysis.md
  - raw/articles/2026-04-19-claude-design-system-prompt-bilingual.md
---

# AI Slop Design（AI 味设计）

## Definition

AI Slop Design（AI 土味设计）是指 AI 生成 UI/视觉设计时反复出现的可识别视觉模式。这些模式源于训练数据的统计偏好，形成了一套"AI 设计指纹"，让生成的设计一眼就能被识别为 AI 作品。

与 [[anti-slop-writing]]（文本领域的 AI 味）相对应，AI Slop Design 是视觉/设计领域的同等问题。

## Claude Design 的反 AI 味清单

[[claude-design]] 泄露的系统提示词中，Anthropic 显式列出了 AI slop tropes 的黑名单：

### 禁止项

| AI 味模式 | 问题 | 替代方案 |
|-----------|------|---------|
| 渐变背景滥用 | AI 最爱用的视觉填充，缺乏设计意图 | 使用品牌色系/oklch 配色 |
| Emoji 滥用 | 除非品牌要求，否则显得不专业 | 仅在品牌系统使用时才用 |
| 圆角 + 左边框强调色 | AI 生成 UI 最经典的视觉指纹 | 使用更有创意的容器样式 |
| SVG 自绘插图 | AI 画的 SVG 图标质量低，反而更显眼 | 用占位符，不用勉强假图 |
| Inter/Roboto/Arial 字体 | 过度使用的默认字体，缺乏品牌个性 | 选择有辨识度的字体 |

### 核心原则

> **占位符优于垃圾的实现。**

没素材就用空白框，没图标就用占位符。别试着用 SVG 画一个差不多的图标——那反而看起来更傻。

## 负面清单设计模式

Anthropic 在系统提示词中使用的策略是**负面清单**（negative prompting）：

1. 显式列出要避免的视觉套路
2. 用强约束词（MUST / NEVER / CRITICAL）强化
3. 提供正面替代方案（如用 oklch 配色代替随机颜色）

这种"告诉 AI 不要做什么"的方法是对抗训练语料回音的有效手段，也是 [[prompt-engineering]] 在设计领域的重要实践。

## 与 Anti-Slop Writing 的异同

| 维度 | Anti-Slop Writing | AI Slop Design |
|------|-------------------|----------------|
| 领域 | 文本/写作 | 视觉/UI 设计 |
| 检测难度 | 需要仔细阅读 | 一眼可识别 |
| 主要模式 | 冗长对冲、假平衡、热情膨胀 | 渐变、emoji、圆角+边框 |
| 解法 | stop-slop 规则集 | 负面清单 + 设计系统约束 |
| 共同策略 | 强约束词 + 负面示例 | 强约束词 + 负面清单 |

## 在设计系统中的应用

避免 AI Slop Design 的最佳实践：

1. **建立设计系统** — 让 AI 基于 UI Kit、品牌文件、现有代码工作，不从零开始
2. **使用 DESIGN.md** — 通过 [[design-md]] 定义品牌色板、字体、间距规范
3. **提供参考** — 给 AI 真实的设计参考，而非让它自由发挥
4. **要求变体** — 3+ 个从保守到创意的变体，避免 AI 默认输出中间态

## Open Questions

- 随着 AI 视觉生成能力提升，slop patterns 会自然消失还是演化出新形态？
- 是否存在客观的"AI 设计质量"评估标准？
- 反 AI 味设计是否会形成新的可识别模板？

## See Also

- [[claude-design]] — 系统性地对抗 AI Slop Design 的产品
- [[anti-slop-writing]] — 文本领域的 AI 味对抗
- [[prompt-engineering]] — 负面清单属于提示词工程策略
- [[vibe-design]] — AI 原生设计范式需要解决 slop 问题
- [[design-md]] — 通过设计规范约束 AI 输出
