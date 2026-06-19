---
title: "不需要压缩，2个方法让Claude对话不中断"
url: "https://www.xiaohongshu.com/explore/69bcef54000000002200c60c?xsec_token=CBhC3Vt0vwBV88kUVWkwI5G_XptMZ7E1r7QlQymCytva0="
source: "小红书"
author: "Erichain"
fetched: 2026-04-18
status: "success"
tool: "Spider_XHS"
likes: 52
collected: 85
share: 16
tags: [vibecoding大赏, vibecoding, claude, chatgpt]
note_id: "69bcef54000000002200c60c"
upload_time: "2026-03-20"
ip_location: "四川"
note_type: "图集"
sha256: 6dc7bcb0fb3e8473
---

# 不需要压缩，2个方法让Claude对话不中断

**作者**: Erichain | 👍 52 | ⭐ 85 收藏 | 🔗 16 分享 | 📍 四川

无论是 Claude Code 的 Auto Compact 还是手动跑一次 Compact 指令，都会消耗大量的 Token！

运行一次上下文压缩指令，你会发现在 usage 中你的当前 session 和 Weekly session 都会直接增长百分之二三十。

这里有两种比较好的实践：

1️⃣ 第一，手动的告诉 Claude Code，总结当前对话，生成一份 handoff 文档，即交接文档，然后新开一个对话窗口，读取这一份交接文档来继续任务。

2️⃣ 第二，在上下文用到 75% 左右的时候，让 Claude 进入 Plan Mode，即规划模式，并且制定新计划来完成剩余的工作。

Claude 生成了新的计划之后，会让你选择是否清除当前上下文，只要选择是并且继续就好了，Claude 会自动地将计划的内容给粘贴进来。

相当于就是，新开了对话并且有计划明细了。

#vibecoding #claude #chatgpt
