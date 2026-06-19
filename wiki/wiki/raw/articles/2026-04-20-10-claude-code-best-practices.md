---
title: "84条Claude Code最佳实践，我挑了10条最值得用的"
url: "https://mp.weixin.qq.com/s/biLJNN8sh21WITEsdRHNIg"
source: "微信公众号「AI编程实验室」"
author: "鲁工"
fetched: 2026-05-27
sha256: 73a795aa7dcc03df
image_count: 9
---

大家好，我是鲁工。

前段时间在GitHub Trending上，看到一个叫claude-code-best-practice的仓库冲到了前几名。20k+Star，点进去一看，作者把Claude Code的最佳实践整理了84条，从Prompting到CLAUDE.md到Skills到调试，相当全面。

仓库地址：

https://github.com/shanraisshan/claude-code-best-practice

作为一个Claude Code老玩家，我周末花了两个多小时逐条看完。有些技巧我早就在用，有些看完觉得非常精妙，还有些让我发现自己一直在走弯路。我从里面挑了10条最实用的，每条都是我之前用过或者是验证过效果的最佳实践。

## 1 CLAUDE.md要精简，60行够用了

这是我踩过最大的坑。

去年我刚开始玩CC的时候，我往CLAUDE.md里塞了快500行，项目说明、代码规范、API文档全往里堆。结果Claude经常选择性忽略规则，尤其是靠后面的内容。

后来看到Boris Cherny（Claude Code之父）的解释才明白：前沿LLM能可靠跟随大约150到200条指令，而Claude Code的系统提示已经占了50条左右。留给你的空间其实没那么多。根据公开信息，HumanLayer团队的实践是控制在60行以内，300行是硬上限。

我后来的做法是只写Claude容易忽略的内容：构建命令、测试命令、分支命名规范、项目特有的架构决策。Claude读代码就能推断的信息，不用写在里面。如果规则实在多，就用.claude/rules/目录拆分成多个小文件，按需加载。关键规则可以用`<important if="...">`标签包裹，防止被忽视。

## 2 复杂任务先进Plan Mode

这条Boris本人说过不止一次。并且现在也应该是大家玩CC的共识了。

复杂任务开始前，先按Shift+Tab切两次进入Plan Mode。这个模式下Claude只做调研和规划，不写代码。等计划确认了再切回Normal Mode执行。

我早期也习惯直接上手干，结果经常Claude写了一堆代码方向不对，推倒重来。现在先Plan再动手，效率会高很多。Anthropic官方的推荐流程是四步：Explore → Plan → Implement → Commit。小任务可以直接开干，单稍微复杂点的功能一定要走Plan。

## 3 让Claude先采访你再动手

先给Claude一个简单的需求描述，让它用AskUserQuestion工具来采访你，把细节问清楚。采访完了，新开一个会话执行。

我做一个API的时候，Claude问了我"并发请求怎么处理"、"超时策略是什么"，这些我一开始压根都没想到。它提的问题往往能帮你发现没想到的边界情况。

关键是采访完要开新会话。采访过程产生的大量对话留在上下文里，反而影响后续执行质量。

## 4 对平庸方案直接要求重写

这条是Boris团队Tips里我最喜欢的。

Claude给了一个能用但不优雅的方案时，别在上面修修补补。直接说"knowing everything you know now, scrap this and implement the elegant solution"。Claude会基于它对问题的全部理解重新设计方案，我试了好几次，重写版确实比修补版好很多。

类似的还有"prove to me this works"，让Claude自己diff当前分支和main来证明改动正确。

## 5 贴bug说fix，别玩微操

这条可能是84条里最反直觉的。

遇到bug，把错误信息贴给Claude，说一个字"fix"。不要指导怎么修，不要猜原因，不要规定方案。Claude定位问题的能力比大多数人想象的强，你越微操越容易把它带偏。我自己的体验是，直接让Claude修的成功率在80%以上。

修了两次没搞定就别死磕，/clear 清上下文换个角度重来。Anthropic官方也建议纠正超过2次就该重启。

这条其实我之前也喜欢用力过猛，修bug的时候也补充描述一大堆，现在来看是多此一举了。

## 6 直接在提示词里说use subagents

在prompt里直接说"use subagents"，Claude会把任务拆给多个子代理并行处理。

代码审查和大规模重构时特别好用。根据公开资料，有人用9个并行子代理做code review，每个聚焦一个质量维度。我在做跨文件重命名时用过，Claude开了3个子代理分头干，比单线程快不少，主上下文也没被搜索结果污染。

这里有个小技巧：建subagents的时候，做功能特定的（比如"前端组件agent"），别做通用的"QA agent"。功能越具体，携带的上下文越精准，效果越好。

## 7 Skills做成文件夹，建Gotchas部分

很多人（包括之前的我）创建Skill就写一个SKILL.md完事。

仓库强调Skills应该是完整的文件夹结构：SKILL.md主文件加上 references/、scripts/、examples/子目录。这种渐进式披露才是Skills的关键，Claude只在需要时才读子目录内容，不会把所有东西一次性塞进上下文。

我把自己的论文写作Skill改成文件夹结构后，效果提升很明显。之前所有规范塞一个文件里，Claude经常漏细节。现在主文件只放核心规则和索引，语料库、检查清单放references/下。

还有一条长期特别有用的技巧：在每个Skill里建一个Gotchas（踩坑记录）部分，每次Claude犯错就把失败模式记进去。时间一长，这部分会变成信噪比最高的内容。我的论文写作Skill里记录了十几种AI味模式，加了之后初稿质量提升了一个档次。

## 8 上下文到50%就手动compact

这条是工作流层面最关键的。

Claude Code有个所谓的agent dumb zone，上下文使用超过60%到70%时表现会明显下降，容易忽略指令，代码出低级错误。仓库建议到50%就手动执行/compact，别等系统自动compact，那时候往往已经晚了。

我之前经常一个会话干到底，或者到10%左右才会手动compact，我尝试50%就compact或者/clear换新会话，效果好很多。可以用/statusline 实时监控使用率，Boris团队的脚本还会用颜色编码提醒：绿色安全、黄色注意、红色危险。

## 9 走偏了按Esc Esc回滚

Claude走偏了怎么办？大多数人的本能是在当前会话里纠正。

仓库建议按两下Esc（或 /rewind）直接回滚到上一个checkpoint。在同一个上下文里修偏差，往往越改越偏，因为前面的错误推理还在上下文里，Claude会被自己的错误思路带着跑。

我现在的习惯是：走偏了Esc Esc回滚，同一个问题偏了两次就 /clear重来。

## 10 小任务别用复杂工作流

仓库里有条特别清醒的建议：原生Claude Code处理小任务，比任何复杂工作流都好。

我之前犯过这个错，改个变量名都走完整的Plan → Execute → Review流程。其实直接说一句话就行了。那些复杂工作流（Superpowers、Spec Kit、BMAD-METHOD之类）是为多文件多步骤的大任务设计的，三五分钟能做完的事，原生Claude Code最快。

以上10条是我从84条里筛出来的，完整版在仓库里，值得通读一遍。作者还整理了8个主流工作流的横向对比和Boris Cherny的全部访谈链接，信息密度相当高。

  

  

如果觉得有用，点个赞或者在看，也方便更多朋友看到。我是鲁工，九年AI算法老兵，AI全栈开发者，深耕AI编程赛道。感兴趣的朋友也可以加我微信（louwill26_）交个朋友。
