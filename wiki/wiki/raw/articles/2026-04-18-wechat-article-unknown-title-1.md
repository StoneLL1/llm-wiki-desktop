---
title: "做了3个 Skills，打通公众号自动排版发布流程。"
url: "https://mp.weixin.qq.com/s/wGwdxJUf6ps1A-_B-07vWA"
source: "微信公众号"
fetched: 2026-04-18
sha256: abf52b5523a01619
---

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzS6bt7JecFHOIjxoIrs74nFEguzLblIs92iapUbNhbjlZRWpib9Vib1sHmEsq8ZjhUs8Btoxw2CFqolib14UM3Z3x5lTUK4g8TnMN4/640?wx_fmt=png&from=appmsg)

这两天我折腾了一个挺有意思的东西，现在我可以解决之前觉得很烦的事。

把公众号选样式和排版并推送这一步，彻底自动化掉。

具体来说就是，你看到一篇排版不错的公众号文章，把链接丢进去，它可以自动帮你把这篇文章的结构和样式提取出来，把排版能力本身抽象出来，让模型去学、去复用。
*
同时现在写内容已经不是问题了，不管是 GPT、Claude 还是各种 AI 工具，生成一篇结构完整的 Markdown 基本就是几分钟的事情，但真正耗时间的还是排版。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzRaZwGdsUIXcrxluAN1scYBvosJ0dSHICpexibZxEjQquSeMvBUth8xcU35Jn7icCiadnyFWrwMqVVSet0FFSVM501J3Xguq996QM/640?wx_fmt=png&from=appmsg)

在这个工作流里面，我把自己的文章丢进去，它会按这个样式帮你重新排一版。

![image](https://mmbiz.qpic.cn/mmbiz_png/thoHNWXYDzRibqn2bIohicV8icTsMdRu6QoGdACd7veOiauNB7CBQfNcJVd3dZGuSI3icZ9LaOiaGRxRp5jTNwCxTzDclOhZm9gic6dmficngoEKnW0/640?wx_fmt=png&from=appmsg)

然后在样式上你觉得OK，可以直接一键推送到公众号草稿箱。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzSic2d2pHcYp2vqy7tBPTORtGPQ68cDtCECicrLOATM2K33jvxlOWDvBbAesxcqXbBianFpfRZQYm5o6SuxmFqa3yw5iaCib3ibWlgic8/640?wx_fmt=png&from=appmsg)

这一整套流程我把它做成了一套小工具，顺便还帮它沉淀成了3个可以被复用的 skills，都是我跟 Claude Code 结伴编程完成的。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzTutHCKxTibYHcCwOBRsMFpl7lB13XIJh0LnYHibLaJLic6saewJwDLk2K0X3Lj8M4EFHGFEh3V0iaXEaxqB5XOS4eMeEFRI0yibWgw/640?wx_fmt=png&from=appmsg)

所以在我的整个 AI agent 框架里面，比如 OpenClaw 这种环境里，都可以直接复用这套能力。

整个项目含3个 Skills 都已经开源，在文末可以获取项目开源地址，下面主要给大家先分享一下整个实现思路，便于大家了解整个项目的实现。

这里顺带说一下我这次接模型的方式，因为这也是我这次顺手一起做掉的一块。

我用的是 Claude Code 这个 IDE，它本身支持自定义模型接入，所以我直接接到了七牛云的大模型 API 服务平台上。

![image](https://mmbiz.qpic.cn/mmbiz_png/thoHNWXYDzSPM1Te5QVN3nSj6bwaUz7FmZw7icRW3judF1FN8YnVcCODgCaGAPActW0aRluxic7xjnXicy5PRruTfExsoVGAgKRwiat2llESBb0/640?wx_fmt=png&from=appmsg)

模型服务接入地址：https://s.qiniu.com/26fa2a

我之前也有推荐过这个平台，里面有不少模型可以选，除了文本模型以外，还有图像模型、视频模型。

新注册用户就可以直接免费领取 1000 万资源包额度，首次体验还可以再领 300 万，直接薅起来，是所有模型都可以通用使用的，不同的模型倍率系数会有区别。
*
另外如果咱们想要接入一些海外的模型，比如 Gemini、Claude、GPT 等模型，也可以到七牛云的海外站点进行接入。

![image](https://mmbiz.qpic.cn/mmbiz_png/thoHNWXYDzRORRIhSUOsGzDfhAdUDM5kPq6m7JsAeiaibxgBHKvLqUE236o2EsONvSaQyplBBNiaTT2BfbEibeond5iboxPPVmFTDeLzeicXuVBTY/640?wx_fmt=png&from=appmsg)

海外模型接入地址：https://sufy.com/zh-CN/services/ai-inference/models

我这边正好之前还有一些还额度没用完，就直接拿来跑今天这一整套流程了。

接入方式其实很简单，本质就是把 base_url 和 API Key  替换掉，然后按照它的接口去请求就行，整体没有什么额外成本。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzQia9NwNMOEQx3wJ5p40ib3LriaPDWs3iazkr0CNiaW9DTvU9HibBI1Xic4EUoFLibEJasLR1EFdpxBfsSFrIoYvdoXugkNExmdgG4SuaM/640?wx_fmt=png&from=appmsg)

具体配置文档可以参考这个：https://s.qiniu.com/AfyAry

我这套流程跑下来，用的是 GLM-5 这个模型，大概只消耗了两百万 token，整体性价比我觉得是可以接受的。

而且你后面如果想扩展，比如做图片生成、封面生成，甚至视频内容，其实都可以在同一个平台上继续往上叠，这一点对我来说还是挺方便的。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzQrgm7zExkC7xNl65T2yauo6AfqqrKMic1BdphPVKZoqXw9gZ3946Au7NB2g88f0kahGMiadd43cibMD5eKRxO3wXIcDMxIHC8Nuw/640?wx_fmt=png&from=appmsg)

整个流程我拆成了三个步骤，第一步是做一个链接解析能力，我先搞了一个简单的网页入口，可以直接输入公众号文章链接，然后把整篇文章的结构信息提取出来。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzRoCf2YeiaxHlSuhZwC6ibr1yK1XNdjcI2AV4zNBDSF1qF7MWjy0gXRTuxUbbHdFrsGVJEnMsZQIe3H0jC64KvBB3CbzcactHoics/640?wx_fmt=png&from=appmsg)

把它的排版结构拆解出来，比如段落类型、图片位置、引用区块这些信息。

第二步是排版重构，当我有了一个样式结构之后，我会把自己的文章丢进去，让模型根据这个结构去做重新排版。

本质上是一个结构化样式的嵌套映射问题。

第三步就是发布，我这里做了两个动作，一个是支持一键复制到公众号编辑器，另一个是直接走公众号的开发者接口，把内容推送到草稿箱里。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzSVIrq3ticum33TQknpauicFc3ougcXbr8ObtuJzyOM1pCIEnqt1ztq5G57NxDBrBXRlr9mvGwNOXTIdCdhCIXXkcWXfE93AiaUPo/640?wx_fmt=png&from=appmsg)

公众号这块本身是开放 API 的，我们在官方平台拿到 AppID 和 AppSecret 之后就可以直接调用，这一段其实不复杂，更多是把内容格式处理好。

平台地址：https://developers.weixin.qq.com/console

把上面这三步能力打通之后，我没有停在这里，而是让模型帮我把这套流程重新抽象了一遍，沉淀成三个核心 skill。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzSbMbdmCiajHrHMak0aGPjPT9pbyicibzrzpCq19FX96zENFCWic6UK6gq5YAswsnq2kmqEwpJyMeiaVoToYicU4jjckckH0vfrROZcw/640?wx_fmt=png&from=appmsg)

一个是输入公众号文章链接提取样式，一个是根据样式去优化排版你的文章，第三个是把文章一键推送到公众号草稿箱。

这一步我觉得还挺关键的，一旦变成 skill，这个能力就不再依赖某一个项目，是可以被复用到任何 agent 体系里。

比如你用 OpenClaw，或者你自己搭的小龙虾，只要把这几个 skill 接进去，你写完一篇文章之后，可以直接让它自动完成排版和发布。

这个链路就变得非常顺了，比如我自己直接把这个Skill安装到了微信中的 OpenClaw，然后直接对话就非常简单的完成了内容创作与发布并自动推送到草稿箱。

![image](https://mmbiz.qpic.cn/mmbiz_png/thoHNWXYDzTSmP7ROTCCygfURqLvLTTrUib6TdSIgZO7WTa4n0T0JAWt9PV0wGCGpfia3S5YJKx45A6JK74Y0C4WRZThjia7GKXDqI2snRbghw/640?wx_fmt=png&from=appmsg)

公众号的鉴权完成之后，可以直接的把写好的文章推送到你的草稿箱里面，咱们直接到微信公众号助手的草稿箱里面确认就好。

![image](https://mmbiz.qpic.cn/mmbiz_png/thoHNWXYDzRwDkOv6bRcibGkYmWJAUH42Ysia0ibWdRfHRgfbgB7epxCFb1Kb690oZOhhT3frF5SjFkz3dicZg4ibEW4KbcrJlW4nRmc0PRA3CyM/640?wx_fmt=png&from=appmsg)

当然如果你现在还没有跑 OpenClaw 这套环境，其实也不用自己从零折腾，现在同样在七牛云那边有一个 9.9元 就能一键部署方案，可以直接把整个镜像环境帮你拉起来。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/thoHNWXYDzQmficVcWqNGH0urwWDUCpGHwxIpkTKMUhKicd1DbjicgYeBzVlzHOWYcnBRypsia534VLpa3RicWicdqiaKKqTkN1Rvj3shcrrY7qe0s/640?wx_fmt=png&from=appmsg)

抢购地址：https://s.qiniu.com/UNBN7r

具体的配置流程细节就不在这展开细讲，大家有兴趣的可以参考这个配置文章：

[OpenClaw：GitHub 史上第一的项目，66 块钱就能养一年龙虾](https://mp.weixin.qq.com/s?__biz=MjM5NzAwNDI4Mg==&mid=2652212181&idx=1&sn=e90d4b5f2a34a984b5645ec22372c403&scene=21#wechat_redirect)

然后咱们把 OpenClaw 接入到微信中，目前也是非常方便的一个流程了，路径是进入「我-设置-插件」，可以查看终端安装指令，直接贴给小龙虾对话框中即可。

![image](https://mmbiz.qpic.cn/sz_mmbiz_jpg/thoHNWXYDzTaMSFQXfC1T4UIbc4eyDmia7FOOKibGl4mUT0yW0Ex6HfesY5LhgIwibFib77dgIeLwD7IprrbvarFu4tssNUcc4jyicDJSyRJDY5E/640?wx_fmt=jpeg&from=appmsg)

有了这个环境，再加上刚刚这几个 skill，其实咱们已经可以把公众号这条内容链路跑通了，写作这个能力小龙虾天生就具备，从写到排版到发布，基本都是自动完成。

让 OpenClaw 写完文章之后，丢一个参考样式链接进去，让它自动提取样式，然后把我的文章做一次重排，最后直接推到公众号草稿箱。

我只需要在后台打开看一眼，确认没问题，点发布就结束了。

完整网页项目开源地址：https://github.com/inhai-wiki/wechat-typesetting

3个 Skill 开源地址：https://github.com/inhai-wiki/wechat-article-skills

项目开源出来啦，大家可以直接去下载来跑，如果有问题也后面我还会继续迭代，大家也可以去薅一些模型 API，自己再迭代优化都不错。

如果你在用的过程中有什么新的需求，或者有更好的想法，也欢迎在评论区给我留言。

© THE END