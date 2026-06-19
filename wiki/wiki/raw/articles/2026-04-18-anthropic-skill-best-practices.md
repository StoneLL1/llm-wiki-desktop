---
title: "Anthropic内部编写Skill最佳实践"
url: "https://mp.weixin.qq.com/s/SgdfFcZ2UrpahT5eW_ibJQ"
source: "微信公众号"
fetched: 2026-04-18
sha256: f1c215b232ebb5ff
---

这是一篇摘录自 x 上的热推：，Claude Code工程师描述他们内部是如何构建和使用 skill 的。这篇推有很多干货，完全可以当做 skill 编写时候借鉴的白皮书。

文章Anthropic内部已经用了几百个Skills来加速开发。我觉得绝不是夸张，因为就这两周开始有意识积累 skill 以来，我们也已经构建了大概 20 个 skill 了。

# 干货一：skill 的 9 大类型

我们都会写 skill，但是写的不够多，是不会有这种“skill 有哪些类型”的概念的。对比下自己写的 skill，也看看大佬们是如何给 skill 分类的：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/OXxbvcqhBTBA7lNNwjtoRP7OVCCBicnL1xiaulLOJP6MgwZZWTtGfxSaxIib64WK8KmhPd73OgeX0b331b6IDWD8HN2FhnYBwZVW7uynZzybus/640?wx_fmt=png&from=appmsg)

## 

## 

## 

## 

## 

## 

## 

## 

## 类型 1：库&API参考

怎么正确用内部/外部库、CLI、SDK，附带代码片段和“坑点列表”。

这个类型我就基本上没写过，但是一直在烦恼。

比如如何调用第三方接口，如何使用内部库，这些都是技巧，需要总结的。作者举了几个例子，比如像支付相关的类库的使用：

billing-lib — your internal billing library: edge cases, footguns, etc.

## 类型 2：产品验证

如何测试代码是否正确（常配合Playwright、tmux等工具），甚至可以让Claude录视频或做断言验证。

这个我也确实从来没有写过，如何让 agent 做完备的集成测试，估计需要用到这类的 skill。作者举的例子：

signup-flow-driver — runs through signup → email verify → onboarding in a headless browser, with hooks for asserting state at each step

教 agent 如何测试整个注册流程。

## 类型 3：数据获取 & 分析

连接监控系统、数据库，快速跑查询、对比cohort等。

这个最近编写很多，一个公司总是有很多系统，监控，日志，db，es，wiki，git 等，你要站在 agent 的界面上，把这些平台打通，就会有一些意向不到效果。

比如分析一个值班问题，模型会聪明到先去监控平台分析监控，然后结合代码找出关键日志，再去日志系统查出日志链路，最后得出结论，总结到 wiki 上。

## 类型 4：业务流程 & 团队自动化

一键完成重复工作（如发周报、建ticket、standup总结）

这个还没具体实践，其实发周报不难，收集发周报的素材最难，一周做的事情除了需求池，还有会有微信群来的需求，甚至还有口头传达的，如何收集这些汇总成周报，其实不容易。

## 类型 5：代码脚手架 & 模板

快速生成符合公司规范的新服务、迁移文件等。

这个能想象到非常有用，但是基本是在新项目新服务的时候才有用。

## 类型 6：代码质量 & Review

自动审代码、强制风格、找bug

code review 其实是非常繁琐的事情，需要你理解上下文，然后分析逻辑，找出别人编写代码的漏洞，这个使用 skill 来补充上下文，能分析到很多你人工根本看不出来的 bug，非常有用。

## 类型 7：CI/CD & 部署

监控PR、自动部署、回滚、cherry-pick等

这个理想化当然是直接一键变更编译发布部署，但是目前还不敢真实在生产环境使用。

## 类型 8：Runbooks

根据告警/错误，自动多工具排查并输出结构化报告。

这个写了不少，值班的各种告警，错误，排查是一个稳定业务需要耗费不少精力做的事情，每次有报警和错误，就交给模型，来自动分析，这类 skill 非常有用。

## 类型 9：基础设施运维

清理孤儿资源、依赖管理、成本分析等（带安全护栏）

估计在运维层面非常有用，不过我看作者给出的几个例子，都是需要人工介入的，还有 user confirms的过程。

-orphans — finds orphaned pods/volumes → posts to Slack → soak period → user confirms → cascading cleanup

# 干货二：写好Skills的实战Tips

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/OXxbvcqhBTDic5pf6Fdf75IhcvQKdDEibEgwTtuLgc9ZtENRZPI0yfj3YTP13xHVKfqFbtWDwnEolsPiaVa1a7wL4icYlfucgB9xNqQ9A8ibNwIo/640?wx_fmt=png&from=appmsg)

也是有 9 个技巧，这种强制对齐很吉利的数，我认为是很高级的写推技巧。。。

这节的内容通过截图基本一看就明白。

## 技巧1：别说废话

模型已经懂很多通用知识，重点写只有你们公司才知道的坑（Gotchas），这是Skills里含金量最高的部分。

![image](https://mmbiz.qpic.cn/mmbiz_png/OXxbvcqhBTBBYW967APpdaf0asziaZQQq6Hp0yYEpS1nvNRPicounrPdbEH1E8PQmBe7F7V5b62Hx0nLzXuT4lXpzguOazHBTXxYTib6Zw69kg/640?wx_fmt=png&from=appmsg)

## 技巧2：用文件夹做“渐进式披露”

把详细API、模板、脚本拆到子文件夹，Claude需要时再读，避免一次性塞太多上下文。

![image](https://mmbiz.qpic.cn/mmbiz_png/OXxbvcqhBTBkY4W4Z5FlOznEPRIIGw62Qn8DXBhqYdH3BlNlWznwKdNiayuqUACOkHdGTiajVwPoibMjf5pLGXYOdhOgPxu50ns6r4xJqrnbk0/640?wx_fmt=png&from=appmsg)

## 技巧 3：别把Claude“绑死”

指令要给信息，但留灵活性，别写得太死板

![image](https://mmbiz.qpic.cn/mmbiz_png/OXxbvcqhBTDyPOjRzbkThMZHaaqkliaHsYhtUQhlIj8dNuBwd4AZldhlvsicBnjA23TkDhwyMVFUka7EIMv6SE9ibyiaG5bgCGXSOBWnpB5YAww/640?wx_fmt=png&from=appmsg)

## 技巧 4：配置 & 记忆

用config.json存用户设置；用日志文件或SQLite存历史数据（有${CLAUDE_PLUGIN_DATA}稳定目录）。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/OXxbvcqhBTBGOFQCiarxhyuAEJ1dlHdaF4Cibr3UY4ibfbicDjThLNichBg1UMfPyozPjfeNaiarJFzebB7SfWr00doBLWIbZNUdFWxAduYzvHObM/640?wx_fmt=png&from=appmsg)

## 技巧 5：存脚本而不是每次都重写

把常用函数库放进Skill，Claude直接调用就行。

![image](https://mmbiz.qpic.cn/mmbiz_png/OXxbvcqhBTAPuybMpWPds60CicceeLQldIgVxBpE4BpiakBfY00X6XUPOzvFLEAFtlkWLGYJqMc2V8arRk2w86RicnwbHrKE7ibnNkxxAUohGKQ/640?wx_fmt=png&from=appmsg)

## 技巧 6：按需钩子（On-demand Hooks）

比如只在特定场景下才开启“禁止rm -rf”的保护。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/OXxbvcqhBTDRNq6UdHiagwO9cNQFXzVp7frYFNHiclgia4zqxsjUIpIR2icgEv41UGQWoibmTdLiamXHZiaXo8FpjYj6WMnpCiaXFnZDdRvSpbmVZzQ/640?wx_fmt=png&from=appmsg)

## 技巧 7：分发方式

小团队直接把Skills放进仓库（.claude/skills）；大规模用内部Plugin市场（可PR提交、审核）

简要说，就是直接进入 git 仓库。

## 技巧 8：测量效果

用PreToolUse钩子统计每个Skill的使用频率，删掉没用的。

作者最后整体的结论：Skills超级强大，但还在早期，大家都在摸索。最好的方式就是多实验、多迭代——很多好Skills都是从几行Gotchas开始，慢慢被团队完善起来的。

整体来说，这是一篇干货满满的文章。