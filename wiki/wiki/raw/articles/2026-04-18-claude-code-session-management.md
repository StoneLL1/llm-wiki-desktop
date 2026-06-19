---
title: "Claude Code会话管理1M Context正确用法"
url: "https://mp.weixin.qq.com/s/S0LfJvhkZTV-xJCrC7uUjw"
source: "微信公众号"
fetched: 2026-04-18
sha256: 58aaf5682451c988
---

Claude Code 现在默认用的是 1M context 的 Opus 4.6 了，日常写代码确实爽了不少，context 窗口大了，能塞进去的东西多了，干活也更连贯了。

但用得多了你会发现一个问题：**context 大了，不代表就能无脑用。**


![](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFmleBthqv007xvWX1ZeFwuEibqlSgjpib8QaGcyLeq2COFwqNp9TIPaELGYGibMrScP4UpgbrAyRibtLLD2WUmlPOI0BvZiahmJVVTQ/640?wx_fmt=png&from=appmsg)
我看到群里也有人在聊这事，有人说自己一个 session 干到底，结果越到后面 Claude 越「迟钝」；也有人说每做一步就开一个新 session，又觉得来回交代背景太累了。

正好，Claude Code 的核心开发者 Thariq Shihipar 刚刚发了条帖子，又紧接着写了篇博客，专门讲这个问题。


![](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFnMviaLub0zyzVKegSfJ3l3r4l0lpVDKejViarZUG3bGrrezR5fysaOItMTRV5hnmJGceKFxaAxsic71OzIVxBzUQzN3EHC6GYS8E/640?wx_fmt=png&from=appmsg)
Thariq 推文
Thariq 在 Anthropic 负责 Claude Code 的开发，题外话，打死不承认降智的那位：[Claude Code 开发负责人：不会降智，说了多少次了](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453482664&idx=1&sn=ef0521ccf99cbb73ccb3badebb1bac8b&scene=21#wechat_redirect) 也是他。

言归正传，他最近两周跟用户做了不少交流，得出一个结论：


“ 会话管理这件事，有一个出乎意料的高技能上限。在 rewind、主动 compact、子 Agent、新建 session 之间做选择，其实需要花不少心思。


然后他直接写了篇博客，把他的心得整理了出来。博客标题就叫「Using Claude Code: session management and 1M context」，阅读时间大概 5 分钟。


![博客首页](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFmAIY3OsgdDgPeCJiaUA4dfbPtsu1sgaJpnZw8UFbAMFkzXulWib4xKgwjpOEvaTnSc8RSpMLa9ZLJG37FZm9AOibQGhAxIur7Jo4/640?from=appmsg)
博客首页
下面，我们就来看看官方开发人员是怎么玩的。

01
## 先说 Context

先来说说几个基础概念。

Context window 就是模型在生成回复时能「看到」的所有内容：系统提示、CLAUDE.md、对话历史、工具调用和输出、读取的文件内容，全都算。

Claude Code 现在的 context window 是 **100 万 token**。


![Context Window 结构](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFlO6NRazic6XI1vkNHtOlT1R6lMzphhP8AZVhIu0vev3BbicR2NmzxkS3uiawmgAodTHtPiaKWGiaBNKS6mXYq2m2OQHJUneVTDNJf0/640?from=appmsg)
Context Window 结构
听起来很大对吧？但问题在于，**context 越长，模型的注意力就越分散**。

老的、不相关的内容会开始「干扰」当前任务，Thariq 把这个现象叫做 **context rot**（上下文腐烂）。

就好比你桌上堆了一千份文件，虽然理论上你都能翻到，但找起来效率会越来越低，还容易拿错。


![上下文腐烂](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFnFqOvSxHjy9NxxibiaB9Cp28NLeDuml6hWaHk44F3Wtey96GVVonUiavbgCr7ibJqsXHUp49oWfZdb34XVXa1R9cocMHI8YpNRgTg/640?from=appmsg)
上下文腐烂
那怎么办呢？

当 context 快要接近上限时，系统会自动触发 **compaction**（压缩），把整段对话浓缩成一段简要描述，然后在一个干净的 context 里继续干活。

当然，你也可以手动用 /compact 触发。

下图直观展示了 context 从左到右越来越「腐烂」（颜色从深变浅），快到 1M 上限时，触发 compact，把所有内容浓缩成一段 summary，然后在新的 context 里继续。


![Compaction 过程](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlcaicfibBibcODNdeUB4vnHMLQYWPd5OEwJ2zU6V5Iqib1OsAD5RctFlFtI7zYHeI5R9yoPNAurwPCuEKnVp3pXtficdhL5GMZHPVY/640?from=appmsg)
Compaction 过程02
## 五条岔路

Thariq 提出了一个观点：**每当 Claude 完成一步操作后，你其实站在一个分叉路口，有五条路可以走。
![五条岔路](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFlPI2YLNMMOe24AibO7bBDBkyG5ULSxqIExbMELibddCH24U1JlictruDQglB9EmOHJvY44EAqLBNpicicIibpW9jLxubnfO8L8ibSxCs/640?from=appmsg)
**

**五条岔路**


**1. 继续对话**，在当前 session 里直接发下一条消息。

**2. Rewind（按两下 Esc）**，跳回之前某条消息，从那个节点重新开始。后面的对话全部丢弃。

**`3. /clear`**，手动写一段简报，然后开一个全新的 session。

**4. Compact**，让模型总结当前对话，压缩后继续在这个 session 里干活。

**5. 子 Agent**，把一块工作派给一个子 Agent，它有自己独立的 context，干完活只把结论带回来。

这五条路的选择，就是「会话管理」的核心。

原文里 Thariq 还画了张图，展示了这五种操作各自会保留多少 context：


![五种操作的 context 保留量](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlO50FHIJbSMRichlMFvu5AWaeyITfTqvfkbD37iaE6qS4CgRefnzxnnkm6vIPvIdD2hic79YJ6QFUtHCFXOfX7XNicnCiam9icftDf8/640?from=appmsg)
五种操作的 context 保留量
从左到右，保留的 context 越来越多。

Fresh session 只带你自己写的简报；Compact 带一段有损摘要；Subagent 带完整指令和结果；Rewind 保留前缀、砍掉尾巴；Continue 则什么都留着。

选错了路，轻则浪费 token，重则让 Claude 越走越偏。

03
## 该开新的吗

Thariq 的建议是：**新任务，新 session。**

虽然 100 万 token 的窗口让你可以在一个 session 里从零搭一个全栈应用，但 context rot 依然会悄悄发生。

那如果两个任务有一定关联呢？比如你刚实现了一个功能，接下来要写相关的测试。

这就有个取舍了。

继续用当前 session，context 里已经有你刚写的代码，Claude 不用重新读文件。但代价是 context 里也堆满了之前调试的过程，那些信息其实已经没用了。

开新 session 呢？context 干干净净，但 Claude 得重新读一遍相关文件。

**这个，就没有标准答案**了，得根据你当前 context 的「干净程度」来判断。

04
## Rewind 的妙用

`/rewind` 这个命令（或者连按两下 Esc），可以让你跳回到之前任何一条消息，然后从那个点重新开始。

但 Thariq 说，大多数人只把它当「撤销」用，其实它的价值远不止于此。

举个例子。

Claude 读了五个文件，然后尝试了一个方案，失败了。这时候大多数人会打一句「不行，换个方式试试 X 吧」。

但这样做的问题是：**失败的那次尝试还留在 context 里，白白占着空间，还可能干扰 Claude 的判断。**

更好的做法是：rewind 到 Claude 读完文件之后、开始尝试之前的那个节点，然后重新给指令：


“ 不要用方案 A，foo 模块没有暴露那个接口，直接走方案 B。


两种做法的差别如下：


![Correcting vs Rewinding](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFmpQbd30L00lYKakYzpRWqrysLwdJeFKcyG9TL0oAbj4RYv0HWex7ODeQ6RjctoCw8GWtaWZkQsxTrjGfqP6Vw0Eny9YDN703g/640?from=appmsg)
Correcting vs Rewinding
上面那条线是「纠正」：读文件 → 尝试 A 失败 → 「试 B」→ 尝试 B 失败 → 「试 C」→ 终于成功。context 里堆满了两次失败的尝试和两次纠正。

下面那条线是「rewind」：读文件 → 尝试 A 失败 → Esc Esc 回到读文件之后 → 直接说「用 C，别用 A/B」→ 一步到位。context 干净得多。
![Rewind 时光信](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFlYcFtg2W75vWZj27S75uSZXlYn9O5lFyiaAh96MbgZbnAKKsGeocicJILMclSib0pBicBISNAwxhn52uAAgLEMKT3vZFL8gIyian8M/640?from=appmsg)


Rewind 时光信


Thariq 还提到一个更高级的玩法：在 rewind 之前，先让 Claude **总结一下它学到了什么**，写一段「交接信息」。


“ 就像是未来的 Claude 给过去的自己写了封信：「我试过这个路了，走不通，原因是……」


然后你拿着这段交接信息，rewind 回去，把它贴给「新的」Claude。

05
## Compact 还是 Clear

这两个操作都是为了「减负」，但方式完全不同。

**`/compact`** 让模型自己总结对话，用摘要替换掉整段历史。好处是省事，而且 Claude 可能会记住一些你没注意到的细节。你还可以给它指令来引导总结方向：


“ `/compact` 重点保留 auth 重构的部分，调试的那段可以丢掉


**`/clear`** 则是你自己来写「简报」，手动记下哪些文件重要、哪些方案已经排除、接下来要干什么，然后开一个全新的 session。费力一些，但 context 里的每一个 token 都是你精心挑选的。

原文里也给了张对比图，把两种方式的适用场景说得很到位：


![Compact vs Clear](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFntPUicbTPZ9FbpJHL0nNUgmdFE7Iicafl70l4ibPG4l0upn4RzChOu6kaZSvAlyDIHn8lLHuu2Cy8FnnwhBha9pwMnm33jzRXhb8/640?from=appmsg)
Compact vs Clear
简单来说，compact 适合「任务还在进行中，细节可以模糊一点」的场景，省力，保持节奏。clear 则适合「下一步至关重要，需要精确控制 context」的场景，费力一些，但精确。
![两种整理方式](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFkbk9N0fh4UX1MpbEYvnX4WDYdawyr51poI04laGVxftkW8icdqfjeu74yHVicGdNSbr8gYzLbuastZ7KQXbSfdiamd62sBE3qyAA/640?from=appmsg)


两种整理方式


这就像是：

compact 是让助理帮你整理桌面，它大概知道什么重要，但偶尔会把你需要的纸条扔掉。

clear 是你自己收拾桌子，慢一点，但每样东西放在哪你都清楚。

06
## 压缩翻车

Thariq 专门聊了一个大家经常踩的坑：**自动 compaction 出来的摘要质量差。**

根本原因是：**模型在压缩的时候，不知道你接下来要干什么。**

比如你花了很长时间调试一个 bug，期间顺带看到了 bar.ts 里有一个 warning。调试完了，自动 compaction 触发，它把对话压缩成了一段关于调试过程的摘要。

然后你说：「对了，我们之前在 bar.ts 里看到的那个 warning……」

但那个 warning 已经被压缩掉了。模型觉得它不重要，没留下来。

更要命的是，Thariq 指出了一个恶性循环：


“ 自动 compaction 发生在 context 快满的时候，而那个时候恰恰是模型注意力最分散、最不聪明的时候。


![压缩翻车恶性循环](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkNtO1hek6GvX2S7XHQEARI8Hiar1s3DXSSXR96Fz6HpFwE6EDNlFE663bicrLibXQCwsxIc5PYYH4gcSliaDDVmxjVSia0XjeFzYM4/640?from=appmsg)
压缩翻车恶性循环
所以他的建议是：**不要等自动 compaction，主动出手。**

现在有了 100 万 token 的窗口，你有充足的空间在 context 还很健康的时候就手动 `/compact`，并且告诉它你接下来打算做什么，这样压缩的质量会高得多。

07
## 派个子 Agent

子 Agent 是另一个容易被忽视的利器。

当 Claude 通过 Agent 工具派出一个子 Agent 时，这个子 Agent 会拿到**一个全新的 context window**。它干活的过程中产生的中间输出，全都留在它自己的 context 里。等它干完了，只把最终结论带回给父 Agent。

Thariq 给了一个判断标准：


“ 我需要这些中间输出吗？还是只要结论就够了？


如果只需要结论，那就派子 Agent。机制如图：


![子 Agent 工作机制](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFmmN9fLTUk6sXbtDPS8cPhahS9L6uW6icgM2AIMPDv7gCnwQicPOR2Rf8Eq9dzf2NrHkL1lQvdd94fdmv1s5BGrCUoIIoibAUjOicc/640?from=appmsg)
子 Agent 工作机制
左边是父 context，干干净净，只有用户的 prompt 和最终返回的结果。右边是子 Agent 的 context，里面塞满了 20 次文件读取、12 次搜索、3 条死胡同，但这些「噪音」在子 Agent 退出后就被回收了，只有 final report 带回来。


![子 Agent 出差](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFmXEOo2XXQDuIeUhQeaHMcANFiaubNZNFHM2iaSBhGERouI8mB8yumuDF4JzaFtFWKkKwojgSLot4jbp1GnZVyrZfosltgf8cZGE/640?from=appmsg)
子 Agent 出差
几个典型场景：

•  「派一个子 Agent 去读另一个代码库的 auth 实现，总结一下它是怎么做的，然后你照着做」

•  「派一个子 Agent 根据我的 git 改动来写文档」

•  「派一个子 Agent 根据这个 spec 文件来验证我们的实现是否正确」

这些任务的共同特点是：过程中会产生大量的中间输出（读文件、对比代码等），但你真正关心的只是最终结果。

把这些「噪音」留在子 Agent 的 context 里，主 context 就能保持干净。

08
## 决策速查

Thariq 在文章末尾给了一张决策表，我觉得应该贴出来：

场景操作理由同一个任务，context 还健康继续对话内容都还有用，别折腾了Claude 走错了路Rewind（Esc Esc）保留有用的文件读取，丢掉失败尝试干到一半，context 被调试信息塞满了`/compact` + 提示词省事，Claude 来筛选，你来引导方向全新的任务`/clear`零腐烂，你来决定带什么走下一步会产生大量中间输出子 Agent中间噪音留在子 context，只拿结论09
## 说到底

其实回过头来看，会话管理这件事的本质，就是**在信息量和注意力之间找平衡**。

context window 从 200K 扩到 1M，真正的意义并非「装更多东西进去」，它给你的其实是「做精细管理的空间」。

就像搬进了一个更大的房子，目的并非堆更多杂物，每个房间各司其职才是关键。

Thariq 在博客最后也提到，Claude Code 新加了一个 `/usage` 命令，可以随时查看你当前 session 的 context 消耗情况。算是一个不错的自检工具。

没准你就是那个把 Claude Code 「一个 session 干到底」的人，可以试着在合适的时机 rewind 和手动 compact，你会发现体感确实不一样。

BTW，文章中许多用法，其实很早之前，我也都分享过了，比如让 cc 提交代码部署环境后，我会再 rewind 回到前面。


我就不往回翻了，有兴趣的可以自己去翻一翻。

◇ ◆ ◇

相关链接：

•  博客原文：https://claude.com/blog/using-claude-code-session-management-and-1m-context

•  Thariq 推文：https://x.com/trq212/status/2044548916742631559

