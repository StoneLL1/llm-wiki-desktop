---
title: "如何管理Claude Code 100w上下文？Anthropic官方推荐的五种使用指南"
url: "https://mp.weixin.qq.com/s/YvDsee0_eyJj8ndIVsgf7Q"
source: "微信公众号"
author: "鲁工"
account: "AI编程实验室"
fetched: 2026-04-21
sha256: 3266402c0c1a1d0e
---

# 如何管理Claude Code 100w上下文？Anthropic官方推荐的五种使用指南

**作者**: 鲁工 | **公众号**: AI编程实验室

Agent时代，之前很多AI专业领域的术语，正在走向大众群体，比如之前引起广泛争议的译法名词token，我之前也写过两篇文章专门说过这事：

[一篇关于token的技术科普：从一次Claude Code会话说起](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247485380&idx=1&sn=3ad8c9a2aa1cdb31250cd222a0d1b855&scene=21#wechat_redirect)

[token，正在成为新的贫富衡量单位](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247485317&idx=1&sn=04e577f2d6eef00986665a5d2ef4a023&scene=21#wechat_redirect)

除了token之外，另一个为大家广泛熟知的词是Context（上下文），以及去年非常火的概念Context Engineering（上下文工程）。

所谓上下文窗口，就是大模型在准备下一次输出时，一次性所能看到的所有输入内容。

日常使用Claude Code，一个非常重要的认知就是上下文管理。Claude Code的上下文通常包括系统提示符、CLAUDE.md、当前对话、每次工具调用及其输出，以及每个已读取的文件。

在Claude Opus 4.6之前，Claude的模型上下文窗口只有200k，当时为了能保持Claude Code的性能，我尝试过很多节省上下文空间的方案，比如把auto compact关掉能节省40k左右的空间、关闭或者删除不常用的MCP工具、CLAUDE.md文档尽可能简洁等。

在Claude Opus 4.6之后，Claude模型上下文升级到了1百万空间，一下子就感觉富裕起来了。

上周，Anthropic发了一篇官方博客，专门讲Claude Code里Session和1M Context的管理办法，让我对Claude Code上下文管理有了更深入的认知，所以有必要专门写一篇解读的文章。

原文地址：https://claude.com/blog/using-claude-code-session-management-and-1m-context

这篇blog有意思的地方在于，它把Claude Code里本来就存在的5个操作摆在一起对比：什么时候继续、什么时候回退、什么时候开新会话、什么时候压缩、什么时候开subagent。每个动作都能用，但用错了场合代价都不小。

## 0 先聊一下Context rot的概念

官方博客开头绕不过去的一个概念叫Context rot，中文可以翻译成上下文腐烂。

意思是随着上下文变长，模型的表现会缓慢下滑。注意力被更多token分散，早期那些已经不相关的内容开始干扰当前任务的判断。

这个现象在200k时代还不算特别明显，一个半天以内的会话大多能撑过去。1M Context上线后反而更容易触发，因为你能塞进去的东西变多了，Claude判断什么重要什么不重要的压力也跟着变大。

官方有个补充我觉得很关键：压缩动作恰好是在模型已经开始注意力涣散的时候触发的。也就是说，autocompact自动压缩出问题的时候，正是Claude最不聪明的时候。

所以会话的上下文管理这件事，本质上是在帮模型把注意力维持在高水位。

官方给出的5种上下文管理方案，每一种都对应着一个不同的目的。

## 1 Continue：最省心的操作

最自然的动作就是继续。

手头有一个任务刚跑完一轮，Context里还都是有用的文件读取和工具输出，这时候接着问下一步是最省事的。

官方的原则很清楚：如果Context里的内容还在起作用，就别动。换成compact或clear都得付出重新构建上下文的代价。

判断是否该继续，问自己一句话就够了：下一步要用到的信息，当前Context里还在吗？还在就继续。

## 2 Rewind：比再试一次效果好很多

这一节是整篇博客里我觉得最值得反复读的。

Claude Code里按两下Esc，或者敲`/rewind`，可以回退到之前任意一条消息，后面的记录全部丢弃。这个操作我其实很早就用过，印象中是先有两次Esc功能，后来再绑定到/rewind命令上。

官方举了一个特别典型的例子：Claude读了5个文件，试了一个方向，没成功。你的第一反应可能是追一句"这个方法不对，换X试试"。

但更好的做法是rewind到读完文件那一刻，带着新信息重新prompt，比如"不要用A，因为foo模块不暴露这个接口，直接走B"。

这个区别看起来不大，实际效果差很多。追一条纠正消息，Context里留下的是"失败的尝试"以及"纠正"两层信息，Claude得从里面推断你真正想要什么。rewind之后，那段失败的尝试直接消失了，Claude看到的是一份干净的、带着你最新理解的指令。

一句话总结来说就是，rewind是在帮Claude把错误记忆擦掉。

官方还提到一个配套用法：配合summarize from here或者`/rewind`，让 Claude先把踩坑学到的东西总结成一段handoff，再rewind回去带上这段 handoff重新跑。相当于让它给未来的自己留一张便条。

这个动作我之前一直低估它的价值。双击Esc这个操作太不起眼，很多人甚至没意识到它存在。

## 3 /clear：自己写brief，开一个新会话

官方给了一条很朴素的原则：开始一个新任务，就开一个新session。

这条规则听着简单，执行起来容易被惰性拖住。同一个终端开着，改完后端接着写文档，顺手就继续让Claude跑下去了。

问题在于，写文档这件事其实用不上后端接口的那些实现细节。Context带着走，除了让继续消耗更多的上下文，没什么实际好处。

`/clear`的定位是：你自己把重要的信息写在提示词里面。比如"我们在重构auth中间件，约束是X，相关文件是A和B，已经排除了Y方案"，这段话成为新会话的起点。

比compact累，因为要自己敲。但换来的是一份你亲自筛过的、干净的上下文。

我自己的用法是：新任务一定/clear，除非很明确知道旧的context里的文件读取对新任务还有用，比如刚实现完接着写测试，这种就直接继续。

## 4 /compact：省事但有损，最好手动触发

/compact的原理是把前面的对话交给模型自己总结，总结之后替换掉原始历史。

这个动作有损，但你不用自己动笔。还能加提示去引导它，比如`/compact focus on the auth refactor, drop the test debugging`，告诉它关注什么、丢掉什么。

官方解释了为什么autocompact有时候会翻车。核心原因是：模型做压缩的时候，它不知道你下一步要干嘛。

官方举了一个很具体的场景：一段很长的调试会话之后，autocompact被触发，总结出来是关于项目调试过程的要点。你下一句输入"现在去修bar.ts里另一个warning"。但那个warning是你之前随手提过的，调试会话里没怎么展开讨论，很可能压缩的时候就被当作无关信息丢掉了。

再叠加前面context rot那条，autocompact触发的时间点恰好是Claude最不聪明的时候，相当于让一个注意力已经涣散的人去决定什么重要什么不重要。那肯定就跑崩了。

我在200k上下文时代写过一篇文章，专门讲为什么要把autocompact关掉（`/config`里把Auto-compact设为false），具体可以参考：

[让Claude Code火力全开：两个关键配置，解锁200K上下文空间](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247484054&idx=1&sn=97e5befe92792df87be2505b7d42d590&scene=21#wechat_redirect)

关了自动之后，就得自己判断什么时候手动/compact。1M context的好处是你有更多时间在出问题之前主动压缩，而且主动压缩可以带hint，引导Claude 保留什么。

## 5 Subagents：只需要一项汇总结果的时候再开

Subagents的用法我之前专门写过教程，这里只讲博客里最关键的那句心法。

官方给的判断标准就一句话：你还会再用到这个工具的中间输出吗，还是只需要最终结论？

只需要结论，就开subagent。还要用中间输出，就别开。

举个例子，让Claude在一个很大的代码库里搜关键字并归纳，这个过程会产生大量的中间读取记录。这些记录对你没用，你只要最后那份归纳。这种场景 subagent 是完美匹配：它用自己的新上下文窗口跑完所有活，只把结论带回来。

官方列了几个可以直接抄作业的prompt：

  * Spin up a subagent to verify the result of this work based on the following spec file
  * Spin off a subagent to read through this other codebase and summarize how it implemented the auth flow, then implement it yourself in the same way
  * Spin off a subagent to write the docs on this feature based on my git changes

这里要特别提一下，Opus 4.7升级之后默认对subagent的开启比4.6克制了很多，之前能自动开的很多场景现在不会自动开。这个我在昨天的文章里面也专门提到过。

所以在4.7下面，如果你知道某个任务适合开subagent，最好在prompt里明确写出来，别指望它自动判断。

## 总结与对比

博客最后给了一张对照表，我翻成中文放在这里。碰上选择困难的时候，照着这张表敲命令就行。

这张表我觉得是整篇博客最值得强化记忆的部分。中文圈几乎没见过把这5个动作摆在一起讲清楚的材料，所以真学习还得看官方的资料。

顺带一提，博客开头还提到了一个新上线的斜杠命令`/usage`，用来查看 Claude Code的使用情况，辅助会话管理的决策。

用下来一段时间的感受是，Claude Code的这些功能其实一直都在那儿，只是没人把它们摆在一起讲清楚。1M context给了我们更多的腾挪空间，也让每一次会话管理的决策变得更值钱。

所以，Claude开源出来的文章，每一个字都值得精读。

如果觉得有用，点个赞或者在看，也方便更多朋友看到。

我是鲁工，九年AI算法老兵，AI全栈开发者，深耕AI编程赛道。感兴趣的朋友也可以加我微信（louwill26_）交个朋友。
