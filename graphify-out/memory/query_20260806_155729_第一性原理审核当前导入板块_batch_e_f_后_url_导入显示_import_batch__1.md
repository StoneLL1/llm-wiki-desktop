---
type: "architecture"
date: "2026-08-06T15:57:29.123135+00:00"
question: "第一性原理审核当前导入板块：Batch E/F 后 URL 导入显示 Import batch (1) 且导入路径失效"
contributor: "graphify"
outcome: "useful"
---

# Q: 第一性原理审核当前导入板块：Batch E/F 后 URL 导入显示 Import batch (1) 且导入路径失效

## Answer

根因是批量背压改造把唯一操作身份、展示标题、任务持久化根、后台启动、取消与终态聚合混在字符串和分离写入中。修复为类型化唯一 ImportBatch operation，单 URL 使用来源名标题，后台可取消地绑定 cohort，按布局持久化，统一登录/重复项/取消路径并用原子终态决策消除竞态；配套前后端与 10k 规模回归。

## Outcome

- Signal: useful