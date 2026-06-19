---
title: "Anthropic Harness 指南：到期清理、别帮倒忙"
url: "https://mp.weixin.qq.com/s/cg4pnifOuVYPxVg2ZLo9bg"
source: "微信公众号「AGI Hunt」"
author: "J0hn"
fetched: 2026-05-27
sha256: 0aac1272d18e185e
image_count: 13
---

有效护栏 vs dead weight：模型升级后，护栏可能变成拖累01

## 三条原则

文章共提出了三条原则。

拆开来看，每一条都在说同一件事：**别帮倒忙。**

###  用 Claude 已经会的

第一条是：别急着造新工具，先看看 Claude 已经会用什么。

2024 年底，Claude 3.5 Sonnet 在 SWE-bench Verified 上拿到了 49%，当时的最高分。用的工具呢？只有两个：bash 和文本编辑器。

![SWE-bench Verified 各版本得分趋势](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFngF231xS3c1ibVibpACj8lxZwY4bEOajFVEHIyUhU03e4cf4ymrViauSY342MKBQy69fq7rMNNfMPRcB386fLS2we6Jed2I747vU/640?from=appmsg)SWE-bench Verified 各版本得分趋势

到了 Opus 4.5，这个数字到了 80.9%。工具还是那两个。

这两个工具都不是为 Agent 场景专门设计的，但 Claude 天生就会用。bash 能执行命令，文本编辑器能读写文件，两者组合起来，Claude 自己就能衍生出一套套复杂的工作模式。

![Claude 的分层工具架构：底层是 bash 和文本编辑器，上层衍生出 Skills、程序化工具调用、Memory](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkDkiaHqfa63U4MaS3qzSSEiamSKacAdcCS0G1scygkicx4RoAP22PhpguYFOUWfGVbqoQmHL7XgTCp6GIXFViakAoYy6SXbAxZCyI/640?from=appmsg)Claude 的分层工具架构：底层是 bash 和文本编辑器，上层衍生出 Skills、程序化工具调用、Memory

文章举了几个例子：Agent Skills、程序化工具调用（programmatic tool calling）、Memory 系统，这些听上去像是独立功能，其实全都是从 bash + 文本编辑器这两个基础工具组合出来的。

**底层工具越通用，Claude 发挥的空间就越大。**  

反过来，你给 Claude 造一堆专用工具，每个工具解决一个具体问题，看上去考虑得挺全面，但其实是在替 Claude 做它自己能做的决策。

### 什么时候该停手

第二条更直接：问自己，**「我还能停掉什么？」**

这一条展开讲了三个方面：编排、上下文、记忆。每个方面都在说同一个道理，把控制权还给 Claude。

**编排。**  

传统做法是把每个工具调用的结果都塞进 context window，让 Claude 看完再决定下一步。但这样做既慢又贵，而且很多时候 Claude 根本不需要看全部结果。

比如……读一张大表来分析某一列，整张表都塞进去了，Claude 要为每一行不需要的数据付 token 成本。

![传统工具调用 vs 程序化工具调用：后者让 Claude 自己写代码编排，中间结果不经过 context](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFnRicNTzWwjFwuuCocwIJO2icOoHXzGbtLsykkafx58dL80WXESGuNksanfaBRXpy4SR6xQhREo5zCMTxYtpXFku82mY4EnSRKz0/640?from=appmsg)传统工具调用 vs 程序化工具调用：后者让 Claude 自己写代码编排，中间结果不经过 context

解法是给 Claude 代码执行能力（bash 或 REPL），让它自己写代码来编排工具调用。中间结果在代码里处理，只有最终输出才进入 context。

这个改动效果有多大呢？在 BrowseComp（一个测 Agent 浏览网页能力的基准）上，让 Opus 4.6 自己过滤工具输出，准确率从 45.3% 直接跳到了 61.6%。

**一个编排决策的调整，提升了 16 个百分点。**

**上下文。**  

传统做法是把所有指令预加载到 system prompt 里。问题是指令越多，注意力越分散，而且大部分指令在当前任务中根本用不到。

![Skills 机制：Claude 按需读取 Skill 文件，渐进式披露上下文](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFnat1mNFt4KVBxPCbtD41Jp2BpSdC8MqK7ROniaXb69u6wPYvicWPn6Xg7981zjqqaPr1HPnHZFDtgN8JDvYOib16WLmpcvvj3KB0/640?from=appmsg)Skills 机制：Claude 按需读取 Skill 文件，渐进式披露上下文

Skills 解决了这个问题。每个 Skill 的 YAML 头部是一小段描述，预加载到 context 里只占很少的 token。Claude 觉得某个任务需要某个 Skill 时……自己去读文件就行了。

这就是渐进式披露（progressive disclosure）：先给 Claude 一个目录，让它自己翻。

而 context editing 是反过来的操作：让 Claude 自己删掉已经过时的上下文，比如旧的工具调用结果、过期的思考块。

subagent 则是另一个维度：Claude 自己判断什么时候该分叉一个新的 context window，隔离处理某个子任务。Opus 4.6 用 subagent 在 BrowseComp 上又多了 2.8%。

**记忆。**  

长时运行的 Agent 会超出单个 context window 的容量。传统做法是在模型外面建一套检索基础设施。

![Memory 机制：Claude 自己选择持久化什么内容](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFkvCERcxibFLeW4CzYMmHmelC1cibDqQhPyqEE9X5qjYEPEAPM0RbMImhBEv07IWFAwJovNcVvfYibRauGOD7zh1hNBVMMty11W1g/640?from=appmsg)Memory 机制：Claude 自己选择持久化什么内容

Anthropic 的做法是让 Claude 自己决定记什么。

compaction（压缩）让 Claude 总结过去的上下文来保持连续性。随着模型迭代，Claude 在「选择记什么」这件事上越来越强。

用数据来说话是：同样的 compaction 设置，Sonnet 4.5 在 BrowseComp 上一直卡在 43%，不管给多少压缩预算都不动。Opus 4.5 能到 68%，Opus 4.6 则到了 84%。

memory folder 是另一种方式，让 Claude 把上下文写成文件，需要的时候再读回来。Sonnet 4.5 用了 memory folder 之后，BrowseComp-Plus 的准确率从 60.4% 升到了 67.2%。

### 宝可梦的教训

![宝可梦记忆对比：Sonnet 3.5 记了 31 个文件还在第二个城镇，Opus 4.6 只记 10 个文件却拿了 3 个徽章](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFnrPXO5qhHu4WgMnaHHxUiaf5h7DPicGcOxNBBXPgc6EmRCSx4pLdILv0yMfs1JShhBvmgBYmNdGx6Xzw7pqLJG9qMjj76kyNJia4/640?from=appmsg)宝可梦记忆对比：Sonnet 3.5 记了 31 个文件还在第二个城镇，Opus 4.6 只记 10 个文件却拿了 3 个徽章

文章里有个特别生动的例子：让 Claude 玩宝可梦。

Sonnet 3.5 把 memory 当记录仪用，NPC 说了什么就记什么。跑了 14000 步之后，攒了 31 个文件，其中两个还是关于毛毛虫宝可梦的近似重复。结果呢……还在第二个城镇晃悠。

而 Opus 4.6 呢？同样的步数，只有 10 个文件，按目录分类整理好了，已经拿到了 3 个道馆徽章，还写了一个「从自己失败中提炼的经验教训」文件。

**模型在「选择记什么」上的进步，直接决定了它在长时任务中能走多远。**

###  边界要谨慎

第三条讲的是：该设的边界还是要设。

**缓存策略。**  

Messages API 是无状态的，每一轮对话都需要把所有历史打包发给 Claude。缓存的 token 成本只有普通 token 的 10%，所以 cache hit rate 直接关系到你的账单。

几个具体建议：

•  静态内容放前面，动态内容放后面 

•  用 `<system-reminder>` 追加更新，别改 prompt 本身 

•  别在会话中间换模型，缓存是按模型绑定的，一换就全失效 

•  工具定义在缓存前缀里，加减工具会让缓存失效。用 tool search 按需加载 

•  多轮对话里，把断点移到最新消息处 

**专用工具。**  

不是所有操作都需要专用工具。bash 已经够用了。但有三种情况应该提升为专用工具：

![代码执行 vs 专用工具：何时该提升为专用工具](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFn0ibO6ibxG9elhQI1rjNZsZt2CvcOnKengnJ1IA6WoxvYBjbpXiamMCPturzZmctzfmTDcL098WeD7HDTgkiaiaKB8w0jA2QFj1SHo/640?from=appmsg)代码执行 vs 专用工具：何时该提升为专用工具

一是安全边界。不可逆的操作（比如外部 API 调用）应该有确认机制。写操作可以加过期检查，防止覆盖已更新的文件。

二是用户界面。工具调用可以渲染成弹窗，给用户展示选项或阻塞等待反馈。

三是可观测性。类型化的工具调用有结构化参数，方便日志、追踪和回放。

文章还提出了一个新模式：Claude Code 的 auto-mode 用第二个 Claude 来审查第一个 Claude 的 bash 命令是否安全。这个模式可以减少专用工具的数量，但只适合用户信任整体方向的场景。

02

## Dead Weight

文章最后的一段话，我觉得是全文最重要的：

> “ 在一个长时任务 Agent 中，Sonnet 4.5 会在感觉到 context 上限的时候提前收工。我们加了 context 重置机制来应对这种「上下文焦虑」。到了 Opus 4.5，这个行为消失了。我们为了补偿它而建的那套重置机制，成了 Harness 里的 dead weight。

**dead weight.**  

这个词，精准地描述了 Harness Engineering 最大的陷阱：你为了解决一个问题而搭的基础设施，在模型变强之后，反而会拖累性能。

之前的 Harness Engineering 文章（[模型不是关键，Harness 才是](<https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481768&idx=1&sn=72a99eef97bc7f0dcb3eddb99573a0ab&scene=21#wechat_redirect>)）中我提到一个概念叫「护栏悖论」：车速越快，护栏越重要。

![护栏悖论：车速越快护栏越重要](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkQmj1EqTLJZeAouCDVJIgPpzN0F8Rpo0hbtByaFG5A2MrDSEMkd7LWt41Ze0DCu9w39y059K1DlNCibUj0oicGbkY8TUQNbY1Os/640?from=appmsg)护栏悖论：车速越快护栏越重要

这篇文章给了这个悖论一个更精确的补充：**护栏该装，但 dead weight 该拆。**

两周前 Anthropic 工程博客那篇多智能体编排的文章里，V1 到 V2 的演进就是这个道理的实战。sprint 分解是护栏，在 Opus 4.5 时代有用；Opus 4.6 来了之后，sprint 成了 dead weight，拆掉，成本省了 37%。

Build to delete. 

造了就要敢拆。

而这篇文章把这个原则推到了更底层：不只是编排架构要拆，你预加载的指令、你设计的工具、你搭建的记忆系统，每一层都应该定期拿出来问一遍：

**模型自己能做这件事了吗？**

**能了，就干掉！**

◇ ◆ ◇

相关链接：

•  原文：https://claude.com/blog/harnessing-claudes-intelligence 

•  推文：https://x.com/RLanceMartin/status/2039783012427333751
