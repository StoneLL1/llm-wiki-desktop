---
title: "ARIS睡前定方向醒来收初稿全自动跑实验改论文"
url: "https://mp.weixin.qq.com/s/tDniVryVGjDkkkWl-5sTkQ"
source: "微信公众号"
fetched: 2026-05-27
sha256: 0c6c285a699e2503
image_count: 23
---

如果你还在熬夜手搓代码、调参跑实验，那这个刚刚开源的科研工作流绝对会让你眼前一亮。

  

它就是 ARIS（Auto-Research-In-Sleep），一款真正帮你实现“睡后科研”的全自动神器。

  

这个项目的核心理念很直接，让 Claude Code 在你睡觉时做科研。

  

睡前丢给 AI 一篇论文初稿，醒来就能发现，站不住脚的 claim 已被剔除，20 多组 GPU 实验默默跑完，整篇论文的叙事框架焕然一新，分数也从 5.0 稳步提升到了可投稿的 7.5 分——而且全流程零人工干预。

作为一套专为机器学习科研定制的 Claude Code Skills，ARIS 既吸收了 FARS 的经验，也呼应了 Karpathy 提出的 autoresearch 思想。

  

它没有采用复杂的四智能体分工，而是采用跨模型协作实现了闭环。

  

在这个框架下，Claude Code 负责干活（读文件、写代码、跑实验、收结果），外部 LLM（通过 Codex MCP）专门负责评审（打分、找弱点、建议修复）。

  

两个模型互不评阅自己的作业，通过反复的交叉辩论，形成真正的正向反馈。

  

为了降低使用门槛，它还支持 GLM + GPT 或 GLM + MiniMax 等替代模型组合，无需 Claude API 也能直接跑通全流程。  

项目地址：  

https://github.com/wanshuiyin/Auto-claude-code-research-in-sleep

  

在本地跑通这套工作流非常简单，拉取代码、配置 Codex MCP，即可在终端一键启动对应流程。
```
    # 1. Install skills  
    git clone https://github.com/wanshuiyin/Auto-claude-code-research-in-sleep.git  
    cp -r Auto-claude-code-research-in-sleep/skills/* ~/.claude/skills/  
      
    # 2. Set up Codex MCP (for review skills)  
    npm install -g @openai/codex  
    claude mcp add codex -s user -- codex mcp-server  
      
    # 3. Use in Claude Code  
    claude  
    > /idea-discovery "your research direction"# Workflow 1: literature → brainstorm → validate  
    > /auto-review-loop                          # Workflow 2: review → fix → re-review overnight  
    > /paper-writing "NARRATIVE_REPORT.md"       # Workflow 3: narrative → polished PDF  
    > /research-pipeline "your research direction"# Full pipeline: Workflow 1 → 2 → 3 end-to-end  
    
```

ARIS 涵盖了从方向探索到论文定稿的各个环节，并在模型调用的自由度与底层算力保护上做了权衡。

  

🔄 **自动 review 循环** — 4 轮自主审稿，一夜从 5/10 提升到 7.5/10，自动跑 20+ 组 GPU 实验💡 **Idea 发现** — 文献调研 → 头脑风暴 8-12 个 idea → 查新 → GPU pilot 实验 → 排名报告🔍 **文献 & 查新** — 多源论文搜索（arXiv、Scholar、Semantic Scholar）+ 本地论文库扫描 + 跨模型查新验证🤖 **跨模型协作** — Claude Code 执行，GPT-5.4 xhigh 审稿。对抗式而非自我博弈📝 **Peer Review** — 以审稿人视角审阅他人论文，结构化打分 + meta-review🖥️ **GPU 部署** — 自动 rsync、screen 会话、多 GPU 并行实验、实时监控🔀 **灵活模型** — 默认 Claude × GPT-5.4，也支持 GLM + GPT、GLM + MiniMax——无需 Claude API🛑 **Human-in-the-loop** — 关键决策点可配置检查点 `AUTO_PROCEED=true` 全自动，`false` 逐步审批📊 **17 个可组合 skill** — 自由混搭，或串联为完整流水线（`/idea-discovery`、`/auto-review-loop`、`/research-pipeline`）  
以某 ML 研究项目实测为例，经过 4 轮自动实验与叙事重构，它将论文质量从 borderline reject 提升到了可投稿水平：

工作流

项目内所有的 Skills 共同构成了一条端到端的科研流水线。其中最核心的三大工作流，既可以作为独立模块单点发力，也能无缝串联使用：

  

探索新方向（比如写 survey）？从工作流 1 开始 → `/idea-discovery`

`  
`

`已有 idea + 初步方案？直接用工作流 2 → ``/auto-review-loop`

`  
`

`准备写论文了？工作流 3 → ``/paper-writing`（或分步：`/paper-plan` → `/paper-figure` → `/paper-write` → `/paper-compile` → `/auto-paper-improvement-loop`）

  

全流程？工作流 1 → 工作流 2 → 工作流 3 → `/research-pipeline`，从文献调研一路到投稿

  

⚠️ 重要提醒：这些工具加速科研，但不能替代你自己的思考。生成的 idea 一定要用你的领域知识审视，质疑其假设，最终决策权在你手上。最好的研究 = 人的洞察 + AI 的执行力，而不是全自动流水线。

  

完整流程如下：

  

```
    /research-lit → /idea-creator → /novelty-check → 实现 → /run-experiment → /auto-review-loop → /paper-plan → /paper-figure → /paper-write → /auto-paper-improvement-loop → 投稿  
      (调研文献)      (找idea)       (查新验证)     (写代码)   (部署跑实验)     (自动改到能投)      (大纲)        (作图)        (LaTeX+PDF)     (审稿×2 + 格式检查)     (搞定!)  
      ├──── 工作流 1：找 Idea ────┤                 ├──── 工作流 2：自动循环 ────┤   ├───────────────── 工作流 3：论文写作 ─────────────────────┤  
    
```

  

  

工作流 1：文献调研与找 Idea

  

  

  

  

还没有具体 idea？给一个研究方向就行——`/idea-creator` 搞定剩下的：

📚 **调研** 全景（最新论文、开放问题、反复出现的局限性）🧠 **头脑风暴** 8-12 个具体 idea（GPT-5.4 xhigh）🛡️ **深度验证** top idea（完整查新 + devil's advocate review）🧪 **并行 pilot 实验** （top 2-3 个 idea 分别上不同 GPU，30 分钟 - 2 小时）🏆 **按实验信号排序** ——有正信号的 idea 排前面

  

输出 `IDEA_REPORT.md`：含假设、pilot 结果、审稿人可能的质疑、建议执行顺序。失败的 idea 也记录在案，避免重复踩坑。

**  
**

**涉及 Skills：**` research-lit` \+ `idea-creator` \+ `novelty-check` \+ `research-review`

`  
`

💡 一键调用：/idea-discovery "你的研究方向" 自动跑完整个工作流 1。

  

🔄 人在回路中：每个阶段都会展示结果等你反馈。不满意？告诉它哪里不对——调整 prompt 重新生成。信任默认选择？它会自动带着最优方案继续。你决定参与多深。

  

⚙️ Pilot 实验预算（最大时长、超时、GPU 总预算）均可配置——见自定义。

  

  

```
    1. /research-lit "discrete diffusion models"    ← 先读本地论文，再搜外部，整理全景  
    2. /idea-creator "DLLMs post training"     ← 自动生成 8-12 个 idea，筛选排序  
    3. 选 top 2-3 个 idea  
    4. /novelty-check "top idea"                     ← 查新：有没有人做过？  
    5. /research-review "top idea"                   ← 让外部 LLM 批判你的想法  
    6. 实现 → /run-experiment → /auto-review-loop    ← 闭环！  
    
```

  

  

工作流 2：自动科研循环（睡一觉醒来看结果）

  

  

  

"帮我 review 论文，修复问题，循环到通过为止。"

  

涉及 Skills：auto-review-loop + research-review + novelty-check + run-experiment + analyze-results + monitor-experiment

  

💡 一键调用：/auto-review-loop "你的论文主题" 自动跑完整个工作流 2。

  

```
    外部 LLM 评审 → Claude Code 实现修复 → /run-experiment 部署 → 收结果 → 再评审 → 循环  
                    ↑ 需要新方向时自动 /novelty-check 查新  
    
```

  

用法：

  

```
    > /auto-review-loop 我的 diffusion model 论文  
    
```

  

  

**🛡️ 关键安全机制：**

🔒 **MAX_ROUNDS = 4** — 防止无限循环；达到分数阈值时提前停止⏱️ **> 4 GPU-hour 的实验自动跳过** — 不会启动超大实验，标记为"需人工跟进"🧠 **优先改叙事而非跑新实验** — 同样能解决问题时，选择成本更低的路径🪞 **不隐藏弱点** — 明确规则："不要隐藏弱点来骗高分"🔧 **先修后审** — 必须实现修复后再重新 review，不能只承诺修💾 **上下文压缩恢复** — 每轮结束后持久化状态到 `REVIEW_STATE.json`。如果上下文窗口满了触发自动 compact，工作流会从状态文件恢复断点继续——无需人工干预

⚙️ MAX_ROUNDS、分数阈值、GPU 限制均可配置——见自定义。

"把我的研究报告变成可投稿的 PDF。" 需要本地 LaTeX 环境——见前置条件。

  

涉及 Skills：paper-plan + paper-figure + paper-write + paper-compile + auto-paper-improvement-loop

  

💡 一键调用：/paper-writing "NARRATIVE_REPORT.md" 自动跑完整个工作流 3。

**输入：** 一份 `NARRATIVE_REPORT.md`，描述研究内容：声明、实验、结果、图表。叙事越详细（尤其是图表描述和定量结果），输出越好。

  

**输出：** 一个可投稿的 `paper/` 目录，含 LaTeX 源码、干净的 `.bib`（仅含实际引用）、编译好的 PDF。

  

```
    NARRATIVE_REPORT.md ──► /paper-plan ──► /paper-figure ──► /paper-write ──► /paper-compile  
        (研究叙事)          (大纲+矩阵)     (图表+LaTeX)      (逐节LaTeX)      (编译PDF)  
    
```

  

```
    典型流程：  
    1. 写 NARRATIVE_REPORT.md（来自工作流 2 的结果）  
    2. /paper-plan — 生成 claims-evidence 矩阵 + 分节计划  
    3. /paper-figure — 生成对比表、训练曲线等图表  
    4. /paper-write — 逐 section 生成 LaTeX（含 bib 清理、de-AI 打磨）  
    5. /paper-compile — 编译 PDF、修复错误、页数验证  
    6. /auto-paper-improvement-loop — 内容审稿 ×2 + 格式合规检查  
    
```

  

  

核心特性：

📐 **Claims-Evidence 矩阵** — 每个声明映射到证据，每个实验支撑一个声明📊 **自动图表生成** — 从 JSON 数据生成折线图、柱状图、对比表🧹 **Bib 自动清理** — 过滤未引用条目（实测 948→215 行）📄 **灵活节数** — 5-8 节按论文类型选择（理论论文常需 7 节）🔍 **GPT-5.4 审稿** — 每步可选外部 LLM 审查✂️ **De-AI 打磨** — 去除 AI 写作痕迹（delve、pivotal、landscape…）🎯 **精确页数验证** — 基于 `pdftotext` 定位 Conclusion 结束位置

  

⚠️ /paper-figure 能做什么、不能做什么：能自动生成数据驱动的图表（训练曲线、柱状图、热力图）和 LaTeX 对比表（从 JSON/CSV 数据）。

  

不能生成架构图、流程图、模型示意图、生成样本网格——这些需要手动创建（draw.io、Figma、TikZ 等），放到 figures/ 目录后再跑 /paper-write。

  

一篇典型 ML 论文中，约 60% 的图表可自动生成，约 40% 需手动制作。

**  
**

**端到端实测：** 从一份 NARRATIVE_REPORT.md 生成了一篇 9 页 ICLR 2026 理论论文（7 节、29 条引用、4 张图、2 个对比表）——零编译错误、零 undefined reference。

  

#### 论文自动润色循环：工作流 3 生成论文后，`/auto-paper-improvement-loop` 自动跑 2 轮 GPT-5.4 xhigh 内容审稿 → 修复 → 重编译，外加一轮格式合规检查，将粗稿自动提升到可投稿质量。

####   

**分数变化（实测 — ICLR 2026 理论论文）：**

**  
**

**  
**

  

最终：正文 8 页（ICLR 限 9 页），0 个 overfull hbox，格式合规。 3 轮共涨 4.5 分。

  

全部 Skills

如何安装？

前置条件

1\. 安装 Claude Code（仅 review 类 skill 需要）

  

2\. 安装 Codex CLI 并配置为 MCP server：

  

```
    npm install -g @openai/codex  
    claude mcp add codex -s user -- codex mcp-server  
    
```

  

  

3.（仅工作流 3：论文写作需要）LaTeX 环境，含 latexmk 和 pdfinfo：

  

  

```
    # macOS  
    brew install --cask mactex    # 或: brew install basictex  
    brew install poppler          # 提供 pdfinfo  
      
    # Ubuntu/Debian  
    sudo apt install texlive-full latexmk poppler-utils  
      
    # 验证  
    latexmk --version && pdfinfo -v  
    
```

  

  

如果只用工作流 1 和 2（找 idea + 自动 review），不需要安装 LaTeX。

  

安装 Skills
```
    git clone https://github.com/wanshuiyin/Auto-claude-code-research-in-sleep.git  
    cd Auto-claude-code-research-in-sleep  
      
    # 安装全部 skills（全局可用）  
    cp -r skills/* ~/.claude/skills/  
      
    # 或者只安装特定 skill  
    cp -r skills/auto-review-loop ~/.claude/skills/  
    cp -r skills/research-lit ~/.claude/skills/  
    
```

至于如何设置通宵免确认、如何让 agent 自动直连远程 GPU 跑实验，官方文档已提供现成模板，直接去仓库抄作业即可。

进阶玩法：深度自定义

系统所有的 skills 均为标准 markdown 文件，为二次开发留足了空间。开发者可自由修改底层参数或替换模型底座。

  

精细化参数调控：

  * 迭代阈值：默认最多执行 4 轮 review（MAX_ROUNDS），论文得分达 6/10（POSITIVE_THRESHOLD）即自动停止。

  * 算力熔断：预估耗时超 4 小时的实验自动跳过转人工；单次 idea 测试支持灵活设置超时限制与 GPU 总预算。

  * 流程审批：通过 AUTO_PROCEED 参数可一键切换“全自动跑通”或“步步人工审批”模式。

  

本地文献库复用：指定 PAPER_LIBRARY 路径后，系统在联网检索前会优先扫描并阅读本地已有 PDF。

  

平替模型底座：原生支持免 Claude/OpenAI API 方案。例如通过修改配置文件，可无缝切换为 GLM-5（执行）+ MiniMax-M2.5（评审）的双模博弈架构。

结语

据作者介绍，目前这套工具已经能跑通从找 idea 到产出论文 PDF 的全流程。接下来的规划则更看重生态集成，比如引入多模态反馈和更顺手的办公流自动化。

  

飞书集成：支持关键节点的消息推送，通过双向桥接在通讯软件内直接完成 idea 审批。

  

W&B 集成：探索对接 Weights & Biases (W&B)，使系统能直接读取训练曲线与 loss 指标，自动诊断问题并给出后续实验建议。

  

MCP 集成：计划开发 Zotero 与 Obsidian 的 MCP 集成，深度读取个人文献库、批注与知识图谱，提升研究上下文的精准度。

  

更多博弈组合：引入 Gemini、DeepSeek 等大模型，探索不同执行者 × 评审者组合的性能边界。  

  

目前该项目已在 GitHub 完全开源。感兴趣的同学可以访问项目主页获取详细配置，用手头的 idea 跑一轮试试效果。

  

  

如何才能让更多的优质内容以更短路径到达读者群体，缩短读者寻找优质内容的成本呢？**答案就是：你不认识的人。**

  

总有一些你不认识的人，知道你想知道的东西。PaperWeekly 或许可以成为一座桥梁，促使不同背景、不同方向的学者和学术灵感相互碰撞，迸发出更多的可能性。 

  

PaperWeekly 鼓励高校实验室或个人，在我们的平台上分享各类优质内容，可以是**最新论文解读** ，也可以是**学术热点剖析** 、**科研心得** 或**竞赛经验讲解** 等。我们的目的只有一个，让知识真正流动起来。

  

📝 **稿件基本要求：**

• 文章确系个人**原创作品** ，未曾在公开渠道发表，如为其他平台已发表或待发表的文章，请明确标注 

• 稿件建议以 **markdown** 格式撰写，文中配图以附件形式发送，要求图片清晰，无版权问题

• PaperWeekly 尊重原作者署名权，并将为每篇被采纳的原创首发稿件，提供**业内具有竞争力稿酬** ，具体依据文章阅读量和文章质量阶梯制结算

  

📬 **投稿通道：**

• 投稿邮箱：hr@paperweekly.site 

• 来稿请备注即时联系方式（微信），以便我们在稿件选用的第一时间联系作者

• 您也可以直接添加小编微信（**pwbot02** ）快速投稿，备注：姓名-投稿

  

**△长按添加PaperWeekly小编**

  

🔍

  

现在，在**「知乎」** 也能找到我们了

进入知乎首页搜索**「PaperWeekly」**

  

  

![](https://mmbiz.qpic.cn/mmbiz_png/VBcD02jFhgnZ3nlEAOI3MyTd7jqeD6cq8uTbkM2xZNpribyNr9liaPJ722zaHxd0YpQvib2nxOYmWibydCVY7W94ew/640?wx_fmt=jpeg#imgIndex=41)
