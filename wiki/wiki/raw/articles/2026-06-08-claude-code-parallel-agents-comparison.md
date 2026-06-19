---
title: "Claude Code并行Agents的四种方案对比，怎样才能按需开团"
url: "https://mp.weixin.qq.com/s/gmIWG-PlPvRcIDREK-mk1A"
source: "微信公众号"
author: "鲁工"
account: ""
pub_date: 2026-06-08
fetched: 2026-06-08
---

# Claude Code并行Agents的四种方案对比，怎样才能按需开团

**作者**: 鲁工 | **公众号**: 

大家好，我是鲁工。

虽然Codex最近用户声量高涨，但关于并行多个Agent干活这块，依然是Claude Code做得最丰富。

这两天我翻Claude Code官方文档，发现Anthropic针对Agents和并行干活单开了一个tab，并且放在了文档最开头，优先级高于MCP、Skills、Plugins等一众拓展功能。这个tab标题就叫Run agents in parallel，我仔细读了一遍，又把自己平时的用法捋了下，顺便写一篇文章记录下来。毕竟，开团这个事情，大家都爱干。

先放个简单粗暴的结论。这几种方法看着眼花缭乱，核心区别其实就一个：谁来拿主意。

是Claude在一个会话里替你委托、收结果，还是你把活甩出去回头再看，还是让Claude当领队带一队人干，还是干脆写一个脚本把计划固化下来。把这个区别搞清楚了，剩下的就都只是具体的用法细节了。

下面挨个说。每一种方案我之前都单独写过实操，这篇不重复基础，只讲它们放一起时该怎么选择。也算是顺带对之前并行Agent这块内容做一期汇总。

## Subagents：一个会话里的临时工

Subagents是Claude在当前会话里派出去的子智能体。

它有自己独立的上下文窗口，干完活只把一段总结传回来。过程里翻的那一堆搜索结果、日志、文件内容都留在它自己那边，不会占用我的主会话的上下文空间。

这是它最核心的价值，省上下文。Claude Code有内置的几个Subagents，比如只读的Explore（跑在Haiku上，便宜又快），规划用的Plan，还有什么都能干的general-purpose。

当然我们也能用`/agents`命令自己建，限定它能用哪些工具、走哪个模型。让它同时研究认证、数据库、API三个模块，它就并发开三个一起跑。

要注意subagent不能再往下套subagent，它只对派它出来的那个会话负责。这块详细用法我之前写过：[用Subagents打造Claude Code专业开发团队。](<https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247483687&idx=1&sn=7e14d5bd5dafc9e8db0181687d3da5bc&scene=21#wechat_redirect>)

## Agent view：分派出去，回头再看

如果你手上有好几件互不相干的活，又不想盯着每一步，那是Agent view的场景。

命令是`claude agents`，目前还挂着research preview（研究预览）的牌子。它开出来是一整屏，每个后台会话占一行，谁在跑、谁卡住等你、谁干完了，一眼看清。

关键点在于，这里每个会话都是一个完整的Claude Code对话，不挂在终端上也照跑。你派一个修bug、一个审PR、一个写论文，分派出去接着干别的，哪行亮了再回来。想看某个会话进展到哪了，按空格瞄一眼最新输出，真要接手就回车整个切进去聊，聊完再退出来即可。我用下来最舒服的就是这个节奏，不用一直盯着滚屏。相当于一个多会话管理的Agent面板。

而且Agent view会自动把每个会话塞进它自己的git worktree，文件层面天然隔离，几个会话互不踩脚。这点和subagent不一样，它回报的对象是你，不是某个主会话。具体用法可以翻看：[Claude Code发布Agent View功能，多会话并行管理一屏搞定。](<https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247485661&idx=1&sn=b14ea132f3795be093dabed5242c1dc5&scene=21#wechat_redirect>)

## Agent teams：Claude当leader带一队人

Agent teams是让Claude当leader，底下带一队teammates一起干。

和前面两个最大的不同，是队友之间能直接通信。他们共享一个任务清单，各自认领任务，还能互相发消息、互相质疑。Subagents只能闷头干完汇报结果，但实际团队干活时，队友是可以吵起来的。

这东西默认是关的，得在settings.json里把`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`设成1才能用，官方明确标了experimental。一队建议3到5个人，再多协调成本就上来了，token也烧得很猛，毕竟每个队友都是一个独立的Claude实例。

还有一点需要提醒的就是，Agent teams不给队友做worktree隔离。所以你得自己把任务切开，让每个人负责不同的文件，不然俩人改同一个文件就互相覆盖。它最适合那种需要讨论的活，比如几个人从不同角度审同一个PR，或者各执一种假设去debug，吵着吵着就能精准定位根因。另外也建议Agent teams结合着tmux一起用。这块我写过：[Claude Code + Agent Teams，并行任务的最佳实践。](<https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247485577&idx=1&sn=5f48937e411001465056a1c9278ed306&scene=21#wechat_redirect>)

## Dynamic workflows：让脚本来编排

到了动态工作流（dynamic workflows），拿主意的换成了一段脚本，不再靠Claude一轮一轮地临场判断。

它本质是一段JavaScript，由Claude替我们写，专门用来大规模调度subagents。前面几种方法，都是Claude一轮一轮决定下一步派谁。workflows把循环、分支、中间结果都搬进了脚本变量，Claude Code的上下文里只剩最后那个答案。

这带来两个实打实的好处。一是量级，一次能跑几十到几百个agents（运行时限制是最多16个并发、单次总共1000个），适合全库扫bug、500个文件的迁移这种一个会话根本协调不过来的活。二是质量，脚本能固化一套套路，比如让几个agents互相对抗着审查彼此的结论，把没扛住交叉验证的直接筛掉。

命令是`/workflows`，同样是research preview。Claude Code自带了一个`/deep-research`，丢给它一个问题，它自己多角度去搜、交叉验证、最后给你一份带引用的报告。跑起来之后会话不卡，你能在后台看着每个阶段开了几个agent、烧了多少token、跑了多久。

不过这玩意儿动辄几十上百个Subagents，token是真的费，所以同样是建议大家谨慎开团。这块我专门写过两篇，一篇讲 Opus_4_8发布_但Claude_Code动态工作流才是这次真正的大招，一篇讲底层的 Claude_Workflows，顶级harness框架。

## 还有worktrees和/batch

这里得单独澄清一下，免得大家把worktrees也当成一种并行Agents方法。

worktrees（git工作树）解决的是文件冲突，不负责怎么分活。一个worktree就是一份独立的工作目录加独立分支，共享同一个仓库历史。使用`claude --worktree feature-auth` 起一个会话，它就在隔离的目录里干，跟另一个会话井水不犯河水。它是给你自己手动开的并行会话用的，Subagents和Agent view也能搭配着它一起用。之前也专门写过相关文章：[Claude Code + Git Worktrees，并行开发最正确的打开方式。](<https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247484869&idx=1&sn=1d723494a0cef93f4a21d1d1e5913f8f&scene=21#wechat_redirect>)

`/batch`则是一个skill，它干的是把一个大改动拆成5到30个带worktree隔离的subagents，每个各开一个PR。它本质就是subagents加worktree的打包用法，算不上一种新的协调风格。

顺便提一句，还有几个东西听着像但其实不是并行。后台跑的bash命令只是不阻塞对话，没派出Agent；forked subagent（分叉子代理）是让子代理继承你当前的完整上下文，是一种派subagent的方式；routine是定时在云端跑一个会话，不在你本机并行。别被名字带偏了。

## 并行Agents如何选择，就问自己三个问题

四种Agents并行的对比表格如下：

方法讲完，然后就是什么情况下该用哪个的问题。Claude Code官方给的判断逻辑就三个问题，我觉得挺实用。

先看谁来协调这摊子活。Claude在一个对话里委托并收结果，用subagents；你想甩出去回头看，用Agent view；想让Claude当leader带队，就用Agent teams；想把Agents编排写成脚本反复跑，用workflows。

再看干活的几个之间要不要互相通信。Subagents只汇报给派它的会话，Agent view只汇报给你，只有Agent teams的队友彼此能直接通信。所以，需要协作、要讨论的任务，才上Agent teams。

最后看他们会不会动到同一批文件。会的话，就用worktrees把文件隔离开。Subagents和你自己开的会话都能各自挂一个worktree，但Agent teams不自动隔离，得你手动切分谁负责哪些文件。

如果觉得有用，点个赞或者在看，也方便更多朋友看到。

感谢您阅读我的文章。我是鲁工，九年AI算法老兵，AI全栈开发者，深耕AI编程赛道与AI科研赛道。
