---
title: "重磅！Anthropic内部Skills经验公开了！"
url: "https://mp.weixin.qq.com/s/FCKdRHxi9c6Vby2ZRKjVTw"
source: "微信公众号"
author: "Datawhale"
account: "Datawhale"
pub_date: 2026-06-07
fetched: 2026-06-07
---

# 重磅！Anthropic内部Skills经验公开了！

**作者**: Datawhale | **公众号**: Datawhale

Anthropic 自己内部是怎么用 Claude Code Skills 的，这次终于公开了。

他们把内部团队的用法做了一次完整复盘：Skills 分成哪 9 类、哪类最值得花力气、怎么写才真的有用。这些经验之前只在 Anthropic 内部流转，现在一次讲清。

今天把这个经验帖的核心干货给你梳理清楚。

## 一、先把 Skill 理解对

Anthropic 先纠正了一个很常见的理解。Skill 不只是几段提示词，它更接近一个围绕任务组织起来的文件夹。

这个文件夹里可以放 `SKILL.md`，也可以放参考文档、脚本、模板、示例、hooks，甚至放会被后续任务继续读取的数据。Claude 调用 Skill 时，拿到的其实是一套完成任务所需的工作材料。

这个定义很重要。因为很多团队真正缺的，从来不是"再补一段提示词"，而是把那些已经验证过的做法、容易错的细节、常用脚本和固定流程，一次整理好，后面反复复用。

## 二、Anthropic 把内部 Skills 归成了 9 类

Anthropic 盘了一遍内部的 Skills，最后大致分成了 9 类。这 9 类连起来看，其实很像一条完整的软件工作流，从补知识到写代码，再到验证、部署、排障和运维。

![Anthropic 内部 Skills 的 9 类分布（图源：Anthropic 官方博客，下同）](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS9poQ3rmVb8Dx3lOSOszQ76qmvOLHdZYUibtA0UWHKpbASm8fDVaLYpk9FN40AOxSN8Z18KvTEicqr0tkwicY1eXoJ7OmRP49aNIw/640?wx_fmt=png&from=appmsg)Anthropic 内部 Skills 的 9 类分布

### 前三类：给模型补知识、补验证、补数据

第一类是 library 和 API reference，给模型解释某个库、CLI 或 SDK 在团队内部到底该怎么用，把容易用错的规则和 gotchas 写清楚。

第二类是 product verification，负责判断产出有没有真的工作，比如在无头浏览器里完整跑一遍注册和结账流程。Anthropic 明说这类对输出质量提升最明显，值得让工程师专门花一周打磨。

第三类是 data fetching and analysis，连着数据仓库和监控系统，把取数方法、字段约定和常见分析路径封装好，模型不用再去猜表结构和字段名。

### 中间三类：开始接住团队里的日常流程

第四类是 business process and team automation，把重复发生的团队流程压成一个命令就能跑的工作流，比如只输出相对昨天增量的 standup，或固定格式的周报。能看出来，Skill 接住的不只是代码任务，也包括协作任务。

第五类是 code scaffolding and templates，生成那些有固定骨架、但又带着大量自然语言约束的代码，比如新 service 或迁移文件。这正是纯模板引擎覆盖不了的部分。

第六类是 code quality and review，让代码尽量符合团队的质量标准。典型例子是拉一个"新鲜视角"subagent 来挑错的 `adversarial-review`，这类能力还能做成 hook 接进 CI。

后三类：已经连到生产环境了

第七类是 CI/CD and deployment，把代码从开发态推到上线态。比如 `babysit-pr` 会盯完一个 PR 的全过程，`deploy-<service>` 会把 build、放量、错误率对比和回滚条件串成一条链路。

第八类是 runbooks，入口不是"我要写什么"，而是"现在出了什么症状"。报警、Slack thread、request ID 进来，它负责映射到该用哪些工具、查哪些路径，最后给出结构化结论。

第九类是 infrastructure operations，处理资源清理、依赖治理和成本排查这类例行操作。这些动作常带破坏性，所以 Skill 里要写清 guardrail，先通知、再确认，最后才真正执行。

## 三、Anthropic 真正强调的，不只是"会写"，更是"写对"

把 9 类梳理完以后，Anthropic 接着讲的，其实是几条更底层的判断：什么样的 Skill 更稳，哪些地方最值得花力气。

### 好的 Skill，往往都很聚焦

Anthropic 说得很直接，最好的 Skill 往往都很聚焦。能清楚落进某一类里的 Skill，通常更稳；试图同时覆盖太多目标的 Skill，反而更容易把模型带乱。

这个判断很有参考价值。因为很多团队一开始做 Skill，最容易犯的错误就是想"一次做大做全"，结果最后既不好触发，也不好维护。

### 所有类型里，他们最看重「验证」

在所有类型里，Anthropic 特别强调 verification。因为模型最容易给人一种"已经做完了"的错觉，而真正容易掉链子的地方，恰恰是最后那一步验证。

原文甚至建议，值得让工程师单独花一周，把验证类 Skill 做到足够好。这个投入听起来很重，但如果它直接影响结果质量，其实很划算。

他们还给了两个非常实用的建议。一个是让 Claude 录下自己测试过程的视频，这样你能清楚看到它到底测了什么。

另一个是在关键节点加程序化断言。状态有没有变化，事件有没有真正落库，最终页面是不是到了目标状态，都尽量不要只靠"看起来差不多"。

### 真正有价值的内容，往往是 gotchas

Anthropic 对 Skill 里的内容优先级也讲得很清楚。最有信号量的部分，通常不是通用步骤，而是 gotchas。

因为 Claude 本来就会写代码，也会读代码库。那些"默认它也会做"的东西，写进 Skill 里只会增加上下文，不一定增加价值。

真正值得写的，是那些会把模型从默认思路里拽出来的细节。比如 `subscriptions` 表是 append-only，要找最高 `version`，不能只看最新 `created_at`。

再比如同一个字段，在 API gateway 里叫 `@request_id`，到了 billing 服务里叫 `trace_id`。还有 staging 返回 200，也不代表 Stripe webhook 真处理成功了，还得去看 `payment_events` 里的真实状态。

这类信息单看都不大，但一旦出错，最后结果就会偏。Skill 的价值，很多时候就体现在这些"团队里人人知道，但模型默认不知道"的地方。

## 四、Skill 到底该怎么写

前面 Anthropic 团队讲完了"先做什么"，这一段他们共享了"具体怎么写，才能让 Skill 真起作用"的内部经验。

### 1. 别把显而易见的话再写一遍

![别把模型本来就会的东西再写一遍](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSictR6ibNFEZ0cmLTvJgMHfgrI948soialwGwib7HiboXGvzst7J4BfbSL3bk5dgGicHnaN3X14bcX6JJyhAiaM4usKGRYm13V91hJGrY/640?wx_fmt=png&from=appmsg)别把模型本来就会的东西再写一遍

第一个细节是别把显而易见的话再写一遍。Skill 不是给人看的摘要，它要补的是模型默认拿不到、或者默认容易走偏的信息。

Anthropic 提到过一个前端设计 Skill 的例子。它的价值不在于教 Claude 怎么写前端，而在于补充团队通过和客户反复迭代后沉淀下来的"设计品味"和避坑点，比如少用一些过于套路化的字体和配色选择。

### 2. SKILL.md 更像目录，不该写成大杂烩

![SKILL.md 做目录，细节资料分文件按需加载](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSicGkDfXyZIMDPaQgsYP6RSyGCyas3FNLPsriaDyxtWRcFjMP1WoXqKX0lEvibVmQZ3JkdibWe5PuIDGj6nPxBnibkaqhkZeYfSNEXg/640?wx_fmt=png&from=appmsg)SKILL.md 做目录，细节资料分文件按需加载

第二个细节是 `SKILL.md` 不要写成大杂烩。更好的做法，是让它做目录和路标，把具体资料按需分发到别的文件里。

比如任务卡住了，再去读 `stuck-jobs.md`。API 的函数签名和用法示例，可以拆进 `references/api.md`。

如果最终要产出一份 markdown 文件，那模板可以放进 `assets/`。脚本、参考资料、例子也都可以分目录放好，让 Claude 需要时再去取。

这套做法对应的就是 Anthropic 说的 progressive disclosure。文件系统本身，也是一种上下文工程。

### 3. Skill 不要写得太死

![留出判断空间，别把 Claude 钉死在固定轨道上](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSicRJ7SMlNFnICURvCXxl2dEuyGP3KlEQpsqLGo5XYsFzy4IQnlxicKsZgDvQbMXdMcHofHmrTbibhfib0Rvqm0O6pzBcr3ibFAF9pc/640?wx_fmt=png&from=appmsg)留出判断空间，别把 Claude 钉死在固定轨道上

第三个细节是别把 Skill 写得太死。你需要给 Claude 关键规则，但也要给它足够的适应空间，不然 Skill 一复用，就容易在别的具体情境里卡住。

### 4. setup 要提前想好

![把用户上下文放进 config.json，缺了就先问](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cS9zOTzYzKWaRNhLlSXAjuPQk3bS2O4Q0ky51Wo9VZSq1iaZUyCSvjzibUrwH4hRHBFVW2tNaYSTl79Gic7KPbKiaX1AAbnjM3HEEZQ/640?wx_fmt=png&from=appmsg)把用户上下文放进 config.json，缺了就先问

第四个细节是提前想好 setup。很多 Skill 真跑起来时，会缺一些来自用户的上下文，比如 Slack 要发到哪个频道。

原文建议把这类配置放进 `config.json`。如果配置还没建好，Claude 就先问用户；如果需要结构化、多选式提问，还可以直接调用 `AskUserQuestion` 工具。

### 5. description 要直接服务触发

![description 写给模型看，决定 Skill 会不会被触发](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS9oJX1VjnEQ0ruVEuxkuW0icvyEYAhlricQbrKXtaDFlOiac7iagN1P2zQNsLTp336pyVAxTQmsb8Y7aSpt6avQb3VVVYdfcBPz1NU/640?wx_fmt=png&from=appmsg)description 写给模型看，决定 Skill 会不会被触发

第五个细节是 description 要写给模型看。Claude Code 一开局会先扫描所有 Skill 的名称和 description，再判断这次请求有没有可用 Skill。

所以 description 不是摘要，而是触发条件说明。用户可能会说什么关键词，会上传什么文件，什么场景下应该激活这个 Skill，都应该直接写进去。

原文还举了一个很小但很典型的点。像 "babysit" 这种触发词，就应该直接出现在 description 里。

## 五、Skill 用深之后，会先长出记忆、脚本和 hooks

Anthropic 专门留了一节讲这个。很多 Skill 一开始只是几行说明，越用越多之后，最先长出来的就是记忆、脚本和 hooks 这三样。

先说记忆。像 `standup-post` 这种 Skill，可以把每次输出都记进 `standups.log`，下次运行时先读历史，再判断今天和昨天相比到底变了什么。

这种记忆可以很简单，用 append-only 文本或 JSON 就够了；也可以复杂一点，直接用 SQLite。原文还提到，可以用 `${CLAUDE_PLUGIN_DATA}` 这个环境变量，拿到一个稳定的持久化目录来存这些数据。

![让 Skill 自己记日志，下次运行先读历史](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cS8GibnQo1cPvBzuD6eeovy8CGr44tIP51icsny7rvkqfzEaZMDKbA1LicDJ3tKqxpXMhonzloZWGxSxgaJicARpgJgQoSKkALHrTt8/640?wx_fmt=png&from=appmsg)让 Skill 自己记日志，下次运行先读历史

再说脚本。Anthropic 的判断很明确，能给 Claude 的最强工具之一，其实就是代码本身。

如果你把常用的数据抓取函数、分析函数或者操作脚本预先放进去，Claude 就不用每次从零重写样板代码，而是把更多回合花在"怎么编排"和"下一步做什么"上。

比如数据分析类 Skill 可以直接带一组 helper functions。这样当用户问"周二到底发生了什么"时，Claude 就能临时拼出一段更复杂的分析脚本，而不是先花很多力气重建基础设施。

![预置脚本，让 Claude 把回合花在编排上](https://mmbiz.qpic.cn/sz_mmbiz_png/zW6S9vt0cSicHMYHVibb6nGBwKwru1PiaUGUU6tozicrS7NrLTsMib8BWNs8QOAScmWhQiaEPkWwPPniajiaBxM5GJ6egic2IZHbgiaicOwyKlpYuRJLCs/640?wx_fmt=png&from=appmsg)预置脚本，让 Claude 把回合花在编排上![helper functions 之上，临时组合出更复杂的分析](https://mmbiz.qpic.cn/mmbiz_png/zW6S9vt0cSicOJgDB2DGT4HoZ5MLNdUrNVba4r0IjYLMibkxggNam8WVbicKLARiaEbKcfHJiaGSyg2R6VAAyxjicHr3fz1Ll1HA6NdzZgw7NsYfc/640?wx_fmt=png&from=appmsg)helper functions 之上，临时组合出更复杂的分析

最后是 on-demand hooks。它们只在 Skill 被调用时生效，而且只在当前会话里存在。

原文举了两个很典型的例子。`/careful` 会拦住 `rm -rf`、`DROP TABLE`、force-push、`kubectl delete` 这类高风险操作；`/freeze` 则会阻止对指定目录之外的 Edit 和 Write，适合排障时防止自己一边加日志一边顺手改坏别的地方。

## 六、当团队开始大量用 Skill，后面就是分发和治理

这是单个 Skill 长大的下一步。Skill 一旦开始在团队里扩散，问题就不只是"怎么写"，还会变成"怎么发给别人用、怎么持续管理"。

两条主路线：repo 内 check-in 和插件 marketplace

一种是直接把 Skill check in 到 repo 里的 `./.claude/skills`。对于规模不大的团队、或者只在少数代码库协作的团队，这已经很好用。

另一种是做成插件，用内部的 Claude Code Plugin marketplace 来上传和安装。团队一变大，这种方式的优势会更明显。

原因很简单。每多 check-in 一个 Skill，模型可见的上下文负担就会多一点；而 marketplace 可以把安装权交给团队成员自己，也方便顺手做 setup 流程。

Anthropic 在治理上也没有一上来就搞中央审批。更常见的方式，是谁有 Skill 想给大家试，就先传到 GitHub 里的 sandbox 文件夹，再发到 Slack 或别的渠道让别人试用。

等这个 Skill 真有了 traction，再由 Skill owner 提 PR，把它正式移进 marketplace。这个流程很轻，但很符合 Skills 这种靠真实使用慢慢长出来的东西。

### Skills 之间也可以互相组合

原文还提到一个很有意思的方向，就是 skill composition。比如你可以有一个文件上传 Skill，再有一个 CSV 生成 Skill，后者生成完文件后，再去调用前者完成上传。

这类依赖管理目前还不是 marketplace 或 Skill 机制里的原生能力，但只要在 Skill 里直接引用另一个 Skill 的名字，模型在安装了它们的前提下，照样能把链路串起来。

### 还可以做 usage measurement

Anthropic 也提到，他们会用 `PreToolUse` hook 记录公司内部的 Skill 使用情况。这样就能看到哪些 Skill 很热门，哪些触发明显不足。

这类数据其实很有用。因为 Skill 做出来以后，真正的问题常常不是"能不能运行"，而是"会不会在该触发的时候被想起来"。

## 写在最后

Anthropic 在文章结尾提到一个细节：他们内部最好的 Skills，一开始往往只有几行字和一个 gotcha，用得越多，才补得越完整。

这句话基本可以当成上手指南。写 Skill 不用追求一步到位，先把验证方法写清楚，把真正踩过的坑记下来，脚本、记忆、hooks 和分发，等用起来之后再慢慢补。

如果你也在用 Claude Code，不妨从手头最常重复的那个任务开始。先写几行说明，加上一个 gotcha，剩下的交给时间和使用频率。

原文链接：https://claude.com/blog/lessons-from-building-claude-code-how-we-use-skills
