---
title: "面试官皱眉：听说你精通 Claude Code？ 我不屑：几十个斜杠命令，够不够？！他跪了：快给我讲一遍！"
url: "https://mp.weixin.qq.com/s/3Vs5uXwhJNUaPeRsN1E5cg"
source: "微信公众号"
author: "程序员鱼皮"
account: "程序员鱼皮"
fetched: 2026-05-27
sha256: 4772acd34a23ff12
image_count: 49
---

大家好，我是程序员鱼皮，又来教大家怎么吊打面试官了。

如今用 Claude Code 的人越来越多了，但我发现很多人的使用方式还是纯聊天，有什么需求就直接打字跟它说。

这当然没问题，AI 模型的能力已经很强了，一般能够理解你的意思。

但其实 Claude Code 内置了很多斜杠命令，输入 `/` 就能看到完整的命令列表。这些命令覆盖了会话管理、上下文控制、并行协作等方方面面，用好了能省很多事。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1ExsW3wBDsjqWfdpeOwurALZFTusZNKO4QsVcosRVRltjMLcer8nnl6ltn0gHVhKIG5n1zuicOIuniazb0JQPhdHMxurxiapRDD0c/640?wx_fmt=png&from=appmsg)

虽然现在 Claude Code 已经有 50 多个命令了，但是你不用死记这些命令。日常使用时，直接用中文跟 Claude 说你的需求，比如帮我压缩一下上下文、切换到规划模式、把这个任务放到后台跑，它会自动帮你执行对应的命令。

但了解这些命令有什么用、什么场景该用，才能让你知道 Claude Code 的能力边界在哪里，打开使用思路，提高效率。面试被问到 Claude Code，你把这些说出来，绝对嘎嘎加分。

下面我会按照分类把常用的命令讲一遍，告诉你每个命令是什么？怎么用？什么时候该用？

需要演示的地方我会提供 Demo 提示词，你可以直接在 Claude Code 里执行试试。

点个收藏，咱们开始~

💡 如果你还没用过 Claude Code，可以看看我的 [这篇实战教程](<https://mp.weixin.qq.com/s?__biz=MzI1NDczNTAwMA==&mid=2247585821&idx=1&sn=4f904632690b8f457b0ec59dcd479bc7&scene=21#wechat_redirect>)，手把手带你配置好 Claude Code 并做出一个完整项目。

## Claude Code 斜杠命令大全

### 一、会话管理

#### /clear

清空当前对话，开始一个全新的会话。

当你做完一个任务，**要切到下一个任务时** ，用 `/clear` 可以把之前的上下文全部清掉，避免旧信息干扰新任务。

清空的对话不会丢失，后续还能通过 `/resume` 找回来。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HqZRXAxoRsC1g7qlKpx7NdB7FhpOpq8tkcp4ZCPjEtP1vbtIDGNpP6ic8NEHeMsEUr98QTltOPDiaCPqQc71nVFovEVibcLeZibhc/640?wx_fmt=png&from=appmsg)

注意，如果你只是觉得上下文太长了、但还在做同一个任务，不要用 `/clear`，用接下来要讲的 `/compact` 更合适。

#### /compact

压缩当前对话，释放上下文空间。

Claude Code 的上下文窗口是有限的，聊着聊着就满了。一旦上下文接近上限，AI 的注意力会开始涣散，容易遗漏之前讨论过的细节，回答质量明显下降。

`/compact` 会把之前的对话浓缩成一份摘要，腾出空间继续工作，同时保留关键信息不丢失。压缩后的对话轮次更少了，后续每次请求发送的 token 也会减少，相当于间接帮你省钱。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FwpI7eVRolH2iaGCicldZP9w6Z1dx9rz4FNLQ2aiaIatE0QM4VfRbErKnlxj3he0ZvJicePO0nYHiaZXYVliadluSe42w71VffwcNbk/640?wx_fmt=png&from=appmsg)

虽然 Claude Code 在上下文达到大约 95% 时会自动触发压缩，但我建议不要等到那时候才处理。自动压缩的时机不可控，可能正好在你实现关键逻辑的时候触发，导致一些重要细节被压缩掉。

更好的做法是在每完成一个阶段性任务后（比如调试完一个 Bug、开发完一个功能），主动执行一次 `/compact`，这样你可以控制保留哪些信息。

你还可以在后面加一段指令，告诉它压缩时重点保留什么。

比如你正在做 API 设计，可以这样：
```
    /compact 重点保留 API 接口的设计决策和参数定义  
    
```

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1FhvZbqj6mHwhSK8OLVNUiboyr2ZLOUPqSDbxiaaicjl6BPoWIwuQJ67fXRP5Ax5oCOrP4lenCSD4wiaQmyJpBwubzcI8cJ3rLYZvM/640?wx_fmt=png&from=appmsg)

我的习惯是，当 `/context` 显示上下文用量超过 80% 时就执行一次 compact。

#### /resume

恢复之前的会话。

假如你昨天做到一半的任务今天想接着做，或者刚才 `/clear` 掉的对话需要回去看看，都可以用 `/resume` 来恢复。

直接输入后，会弹出一个会话选择器，列出你最近的所有会话，选一个就能恢复。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GMLwh0rEvaZnR4mDxIGuIUXR0Yb8icBjZrMBWNibzdJzgZqNlbSGjrRksicz1wVF2T9oakeBvcAH5EndKxYsRSdHEE25wwBTpgnE/640?wx_fmt=png&from=appmsg)

默认只显示当前工作空间（项目目录）下的会话，如果想查看所有项目的历史会话，可以按 `Ctrl+A` 切换到全部列表。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GMYYKpEeZ7XJ7jwpWGu0Hiaia7yjfZTWmhzuQW2wA7PfUpPIjghtLY8AkqgoCwVCianhPb2TxpPWrP81ickvAzACkRGtjeYnLkh3M/640?wx_fmt=png&from=appmsg)

如果你记得会话的名字或 ID，也可以直接指定：
```
    /resume 昨天的重构任务  
    
```

#### /branch 和 /fork

这两个命令的作用完全相同，从当前对话分支出一个新会话。

当你想基于当前的讨论内容去探索一个不同方向，但又不想影响当前的对话进度时，就可以用它。

执行后你会切到一个新分支上，原来的对话保持不动，随时可以 `/resume` 回去。

比如你在讨论一个架构方案，想试试另一种实现思路，又怕改着改着原来的思路找不回来了，就可以先 `/branch` 出去探索，不满意再回来。
```
    /branch 试试用 Redis 替代内存缓存的方案  
    
```

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1E6xDM4A3g1CAoOPkxpBuYic9V1FmzetGNSXnfralWyez5ibxEfapm3oEfqptSnre5W7OHticUOPMIlqowDgNlFWdhyr7I3r4f9wA/640?wx_fmt=png&from=appmsg)

#### /rewind

把对话和代码回退到之前的某个节点。

Claude 在工作过程中会自动保存检查点，也就是每一次你按回车让它执行操作之前的状态，都会被记录下来。

如果 Claude 改了一堆代码，但你觉得改歪了，用 `/rewind` 可以回到之前的任意一个节点，对话记录和文件改动都会一起回退。

执行后会弹出一个节点选择器，你选择回退到哪一步就行。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1GtkVITrsv8RsmbwLC7hfawHFQ0dqhWyeF7uT9suOhh7YSiaH3tZI6OHlUfgPhMV9nCl4dpjk0FgtR8QEEQ3Ry0s9Lib7PVJuU2I/640?wx_fmt=png&from=appmsg)

你可以把它理解成编辑 Word 文档时的撤销，或者 Git 里的版本回滚，它同时回退对话记录和文件改动。

#### /recap

生成当前会话的一句话摘要。

如果你离开了一会儿回来，忘了刚才做到哪了，`/recap` 可以快速帮你回忆。

其实 Claude Code 本身在你长时间离开后回来也会自动显示一个 recap，这个命令是让你随时主动触发。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1H3UJb0jvjSQc8q5qXib8oW7SPev8hVgWsRNO9E0rEHtebia2AotEiaFsx71bjyicYjrhRfBVVhTxxrTsRCHDGdWLiafLOLTCWEGgdM/640?wx_fmt=png&from=appmsg)

#### /btw

btw 就是 by the way 的缩写，作用是快速插入一个问题，不会污染当前对话上下文。

有时候你正在做一个任务，突然想问一个不太相关的小问题，比如：Python 的 dataclass 怎么设默认值来着？

如果直接问，这段对话会被加到上下文里，不仅占用空间，还可能干扰后续任务。

用 `/btw` 就不会有这个问题，它相当于开了一个临时小窗，问完就关：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Fk8wWxOqhJKaVLXF3ughicpnR4ToPNltlwKtl2pwcWcULrTtK58XtnttXxhNZic6UIu6v2r1OZnsuTEXGCjrtNBw76q4dQra5hs/640?wx_fmt=png&from=appmsg)

#### /copy

把 Claude 最近一条回复内容复制到剪贴板。

直接输入 `/copy` 就是复制最新的回复。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FypbeDJdcv18ibqEBXggoGOYzLOuiaTR5mbs2fdZTehlYNyrncI32lpic3la2hpcVJpxbibF1QpTWia7de1LqEQyNtTq9DpQW5nymA/640?wx_fmt=png&from=appmsg)

如果回复里有代码块，会弹出一个选择器让你选复制哪一段。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1E26fb2yZxp9wVibUia80kBFgdceLSMicXbKGLibQYEXicuqsHtA8DghJdkuY5wwnlFaSBCTpvvcybU8iaCnPISOPZBId1z1icfEHkK2o/640?wx_fmt=png&from=appmsg)

加数字可以复制第 N 条回复，比如 `/copy 2` 就是复制倒数第二条。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HrwaBpYJuxA3IKvxG1huMgvyjunHjHxzFLRaETUAqxwPibOcCKCuR7f55PhGJF8hE2iaY47lsRpLQRxaX3P5y7GGiaWsvdiaB5PLk/640?wx_fmt=png&from=appmsg)

这个功能虽然简单，但我觉得特别有用，因为终端里直接选中复制经常会出问题。

#### /export

把当前整个对话导出为纯文本文件。

如果你想保存一段对话记录，或者分享给同事看，可以用这个命令。默认会让你选择复制到剪贴板还是保存到文件。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1EyZvIeQ426KAmCiaaWRY38ic8yGmib8viawibvDC2qz3IyehOmdNPTuE6oRR8RhfkfEZ1puJnp6NL0Ebdic8yaEOHMWxT9KWOVJeE5o/640?wx_fmt=png&from=appmsg)

在命令后面加文件名的话，就会直接保存：
```
    /export 开发记录.txt  
    
```

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1EObH2NibflTKBJsc7icoraOicxqVukvzOzUmPXEkmeUWsun7rWRlbel22nT9Owkd0NcZvDNFfM4qYjefibWr3KBrLt75YMgPLqY5A/640?wx_fmt=png&from=appmsg)

#### /exit

这个没啥好说的，作用是退出 Claude Code 会话，连按两次 `Ctrl+C` 也能退出。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1Hr91XA1K10yetTxKzHCecGHGwzH83MhBXUyrueLlhkj8pmNfSZ5MZmI2AQkMl5iaHqfokmj8sl0uM4vSy1CrYKGEPib19utdRjY/640?wx_fmt=png&from=appmsg)

但是要注意一个细节，如果当前有子代理（subagent）或者后台任务（background）在运行，退出时 Claude Code 会询问你是否要一并关闭它们。

### 二、信息与诊断

#### /usage

查看当前会话的 token 用量和费用。

用 Claude Code 最怕的就是不知不觉烧了很多钱。`/usage` 会告诉你这次会话花了多少 token、换算成多少美元、离你的计划用量上限还有多远。建议在长时间工作后定期看一眼。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1Hx8Ykh646POZ5YGXH8HE0sZ0ibVoAIz8fTwzqofhuET2xTtTwfIP4pxhYfNItyjGZ94rDcvNvzwgvU8qs7rribfaPkTB8W5Tpp4/640?wx_fmt=png&from=appmsg)

不过要提一句，如果你通过 API Key 接入了第三方模型（比如国内的 DeepSeek），`/usage` 显示的费用可能不准确，因为它是按 Claude 官方的定价来估算的。实际花了多少钱，还是以模型供应商后台的账单为准。

#### /context

可视化当前上下文窗口的使用情况。

执行后会显示一个彩色网格，直观地告诉你上下文空间被什么东西占满了，是对话记录太长、还是加载的文件太多、还是系统提示词太大。如果快满了还会给出优化建议。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1FH1ibIwRYykIcghjKFkDicSrTOnK090bWlMToCw77RXsfPl3FzvcBfGVSxQhjylXUTSSAg21ImibrQqt9Aylgk3DWNdj1VfIJIqc/640?wx_fmt=png&from=appmsg)

后面加 `all` 可以展开每一项的详细信息：
```
    /context all  
    
```

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1EhotUCzCCld4UwgJvpBzSSlciaEDNWugYvmEkicPicQVsahLibkLlL2eEIiczzNCDxQls6Dm30Via9U1HiaFicnosw6tH5iaEpEoEJbHSQ/640?wx_fmt=png&from=appmsg)

我一般会跟 `/compact` 配合着用，先看 `/context` 确认上下文压力，再决定要不要 compact。

#### /diff

打开交互式 diff 查看器，看 Claude 到底改了什么。

这个命令会展示当前所有未提交的代码变更。你可以用左右键切换查看整体 git diff 和 Claude 每一轮的单独改动，用上下键浏览不同文件。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Hz1bVdBUY7IxHuPKVnuWqsd3T1EloSUQylQZV1noXHibzJcAFkZCPVf4HxFhVQygLp0csq9ZxicTa9NibXoX0mz1L5A91LVQZyyE/640?wx_fmt=png&from=appmsg)

在 Claude 改了很多文件之后，提交代码前用 `/diff` 过一遍是个好习惯。不过在终端的小黑框里看代码 diff 还是挺痛苦的，如果改动比较多，建议还是配合 VS Code 的 Git 插件或其他支持可视化代码对比的 IDE 来看，体验会好很多。

#### /status

打开设置界面的状态标签页，显示当前的版本号、使用的模型、账户信息和连接状态。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1Eg7TcxWZVephcpQzzguIEVzYMRq8NNUyWG1ibRHV2UA0GScJh01qscIIQMNibTSun5PKhVhqJIu5BjcWzOR4j2ldI0b7W5uF5BI/640?wx_fmt=png&from=appmsg)

这个命令有个优点，它不需要等 Claude 当前的回复结束就能用。如果 Claude 正在跑一个很长的任务，你随时可以输入 `/status` 看看当前状态。

#### /help

显示所有可用命令的列表和简短说明。记不住命令的时候随时 `/help` 查一下。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1GXzU9SkcOVGqERyicFmuh1X5YROPWSSPtibg4u22XWE5bk4auHRibt6Mb73VZg5KhiaHXI3uCKnqCpjsPsd1x2RHAHBTUbicLIP7mc/640?wx_fmt=png&from=appmsg)

这其实是命令行工具的一个通用惯例，几乎所有命令行工具都支持 `--help` 或类似的帮助命令。养成习惯，之后遇到不熟悉的命令行工具，先敲个 help 看看有什么功能。

#### /insights

生成一份你使用 Claude Code 的分析报告，包括你经常在哪些项目目录工作、交互模式是怎样的、哪些地方产生了问题。

如果你想回顾自己的 AI 编程习惯、找找优化空间，可以定期跑一次看看。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Ejqx0OZoLf9xaKpjljc17icZNbIb5cpkDibCHK3nXFlBnC4GcQYxbzH9YrDicI66JuLticBpfMVROQCL4Alib8VBFQJWrbCtwuJeXQ/640?wx_fmt=png&from=appmsg)

生成出来的报告默认是英文的，你可以把这个报告文件扔给 AI，让它再帮你分析总结。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Gada8a5zzVVzOedbpk7AWWuC7jFZlSOiamzFSLDfAzB0342BYYltfpiaa9ALfDdLDdB1aLWpd4ZEI7iaCP7xOGYlEsDplmIiaLopQ/640?wx_fmt=png&from=appmsg)

### 三、模型和模式控制

#### /plan

在开发全栈项目、或者做大改动之前，不建议直接让 AI 直接开始写代码。

使用 `/plan` 可以进入规划模式，会让 AI 先制定一个执行计划给你看，确认方案没问题后，再动手实现。

你可以在命令后面直接描述任务：
```
    /plan 重构整个项目，增加用户系统和 JWT 认证  
    
```

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1HNbhib1picRMciczHycfFWdo29d7DicuvHqicQBpshhiaTwRfiaruA8IToshxjBdJ8JHvL8Ibx3fz2E6Pm5o1xUL6Diaj5evEP6GALteE/640?wx_fmt=png&from=appmsg)

AI 可能会通过交互式提问，来确认更多的信息：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1E4NocVwfg96THCrlwxmxY7VT9IWvj6lt3icunYboxJobEW8JUfjmYgTx9gSSySzDmy9Mn5BHwspMibWDLM2sEFWWXEYqQqic0Q4k/640?wx_fmt=png&from=appmsg)

然后输出一份详细的实施方案，等你确认了再开始执行。适合那些改动范围大、不确定最佳方案的任务。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GmmrxWgn0xKAUVvXLTf9icibialVlFxfvW0axZeNdzUicR53JE5wLwe5wThTdEUahCTrry75s9IicnSSFia2luY45hPYqRU8Uq284Hk/640?wx_fmt=png&from=appmsg)

#### /goal

这是我认为 Claude Code 目前最强大的命令之一。

一般情况下，Claude Code 每完成一轮操作，就会停下来等你确认或者下一步指示。

但有些任务你其实不需要盯着它一步步做，只需要告诉它「最终达到什么状态就算完成」，让它自己闷头干到底。特别适合睡觉前丢一个任务给它，第二天早上起来直接验收成果。

`/goal` 命令就是干这个的。你设定一个完成条件，Claude Code 会自动持续工作，不需要你每一轮都按回车确认。设置了 goal 之后，每轮结束会有一个轻量评估模型检查条件是否满足，没满足就自动开始下一轮，满足了才停下来。

先演示一下基本用法。比如我想让 Claude Code 修复整个项目的代码：
```
    /goal 修复整个项目的代码，直到全部测试通过且没有报错  
    
```

执行后 Claude 会立刻开始工作，直到所有测试通过且没有报错为止，整个过程你完全不用干预。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Hxx8ANpMEIBaMJrR2RtAkz1oMAF6MHQoAnUhebiaPuKF6bUVJHSrVwF4ZbOrUORaZTnicj1lmlmWugyEVRl9Ta0g2GaVThOj9aI/640?wx_fmt=png&from=appmsg)

不过这个命令能不能用好，关键在于你怎么编写完成条件。因为评估模型只会读对话记录里的内容，它不会自己去跑命令检查，所以条件必须是 Claude Code 执行过程中能自证的，比如「跑某个命令的结果是什么」。

下面是几个例子：

  * ✅ 运行 npm test 退出码为 0，且 git status 没有未提交的更改（Claude 跑完测试，结果就在对话记录里，评估模型能看得到）
  * ✅ src/ 目录下不再有任何 TODO 注释（Claude 可以 grep 检查并展示结果）
  * 

如果条件写得不够好，或者任务本身就很难收敛，Claude 可能会无限循环下去，一直烧 token。所以我建议在条件后面加上一个熔断限制，比如：
```
    /goal 把所有 API 调用迁移到 v2 格式，直到测试通过，如果 20 轮还没搞定就停下来  
    
```

这样即使任务没完成，20 轮之后也会自动停下来。

goal 命令虽然强大，但因为它会自动跑很多轮，token 消耗是比较大的。所以不是什么任务都适合用它，建议在这些有明确终止条件、且手动盯着比较费时的场景下使用。比如：

  * 模块迁移：把旧 API 调用全部迁移到新版本，直到编译通过
  * 批量重构：拆分大文件，直到每个文件都在指定行数以内
  *   * 清理积压：处理标了特定 label 的 issue，直到清空

如果你想中途看看进度，直接输入 `/goal` 就行，不用带任何参数，能看到当前任务跑了多久、花了多少 token。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GibTNzVibkQLBfv9XiadyEJTCvZZBPh02ZKqjQqiajJgBvtppoiaU90qekxeAvR4SJGkgkQRRfng4Aib9nZPdjKbbmWX3GkzVKPY3iaE/640?wx_fmt=png&from=appmsg)

想提前喊停的话，用 `/goal clear` 即可。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1ECugQNAF2owGQcMJbEc8RICXlmRBw6ZzOyWsnCvb543IemGiaicjHwD57k85LvWXQib6KUhmzvXfHQ40YdSN39uMG41icQEHY8Mzw/640?wx_fmt=png&from=appmsg)

#### /model、/effort、/fast

这三个命令都是控制模型行为的，我放在一起简单聊聊。

  * `/model` 切换 AI 模型，输入后会弹出一个选择器让你挑
  * `/effort` 调整模型的思考力度，从 low 到 max 共五档。任务简单的时候调低点儿省 token，复杂任务调高点儿，我一般都是 high 或者 xhigh。
  * `/fast` 用来开关快速模式。开启后响应速度大约快 2.5 倍，质量不变。但 token 单价更贵，适合需要快速迭代的简单改动，长时间跑的复杂任务就没必要开了。反正我是开不起（

![image-20260519181459065](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1FiamkOwz3vrYVrqUWhicMFMqTLCYgiaunNDJUj1TqCateQoCP3ibsgyquJmLZMUvDRsYycuibVzG2BW3R9n0ZicvJkKTNbXYG4wzZ7M/640?wx_fmt=png&from=appmsg)image-20260519181459065

一般来说，大家是用不到这些命令的，反正我是很少手动调整这些。不妨试试开源的 CC Switch 工具，提供了一个可视化界面来切换模型和调整参数，方便多了。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FDgtrgqflkokrnDXP8ldrxg8uyEW3d2Cr7sF3Dned8RWytslWZPHxOoEaLWObrn4k3WX2MqibedHOscRCWiaXQlOuaQuBGabqlI/640?wx_fmt=png&from=appmsg)

### 四、配置与扩展

#### /config

打开 Claude Code 的设置界面，可以调整主题、模型、输出风格等各种偏好。

相当于一个总控面板，记不住具体命令的时候可以直接 `/config` 进去找，还支持搜索配置项。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1ETbtLY8AJMvdmAe1Tibv95OnC6gA05WeqKtEG2unYLlGXicF3xN7ibR6T78ibOK3kjRde6MiahmqEiaWnLnEX4Hgxw4tibbCxmX2gwS8/640?wx_fmt=png&from=appmsg)

#### /mcp、/skills、/plugin

这三个命令分别用来管理 MCP 服务器连接、Skills 技能列表和插件。

MCP 是让 AI 连接外部工具和服务的协议，比如连接数据库、操作浏览器。

Skills 是可复用的自定义命令，你可以把常用的工作流封装成一个 skill，下次更快速规范地完成任务。

Plugin 可以理解为技能的打包集合，一个插件里可以包含多个 skill、自定义主题、hook 等，方便分享和安装。

这三个命令我用得不算多，一般配置好一次就不用再动了。如果你觉得命令行管理这些东西不够直观，用 CC Switch 等第三方工具也能可视化管理 MCP、技能和插件配置。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1GNG5o29E8l4xb9BLjvlepuSria94K1RoumWWnUAzAuVkJtgIeATUDotvbUWibQvQUmo5OXzCx5ygt8UjU3MYSibh6EibfMDe4trx0/640?wx_fmt=png&from=appmsg)

### 五、代码审查

#### /review

让 Claude 审查当前分支对应的 Pull Request。

输入 `/review` 命令后，AI 会自动检测当前分支关联的 PR 并开始审查：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1EWhMcRicbXmWEotDn6s3TNRe7wZrsDKZHPEKrKF2vibaVR4OXGtdoibb1yo8XdFmfzRqQ6narS2J49LKZYXwgsLISRc5KRFkxmWI/640?wx_fmt=png&from=appmsg)

也可以指定 PR 号来审查特定的 PR：
```
    /review 42  
    
```

审查过程中，Claude 会启动多个子代理并行检查代码，重点找 Bug 和逻辑错误。适合在让同事 review 之前或合并之前，自己先用 AI 过一遍，提前发现明显问题。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FuLVE543rUIKJWkoNQS0eibl2hIlWnzeG1EibatfYLwClichehrOiamtKuicTKWLp4IRgtzic0aAzzQMrUG1HZEiaWappvgVV6FZGZCY/640?wx_fmt=png&from=appmsg)

如果你想要更深度的云端多代理审查，可以试试 `/ultrareview` 命令，不过那个会额外消耗 usage credits，也就是在你的订阅套餐额度之外单独计费的补充额度，Pro/Max 用户有 3 次免费机会。

#### /simplify

虽然这个命令的名字叫 simplify，但它干的事不只是简化代码。

它的核心作用是审查你最近修改的文件，从三个维度找问题并直接修复：代码复用（有没有重复逻辑可以抽取）、代码质量（有没有 hack 写法和冗余状态）、执行效率（有没有不必要的计算、可以并发的地方）。

之所以叫 simplify，大概是因为这三个方向做下来，代码确实会变得更简洁。

跟 `/review` 不同的是，`/simplify` 不只是给意见，它会直接动手改代码。它会并行启动三个审查代理，分别从上面说的三个角度检查，然后汇总问题并应用修复。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HTZWJymmCD1Uibibf2PEj1n44tsMPeCY4l8iaiaxvdZ7mzQDegrxnrESCYj672qaXnBdZMYicdaXAjmSibQYvgeiaWzVB2gP1Pu3ajRU/640?wx_fmt=png&from=appmsg)

你也可以在后面加一段描述，让它聚焦在特定方向：
```
    /simplify 重点关注内存效率  
    
```

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1GKCErVJ6bbW66QvibZ6oSsdag5wIVlELqQy66OiczmpuDxjNTq70szhPVPEaOjBvFZAFGGVlLN7tlHmbrYpgpbtdTGaiaIibsicEBw/640?wx_fmt=png&from=appmsg)

### 六、子代理和并行

#### /agents

Claude Code 可以把一些子任务委派给子代理来并行处理，`/agents` 命令用来查看和配置这些子代理。

不过对大多数人来说，不需要手动去配置子代理，Claude Code 在需要的时候会自动启用。这个命令更多是让你看看当前有哪些代理在跑、状态如何。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1EXIrIGDG4Agyal3A4zzIdibQiaibygGKmTUJqsmWibiaC4ic3Z0L6kicuQOiaPfAU4F04fEHcNpZPq4qDS9c53IdksKzmdJ7vfh5GLFSw/640?wx_fmt=png&from=appmsg)

#### /tasks

查看和管理当前会话里正在运行的后台任务。

Claude 在执行复杂操作时，可能会同时启动多个子代理并行处理不同的子任务。`/tasks` 让你看到这些子代理各自在干什么、进度如何。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HRiaPgXTctd0Yq7gu5KlEEkF8qywLgmNYu7160MiaNdSmu4Mibs0pfghdre5ZkxhGXxabGYM2ibc3amA6CzKtlicEfHTXvsdn9qcZI/640?wx_fmt=png&from=appmsg)

选中某个任务按回车，还能进入查看这个子代理的完整执行过程和输出详情：

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1El6LfaSgJXWpOIjhyNuWl3dBzCxUOHGPY6KoZUBWaRonxDZ7eUaPhzg2Eib5sULmiczPH6TA8TFJI2WaFibONibwVHtuVObMqFzGU/640?wx_fmt=png&from=appmsg)

#### /background

把当前整个会话放到后台去跑，释放你的终端。

我觉得这个挺实用的，比如我让 Claude 跑一个比较耗时的任务（重构一个大模块），不想一直开着终端等它，就可以用 `/background` 把它扔到后台。

执行后，当前会话会从终端分离出来，在后台继续运行，你的终端就空闲了，可以去做别的事情。后面想接回来看看进度，再附加 attach 上去就行。

你还可以在后面加一句话作为分离前的最后指令：
```
    /background 编写单元测试并运行，执行失败则自动修复  
    
```

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1EgF0Vwib9QpOhUYqoribbSD1kNocrd9XmDmia2IcMAu8IwH3jD7cnWmBIqjicia9CYRiaT5PlmyKxRHkibrEzG4XxjHIFOxBfKvAbicibQ/640?wx_fmt=png&from=appmsg)

之后想看进度，用 `claude agents` 命令就能监控所有后台会话的状态。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1EL8x9eLVjB4vA7MhvPtu6k2NPWGv9F6bZPWR1s2eD3Via4An3VMhsBj9H96krrjtnLBvKzr08LnouwjsBhFia95F9ICvreZW03w/640?wx_fmt=png&from=appmsg)

选中某个后台会话，然后按右方向键，就能进入这个任务的对话界面了：

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GPhv4ySdBqkqExK9V2QXgDic47RQ6Hxib2TO2NibLWcV21KyKRKOiaHxZ4PlGkCZDYicoickqGs6Us75JvkDqPmkgPvPNIbv4QncoibA/640?wx_fmt=png&from=appmsg)

#### /loop

通过 `/loop` 命令，你可以设置一个时间间隔，让 Claude 每隔一段时间执行一次某个操作，也就是定时任务。

比如每 5 分钟检查一下部署是否完成：
```
    /loop 5m 检查项目前后端的部署状态  
    
```

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1EqPbBzVYKyLMPRK6tsjCNAxQk4OHictWEI7bC9DsdJ9w8EdnICHDnGFCTJa8Pn6OOh1aRWncSgoibjtQOql9BrjVpQOxXE1oQAo/640?wx_fmt=png&from=appmsg)

如果不指定间隔，Claude 会根据任务性质自己决定执行节奏。如果连提示词都不指定，它会去读项目里 `.claude/loop.md` 文件的内容来执行（如果有的话），没有这个文件就做一次自主维护检查。

这玩意适合用在需要持续监控的场景，比如等部署完成、等 CI 跑完、定期检查日志有没有异常之类的。

但是我个人不太喜欢用 `/loop` 来跑长时间的定时任务。如果设了之后忘了关，它会一直在后台循环执行，不知不觉消耗资源。

## 最后哔哔

以上就是我日常用 Claude Code 时最常用的斜杠命令了。

再次强调，完全不需要记住，看一遍有个印象就行，用的时候输入 `/` 就能搜索。而且前面也说了，大部分操作你直接用中文描述让 Claude 执行就行，它自己知道该调哪个命令。

AI 时代，该给自己的大脑减减负了~

OK 就分享到这里，本文会收录到我免费开源的[ 《Vibe Coding 零基础入门教程》](<https://mp.weixin.qq.com/s?__biz=MzI1NDczNTAwMA==&mid=2247580489&idx=1&sn=ea128f6a8c5d79e641133615f85f7ab8&scene=21#wechat_redirect>)，上千张图、几十万字，带你从 0 开始快速学会 AI 编程，做出自己的产品、跑通变现全流程，一次拿捏。

> 开源指路：https://github.com/liyupi/ai-guide

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Ft2iaWu0TeQ7SyZ6GHVxKDZLe7UzUzUtPKVaDVfrOwrmEYZDS3eThufV346eU42GicVnwyBN1qLN90stQ5LIShXbNkhIgXvvCvM/640?wx_fmt=png&from=appmsg)

我是鱼皮，持续分享 AI 编程干货。觉得有用的话记得点赞收藏~

也欢迎在评论区聊聊：你用 Claude Code 最常用的命令是哪个？有没有什么骚操作可以分享的？
