---
title: "别再手写 Skill 了！微软最新研究：像神经网络一样训练 Skill"
url: "https://mp.weixin.qq.com/s/sqHF3d3l5PX3VOs0Mtwk3A"
source: "微信公众号"
author: "尹John"
pub_date: 2026-05-26
fetched: 2026-05-27
image_count: 20
sha256: 496e17e5faaf9f95
---

Microsoft Research

# 别再手写 Skill 了  
像训练神经网络一样  
训练你的 _Skill_

SkillOpt · 52/52 最优 · 平均 +23.5 分

这篇微软最新的研究论文，可能会改变你写 Skill 文档的方式。

相信你已经在写各种 Skill 和给 Agent 的文档了：CLAUDE.md、best_skill.md、agent instructions……

你可能会和我一样，花上一两个小时甚至半天时间，精心打磨一份指令，希望 Agent 能因此而变得更加聪明。

但微软这篇论文的结论，有点点扎心：**你手写的那份，大概率不是最优的。**

微软的研究团队提出了一个叫 SkillOpt 的方法，核心思路是：

把 Skill 文档当成神经网络的「权重」，用类似训练神经网络的方式去自动优化它。

![SkillOpt 概览](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFmTYcGP1tv99VljDkZJypXuoBgTs2U3rns7ubibXlgXBZ3bZGib8G1yPgraibtDcOBuFFZusEM92Ftg9DI4F8spdRkhEyVo3hmCJg/640?from=appmsg)

SkillOpt 架构概览（来源：论文）

结果，该方法在 52 个测试组合中全部取得最优或并列最优，平均提升 23.5 分，碾压了人类手写的 Skill。

##  关于 Skill 

Claude Code、Codex、Cursor 这些工具都已经支持用户写一份「指令文档」来指导 Agent 的行为。不论是 Claude Code 的 CLAUDE.md 或是 Codex 里的 Agents.md 或是 Skill 的各种文档，它们的共性是：

一段纯文本指令，告诉 Agent 遇到什么情况该怎么做。

比如你写了一句「遇到 Excel 公式时，先检查工作表结构，再写入静态值而非依赖 Excel 自动重算」，Agent 在处理 SpreadsheetBench 这类任务时就会照着做。

这个看起来很顺理成章，好像没什么。

但问题在于，你怎么知道自己写的那几条规则就是最好的呢？

你凭经验写了 5 条规则，可能漏了 3 条关键的，还有 2 条写得不够精准。更麻烦的是……你根本不知道自己漏了什么，因为你没法穷举所有可能的写法。

在灵活性和指导性之间，你很难找到那个平衡点。

SkillOpt 的出发点是：**既然人写不好，那就让 AI 自己来优化自己的说明书。**

##  训练循环 

SkillOpt 的核心思路，用一句话概括就是：

**Skill 文档是 Agent 唯一可变的外部状态，那就把它当「权重」来训练。**

Agent 的模型参数是冻结的，不能动，但 Skill 文档是纯文本，可以随便改。既然如此，为什么不能像训练神经网络一样，用一套完整的优化流程来迭代优化这份文档呢？

一旦开始这么思考，整个方法论其实就水到渠成了。

![SkillOpt 流程](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFkFB00gU4YHablHnbGIeeYfSicQiaKQReJy66kdpsFr7fqoNMGvwvGAdrJibtlLoCp8jTT77tR2ozMebDmASpdLhRcpvwYLYj0D6E/640?from=appmsg)

SkillOpt 训练流程（来源：论文）

我们先来看看深度学习中的各种概念，是怎么映射到这里的文本空间的。

训练神经网络时，你需要把数据送进去做前向传播（forward pass），对应在 SkillOpt 中的操作叫 **rollout** ：让 Agent 带着当前的 Skill 文档去做一批任务，收集完成情况。

前向传播之后要算梯度（gradient），SkillOpt 中的对应操作叫 **reflection** ：用一个优化器模型去分析哪些任务失败了、为什么失败，并提炼出改进方向。

有了梯度就要更新权重（weight update），对应 SkillOpt 中的操作是 **edit** ：对 Skill 文档做 add、delete、replace 三种结构化编辑。

训练时还有学习率（learning rate）控制步长，SkillOpt 也有一个 **textual learning rate** ：每轮最多只允许改 L_t 条规则（默认 4 条），还有 cosine decay 衰减。

最后，训练时会用验证集做 checkpoint 来保存最优模型，SkillOpt 同样有 **validation gating** ：改完之后在验证集上跑一遍，如果分数没涨，那就不能接受这次修改。

整套流程下来，其实就是**把深度学习的训练循环，一比一翻译成了文本编辑的循环。**

##  两个模型分工 

在 SkillOpt 中，用了两个模型。

一个叫 **target model** （目标模型），就是你平时用的那个 Agent，比如 GPT-5.5 或 Claude。它负责带着 Skill 文档去做任务，模型本身冻结不动。

另一个叫 **optimizer model** （优化器模型），它是另一个超强的前沿模型，负责分析 target model 的表现，然后提出修改建议。

打个比方，target model 像是工厂里的操作工人，optimizer model 像是站在旁边观察的管理顾问。工人按操作手册干活，顾问看工人哪里做得不好，然后改手册。

这个分工带来一个好处：**optimizer model 的成本只在训练阶段产生，部署时完全不需要它。**

论文也测了用同级别模型做优化器的效果（比如用 GPT-5.4 优化 GPT-5.4 自己的 Skill）。

结果是，同级别优化器也能工作，大约能恢复强优化器 56%-74% 的增益。但用更强的优化器效果显然更好，因为它能看到 target model 自己看不到的问题。

##  克制的学问 

这里有一个设计是：SkillOpt 每轮最多只改 4 条规则。

你可能会从直觉来想：既然都让 AI 来优化了，为什么不让它一次性把整份文档重写呢？

研究团队还真试过了……结论是：**不限制反而更差。**

无限制重写（unbounded）的效果比 L_t=4 低了 2-3 分。

原因很好理解，跟训练神经网络时学习率太大会导致震荡一样，一次改太多，好的改动和坏的改动混在一起，验证集没法准确判断哪些有用。

还有一个设计叫 **rejected-edit buffer** 。

被验证集否掉的修改并不会直接丢掉，它们会存进一个缓冲区。后续的 reflection 阶段会看到这些「前车之鉴」，避免重复犯同样的错。

这就像训练时的 negative feedback，让优化过程有记忆。

另一个关键机制叫 **slow/meta update** ，类似于深度学习中的 momentum。

每个 epoch 结束时，优化器会回顾这个 epoch 和上个 epoch 的 Skill 文档，做一次跨 epoch 的纵向更新。这种慢更新的内容受到保护，step 级别的编辑不能覆盖它。

消融实验显示，去掉 slow/meta update 会导致 SpreadsheetBench 从 77.5 暴跌到 55.0，足足掉了 22.5 分之多。

![消融实验](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlniaSXO6T4O5hWvGPPyRFibAhCDQoBMu5SibNic0069kf8BM6zeEibwyoDbzcEHt9icBGy212kcD0LkEO9bfyXqLLMWiahl4Vv1jqHAs/640?from=appmsg)

各组件的消融实验结果

**克制，有时候比激进更有效。**

##  效果生猛 

讲了这么多设计，效果怎么样呢？

**一个字：非常生猛。**

研究团队在 6 个 benchmark 上测试，覆盖了单轮问答、多轮代码生成、文档操作、多模态文档理解、数学推理、以及具身环境交互。

相比直怼 GPT-5.5 对话的结果：

SearchQA 77.7 → 87.3 +9.6

SpreadsheetBench 41.8 → 80.7 +39.0

OfficeQA 33.1 → 72.1 +39.0

DocVQA 78.8 → 91.2 +12.4

LiveMath 37.6 → 66.9 +29.3

ALFWorld 83.6 → 95.5 +11.9

+23.5

平均提升分数

![训练曲线](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlYGFNvzANAj4fMEt8QJXicA67ibnq8e0heWJ7XkDrlkg7OTkS3qxFjJL62pQBg1OaXvXTjQbylicKupE85fN48ZbicoEQO7koVEcQ/640?from=appmsg)

训练过程中分数的变化曲线

SpreadsheetBench 和 OfficeQA 各涨了 39 分……这可以说，远远不是小打小闹的微调了，几乎是从「不太能用」到「相当能打」的质变。

而且不光在直接对话场景有效，在 Codex 执行环境中平均 +24.8 分，在 Claude Code 执行环境中平均 +19.1 分。

![完整结果](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFkibpzA1zCRZYQoGeEcXDbDceFLLcLnALNCicUkRq9icKpicXeHib0VcErueYgORefDsgfJmbymxk3RhTTAlVu2uvCBrQZUPOdeOibVE/640?from=appmsg)

全部模型 × 环境的完整结果

**52 个测试格，全部最优或并列最优。没有一个输的。**

##  碾压人类手写 

你可能想问：那跟人类手写的 Skill 比呢？

研究团队专门做了对比。

![方法对比](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkf3fYMibWyepGMH5u183lxT749ibCYxknicib4Yu5UicECRGoztEgvpKEObIkZ2ZvfkQiaS4cLj9r7DVyhRqjv9JFhP9zibU8d0gu9hI/640?from=appmsg)

SkillOpt vs 所有基线方法

把人类精心编写的 Skill 文档（145-516 tokens）作为基线，SkillOpt 在 GPT-5.5 直接对话上的平均分是 82.3，而包含人类手写 Skill 在内的所有其他方法的「逐个 benchmark 最佳选择」组合平均只有 76.9。

也就是说，**即使你每个 benchmark 都挑表现最好的那个基线方法，组合起来的平均分也比不过 SkillOpt。**

被对比的方法包括：One-shot LLM 生成的 Skill、Trace2Skill（从轨迹中蒸馏）、TextGrad（梯度风格优化）、GEPA（Pareto 反射演化）、EvoSkill（技能文件夹演化）。

全部，被碾压。

##  AI 学到了什么 

而被优化出来的 Skill 文档长什么样呢？

论文里展示了几条学到的规则，看完之后你会觉得，这些规则人类确实很难想到，但一旦看到又会觉得「确实应该这么写」。

SearchQA

根据线索的措辞推断预期答案的类型，然后从共现的独特证据中选择最短的规范实体。

告诉 Agent 不要给出冗长的答案，要精准定位到最短的、符合规范命名的实体。

SpreadsheetBench

先检查工作簿结构和公式，然后在整个请求的目标范围内写入已计算的静态值，而非依赖 Excel 自动重算。

抓住了一个关键 bug：不少 Agent 会写 Excel 公式然后期望自动计算结果，但在自动化环境中……这往往靠不住。

ALFWorld

维护一个包含地平线感知的已访问/前沿位置清单，在连续相同类型的失败后切换搜索方向，并在拿到目标物品之前避免重新访问目的地。

教会 Agent 在虚拟环境中做空间记忆管理，防止在同一个地方转圈。

这些规则有几个共同特点：

极度具体 没有「仔细检查」「认真思考」这类你的老板爱讲的空话，每一条都精确到了操作层面。

反直觉 涉及的场景，人类在写 Skill 时压根不会想到。

紧凑 最终的 Skill 文件只有 379-1995 个 token，中位数大约 920 个 token。有的 benchmark 甚至只需要一条被接受的修改就涨了 39 分。

##  演化过程 

光看最终规则，你可能会感受不深。

于是论文里还展示了 Skill 文档的完整演化过程，你能看到一份空白的 Skill 是怎么一步步长成最终版本的。

拿 **ALFWorld** 举个例子：

初始状态

一份泛泛的指令：搜索、变换、放置，类似「找到东西、处理一下、放到指定位置」

第一轮 Rollout

发现 Agent 经常找不到目标物品，在同一个房间反复搜索。加了规则：记住你去过哪些地方，别重复访问

继续迭代

发现 Agent 拿到东西后在路上弄丢了。加了规则：拿到物品后锁定进度，不做多余操作

最终：49.3 → 74.6（+25.3）

从几乎不能用到相当能打

**SpreadsheetBench** 的演化也很类似。初始 Skill 只是个通用的自动化指令，经过几轮优化后，Agent 学会了先检查工作簿的 header 和 range、做 key 归一化处理、用静态值替代公式依赖、保留辅助计算列等一系列细致操作。

最终效果：**从 40.4 涨到 78.9，提升了 38.5 分。**

这些演化过程都展示出：好的 Skill 文档，不是一个人坐那想出来的，**它应该是从实践中跑出来的** 。

##  跨模型跨环境 

SkillOpt 还有一个特性是：**优化出来的 Skill 可以跨模型、跨执行环境迁移。**

跨模型迁移

GPT-5.4 → GPT-5.4-mini（SpreadsheetBench）

+9.4

跨执行环境迁移

Codex → Claude Code（SpreadsheetBench）

+59.7

跨任务迁移

OlympiadBench → Omni-MATH（GPT-5.4）

+3.7

也就是说，**你用一个模型优化出来的 Skill，换个模型、换个工具……甚至换个任务，大概率还是有效的。**

训练成本是一次性付出的（离线完成），而部署时的额外开销是零。优化后的 Skill 文件就是一段纯文本，拿过来就可以直接使用。

##  $ 训练成本 

而这个优化过程要花多少钱呢？

论文给出了的数据是：对于流程类 benchmark（SearchQA、DocVQA），每提升一个绝对测试分数需要 0.6-3.6M 训练 token。对于复杂轨迹类 benchmark（SpreadsheetBench、ALFWorld），则需要 37.9-46.4M token。

这笔钱其实并不算贵（甚至可以说很便宜了），且关键在于：**这只需要训练一次。**

训练好的 Skill 文件，在每次使用时都没有额外的成本。如果你的 Agent 要跑成千上万次任务，这点点训练成本早就摊平了，但带来的提升之大，显然是极其值得的。

就好比，花一笔钱请了个顾问来写操作手册，以后所有员工照着操作手册干活就行了。顾问当然是用一次就可以滚蛋了……

##  大小通吃 

论文还测了不同规模模型的表现。

除了 GPT-5.5、GPT-5.4、GPT-5.2 这些前沿大模型之外，研究团队还在 GPT-5.4-mini、GPT-5.4-nano、Qwen3.5-4B、Qwen3.6-35B-A3B 这些更小的模型上做了实验。

结果显示，**所有规模的模型都有一致的提升。**

这表明，你不一定非得用最贵的模型才能受益于 SkillOpt，哪怕是小模型配上优化过的 Skill，也能比大模型裸跑的效果好。

另一个数据是：**训练数据量对效果的影响。**

以 SpreadsheetBench 为例，用 1% 的训练数据做优化，分数是 47.5。用 100% 的训练数据，分数涨到 78.0。

数据越多，Skill 优化得越好。但即使数据量不大，SkillOpt 依然能带来可观的提升。

##  动手试试 

微软已经把 SkillOpt 完整开源了（MIT 协议），你可以直接跑起来了。

安装很简单：

```bash
    git clone https://github.com/microsoft/SkillOpt.git
    cd SkillOpt
    pip install -e .
```

再配好 API 密钥：

```bash
    cp .env.example .env
    # 填入你的 API 密钥，然后 source
    source .env
    
    # Azure OpenAI（推荐） （...毕竟微软自家产品）
    export AZURE_OPENAI_ENDPOINT="https://your-resource.openai.azure.com/"
    export AZURE_OPENAI_API_KEY="your-key"
    
    # 或者直接用 OpenAI
    export OPENAI_API_KEY="sk-..."
    
    # 也支持 Anthropic Claude
    export ANTHROPIC_API_KEY="sk-ant-..."
```

然后，一行命令就能开始训练：

```bash
    python scripts/train.py \
        --config configs/searchqa/default.yaml \
        --split_dir /path/to/your/searchqa_split \
        --optimizer_model gpt-5.5 \
        --target_model gpt-5.5 \
        --num_epochs 4 \
        --batch_size 40
```

目前支持 6 个 benchmark：SearchQA、SpreadsheetBench、OfficeQA、DocVQA、LiveMathematicianBench、ALFWorld。每个都有对应的配置文件，放在 `configs/` 目录下。

训练完成后，输出目录里会有一个 `best_skill.md`，这就是最终优化好的 Skill 文件，可以直接拿去使用：
```
    outputs/<run_name>/
    ├── best_skill.md            # 最优 Skill 文档
    ├── history.json             # 训练历史
    ├── skills/skill_vXXXX.md   # 每步快照
    └── steps/step_XXXX/        # 每步的 patch 和评估
```

可以单独跑 eval 来进行评估：

```bash
    python scripts/eval_only.py \
      --config configs/searchqa/default.yaml \
      --skill outputs/my_run/best_skill.md \
      --split valid_unseen \
      --split_dir /path/to/searchqa_split
```

甚至，团队还提供了个 WebUI 可以实时监控训练过程：

```bash
    pip install -e ".[webui]"
    python -m skillopt_webui.app --port 7860
```

整个项目还支持断点续训，中断后重新跑同样的命令，会自动从上次完成的 step 继续。

##  别再手写了 

微软这篇论文传递的信号是：

**手写 Skill 文档或许是个好起点，但不应该是终点。**

当然，论文也坦承了该方法的局限：SkillOpt 需要任务有可自动评估的标准（exact match 或自动评分器），开放性任务目前暂还不太适用。

总结一下，SkillOpt 的核心循环是：

让 Agent 做任务 → 分析失败原因 → 生成修改建议 → 在验证集上验证 → 接受或拒绝。这个流程你完全可以手动模仿：观察 Agent 在哪些任务上犯错，分析错误模式，针对性地补充规则，然后验证效果。

**Skill 文档不应该是一次性写完就放着不管的东西，它应该像模型权重一样，持续被优化。**

##  相关链接 

论文：https://arxiv.org/abs/2605.23904

项目主页：https://microsoft.github.io/SkillOpt/

GitHub 代码：https://github.com/microsoft/SkillOpt

演示视频：https://youtu.be/JUBMDTCiM0M

相关项目 SkillLens：https://microsoft.github.io/SkillLens/
