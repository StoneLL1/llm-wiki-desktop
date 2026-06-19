---
title: "13.2w star的everything-claude-code，堪称全网最强Claude Code开源插件库"
url: "https://mp.weixin.qq.com/s/4Pls9vl7g388Qyn3tSJ7Dg"
source: "微信公众号"
fetched: 2026-04-18
sha256: 21cd50f5722dd198
---

大家好，我是鲁工。

Claude Code现在是大家首选的Vibe Coding工具，围绕Claude Code的开源生态也越来越丰富。

我之前介绍过superpowers和claude-code-best-practice两款开源插件和仓库：

[Superpowers，一款强大的软件开发工作流skills](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247484409&idx=1&sn=990483ac594610c839d478d209ca87d0&scene=21#wechat_redirect)

[84条Claude Code最佳实践，我挑了10条最值得用的](https://mp.weixin.qq.com/s?__biz=MzE5ODY5MDU4Mw==&mid=2247485250&idx=1&sn=d1d12df5379a061f9455a88de9eec966&scene=21#wechat_redirect)

其中，claude-code-best-practice在GitHub有29.4k star，而superpowers更猛，直接干到了131k star，简直吓人。

但是，我这两天又发现了一款更猛的Claude Code开源插件项目，叫做everything-claude-code，有18.9k的Fork和13.2万Star。竟然还有比superpowers更猛的插件项目。

仓库地址：

https://github.com/affaan-m/everything-claude-code

我从去年8月份开始用Claude Code，到现在才大半年，从最早手写CLAUDE.md到后来用superpowers框架，再到自己攒了一套skills体系，踩过的坑不少。

所以看到这个仓库的时候，我的第一反应是这star怎么可以这么高，然后就是：这东西跟我现在的配置比，到底强在哪？我花了点时间仔细翻了一遍，顺便也实际装上试了试，这篇文章就简单聊一下everything-claude-code。

简单来说，everything-claude-code是一个Claude Code的插件集合（也可以叫agent harness优化系统），作者Affaan Mushtaq是Anthropic黑客松获奖者，这套配置是他10个多月实战打磨出来的。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/MHVsz9lwuhWjcsbbVYiagtfL3yg4tbibOZQeaJ2Q94pgib5VZ6C8IWghMsjONuo6JTjx1xKZ0f5sxEOrQ74y2xQpfDOkicjEetjRFIA7MWViclhw/640?wx_fmt=png&from=appmsg)

规模有多大？36个专用subagent，150多个skills，68个命令，覆盖10种编程语言的rules。简单来说就是一个开箱即用的Claude Code增强包，你装上之后，Claude Code在代码审查、测试驱动开发、安全扫描、架构设计这些场景下的表现会有明显提升。

作者也给这个项目做了个主页：

https://ecc.tools/

![image](https://mmbiz.qpic.cn/mmbiz_png/MHVsz9lwuhX0vNP9IVtqLicQnG9BwLBa7dibWoA8diblKmt77p4hA04YfpCZC4gvEYR9mCIaLD0AJakGkzdbE13OvEpdxO3icbpVQdckVIHOBcs/640?wx_fmt=png&from=appmsg)

更关键是，它不只支持Claude Code。Codex、Cursor、OpenCode、Antigravity、Gemini这些主流AI编程工具都能用，一套配置通吃所有平台，这个跨平台能力也是拉满了。

安装也很方便，如果不需要安装其中的rules的话，可以直接使用/plugin命令来安装：

- 

- 

`/plugin marketplace add affaan-m/everything-claude-code``/plugin install everything-claude-code@everything-claude-code`
```

![image](https://mmbiz.qpic.cn/mmbiz_png/MHVsz9lwuhXjSQNzGsjQiaZGnFQqFfL2suzN7PAq3FEaPLJ9odX3Jd6A3p0NjG9PlzHwWIMcLM9v8iaEGn09xs4mZiahgc9BZAs4J6zLL9wlaQ/640?wx_fmt=png&from=appmsg)

需要安装rules的话，clone下来手动安装也行：

- 

- 

- 

- 

`git clone https://github.com/affaan-m/everything-claude-code.git``cd everything-claude-code``npm install``./install.sh --profile full`
```

Windows用户用PowerShell脚本`install.ps1` 就行。而且从v1.9.0开始支持按需安装，比如你只写TypeScript和Python，可以只装这两个语言的规则和agent，不用全量安装。

翻完整个仓库，有三个点让我觉得确实想得比较深。

**pass@k和pass^k 验证指标。**这是我在其他Claude Code配置库里没见过的概念。具体来说就是用一种量化方式来评估agent生成代码的可靠性。pass@k是跑k次里至少有一次通过的概率，pass^k是k次全部通过的概率。作者给了一组数据：当k=3时，pass@k能到91%，但pass^k只有34%。这组数字的意义在于，它能帮你判断一个agent的输出到底是稳定可靠，还是碰运气碰对的。我个人觉得这个思路相当实用，尤其是在做关键代码的自动化生成时，光看能跑通远远不够，得看每次都能跑通才算稳。

**AgentShield安全集成。**这是v1.6.0引入的功能，内置了102条安全规则和1282个安全测试。你可以用`/security-scan`命令让agent在提交代码前做一轮安全审查，覆盖常见的OWASP漏洞类型。这对于做生产级别的项目还是相当有用的。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/MHVsz9lwuhVpbuZ7Dueu9sXSSUyWzGF6y4ovjicp279q1wZhb5SlQLVXsQMyr7a8arnCs3XQWt6sXmGAO97bhL3sBJRsvT6W9YgbrbEm02Cg/640?wx_fmt=png&from=appmsg)

**沙盒化subagent。**everything-claude-code给每个subagent做了工具权限限制。比如代码审查agent只能读文件和搜索，不能执行写操作。这个设计能有效防止agent在执行复杂任务时误操作，特别是多agent并行工作的场景下，这层隔离非常有必要。

聊到Claude Code的开源插件，superpowers（作者Jesse Vincent）肯定是另一个绕不开的项目。这两个库我都在用，路线完全不同。

![image](https://mmbiz.qpic.cn/mmbiz_png/MHVsz9lwuhVqeiaWofZ5E6YhD4QNSn1HHCSh1X4ErXCrt0hENc94TFpGQMDcU8QP7vmONDOGIN4Na1M7qfuRmcxZPw2wTzTiahxdkYKLgPooQ/640?wx_fmt=png&from=appmsg)

superpowers像个资深架构师，它的核心不在于给你多少工具，而是定义了一套完整的开发方法论。从brainstorm到plan，再到TDD实现，最后code review，整个流程是固定的。你用了superpowers之后，Claude Code会自动在写代码之前先跟你讨论方案，然后开git worktree做隔离开发，写测试再写实现，最后做review。这套流程对代码质量的提升和软件工程的约束是实打实的。

![image](https://mmbiz.qpic.cn/mmbiz_png/MHVsz9lwuhXV0CNYzmcibzjTwNbKian1OGWCC612fsLxaVoNcfZu1Je8SP3ZiaOd0KiaFDIBm9Us4GajiaXlSqT5D2He44GG0hBediaZU9E1XcpTg/640?wx_fmt=png&from=appmsg)

everything-claude-code更像个装备齐全的工具箱。它不怎么管你的开发流程，但是给你提供了足够多的agent和skills，让你在各种场景下都能找到趁手的工具。需要做TypeScript代码审查？有专门的agent。需要做数据库迁移？有对应的skill。需要做E2E测试？Playwright的配置都给你准备好了。

建议是两个都装一下，superpowers管流程，everything-claude-code管工具。superpowers确保开发节奏是对的（先测试后实现），everything-claude-code确保在每个具体环节都有最好的agent可用。

简单做个对比就是：

everything-claude-code的优势在深度和工程化，36个agent都是针对具体编程场景打磨的，安全扫描和验证指标这些功能也比较独特。superpowers的优势在方法论和流程约束，TDD和code review的流程设计非常成熟。claude-skills胜在广度和跨领域覆盖。

如果要给everything-claude-code打个分的话，我给9分。

加分项很明确：规模大、覆盖广、工程化程度高，pass@k这种验证思路确实有创新，跨平台支持也做得到位。从一个Anthropic黑客松项目发展到现在这个体量，能看出作者在持续迭代。

扣分的地方主要是学习成本，36个agent、150个skills、68个命令，光是搞清楚每个的用途就得花不少时间。其次是配置偏Go语言（作者的主力语言），其他语言的agent和rules虽然也有，但深度上有差距。

我的建议如下：如果你是Claude Code的重度用户，每天都在用它写代码，everything-claude-code值得装上。如果你更关心开发流程和代码质量，superpowers可能更适合你。两个都装也完全没问题，它们的定位本身就不冲突。

如果觉得有用，点个赞或者在看，也方便更多朋友看到。

感谢您阅读我的文章。

我是鲁工，九年AI算法老兵，AI全栈开发者，深耕AI编程赛道。感兴趣的朋友也可以加我微信（louwill26_）交个朋友。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/MHVsz9lwuhVHoC2oaSxvHo0abDZFiarFSK2ACKCtbEiaM7To6TEIqADmCtcvYneA30JicMhgMib75UIqVAPxWCg79hzkZ8PkZbJXK1rJ4ZibX8lA/640?wx_fmt=jpeg&from=appmsg&wxfrom=5&wx_lazy=1&tp=wxpic#imgIndex=2)
>/ 作者：鲁工