---
title: "刚刚，Claude Code 之父分享 15 个隐藏功能！附赠我常用的另外 5 个"
url: "https://mp.weixin.qq.com/s/T-9ErjThlQAtIdmP3VKrPA"
source: "微信公众号"
fetched: 2026-04-18
sha256: d8e0eef81e9ff9ad
---

Claude Code 之父鲍里斯·切尼（Boris Cherny）刚发了一条长帖，一口气列了 15 个他在 Claude Code 里最常用的「隐藏功能」。

![image](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFn8kNviaCYCCHyKkqtvyNicPiagRiaPTRKctLwy4mOGCgES7yXjTjkIg8Uuw9jU2uZDiaRgzF7DiccnrcQNj1na0Us86dpmdFM9R1zvs/640?wx_fmt=png&from=appmsg)

这些功能其实都摆在那里，但大部分人可能从来没点开过。

而对喜欢使用快捷键的我（见：[AI 时代，你更需要用好快捷键](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481742&idx=1&sn=9201d8ec84443b8b0d188dda62cc0123&scene=21#wechat_redirect)）而言，其实我几乎全都在用了。于是我挑了几个印象最深的聊聊，末尾再附上我自己日常用得最多的 5 个快捷操作。
01
## 手机写代码

这可能是最出乎意料的一个：Claude Code 有手机 App。

鲍里斯说他很多代码都是在 iOS App 上写的，不想开电脑的时候，掏出手机就能改代码。下载 Claude App，左侧切到 Code 标签页就行。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFmJN97Uwv8ccLSAtBVjmwRh0TgTvn4YzeNe6lsRTv0jTW0I04ZmLproSnB2NvouNDyKlwDJpAl2Fpvn3dhCOW0HyPr4ibyX0sHU/640?from=appmsg)
Claude Code 移动端界面
更妙的是，会话可以在手机、网页、桌面端和终端之间无缝切换。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFldzThdvWukyWKQF879sm7JmRYqNXRFiba3xibwB8gbALuWVOMX2BxUrZtMHT6N1UKpqHGicfJ3kIvS80KmcxibnHpibiaeQd2iaAs1icU/640?from=appmsg)
多端流转概念图
在终端里跑 `claude --teleport` 或者输入 `/teleport`，就能把云端会话拉到本地继续。反过来也行，`/remote-control` 能让你用手机远程操控本地正在跑的 Claude。前文（[刚刚，Anthropic 发布官方「龙虾」](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481834&idx=1&sn=b23d428cde36fa7f2d1367e04d67c80c&scene=21#wechat_redirect)）介绍过的 Session Spawning 更进一步，在电脑上跑一次 `claude remote-control`，之后随时可以从手机发起全新会话。

你的笔记本电脑，变成了一台 headless 编程服务器。而你的手机，就是遥控器。
02
## 定时巡逻

`/loop` 和 `/schedule` 应该算 Claude Code 里最被低估的两个命令了。

它们能让 Claude 按固定间隔自动执行任务，最长可以跑一整周。语法也简单：`/loop 5m 检查部署是否完成`，支持秒、分、小时、天的时间单位，不写间隔就默认 10 分钟。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlGbghzYDaIret1f5IrZn0bRLLuyPSFzfeXl0RnLSbEHrqgcC6iacCxzqENHWsg3OSE1cic3ZzQpcbniaZo46M2icXgB6Wdzlh4lO4/640?from=appmsg)
/loop 自动巡逻循环
鲍里斯自己设了好几个本地循环：

• `/loop 5m /babysit`，每 5 分钟自动处理 code review、自动 rebase、自动跑 CI 

• `/loop 30m`，每半小时同步一次上游更新 

这就好像给你的项目请了个值班员，你去睡觉了它还在那盯着。

我在前面的文章：[Claude Code 推出 /loop 无限循环，一台电脑即可化身无数小龙虾](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481069&idx=1&sn=4e6a0b2d6abecd04425d3c30149eb55a&scene=21#wechat_redirect)中也有介绍过，`/loop` 还支持嵌套，Skill 套 Loop，Loop 套 Skill，加上 CLAUDE.md 做记忆、Git hook 做持久化，一台电脑就能跑出一整支 Agent 团队的效果。

![image](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFlvj1ubr5cFNdELibicic9Dho3SKgbBwlCgQ5pPkfYib6icDZICaaMEvndM8AhPx8O2OHjYtdWwicthRnbdNO3wIynkG6FoT7icvePiaQI/640?from=appmsg)
/loop + CLAUDE.md + Git Hook 三件套03
## Hooks 玩法

Hooks 能让你在 Agent 的生命周期里插入自定义逻辑，这是个挺强大的机制。

举几个场景：每次启动 Claude 时自动加载特定上下文（SessionStart）；记录模型跑的每条 bash 命令（PreToolUse）；把权限审批提醒推送到微信或 WhatsApp，出门在外也能远程批准。
04
## Dispatch 派活

鲍里斯提到他每天都在用 Dispatch。不写代码的时候，他用 Dispatch 来刷 Slack、处理邮件、管理文件。

Dispatch 是 Claude Cowork 里的一个功能，前文也详细介绍过：用手机给电脑上的 Claude 派任务，干完了回来收结果。代码在本地沙箱里跑，文件不出你的机器，权限也是分级管控的。

从工具，到助手，到同事。Dispatch 算是踏出了「同事」这一步。
05
## 前端神器

做前端的同学应该会喜欢这个：Claude Code 有 Chrome 插件。

鲍里斯说的是：
**
“ 用 Claude Code 最重要的一个技巧是，给 Claude 一种验证产出的方式。一旦有了验证手段，Claude 就会自己迭代到满意为止。

道理其实跟带新人一样，你让人做个网页但不给看效果，那质量全靠运气。装了 Chrome 插件之后，Claude 能自己看到页面渲染的结果，自己调，自己改，直到像样为止。

桌面端更进一步，内置了浏览器，Claude 可以自动启动 web server 然后自己测试。
06
## 会话分叉

经常有人问：怎么从一个会话里「分叉」出去呢？

两种方式：在会话里输入 `/branch`，或者从命令行跑 `claude --resume <session-id> --fork-session`。

![image](https://mmbiz.qpic.cn/mmbiz_jpg/ZKqVLiaIpzFn6Uu7C6Qwgkp1stonZFByKfkvGuzhPotxwtOUxXyAAEqWXCUG1WaYZ3R6fFbH4zeDfqFUWSO3vZ5CO5R6RoAlJxs5maHUEasU/640?from=appmsg)
/branch 命令演示
分叉之后，原来的会话还在，新会话继承了全部上下文。你可以在新分支里放心探索另一条路，不用担心搞乱原来的进度。
07
## 插个嘴

`/btw` 是个容易被忽略的小功能，但用起来特别顺手。

当 Claude 正在执行任务的时候，你可以用 `/btw` 插一个不相关的问题进去，Claude 会快速回答你，然后继续干活。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFndPdKNnPcficFkdI29mfQwtLaaF2CX45jkSVpZC3Lnom365et5icxpaPFlicUEkuyko4s3bqVQ4So9q5XibygicuLLvnh9DVAxOLU4/640?from=appmsg)
/btw 命令演示
上图里鲍里斯问了句「daushund 怎么拼」，Claude 秒回「dachshund，德语的獾犬」。

前文[Claude Code 新增 /btw 功能：让你在 AI 干活时「插嘴」提问](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481272&idx=1&sn=3cc80c96d833fb589598015354cb638b&scene=21#wechat_redirect)中，我也介绍过，`/btw` 继承了完整的对话上下文，但没有工具权限，只做单轮问答，回答完就消失，不会污染主对话历史。利用了主对话的 prompt cache，额外 token 消耗几乎为零。

就像随手撕一张便签纸问旁边的同事一个问题，问完就扔掉。
08
## 并行作战

`claude -w` 能在 git worktree 里启动新会话，让多个 Claude 在同一个仓库里并行工作，互不干扰。鲍里斯说他经常同时跑几十个 Claude，靠的就是 worktree。

还有个更猛的：`/batch`。

它会先问清楚你要干什么，然后把任务拆成若干份，分发到尽可能多的 worktree Agent 里并行执行。几十个、几百个……甚至上千个都行。大型代码迁移、批量重构，用这个命令就对了。
09
## SDK 提速

用 `claude -p` 或者 TypeScript/Python SDK 做非交互调用时，默认会搜索本地的 CLAUDE.md、settings、MCP 配置。

但很多时候，你其实并不需要这些。加个 `--bare` 参数，启动速度能快 10 倍。

![image](https://mmbiz.qpic.cn/mmbiz_jpg/ZKqVLiaIpzFniad2f7mo1D7ia4fT1VIicfCss4eGufLN8176EOb3fz1qQhic4icAEB9RYyq9ZIkZfKRtic8jWxRrogicoPv60mlnxD0udcNSORRgQdw/640?from=appmsg)
--bare 参数演示10
## 自定义 Agent

`--agent` 参数能让你给 Claude Code 指定自定义的系统提示词和工具集。

在 `.claude/agents` 目录下定义一个 Agent 文件，然后用 `claude --agent=<名字>` 启动就行。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFk77KHIfn4yb3qMsOaXFicav5iadHJYYd00uIjPmkJWWXjl6kZDsdMibEFe8OibNq47efMXGnKASZr9K9ibUIuiafn37rDdDLhiaPcNDI/640?from=appmsg)
自定义 Agent 演示
上图的例子是一个「ReadOnly」Agent，只允许读取文件，不能编辑也不能跑命令。这种受限 Agent 在代码审查、安全扫描这些场景下，应该还挺实用的。
11
## 语音编程

最后一个让人没想到，鲍里斯说他大部分代码是**说**出来的，不是打字。

在 CLI 里跑 `/voice`，然后按住空格键就能语音输入。桌面端有语音按钮，iOS 上开系统听写就行。

不过，目前不支持中文……即便你是 Max 用户。

我自己，一直就是语音输入的重度用户。

之前专门做了一套 DJI Mic Mini 无线麦克风 + AI 语音输入的方案，按一下按钮就能在任何应用里语音输入，Claude Code、微信、飞书都行。

见我前面的文章：[我做了一个 AI 时代的效率神器，已开源](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481122&idx=1&sn=d5650b5cd406dee007b4fa243fa394fe&scene=21#wechat_redirect)且代码已开源：https://github.com/Johnixr/dji-mic-dictation ，一行命令就能装好。配合下面要说的 `Ctrl+G`，语音输入后进 vi 改错字，整个流程特别丝滑。
12
## 我的 5 个常用操作

聊完了鲍里斯的 15 个，我再分享其他 5 个自己每天都会用，便可能很少人在用、且鲍里斯也没怎么提到的操作。

这几个功能配合起来，基本上能让你在 Claude Code 里像管理 git 分支一样管理对话。

![image](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFmQaR2icH1m1Fsp5Lw3FG65aM9R5AG4UcMQbILTRbUV9fCrzeIVzic8icreCYDNvkDTUeibpTRiaCfAetcvFo0NhTX4ibNyWgZmvSHU0/640?from=appmsg)
会话管理五件套
**`/rename`：给会话起个名字**

Claude Code 默认会话名是一串 ID 或者你 prompt 里前面的一堆指令文字，找起来跟大海捞针似的，极其麻烦。`/rename` 能给当前会话起个有意义的名字，比如 `/rename 重构登录模块`。之后再找这个会话，一眼就能认出来。

这个习惯一旦养成，你会发现自己的 Claude Code 从「一堆匿名聊天记录」变成了「井井有条的项目日志」。

**`/branch`：随时劈叉**

前面提到过 `/branch`，但我想再强调一下这个功能有多好用。

你正在跟 Claude 讨论方案 A，突然想试试方案 B？`/branch` 一下，在新分支里随便折腾。觉得不行……回到原来的会话继续方案 A，上下文一点没丢。

这就像 git 里开分支一样自然，只不过分的并非代码，而是对话。

**`claude --resume`：接着聊**

昨天和 Claude 讨论到一半的问题，今天想继续？`claude --resume` 会列出最近的会话，选一个就能接着聊。Claude 记得所有之前的上下文，不用重新解释一遍背景。

配合 `/rename`，你可以给重要的会话起好名字，以后随时 resume 回来。

**`Ctrl+R`：搜索历史会话**

会话多了之后，光靠眼睛翻列表肯定不够。`Ctrl+R` 能打开一个搜索框，输入关键词就能从所有历史会话里找到你要的那个。

跟 shell 里的 `Ctrl+R` 反向搜索是一个逻辑，用过 bash 的人应该秒懂。

**`Ctrl+G`：打开编辑器写 prompt**

这个是我的私藏技巧。

我平时经常用语音输入来跟 Claude 对话，但语音识别难免有错字、漏字。按 `Ctrl+G` 会打开一个 vi 编辑器，语音输入的内容已经在里面了，修修改改很方便。

改完之后 `:x` 退出（比 `:wq` 少敲一个键，也可以按 `ZZ` 退出），内容就自动发送给 Claude 了。

这个操作流特别适合写长 prompt 的场景。单行输入框里写几百字的指令实在太痛苦了，打开 vi 就舒服多了，还能用 vim 的各种编辑命令来调整文本。

◇ ◆ ◇

鲍里斯在帖子最后说，他其实还想继续写，但强行收住了。

能看得出来，Claude Code 里藏着的功能，比大多数人以为的要多得多。很多人可能还停留在「输入问题，等回答」的用法上。

但其实它更像一个……操作系统。

**会话能命名、能分叉、能搜索、能恢复。**

**任务能定时、能并行、能远程控制。**

**工具用不用、用哪些，你说了算。**

◇ ◆ ◇

原帖链接：https://x.com/bcherny/status/2038454336355999749