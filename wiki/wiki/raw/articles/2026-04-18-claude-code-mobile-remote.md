---
title: "最好的Claude Code手机遥控器"
url: "https://mp.weixin.qq.com/s/Jtbhcdgpl3pKX7Opq3PUzg"
source: "微信公众号"
fetched: 2026-04-18
sha256: 2744f8e1ea93c3cf
---

我有位朋友 Librae，投资人，也是为数不多我线下面基过多次的公众号粉丝。

我们从去年还是前年开始，就一直在聊 AI，聊 Coding，聊创业，他自己投的项目中，比较好记的一个是玉伯的 YouMind。不过和大多数投资人不一样的是，这哥们儿自己也写代码，而且是真写，不是那种「我懂技术」的懂。

投资人嘛，大家也都见过不少了。有些人开口全是「范式转移」「底层逻辑」「第一性原理」，生怕你能听懂。不论好坏，但 Librae 不属于这种风格，他属于特别务实的类型，聊天从来不绕弯子，说的都是实在话。投资上也一样，不会撒网式地投一堆，更喜欢先和人交朋友，看清楚了再出手，做的是长线。

而今天要介绍的，是他搞的一个叫做 **Nexus4CC**的开源项目，解决的问题是：**
**

**在手机上操控电脑里的 Claude Code。**


![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFnH5aOHQHIzcgzkCub5zjay60oibmNoibPiaAg6OKicQmCnrsibFGGibqB81icTAjibOo71qBDCPq9KXaKDKp0rENBtPCxvMibrOzua2vRc/640?wx_fmt=jpeg)
你先别急，我知道你想问什么，这个项目还真有些不一样。

我先介绍一下：为什么他要做这个项目呢？

他的日常是这样的：写代码、管公司、看项目、做投资决策，四条线交织。而灵感这东西，从来不挑时间和地点，通勤路上、会议间隙、甚至半夜躺床上，脑子里突然蹦出个想法，想验证一下……


![碎片时间场景：地铁、咖啡厅、机场、深夜，随时连上终端](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlDD9ibaZmPxCb5zbibibNIudxia4Pex2nXzfXHyp5Qu2szzmPMUZmDAk66tFoeS0Q0uXogciaFrl0mvKYJY4ibrF9sHpyoATgjqEo7s/640?from=appmsg)
碎片时间场景：地铁、咖啡厅、机场、深夜，随时连上终端
想打开笔记本？不在身边。

用远程桌面？TeamViewer、向日葵这些工具（我也在用），连上去倒是像一台真电脑，但在手机上操作简直是灾难，小屏幕上点来点去，光是找到终端窗口这件事……可能就要半天。

用 termius？倒是也行，但不那么好用和专用：


![](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFlfHVm38WSUI6dYziaOxpQYHePrWd1icwG1RsFWWwRCvJpiahqogP7C9ZkF0aKfc9pLibJ3dHBXEVXd43zOwXWHofYvmmic3ZznV14E/640?wx_fmt=png&from=appmsg)
他一琢磨：其实大部分时候我也不需要看整个桌面啊，**终端就够了**。

于是，在某个未知的周末，他动手了。

01
## 四周 200+ commit

Nexus4CC 从 3 月中旬启动，到 4 月 10 日基本完成 v1.0，一共 **247 个 commit**。

其中 **225 个 commit 是和 Claude 一起写的**，AI 辅助率高达 91%。

整个项目大约 9,271 行代码，3 个贡献者（他自己 + Claude + 1 位外部贡献者）。这些数据是我让 Claude Code 分析项目仓库后得出的结论。

而值得一提的是，**Nexus 本身，就是在 Nexus 中开发出来的**。

他在地铁上用手机让 Claude 实现新功能，在咖啡厅里调试代码，在会议间隙修 Bug。这个工具在解决自己问题的同时，也在证明自己的价值，也是够努力了……

02
## 原理

项目的核心架构可以用一句话概括：**
**

**把电脑上的 tmux 终端会话，通过 WebSocket 实时投射到手机浏览器。**


![四层桥接架构：手机浏览器到 tmux 会话的数据流](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFkkKC2gxSuhNHCLD6Bjf53WJGybXZtnuJgtVE5uIKfATnKnx4VsPgsoP0nUHNZSXJbloqEFpnnKLLT1QhbotEmsnfIDd45ckQs/640?from=appmsg)
四层桥接架构：手机浏览器到 tmux 会话的数据流
数据流是这样的：


```
●●●

手机浏览器 (xterm.js)
    ↕  WebSocket 双向通信
Node.js 服务端
    ↕  stdin / stdout
node-pty (伪终端)
    ↕  tmux attach-session
电脑上的 tmux 会话
└


```
tmux 管理所有终端会话，node-pty 负责生成伪终端进程并附着到 tmux 窗口，WebSocket 做实时双向通信，xterm.js 在浏览器里渲染终端界面。


![](https://mmbiz.qpic.cn/mmbiz_jpg/ZKqVLiaIpzFmYtz7rQHKgwgiaUx9icYicibT5ic9r6Knq0TnnjMQCsLg1S5O1CInd8O0v8OcmR1nzbbWLcbezFXbvllfKAdNaia0cOkibnqSoUljLB0/640?wx_fmt=jpeg&from=appmsg)
关键点在于：**手机和电脑看到的是同一个 tmux 会话。**


![](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFlAnUgOOGNzVjlibgBAhtITPuETRgMaMT8tMOl9eXzP6rJZMuUVeQQXQYwJHXCsibMibHDtLfdzWpiaPCcypibChjQqCm4Qx18zas4c/640?wx_fmt=jpeg&from=appmsg)
你在电脑上用 Claude Code 启动了一个长任务，然后拿起手机去沙发上躺着，手机上实时看到 AI 的输出，需要时随手打几个字确认，任务全程不中断。

两端同时连接也不会互相踢掉，本质上你的手机就是电脑终端的一块可交互的「副屏」。

03
## 碎片时间杀手

这个工具最大的价值在于，它**把碎片时间变成了无处不在、随时随地的生产力**。

以前出差的时候，脑子里冒出个想法，只能先记到备忘录里，等回到电脑前再说。现在呢，掏出手机，三秒连上家里电脑的 Claude Code，想法直接变成代码。

而且最关键的是「发射后不管」这个能力：


![发射后不管：下指令、AI 持续工作、随时查看进度](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFmjGJn7RbCAup9n7r5rwxwgOKYaA7Aj9aemykAd1PYTD9UpLSV7Hql3AvSUhsiauX3EMbSqC9S2gvRq0aLocUoArJstWxxJMumk/640?from=appmsg)
发射后不管：下指令、AI 持续工作、随时查看进度
下达指令，锁上手机，该干嘛干嘛。AI 在后台持续工作。你随时回来查看进度，就像查看外卖配送状态一样简单。

手机要没电了？没关系，只要断电前发出去，AI 就一直跑。

网络中断了？恢复时它会自动重连，自动恢复。（这个功能体验非常好，不像各类 ssh 的糟糕体验）

浏览器崩了？刷新页面就好了。

因为任务跑在 tmux 里，它完全不依赖你的手机连接。这种安全感，用过的人应该都懂。

04
## 手机上打字的痛苦

这应该是大家最关心的问题了。

普通的终端模拟器在手机上体验实在太糟糕了，密密麻麻的文字，想点个位置得放大再放大，输入命令一个字母一个字母地敲……

但 Nexus4CC 从第一天起就是为移动端设计的：

•  左右滑动切换 tmux 窗口，像刷短视频一样流畅

•  双指缩放调整字体大小

•  底部有一排可配置的快捷键工具栏，Tab、Ctrl+C、Esc 这些高频键一键触达

•  文件浏览器可以直接点目录树，拖拽上传文件

而且因为本质上是浏览器的文本输入框，中文输入法这些 IME 组合输入也都没问题。

其实换个思路想，手机上打字确实不方便，但语音输入完全 OK 啊。

Typeless、豆包、微信输入法或者系统自带的语音识别，随便哪个都可，非常方便。


![](https://mmbiz.qpic.cn/sz_mmbiz_png/ZKqVLiaIpzFkt5iba42EB1IRFlsdg3pP9nHhD9Uhm1frHF91D0icD1m7nIGBwl0OOjlJkHVgKicC8JJCh3oCDutgnjJmicxMQKIqQXdEuE3Q1U7g/640?wx_fmt=png&from=appmsg)
05
## 远程桌面

可能有人会问：我用 TeamViewer 或者向日葵不也一样吗？

确实，远程桌面更是一台「真正的电脑」，什么文件都能看，什么软件都能操作。但，在手机那块小屏幕上操作完整桌面，体验确实还是不太方便的。


![远程桌面 vs Nexus4CC：什么都能做 vs 只做终端做到极致](https://mmbiz.qpic.cn/mmbiz_png/ZKqVLiaIpzFltz87VONOUTg7IwTkoj0DoAyShexhUQuhgvodtzLCxMKcT5tDicgEiaUvmnicFqWaMPfctPn001MexwFUerporp2exiaLicibWOicXFk/640?from=appmsg)
远程桌面 vs Nexus4CC：什么都能做 vs 只做终端做到极致
鼠标定位靠「放大，再放大」，打字靠虚拟键盘一个个戳，延迟也不算低。

而 Nexus4CC 的思路反过来了：**大部分时候，终端就够用了。**

写代码、跑脚本、看日志、让 AI 干活……这些事情在终端里效率反而更高。放弃「什么都能做」，换来「在手机上真的好用」，这个取舍其实挺聪明的。

尤其对于 AI Coding 场景，你本来就是在终端里跟 Claude Code 对话，根本不需要看桌面。

06
## 后续计划

当然，毕竟这只开发了四周的小项目，Nexus4CC 目前还有些地方在继续完善。

目前不支持原生手机推送通知（没有 APNs / FCM），任务完成后只能靠浏览器通知或者 Telegram Bot 来提醒。在 iOS 上得先把网页添加到主屏幕（PWA 模式）才能收到浏览器通知。

或者，在电脑上使用 hooks 通知也能临时够用，我用的虾推啥：[让 Claude Code 在完事后，给你发条微信提醒](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453477094&idx=1&sn=18dd6c85f7b79861a8ffad234e0ba627&scene=21#wechat_redirect)。

每个 Nexus 实例也只能控制一台电脑，还不支持多机器管理。

此外，还有个多人协作功能还处于讨论阶段，比如把调试任务转交给同事、让别人直接连你的终端实时结对编程。这些涉及权限控制和安全审计，需要从单用户架构升级到多用户体系，  Librae 和我讨论过，但目前还没落地，我之前也有写过类似的小工具：[我做了个 Claude Code 对话共享工具，可以让别人半路接手你的工作。已开源](https://mp.weixin.qq.com/s?__biz=MzA4NzgzMjA4MQ==&mid=2453481579&idx=1&sn=ede0512ec9c0d0a36092650f24f57e58&scene=21#wechat_redirect)。


![cc-go-on 导出导入流程](https://mmbiz.qpic.cn/sz_mmbiz_jpg/ZKqVLiaIpzFkNibtA3mEGdvoJr4UKFC4uujkdE2IjpFcRsp4VugWdbP3hiaxkCV8VmkiaLkwlQ1Ykn7cDiaMmRq3ZxphJDfiatKXPRq8Byg3mUN4s/640?wx_fmt=jpeg&watermark=1&tp=webp&wxfrom=5&wx_lazy=1#imgIndex=2)
原生 App 和多设备支持都在项目的 Roadmap 里，后续版本会陆续补上。

07
## 如何安装

部署很简单，但你需要提前准备好如下资源：

•  一台电脑（Linux / macOS / WSL2 都行），装好 Node.js 20+ 和 tmux

•  如果只在家里局域网用，手机和电脑同一网络下直接内网 IP 访问就行

•  如果想在外面也能用，需要做内网穿透（Cloudflare Tunnel、Tailscale、frp 都可以），或者有公网 IP + 域名

五分钟部署流程：


```
●●●

# 克隆项目
git clone https://github.com/librae8226/nexus4cc.git
cd nexus4cc

# 配置环境变量
cp.env.example .env
# 编辑 .env，设置 JWT_SECRET、密码哈希、工作目录

# 安装依赖并构建
npm install
cd frontend && npm install && npm run build && cd ..

# 启动
npm start

# 在任意设备打开 http://你的服务器IP:59000
└


```
当然，如果你懒得自己折腾环境配置，还有个更省事的办法（往下看）。

我自己也 Fork 了一个版本在做一些个人需求的开发。随手实现想法比什么都重要，**想到了就去做**，不需要也不应该有任何耽搁。

如果你也是那种经常出差、到处走动、想充分利用碎片时间的人，这个工具应该非常地适合你。

08
## 让 Agent 替你装

 Librae 说，他自己现在已经完全离不开这个工具了。

如果你也想试试，最简单的方式：

**把下面这段话，连同 GitHub 地址，直接扔给你的 Coding Agent（Claude Code、Codex、Droid 都行）：**


“ 参考这个 GitHub 仓库 https://github.com/librae8226/nexus4cc 帮我完成相关的安装和部署。需要我的时候告诉我，但尽量不要找我。


然后……就没你什么事了。让 Agent 自己去搞定吧。

◇ ◆ ◇

Librae 日常也会在公众号上发布 Nexus4CC 的最新更新和使用心得。有兴趣的可以关注一下他：

如果有创业或投资相关的话题想聊，也欢迎加微信（librae8226）深入交流，一起探讨 AI、创业和投资。

当然了，Star、Issue、PR，都欢迎。

项目地址：https://github.com/librae8226/nexus4cc

技术栈：Node.js + React + WebSocket + tmux + xterm.js

