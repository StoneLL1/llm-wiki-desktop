---
title: "我用Obsidian 给 Coding Agent装了一块硬盘！它终于不再失忆了"
url: "https://mp.weixin.qq.com/s/3yH25tfHUmVLmdCBkWE2GQ"
source: "微信公众号"
author: "字节笔记本"
account: "字节笔记本"
fetched: 2026-05-27
sha256: 92b9350c8314793d
image_count: 8
---

我们每天使用Coding Agent写代码的场景大抵如此：

早上打开Codex/Claude Code重构一个 Go 后端。

花了 40 分钟给它交代背景，项目结构是什么、用的什么数据库、有哪些已经定下来的技术决策、有哪些坑踩过了不要再踩。

这段时间不管是国产模型还是官方订阅，它们干得很好。

第二天，/compact 一下，或者新开一个会话，上下文本信息全部丢失或者压缩没了。

又是花了20分钟重新交代。

第三天，同样的事情再来一遍。

其实这也不是 Claude Code 的 bug，也不是 Codex 的缺陷，这是当前所有Coding Agent共同的问题：

**上下文窗口就是它的全部记忆，窗口一清，它就失忆成陌生人。**

怎么解决呢？

### 先搞清楚问题的本质

Agent 到底在什么层面不记得呢？

其实包含了三层：

**第一层，跨会话失忆。**

今天的对话和昨天的对话之间没有连接，Agent 不知道昨天发生了什么。

**第二层，上下文压缩失真。**

当对话变长，/compact 会把早期内容压缩成摘要，细节丢失，但往往细节里藏着魔鬼。

**第三层，重复劳动成本。**

Agent 会重新做已经做过的事，因为它不知道那件事已经做完了。

基于以上几点，我们非常有必要在 Agent 工作记忆之外，给它建一个持久化的长期记忆库。

认知科学里有一个经典模型，人类记忆分工作记忆和长期记忆。工作记忆容量有限，处理当前任务；长期记忆容量大，存储经验和知识。

两者协作，才让人能应对复杂问题。

上下文窗口就是 Agent 的工作记忆。而Obsidian 就是我们要帮它建的长期记忆。

现在的问题是，如何组织和使用Obsidian来建立长期记忆可以按下面的架构和方式来？

### 三层记忆架构

我把这个整套系统分成了三层，每层职责不同：

第一层：AGENTS.md / CLAUDE.md

告诉 Agent 去哪里读记忆，遵守什么规则。它是工作手册的目录页。

第二层：Obsidian 项目笔记

用来保存长期状态、技术决策、踩坑记录、下一步任务，是整个工作手册的正文。

第三层：sessions/YYYY-MM-DD.md

记录每次 Agent 工作完成了什么，防止第二天重复，相当于你的会话工作日志。

三层缺一不可。

只有第一层，Agent 知道规则但没有记忆。  
只有第二层，Agent 没有入口读到记忆。  
没有第三层，今天做了什么明天又要重新交代。

AGENTS.md 是启动式，Obsidian 是长期记忆库，Coding Agent 是执行工人。

### 具体怎么搭？

这里要推荐使用Obsidian CLI，而不是纯靠手动去维护。

借助官方Obsidian CLI，我们可以让Coding Agent直接从终端直接控制 Obsidian，实现脚本、自动化、插件开发、搜索、打开/创建笔记。

开启方法很简单，下载或者更新Obsidian后，启用 Command line interface 并注册 CLI工具。

![8c47010c-0067-4c3b-a1b6-38cd05f20091.png](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1yPAOdT2u7iaak8D9gJLLSt7zViblUteuPYWFLKic0ZnFLZc9fudYCDwZibEdrK4orS1LsOoXSBlGcAbFtiahNP3smCnov7n0aBRfwc/640?wx_fmt=png)

我这里用一个真实项目来演示整套工作流怎么用。

假设项目叫 Demo Todo App，放在 `~/development/demo-todoapp`，

一个非常简单的任务，用 Go 标准库 `net/http` \+ SQLite 实现一个 Todo REST API，不用框架，不用 ORM。

项目本身足够精简，专注看记忆工作流是怎么运转的。

### 第一步：建 Obsidian 记忆目录

Obsidian Vault 最终的文件结构如下：

![4958c403-d98b-42a4-be8b-f5f307d63d6d.png](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1zaKbDqk6SaicPrpo2Nfq2tfCgXTbn5X5L2AqUutq2drqOm1K8jlNcjgeia4echibURXc94ploCicNwCfHgiaG4ic2WsibrByEapUjADc/640?wx_fmt=png)

**decisions.md** 写已经定下来的技术决策。这是最关键的文件，它防止 Agent 反复质疑已经做完的选择：

![b547d61a-6c87-442f-be16-9f14f0fa2ee4.png](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1x2DTTPzd6LtwfDiaPG7IQxuocshL93ZzDUT2N6eDCCXgI6lWia6Y3OO0WfEASpNSrxXptyOR8tAFsicXeODI2PibgPOcGPdR2TS5A/640?wx_fmt=png)

**errors.md** 写踩过的坑。每次踩坑就追加，让 Agent 下次不要重蹈覆辙。

![99785586-f219-414f-b11f-9017e7773e1d.png](https://mmbiz.qpic.cn/sz_mmbiz_png/iacaCWlP1x1zyecSxDdcmSdeiazh8VX9WGcwZrttCu2UEzxuDYHW18nNPPibc4aDd1hbp1bhahG85ict9IykXrYOsOsmLibfGS8O96oeru2TFoY0/640?wx_fmt=png)

**todo.md** 写下一步要做什么。每次任务完成后更新：

![fb53fe33-6a18-468f-9a6e-1cf680adbc0e.png](https://mmbiz.qpic.cn/sz_mmbiz_png/iacaCWlP1x1wK0SSmZdQAUfbGOvXU5SOiaRjdsaXkzVvqF6ibGSsiaXmSCuCic5ic07MicG3l15qAib9Hx3zEgfFIlnvbqyw2BdicpDiaxlwaDs3Mzvb8/640?wx_fmt=png)

### 第二步：在项目根目录放 CLAUDE.md 和 AGENTS.md

这两个文件是 Agent 的入口，告诉它开始工作前先读记忆，它是通过Obsidian CLI来自动更新流的关键所在，上面的文件结构里面的内容就是通过它来自动维护创建的。

**CLAUDE.md，** 给 Claude Code 用：
```
    # CLAUDE.md
    
    ## 长期记忆
    
    本项目使用 Obsidian 作为长期记忆库。
    
    记忆路径：~/Obsidian/AI-Dev-Memory/Projects/demo-todoapp/
    
    开始重要工作前，必须先读取：
    1. overview.md - 了解项目当前状态
    2. decisions.md - 了解已确定的技术决策
    3. errors.md - 了解踩过的坑，避免重复
    4. todo.md - 了解下一步任务
    
    ## 行为要求
    
    - 开始编码前，先输出你对当前项目状态的理解。
    - 不要重做已完成的模块，除非被明确要求。
    - 发现重复出现的 bug，追加到 errors.md。
    - 做了架构选择，追加到 decisions.md。
    - 完成一个模块，更新 todo.md。
    - 每次会话结束后，写 session 摘要到 sessions/YYYY-MM-DD.md。
    
```

**AGENTS.md，** 给 Codex 或其他 Agent 用：
```
    # AGENTS.md
    
    ## Memory Protocol
    
    Before coding:
    - Read ~/Obsidian/AI-Dev-Memory/Projects/demo-todoapp/
    - Check overview.md, decisions.md, errors.md, todo.md
    
    During coding:
    - Do not repeat completed work
    - Keep changes minimal
    - Run tests after modifications
    
    After coding:
    - Update todo.md
    - Append session summary to sessions/YYYY-MM-DD.md
    
```

### 第三步：封装成 Claude Code 自定义命令

如果用 Claude Code，可以把常用操作做成 /命令，放在项目的 .claude/commands/目录下。

**`/init-memory`** 命令：
```
    Read CLAUDE.md and AGENTS.md first.
    
    Then read these Obsidian memory files:
    - overview.md
    - decisions.md
    - errors.md
    - todo.md
    
    Before writing any code, summarize:
    1. Current project status
    2. Key constraints and decisions
    3. Known pitfalls to avoid
    4. What should NOT be modified again
    
```

**`/save-memory`** 命令：
```
    Append a session summary to sessions/YYYY-MM-DD.md.
    
    Include:
    - What was completed
    - Files changed
    - Decisions made
    - Bugs encountered
    - Next steps
    - Modules that should not be refactored again
    
```

这样每次开始工作输入/init-memory，结束时输入/save-memory，两条命令，整套的记忆工作流就会自动并完整地运转。

### Coding Agent使用时的工作流

搭好之后，我们就可以这样开始使用。

进入项目之后，给 Agent 这样的提示词：
```
    先读取 CLAUDE.md，然后读取 Obsidian 项目记忆目录里的
    overview.md、decisions.md、errors.md、todo.md。
    不要急着改代码，先输出你理解的当前项目状态和本次修改计划。
    
```

让 Agent 先报告现状，是确认它真的读到了记忆、理解了上下文，而不是直接开干。

工作过程中，让它遵守三条规则：

  * 发现新坑，追加到 errors.md。
  * 做了设计决策，追加到 decisions.md。
  * 完成一个模块，更新 todo.md。

结束任务后，让它生成 session 总结：
```
    请把本次工作写入 Obsidian 项目记忆：
    1. 完成了什么
    2. 修改了哪些文件
    3. 仍然存在什么问题
    4. 下次继续时应该从哪里开始
    5. 哪些模块不要重复修改
    
```

这五条都是精华，尤其是第五条，哪些模块不要重复修改，这条信息如果没有显式记录，Agent 下次很可能把你已经花时间打磨好的代码重构掉。

当然，这是我个人给出的最小化结构的版本，用几天，你自然会知道哪里需要补充，哪里需要拓展，完全可以根据你自己的业务和使用习惯来改造。

我用这套方案跑了一段时间，最直观的感受是：**开会话的心智成本降低了。**

以前每次开新对话，我都要想又要重新交代一遍，有时候嫌麻烦就干脆不开新对话，把一个会话拉得很长，结果上下文膨胀compact 失真。

现在不一样了。

开新会话，两句话让 Agent 读记忆，它自己总结出来的状态比我自己描述的还要清晰。

我觉得这才是 Coding Agent 应该有的工作节奏：上下文窗口只是工作记忆，Obsidian 作为长期记忆。

每天结合Obsidian CLI或者手工整理项目的markdown文件也成为了我每天的必做项目。

Coding Agent外挂硬盘之后，终于有了记性，再也不用担心会话的丢失，不管是上下文信息的管理，项目总结或者是模块的迁移都大有裨益。

几分钟的事，收益却是长期的。

更多AI编程的高级技巧可以关注我的专栏合集：[AI编程高效开发指南](<https://mp.weixin.qq.com/mp/appmsgalbum?__biz=MzIzMzQyMzUzNw==&action=getalbum&album_id=3955838883623043087&from_itemidx=1&from_msgid=2247515311#wechat_redirect>)
