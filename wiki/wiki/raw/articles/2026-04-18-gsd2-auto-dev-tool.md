---
title: "自动开发卡住后，我换成了GSD2"
url: "https://www.xiaohongshu.com/explore/69b98243000000002202507c?xsec_token=AB5lKoY8kMt0FnAP3Zg9mr1Xk6Xg30a7U-iUXdwbhvNdc="
source: "小红书"
author: "devling"
fetched: 2026-04-18
status: "success"
tool: "Spider_XHS"
likes: 20
collected: 20
comments: 4
tags: [vibecoding大赏, vibecoding, 独立开发者, 程序员, 个人开发者, 编程, 开发, ai, openclaw]
sha256: f5006e2c145acfc6
---

# 自动开发卡住后，我换成了GSD2🤯

**作者**: devling | 👍 20 | ⭐ 20 收藏 | 💬 4 评论 | 📅 2026-03-18

分享一下最近折腾的 vibe coding 方式。

最开始用 superpowers / GSD（skill 版本），都还不错，就是自动化差点意思，最后发现了 GSD2（gsd-build/gsd-2），它底层其实是基于 pi agent 做的。

## 之前的思路（Taskmaster）

1. GPT 总结出 PRD 文档
2. 给 taskmaster 去拆解 tasks
3. 最后一步步去实现

**问题：**
- 链路太长
- 人工介入太多
- 容易把任务弄散

## GSD2 之后

更像下场干活的团队，把 GPT 的流程放在核心里面：

**Research → Plan → Execute (per task) → Complete → Reassess Roadmap → Next Slice**

作者也是一个 solo developer，真的从我们独立开发者的痛点出发。

最近也在思考如果对接上 OpenClaw 估计真的就爽歪歪了。
