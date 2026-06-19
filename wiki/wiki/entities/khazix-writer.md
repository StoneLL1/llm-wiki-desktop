---
title: khazix-writer（卡兹克风格创作 Skill）
created: 2026-05-23
updated: 2026-06-04
type: entity
tags: [person, open-source, skill]
sources:
  - raw/articles/2026-04-18-kazike-creative-skill-open-source.md
  - raw/articles/2026-06-03-khazix-mac-cleaner-skill.md
---

# khazix-writer（卡兹克风格创作 Skill）

## Overview

khazix-writer 是 AI 博主「数字生命卡兹克」开源的个人内容创作 [[skills|Skill]]，将其三年公众号写作的全部方法论、踩坑经验和风格规则蒸馏为一个 SKILL.md。开源在 [GitHub 仓库](https://github.com/KKKKhazix/khazix-skills)。

## 核心内容

### 风格规则

- 什么角度切入是卡兹克风格
- 什么词和标点绝对不用
- 开头怎么切入、节奏怎么控
- 如何分享才不会显得「爹味」重

### 四层自检体系

借鉴 Anthropic 代码质量评估体系的思路，将「反幻觉」制度化为不可跳过的质量门：

| 层级 | 名称 | 检查内容 |
|------|------|----------|
| 第一层 | 硬性规则扫描 | 禁用词、禁用标点、结构性套话（类似代码语法检查） |
| 第二层 | 风格一致性检查 | 开头是否从具体场景切入、长短句交替、口语化表达、一句话独立成段 |
| 第三层 | 内容质量检查 | 每个观点有具体场景支撑、知识以聊天感呈现、连接更大文化参照 |
| 第四层 | 活人感终审 | 最主观也最重要——读完后感觉是一个有见识的普通人在认真跟你聊，还是 AI 在输出信息 |

## 推荐使用方式

- **最佳**：Claude Chat 模式 + Opus 4.6
- 其次：Cowork + Opus 4.6
- 再次：Claude Code + Opus 4.6

事实核查建议：将 Opus 4.6 产出扔给 Codex + GPT-5.4 检测事实不符（GPT-5.4 的幻觉率极低）。

## Skill 迭代方法论

卡兹克提出了一个通用的创作 Skill 构建流程，比 Skill 本身更重要（「渔」而非「鱼」）：

1. **初版蒸馏**：扔 2-3 篇代表性文章 + 方法论白皮书，让 AI 总结为初始 Skill
2. **AI 试写**：按当前 Skill 生成一篇文章（几乎不可能直接可用）
3. **人工重写**：在 AI 版本基础上动手改，保证主题和观点一致
4. **差异分析**：将 AI 版本和人工重写版本同时丢给 AI，分析差异（哪里太正经、用了禁用词、节奏太平），迭代回 Skill
5. **重复 3-4 轮**（不建议超过 4 轮，容易过拟合）

关键原则：
- AI 能生成 70-80% 可用的内容就已经是上天善心了
- 不要指望 AI 比你自己还稳定——活人感来自于不完美
- 最后一轮的「神之一手」（跨领域的灵感嫁接）是 AI 永远无法复现的

## AI 辅助创作的边界

卡兹克明确区分了「AI 辅助创作」与「AI 替你写完」：
- **AI 做不了**：实测内容、教程内容（必须亲自体验）
- **AI 辅助加速**：找证据写观点、找类比和比喻、确定角度框架后的扩写
- AI 找的是弹药，但开枪的方向是自己选的

## Relationships

- 属于 [[skills]] 生态的一个具体实现
- 与 [[hv-analysis]] 同一作者不同工具
- 自检体系灵感来自 [[anthropic]] 代码质量评估体系
- 产出后用 Codex (GPT-5.4) 做事实核查

## See Also

- [[skills]] — Skill 的定义和加载机制
- [[claude-code]] — Skill 的运行平台
- [[hv-analysis]] — 同作者的研究方法论 Skill
- [[khazix-mac-cleaner]] — 同作者（同一 GitHub 仓库）的存储清理 Skill
- [[stop-slop]] — 另一个反 AI 味的写作工具
- [[anti-slop-writing]] — 反 AI 写作风格的综合概念


## 开源背景与创作哲学

卡兹克（数字生命卡兹克）于 2026 年 4 月开源此 Skill，核心动机：
- 互联网开源精神回归的时代
- 「以一灯传至诸灯，终至万灯皆明」

**推荐使用方式**：
- 最佳产出：Claude Chat 模式 + Opus 4.6
- 其次：Cowork + Opus 4.6 → Claude Code + Opus 4.6 → Claude Code + K2.5
- 幻觉处理：不强行抑制幻觉（幻觉是创意前提），用 Codex + GPT-5.4 检测事实不符

**关于 AI 辅助创作的立场**：
- AI 辅助创作 ≠ AI 替你写完
- AI 找弹药，但开枪方向是你选的
- 凌晨 2:30 抬头看书架的那一秒钟 → 独属于人类的「神之一手」

GitHub: https://github.com/KKKKhazix/khazix-skills

### Sources
- raw/articles/2026-04-18-kazike-creative-skill-open-source.md
