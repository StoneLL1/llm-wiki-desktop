---
title: "分享一下我的Claude Code终端方案"
url: "https://www.xiaohongshu.com/explore/69db74ea0000000021004a7a?xsec_token=ABZvNgcsOr1aHFzgvRSPMyTh5sFQqHlFQ9yyB9vPUu1D8="
source: "小红书"
author: "CrazyAllen"
fetched: 2026-04-18
status: "success"
tool: "Spider_XHS"
likes: 542
collected: 836
comments: 50
tags: [vibecoding大赏, 个人开发者, claudecode]
sha256: 14d1d6871b394cfd
---

# 分享一下我的Claude Code终端方案

**作者**: CrazyAllen | 👍 542 | ⭐ 836 收藏 | 💬 50 评论

等待Claude Code Usage恢复的过程无比空虚，于是把终端好好升级了一下。

分享一下我搭配的方案：

1. **Ghossty**，最大的亮点是灵活分屏、标签页，多线作战的神器，可配置项非常多，这里只说几个最有用的：
   - `copy-on-select = true` // 框选文本后自动复制到剪贴板
   - `clipboard-trim-trailing-spaces` // 自动去掉粘贴文本时结尾的空格
   - `clipboard-paste-protection` // 粘贴保护机制，检测并提醒敏感信息或者高危操作
   - `link-url = true` // 按住⌘能直接点开Claude返回的文件

2. **Yazi**，终端里的文件管理器，可以直接在当前窗口内快速预览、编辑一些文件或者图片

3. **`/statusline`**，CC里直接执行这个命令，可以增加一个常驻的状态栏，包括当前模型、上下文窗口占用、Token花销、Git分支等等，这个非常之实用

*还有一个最新的Tips，`CLAUDE_CODE_NO_FLICKER=1`，Claude Code的配置文件里增加这个，就能用鼠标控制光标了！

搞完瞬间觉得CLI也没那么枯燥了，反而给人一种非常专注和纯粹的感觉。
