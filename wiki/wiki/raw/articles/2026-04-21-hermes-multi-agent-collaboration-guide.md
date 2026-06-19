---
title: "搞完 Hermes 多 Agent 我才发现，这根本不是技术活，是管理活"
url: "https://mp.weixin.qq.com/s/oGXo8psXgP6A24mmKbTGIw"
source: "微信公众号"
author: "林月半子聊AI"
fetched: 2026-04-21
sha256: b26e5fc0fca8d7ca
---

# 搞完 Hermes 多 Agent 我才发现，这根本不是技术活，是管理活

**作者**: 林月半子聊AI | **日期**: 2026-04-20

当 Hermes 出来的时候，好多人问我多 Agent 之间的协作是怎么玩的。

周末我找了时间自己做了一把实践，原本以为会很顺利，没想到中间翻了好几次车，最后硬是一个坑一个坑填过来的。这篇把完整过程记下来，跟着做，你也能在自己的 Discord 里，看到几个 AI 像同事一样互相接力干活。

但在动手之前，有句话得先讲在前头。协作是能力的放大器，不是补丁。如果单个 Agent 本身是个废柴，拉三个废柴来协作，结果就是三倍的废柴，三个废柴开会，废柴还是废柴。SOUL.md 写细、skills 配齐、模型选对，把 Agent调教好，这是多 Agent 能跑的前提，不是结果。

好，话撂这儿了，开始正题。

## 先聊聊 profile，这是整个多 Agent 的基础

要做 Agent 协作，第一步得先把不同的 Agent 建出来。在 Hermes 里，这件事是通过 Profile 来实现的。

profile 其实就是 Hermes 的人格档案。一个 profile 就是一个完全独立的 AI 分身，有自己的 config.yaml、.env、SOUL.md、独立的 memory、独立的 skills、甚至独立的 gateway 进程。

底层实现其实挺朴素，靠一个 HERMES_HOME 环境变量切换根目录，但效果是实打实的隔离。

💡 思维引导

为什么要强调"真隔离"这件事? 因为多 Agent 架构里最怕的就是"一个挂了全挂了"。Hermes 的 profile 是进程级隔离，每个 profile 跑自己的 gateway 进程，互不依赖。即便某一个 agent 挂了，也完全不影响其他 agent 继续干活。

这点和 OpenClaw 是有差别的。用过 OpenClaw 的朋友都懂，它的多身份更多是配置层面的切换，进程还是同一套。

Hermes 这种物理隔离在企业交付场景下是真的香，客户不会因为一个 bot 崩了，整套自动化系统跟着下线。

好，理解了 profile 是什么，我们开始搭建。

## Step 1：建三个 Agent，分工明确

这次我准备搭一个三人小组，模拟一个真实的内容生产协作流:

* 林小墨 (Ink) —— 文案与笔记整理专家
* 林小探 (Search) —— 搜索与调研专家
* 林小管 (Admin) —— 任务分发与调度员

这个组合不是随便定的。它对应着一个完整的"查资料 → 写笔记 → 归档"工作流，而且引入了一个专门的调度 Agent(林小管)，让协作路径更清晰。

先建林小墨:

```bash
hermes profile create ink --clone
```

这里我用了 --clone 参数，主要是直接继承 default 的一些配置(模型、API key 等)，不用重新配一遍。

🎯 多说一句 --clone 的选择。Hermes 给了三档克隆策略，看场景选:

* 什么都不加(hermes profile create mybot):空白 profile，连 API key 都要重新配，适合从零搭一个完全独立的 agent
* --clone(我这次用的):只复制 config.yaml、.env、SOUL.md，记忆和 session 是全新的。这档最适合搭多 Agent——共享模型和 API key，但每个 agent 从干净的上下文开始，互不串味
* --clone-all:连 memory、sessions、skills、cron jobs 全拷贝，等于整个人"克隆一份"，适合备份或者 fork 一个已经有上下文的 agent

多 Agent 协作场景，基本都是 --clone。

执行后的输出长这样:

```
Profile 'ink' created at /Users/lunaraitalk/.hermes/profiles/ink
Cloned config, .env, SOUL.md from default.
77 bundled skills synced.
Wrapper created: /Users/lunaraitalk/.local/bin/ink

Next steps:
  ink setup              Configure API keys and model
  ink chat               Start chatting
  ink gateway start      Start the messaging gateway
```

注意最后几行。ink 直接变成了一个独立的命令，你后面用 ink chat、ink gateway start 就能直接操作这个 agent，不用每次都写 hermes -p ink xxx。这个细节真的贼方便。

然后给林小墨覆盖一个人设:

```bash
echo "你是'林小墨'，一名专业的文案专家和知识管理助手。你擅长将碎片化信息整理成结构化的 Markdown 格式，并熟练运用 Obsidian 的双链体系。你的回复风格文雅、逻辑严密。" > ~/.hermes/profiles/ink/SOUL.md
```

💡 注意左下角的 ink ❮ 标识。Hermes 用 prompt 前缀告诉你当前是哪个 profile 在说话，多 Agent 场景下这个小设计特别救命。

## Step 2：接 Discord，为什么不用飞书?

消息平台这块我纠结过一下。先说结论:这次我选了 Discord，没用飞书。

原因很简单，飞书群因为平台的限制，确实不支持 bot 被 @。多 Agent 协作最核心的动作就是"一个 agent @ 另一个 agent 来接力"，飞书这条路直接堵死。Discord 在这方面开放得多，也是 Hermes 官方文档里演示最充分的平台。

配置完后，启动 gateway:

```bash
ink gateway install && ink gateway start
```

💡 踩坑提醒

这里用的是 gateway install + gateway start 组合，install 会把 gateway 注册成 launchd 服务(macOS)或 systemd 服务(Linux)，关机重启后会自动拉起。长期跑的话，强烈建议 install。

## Step 3：翻车从私聊开始

gateway 起来了，先做个简单的私聊测试。

结果第一个坑就来了:@ 它不给响应。

折腾了一会儿我才意识到，Hermes 的 Discord gateway 默认有个 allowed_channels 的白名单机制，不在白名单里的频道，bot 压根就不响应 @。

解决方法:

```bash
hermes -p ink config set discord.allowed_channels "1495255615545544819"
```

💡 这个默认设计是挺合理的。如果 bot 默认响应所有它能看到的频道，你把它拉进一个大服务器，它就会在所有频道里到处开口。allowed_channels 强制你显式指定哪些频道允许它说话，是个安全默认值。

## Step 4：顺便聊聊 Hermes 的 thread 机制

@ 通了之后，我发现每次我 @ bot，它会自动开一个 thread 来回复，而不是在主频道直接回。

这是 Hermes 默认开启的 auto_thread: true 行为。这个机制设计得真的挺好:

* 主频道不被刷屏:AI 回复动辄几百字，全丢主频道里谁受得了
* 上下文干净:每个任务在独立 thread 里，不会互相干扰
* 多人并发友好:几个人同时用也不会打架

## Step 5：复制两份，搭出小团队

林小墨跑通了，剩下两个照葫芦画瓢:

```bash
hermes profile create search --clone --clone-from ink
hermes profile create admin --clone --clone-from ink
```

然后分别写人设:

```bash
echo "你是'林小探'，情报专家。你擅长从海量互联网信息中筛选核心数据。你的任务是提供客观、准确的市场调研报告和技术趋势分析。你会引用所有信源。" > ~/.hermes/profiles/search/SOUL.md

echo "你是'林小管'，团队协调官。你负责接收用户的原始需求，并将其拆解为具体任务分发给墨、探两位专家。你还负责 Discord 频道的日常运作和权限维护。" > ~/.hermes/profiles/admin/SOUL.md
```

踩坑提醒: 三个 profile 配 .env 的时候，每个 bot token 必须独立，千万不能复用。Hermes 内置了 token lock 机制。

## Step 6：以为成功了，结果发现是假的

把任务扔进频道:"帮我调研一下2026年最新的AI智能体趋势并整理成文章"。

任务是跑出来了，结果也像模像样。但我盯着日志一看，不对劲。它走的是 Hermes 内置的 delegate_task 模式，根本没走多 Agent 协作。

Hermes 有个叫 delegate_task 的内置机制，单个 agent 可以 spawn 一个隔离的 subagent 来跑子任务。subagent 是临时 spawn 的无状态执行者，用完即焚，不是你精心配置的那三个独立 profile。而且 subagent 的 send_message 技能是被 Blocked 的，它根本没办法在 Discord 里主动发消息。

想让真正的多 Agent 接力跑起来，必须在 admin 的 SOUL.md 里把协作协议写死，强制它通过 Discord 频道公开"点名"，走真实的 @ + 消息路径。

## 坑 1：没有 @，直接就结束了

LLM 其实理解"林小探是团队里的人"，但它不知道在 Discord 里要怎么真正"叫醒"对方——Discord 点名是靠 `<@用户ID>` 这种特殊格式的消息才能触发的。

解决方法——在花名册里，直接把每个人的工牌号挂在名字后面:

```markdown
## 团队成员
- **林小探 (Search)**: 【Executor】负责联网搜索、情报搜集和市场调研。ID: `<@***>`
- **林小墨 (Ink)**: 【Executor/Reviewer】负责文案润色、逻辑梳理和 Obsidian 笔记格式化。ID: `<@***>`

## Discord 艾特指令 (Crucial)
当且仅当你需要某个队友**立刻开始工作**时，必须使用以下格式:
- 召唤林小探: `<@***>`
- 召唤林小墨: `<@***>`
**注意**: 在非执行环节，仅使用纯文字"林小探"或"林小墨"，不要带 `<@` 符号。
```

踩坑提醒: 这里最容易犯的错是——在任务规划阶段也用了 `<@ID>` 格式。结果就是 admin 还在列计划，林小探和林小墨已经被点醒冲进来了，场面混乱。严格区分:计划阶段用纯文字，执行阶段才用 `<@ID>`。

## 坑 2：任务结束后，停不下来了

@ 的问题搞定，接力终于跑起来了。但总结完之后它不停了，一直刷表情符号。👍 👋 🎉，跟发癫一样。

这其实是多 Agent 架构里一个非常典型的问题，bot 之间互相触发的死循环。

### 第一层:DISCORD_ALLOW_BOTS —— 死循环的真正源头

```bash
# 三个 profile 的 .env 统一改
DISCORD_ALLOW_BOTS=mentions
```

### 第二层:replied_user: false —— Discord 的反直觉机制

Discord 的 "reply" 功能默认会自动给被回复的人发一个 mention。哪怕你文本里压根没写 `<@ID>`，只要你用了 reply 功能，被回复的那个人还是会收到提醒。

在 admin 的 config.yaml 里加上:

```yaml
discord:
  allow_mentions:
    everyone: false
    roles: false
    users: true
    replied_user: false
```

### 第三层:SOUL.md 里的终止协议

```markdown
## 任务终止与防循环规范
- **明确终结**: 当确认林小墨完成笔记整理后，请发出简短总结，并以"【任务结束】"结尾。
- **禁止冗余**: 任务结束后，严禁发送无意义的表情(👍， 👋)、寒暄或单纯的确认消息。
- **中断反馈**: 不要对其他 Bot 发出的"收悉"、"待命"等结束类消息做出二次响应。
- **艾特控制**: 在任务结束总结中，禁止再次艾特任何 ID。
```

三层互相兜底，才是稳态。

## 坑 3：直接把两个人都 @ 了

admin 一上来同时 @ 了林小探和林小墨两个人。正常的协作逻辑应该是:先 @ 林小探去查资料，等林小探查完，再 @ 林小墨去整理笔记。

解决方案是在 SOUL.md 里强制时序规范:

```markdown
## 协作时序规范 (Strict Timing)
- **逐一唤醒**: 严禁在任务开始阶段同时艾特多个专家。
- **当前阶段**: 仅在当前步骤需要执行时，才发出对应的 `<@ID>` 指令。
- **接力逻辑**: 必须等到 **林小探** 明确回复"调研完成"后，你再发出下一条指令并艾特 **林小墨** 开始 Step 2。
```

## 写在最后

整个过程跑下来我反复在想一件事，多 Agent 到底是个技术问题，还是别的什么问题?

搞完这三个坑之后我明白了，它其实是个管理问题。

profile 给你的是工位，Discord 给你的是会议室，但真正让三个 AI 像团队一样跑起来的，是那份被你一次次打磨的 SOUL.md——那是职责说明书、协作流程、以及明确的下班时间。每一个坑，本质都不是技术 bug，是管理漏洞:

📌 坑 1 没 @ → 下属不知道该找谁汇报
📌 坑 2 停不下来 → 没有明确的项目终结机制
📌 坑 3 同时 @ → 任务分派时序混乱

拿去套人类公司一样成立。

一个 Agent 不是超人，是岗位。三个 Agent 不是炫技，是团队。

这篇只是把三人小组跑通了。五人、十人的团队，同一套逻辑——难的永远不是模型，是组织设计。
