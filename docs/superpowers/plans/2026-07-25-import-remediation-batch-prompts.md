# Import 整改分批对话提示词

> 配套：`docs/superpowers/plans/2026-07-25-import-source-media-flow-remediation.md`（执行计划）
> 用法：每个 batch 开一个新对话，把「通用模板」里的 `{N}` 和 `{标题}` 换成对应批次，直接粘贴发送。
> 一个对话只做一个 batch。

---

## 通用模板（每次都用这段）

```text
先用 skills/llm-wiki-desktop-context 了解项目上下文。

本次只做 Batch {N}：{标题}

- 产品基准：docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md（唯一权威，冲突时以它为准）
- 执行计划：docs/superpowers/plans/2026-07-25-import-source-media-flow-remediation.md
  读全局约束（§1）、九条禁止的关闭方式（§1.1）、交付模板（§3），以及 Batch {N} 全节。
- 问题清单原文：docs/reviews/2026-07-25-import-source-media-flow-implementation-review.md

要求：
1. 先给一份详细实施计划，我回复「开始」之后才动代码。
2. 严格按计划里 Batch {N} 的范围做，超出范围的 finding 不要顺手改，发现问题就记下来告诉我。
3. 不要碰 UI-Frontend-design/ 和 wiki/wiki/。
4. 工作树是脏的，保留与本批无关的改动。
5. 收尾：npm run check 从头跑通 → 双子代理审查（A 共享上下文查逻辑与设计一致性，B 全新上下文查盲点）→ 修完所有有效问题 → 再跑一次 check → 追加 SPEC/progress.txt，有坑就追加 SPEC/gotchas.txt。
6. 最后按计划 §3 的十项模板逐条回答，并逐条确认 Batch {N} 的退出门槛。
```

---

## 各批次替换值

| N | 标题 | 关闭的 finding | 对应 Gate |
|---|---|---|---|
| 0 | 冻结反基准路线，建立新合同的编译期护栏 | P0-03、P0-07（+ 多条的删除部分） | — |
| 1 | 统一 Source finalization 与 URL 原子提交 | P0-01、P0-02、P1-01 | Gate A |
| 2 | Compile V2 bridge 与提交完成摘要 | P0-04、P1-10 | Gate D |
| 3 | 本地输入、格式 discovery 与 Source package | P1-02、P1-03、P1-04（路由） | Gate B 前半 |
| 4 | 统一 OCR、字幕、ASR 与媒体门禁 | P0-05、P1-08、P1-09、P1-04 | Gate B 后半 / C |
| 5 | Web/平台媒体、登录和远程原件策略 | P1-07、P1-12、P1-13、P2-08 | Gate C |
| 6 | Import 工作台、会话和批量效率 | P1-05、P1-06、P2-01~07、P2-10~12、P3-01 | Gate G 主体 |
| 7 | Source reader、右栏和 Source 生命周期 | P0-06（非 AI 部分）、P1-11 | Gate E |
| 8 | AI 整理完整候选闭环 | P0-06（AI 部分） | Gate F |
| 9 | 兼容清理、无障碍、文案与全矩阵回归 | P2-09、P3-02、P3-03 | Gate G 收口 |

**顺序**：0 必须最先，9 必须最后。1 之后 2 和 3 可并行；6 建议放在 3/4/5 之后再做。

---

## 几批需要额外加一句

**Batch 0** —— 删除面最大，加上：

```text
注意：只删 Import 侧的 BYOK（import_v2* 和 src/features/import/）。
Chat / Compile / Export / Lint / Settings / Task 的 BYOK 是合法的，不要动。
```

**Batch 1** —— 后续批次都要复用它的测试，加上：

```text
Gate A 的契约测试必须写成表驱动，后面每加一种输入只加数据行，不改测试结构。
manifest schema 在本批一次定完（review §9.3 的全部字段 + schemaVersion），后续批次只填值不改结构。
```

**Batch 6** —— 前端改动最集中，加上：

```text
视觉密度对照 UI-Frontend-design/assets/app.css：UI 正文 13px、次要 12px、muted/mono 11px、
小标签 10.5px；顶栏 48px、主区头 52px、右面板头 52px、状态栏 28px、导航项 30px、面板头 44px。
只引用 src/styles.css 的 token，不要硬编码颜色值。
```

**Batch 9** —— 收口批，加上：

```text
本批要形成可重复的验收证据：基准 §19 的 32 条场景、review §12.2 的 26 条契约测试、
§11 格式矩阵 14 行 fixture、九条禁止关闭方式的反向测试，逐条对应到具体测试或手动脚本。
```
