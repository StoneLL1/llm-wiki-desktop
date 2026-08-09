---
type: "explain"
date: "2026-08-07T03:54:27.530234+00:00"
question: "第一性原理审查当前导入板块：Batch E/F 后 URL 导入显示 Import batch (1) 并解析失败的真实原因是什么？"
contributor: "graphify"
outcome: "corrected"
correction: "初始全局 HTTPS/all-Fake-IP 例外会对 split-DNS/private redirect 重新打开 SSRF；已改为 reviewed host allowlist + explicit private grant。"
---

# Q: 第一性原理审查当前导入板块：Batch E/F 后 URL 导入显示 Import batch (1) 并解析失败的真实原因是什么？

## Answer

真实项目证据显示任务标题问题已由 operation/source-aware title 修复；两条 URL 在 8-54ms 内以 IMPORT_V2_URL_REJECTED 失败。系统 Mihomo/TUN DNS 将 mp.weixin.qq.com 与 www.bilibili.com 映射到 198.18.0.50/51，URLPolicy 在 HTTP 前把 RFC2544 benchmark range 当 reserved 拒绝。Batch E/F 改变批次任务所有权与日志可见性，不是该 URLPolicy 的引入点。安全修复不能全局放行 198.18/15：仅 reviewed HTTPS platform host allowlist 可自动兼容，generic/private 走 item/origin/DNS-bound explicit grant；尾点域名必须在 reqwest DNS pin 前规范化。

## Outcome

- Signal: corrected
- Correction: 初始全局 HTTPS/all-Fake-IP 例外会对 split-DNS/private redirect 重新打开 SSRF；已改为 reviewed host allowlist + explicit private grant。