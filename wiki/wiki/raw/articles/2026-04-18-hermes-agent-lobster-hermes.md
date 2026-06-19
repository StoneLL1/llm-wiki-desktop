---
title: "今天开始除了养龙虾（OpenClaw）还要养\"爱马仕\"（Hermes Agent）！记性好，Skills还能自我进化！"
url: "https://mp.weixin.qq.com/s/7E8Vxp54CfIH6J-szYfVVA"
source: "微信公众号"
fetched: 2026-04-18
sha256: 02091298d375d4e6
---

养了很多只龙虾，虽然有几只已稳定在岗，但依旧对OpenClaw脆弱的记忆机制感到恼火，明明昨晚千叮万嘱的工作流，第二天一早就忘的一干二净。虽然也找了各种补丁给OpenClaw打上，但效果却各种不尽人意。
直到...最近在Twitter不断看到有人给Hermes Agent打出好评，今天决定上手一试，果然，真香！

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGh0ndruBmwa9GD1Y33wW6oS7o8O7XRwHEXwibjgZp5YgYZsc1PGicf9iazb1SK7MxlrS1Zvok4mVGRmKejdBVzMLyw6egDe0Hkbt8/640?wx_fmt=png&from=appmsg)
Hermes Agent的Github仓库： https://github.com/NousResearch/hermes-agent
该项目的star数也是原地起飞状：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGiaYsceQVMEvsdv2NkcZnBVgeyTDicicSwuicvNT6GfASMUK15J04DPrxVJwORTnY2ZHO9ntFIdIZwI7RVzwbRrBGhFYsbuGsOQ30A/640?wx_fmt=png&from=appmsg)

和OpenClaw相比，Hermes Agent的最大特点：`Hermes Agent 的 Skills 带有自我进化机制`

由于我只有主观体验，于是，我让GPT-5.4读了Hermes Agent的源码，并做出了以下总结：

**Hermes 把 skill 做成了学习闭环的一部分**，而不只是插件系统。可以拆成 5 点看：

#### 1. Hermes 把 skill 定义成“程序性记忆”(procedural memory)

在 Hermes 里，skill 不是单纯的命令扩展，而是明确被当成：

- • 复杂任务经验的沉淀

- • 可复用工作流

- • 未来任务的执行模板

从它的系统提示和工具设计看，Hermes 会被持续鼓励去做这件事：

- • 复杂任务完成后，把方法保存成 skill

- • 使用 skill 时发现过期或缺步骤，立即 patch

- • 把“本次学到的方法”转化为长期可复用能力

这就导致 skill 不再是静态资源，而是 **经验产物**。

#### 2. 它有专门的 skill 管理工具链

Hermes 不是只能“读 skill”，而是有完整的可写工具链：

- • `skills_list`

- • `skill_view`

- • `skill_manage`

尤其 `skill_manage` 很关键，因为它支持：

- • create

- • patch

- • edit

- • delete

- • 写辅助文件

这意味着 agent 不是停留在“建议以后记住”，而是真的能：

- • 创建技能

- • 修补技能

- • 维护技能

所以它具备了 **把经验写回系统** 的能力。

#### 3. Hermes 在 prompt 层强制推动 skill 演化

这点非常重要。

Hermes 的系统提示里，不只是“允许”它用 skill，而是明确要求它：

- • 先扫描可用 skills

- • 命中就加载 skill

- • 任务复杂后主动保存 skill

- • skill 发现问题时立刻 patch

这和很多 agent 最大区别在于：

- • 别的 agent：skill 是“可以用”

- • Hermes：skill 是“应该维护、应该演化、应该沉淀”

所以它的“自我进化”并不是偶然行为，而是被系统行为规范持续强化的。

#### 4. 它把 memory、session recall、skills 串起来了

Hermes 强的地方不是单个 skill 系统，而是三者联动：

- • **memory**：记稳定事实与偏好

- • **session_search**：召回过去会话经验

- • **skills**：沉淀成可执行方法论

这三层分工很清楚：

- • “用户喜欢什么” → memory

- • “之前怎么处理过类似问题” → session_search

- • “以后遇到这种任务怎么标准化执行” → skill

所以 Hermes 的 skill 进化不是孤立的，而是建立在： **记住事实 → 回忆经验 → 固化流程** 的链条上。

#### 5. Hermes 的设计目标本来就是“自我改进型 agent”

Hermes README 里就直接把自己定义为：

- • built-in learning loop

- • creates skills from experience

- • improves them during use

- • nudges itself to persist knowledge

这说明“skill 自我进化强”不是副作用，而是产品核心设计目标。
**
Hermes Agent就是为了Skill自我进化而生的！！！
而Skills就是工作流（know-how）的沉淀！
你用龙虾显然不是为了聊天聊的爽，而是希望有一套稳定且能持续优化的工作流，不是么？
龙虾给不了你的，Hermes Agent能给！

此外，还有个小细节：Hermes Agent在工作过程中会把log打印在聊天框中，和OpenClaw时不时会在聊天框中好几分钟什么输出都没有让人提心吊胆相比，Hermes Agent显然更让人安心。

![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGhnVo1u8peWGIoXcMk1fJLrMXDIH58oRsAdUFVficKPsKbn2tURakGOINgdYPpj1T4wr5VmMCtTUOen46MIyDtMPBceG31CVsvM/640?wx_fmt=png&from=appmsg)

关于Hermes Agent的安装，如果你之前手动安装过OpenClaw，其实也比较简单（今天时间所限，仅给出一个大致流程，改天给出保姆喂饭教程）：

- • 需要预先安装好git；

- • 对于Mac/Linux/Windows WLS2，输入以下命令完成安装和onboard：

`curl -fsSL https://raw.githubusercontent.com/NousResearch/hermes-agent/main/scripts/install.sh | bash`
```

- • 安装完成之后通过以下命令启动Hermes Agent：

`source ~/.bashrc    # reload shell (or: source ~/.zshrc)
hermes              # start chatting!`
```

- • 国内的聊天软件channel显式支持：飞书、钉钉

- • 可以在完成安装之后，在终端（Terminal）输入`hermes gateway setup`来触发添加新的聊天软件channel入口：

![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGiaPt8adkr3JcXm4lU9SvCLgia5eBg1FFb10YtTQ64o0Lf06EKo5lgHeR9ibtLGX43SXWFu49XnXKicsqibOQQrWbCCKgzxq4WABEWM/640?wx_fmt=png&from=appmsg)

- • 我是默认选择了飞书，聊天体验上和龙虾基本一致甚至比龙虾更solid；

![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGiaiaYZAJyM3UibFcjbQty0bMRJUwsyabUhQnHMvJicl7APeuJbgiaSqnrtV398cc5WHzzkmryJxYqpch5l6w5nzFCBkkL93GE7ZjDs/640?wx_fmt=png&from=appmsg)

截止此文推送时，我给”爱马仕“Hermes Agent装上了：

- • 飞书CLI

- • 企微CLI

- • ima知识库skills

- • Obsidian CLI

使用体验上是比default龙虾要好一大截的！
后续会给出更多Hermes Agent相关经验总结和分享。