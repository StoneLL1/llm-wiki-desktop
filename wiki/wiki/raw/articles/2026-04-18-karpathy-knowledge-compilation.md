---
title: "Karpathy大神的个人知识编译理论和我的一些理解实践"
url: "https://mp.weixin.qq.com/s/P6UgixMvCNz0gWm1tp3MJA"
source: "微信公众号"
fetched: 2026-04-18
sha256: bcf04019feca2e01
---

Andrej Karpathy 昨天发了一个 gist，没有配文，就一个 Markdown 文件，标题叫 LLM Wiki。

https://link.bytenote.net/6cQAbq

内容不长，但我读完觉得这个思路值得认真学习。

RAG 的根本问题

现在大多数人用 LLM 处理文档的方式，本质上都是 RAG，把文件上传上去，问问题的时候检索相关片段，拼出一个答案。

NotebookLM 是这个逻辑，ChatGPT 文件上传是这个逻辑，大多数所谓的知识库产品也是这个逻辑。

这套方法能用，但有一个根本缺陷，每次都在从零开始。

你问一个需要综合五份文档的问题，LLM 就得重新找、重新拼、重新推导。上次的推导结果？消失在聊天记录里了。

结果就是什么都没有积累下来。

Karpathy 的想法是换一个范式。

去编译，而不是检索

核心思路是，不要在查询时检索，要在写入时编译。

比如每次你加入一份新资料，LLM 不是把它存起来等你问，而是直接读它、消化它、把信息整合进一个持续维护的 wiki，然后去更新相关页面，标注与旧内容的矛盾，建立交叉引用，补充综合结论。

最后的产出物wiki 是一个由 LLM 生成的 Markdown 文件目录，摘要、实体页面、概念页面、对比分析、全局索引，全部互相链接。

他的原话是：Obsidian 是 IDE，LLM 是程序员，wiki 是代码库。

那具体怎么做呢？

他把这个编译过程分成了三层结构：

第一层是原始资料。

文章、论文、图片、数据文件，只读不改，这是事实的源头，LLM 不能动它。

第二层是 wiki 本体。

完全由 LLM 负责写作和维护，你只负责读。

第三层是 schema。

一份配置文档，在Claude Code 里这个配置文件夹 CLAUDE.md，在Codex 里叫 AGENTS.md。在这个文章里面有详细的讲解：[Codex CLI 的使用指南和一些最佳实践](https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247515311&idx=1&sn=a9025eb1fb55741ff77cfc7f7f4f2e15&payreadticket=HH5LW1Q9P1kDaGG29FJEjEqGMrsseaIaQAmLeZ-uwKG2LZX4QdsrDaaFfoheup9pMbNOdCg&scene=21#wechat_redirect)

它的作用就是告诉 LLM wiki 的结构是什么、约定是什么、遇到新资料该怎么处理。

日常操作就三件事。

摄入，加新资料进来，LLM 处理并更新相关页面。

查询，向 wiki 提问，LLM 综合相关页面给出带引用的答案，好的答案本身也可以归档回 wiki。

整理，定期让 LLM 做健康检查，找矛盾、找孤儿页面、找数据空白。

为什么不要人工维护？

这套东西人类自己理论上也能做，问题是没有人真的会坚持做。

大部分人收藏和整理资料都是为了满足收藏癖而已，收藏就是学会，收藏就是吃灰。

因为这里面存在了一个巨大的摩擦。

维护知识库最烦的不是读资料、不是思考，是记账，更新交叉引用，修订被新资料推翻的旧观点，在几十个页面之间保持一致性。

这些工作既无聊又耗时，而且随着资料量增长，负担会越来越重，最终大多数人的知识库都死在这里。

LLM 不会厌倦，不会忘记更新某个引用，一次可以处理十几个文件，维护成本几乎为零，知识库才有可能真正活下去。

Karpathy 提到，这个想法在精神上和 Vannevar Bush 1945 年提出的 Memex 相关。

Bush 当年设想了一种个人的、经过策划的知识存储系统，文档之间有关联性的路径，文档之间的连接和文档本身同样有价值，但他没有解决的问题是谁来做维护。

LLM 解决了这个问题。

适合什么场景？

Karpathy 列了几个：个人成长追踪、研究课题深潜、读书笔记积累、企业内部知识库。

我觉得最有潜力的是两端。

一端是个人研究者。

长期跟踪某个领域，资料越积越多，但一直散着，这套方法可以把散料变成一个持续生长的知识图谱。

另一端是小团队。

会议记录、客户通话、项目文档，这些东西现在基本上都是沉没的，没有人有精力整理。让 LLM 维护一个团队 wiki，理论上可以把这些信息真正盘活。

其实他的整个思路非常的简单和明确，RAG 的问题不是技术问题，是范式问题。

检索不如编译，聊天记录不如持久知识库，每次从零开始不如持续积累。

要做的就是把知识当作可编译的资产，把 LLM 当作主要编辑者，而不是手动整理笔记。

不按照之前的惯例一样，他没有给出非常具体的步骤，那么我按照我自己的理解，做了以下的整理：

整个知识库围绕可以一个特定目录来完成。

`knowledge-base/
├── raw/ ← 原始资料暂存区
│ ├── articles/ 网页剪藏（Obsidian Web Clipper 转 .md + 本地图片）
│ ├── papers/ arXiv 论文 PDF
│ ├── repos/ GitHub 仓库
│ └── datasets/ 数据集
│
└── wiki/ ← LLM 编译产物（你几乎不需要手动编辑）
├── index.md 索引文件（自动维护，所有文档的简短摘要）
├── concepts/*.md 概念文章（~100篇，~40万字，互相 backlinked）
├── outputs/ 衍生输出（幻灯片、图表、Q&A 答案）
└── ... 交叉链接网络

`
```

工作流分成四步：

Phase 1：数据摄入

用 Obsidian Web Clipper 浏览器扩展把网页文章一键转成 .md 文件，中间设置热键自动下载文章中的所有图片到本地，这样 LLM 可以直接引用，将arXiv 论文、GitHub repo、数据集等统统丢进 raw/。

你只负责收集，不负责整理

Phase 2：LLM 编译

这是最核心的一步。读取 raw/ 中的新文件，用LLM写摘要，提取概念，创建概念文章，建立双向链接，如果后期有内容的更新，就自动去维护索引。

同样也不需要手动编辑 wiki，LLM 写和维护几乎所有内容。

Phase 3：查询与增强

这一步是向知识库提问复杂问题，LLM读索引、跟踪链接、跨文章综合信息，并生成相关的Markdown 文件、Marp 幻灯片、matplotlib 图表。

Phase 4：健康检查与维护

LLM 定期扫描整个 wiki，做代码lint式的检查，包含补全缺失信息，发现新连接，基于现有知识推荐值得探索的方向等等。

**不过Karpathy 的知识库方法论适用范围太广泛了，如果是用于编程，那么其中一半的部分得改。**

Karpathy这套知识库编译的本质是用AI来处理上下文，进行上下文的管理。

这点其实没有问题，但是在对于特定编程或者我个人的使用习惯上来说，可能会有点出入，以下仅代表我个人的使用和工具使用，仅供参考。

**第一，我不用 Obsidian**

Obsidian实在是太丑了，实难下咽。另外，我觉得AI时代，每个人都应该去Vibe Coding打造一个专属于自己的AI笔记。

因为你所能下载到和能见到的笔记工具都是面向大众的，它很多的功能和功能或者操作逻辑并不能满足你特定的需要和需求。

现在用 Cursor、Claude Code 搭一个自己的笔记工具，一个上午就能跑起来。里面放什么、怎么组织、索引怎么设计，完全按照自己的工作方式来。这个东西别人复制不了，因为它是从你自己的工作流里长出来的。

![image](https://mmbiz.qpic.cn/mmbiz_jpg/iacaCWlP1x1wg7CMVdGJk0AKxCuIO8cqR2JoylEgcyibuPVyiavu9ykba0EmwZxnx1QzHCrDuDntYfJiaGS8ib7xMw6oXHmDBkRsm4PCxtCNj1MQ/640?wx_fmt=jpeg)

我自己就写了这样的一个笔记工具，这个工具简单到没有导入导出，只有复制，代码着色，没有渲染，使Google Drive 同步备份。

第二，只对话不收藏。

Karpathy 这套东西的前提是你有大量值得整理的收藏。文章、论文、repo，攒够了再让 LLM 编译。

今年年初的时候我就把收藏夹里所有内容清完了，现在基本不再做任何收藏。[Sonnet 4.6 用爽了！极速上架了一个Chrome书签整理插件](https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247513711&idx=1&sn=3d9a67392c1811f108e746d6faf99553&scene=21#wechat_redirect)

原因很简单，在AI时代如果一个东西你不能快速接受或理解，收藏起来大概率也不会再看，收藏行为本身满足的是收藏欲，不是知识获取。

我现在的做法是只对话，不收藏。

看到有价值的东西，直接打开网页，扔给 Claude 对话，处理完就结束。有用的结论落到知识库，原始链接不留。这个流程比攒一堆 raw 文件再批量编译快得多，信噪比也更高。

**第三点，编码知识库是以项目为单位**

Karpathy 的方案是一个大库，所有主题放在一起，靠 wiki 内部的交叉链接组织关系。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/iacaCWlP1x1xZERjpobGiaRUCFpjg89P0GP1UvMYcLCRC4l5lhaibnO6eSZibdJtyesYn2UscE5XoeNiaPUassMA2WU9UqTiaOpWnKT02tAPUnvX8/640?wx_fmt=png)

我的做法是每个项目有自己独立的 KB 知识库。

必要的冗余是允许的，知识的最终归宿是具体项目，不是一个统一的大目录，就好比你编译出来的应用，最后都是跑在具体的容器当中。

尤其是对于编程项目而言更加的重要，因为通用知识库的维护成本会随规模指数级上升，而项目知识库的范围是有界的，维护成本可控。

因为所有的知识库的目的最后还是应用，这收藏对话最终会根据具体项目落到对应的 KB 里进行固化。

![image](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1wMibgU9nIdr3CHn9Ehg40C2YMmUCaq4mCF7jr2ibUbiaecpC3a4k4LNF06szmZicnVzYWqZ2so01wGleGmm7mhDBI6QT5iaURxPrvw/640?wx_fmt=png)

**第四点，索引这件事，我们做法完全一致**

在索引上，我和 Karpathy 的逻辑完全一样。

我的 KB 知识库内部有分级索引：踩过的坑、项目架构、具体方法。

AI 启动时不会加载所有内容，CLAUDE.md 里直接告诉它常用知识库的路径，它通过索引快速理解知识库全貌，需要细节时再去读具体文件。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/iacaCWlP1x1y6BbAictMTzjn3AFbSkzXnxSvA79hPRLqFCPzXKYCwt0Y8pssGdfknAweWaUZ85XJCITWicQ2FA5HNL7H7ckq2OSPOW6iaTXXFUs/640?wx_fmt=jpeg)

Karpathy 讲的增量更新，我这里对应的是只维护索引本身。

新内容进来，更新索引，AI 下次启动就能感知到变化。整体逻辑和他的方法完全一致。

**最后，个人笔记这件事，不应该照搬别人的方案**

Karpathy他自己最后说了一句话：这套东西目前像一堆 hacky scripts。

这套方法现在的使用门槛是你需要会配 Obsidian 插件，会写 prompt，会自己搞一个搜索工具，还得有足够多值得整理的原始资料。

能把这些都跑通的人，本来就是少数。

Karpathy 的价值在于他把一个范式说清楚了：

知识库应该像代码仓库一样管理，有输入、有编译、有产物、有测试。

这个框架是对的，但具体工具怎么选、粒度怎么定、收藏还是对话、大库还是项目库，没有标准答案。

你的工作流决定你的答案。

照搬任何人的方案都是错的，包括 Karpathy 的。

![image](https://mmbiz.qpic.cn/mmbiz_jpg/iacaCWlP1x1zsgYbsHV30iaM4kBsJKHgAPgS7O7rx1U8rhrMwGu2KP2gR2UZftfric1tiaUL7gvP5YxyJaoWMiarib0SodQiaEXZafNMcBTjJDRItI/640?wx_fmt=jpeg)