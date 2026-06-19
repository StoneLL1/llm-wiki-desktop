---
title: "Hermes Agent的确比OpenClaw强，建个【Hermes Agent中文社区】飞书群一起来聊聊吧~"
url: "https://mp.weixin.qq.com/s/xKGLpJnlUH0RBEegQmFCsQ"
source: "微信公众号"
fetched: 2026-04-18
sha256: d0db85eea3a8c23e
---

今天用OpenClaw和Hermes Agent做了个实验，将我之前的EPUB电子书转视频播客项目（ https://epub2podcast.aigc.green/ ）完全skill化。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGgUJLdOFestp7uXNwn5bfB3tdQOZnhb4JWVGogvbd2OBSRz3uxzdHHEM4V0Lb4EhKVQia4YFSFySN0ewqOWOfjSgtBq0Kt4cfyE/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGjKX7FeyDc5OEZbkyQGgVwrfFGmuCahasibq4OichDUhbTBYPgwfgLQwFlmDbWzYC27Lr51PbSia7Ty8hnw81UV2LYeTRnJ5N93Rw/640?wx_fmt=png&from=appmsg)

OpenClaw？完全失败！
Hermes Agent？一次成功！

大模型用的都是GPT-5.4，既然目前已有基本共识 Agent=LLM+Harness，那得到的结论就是：
**
Hermes Agent的Harness的确比OpenClaw强（即便必须加个限定“在此项目上”）

Skill我放在Github仓库里了：
**
https://github.com/dracohu2025-cloud/draco-skills-collection/tree/main/epub2podcast

装了这个skill，你可以在飞书里给Hermes Agent上传一个EPUB电子书（也支持AZW3/MOBI/PDF，但EPUB兼容性最好），然后让它给你一次性生成一个10分钟以上的视频播客，并且上传到飞书云盘。 你只需要OpenRouter的API Key和火山引擎豆包TTS2.0的API Key。欢迎在Github上给我颗star，有bug也请留言~

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGjyQlibPQCOqrnMuVtE0MdpYCBmRxBwms6jcj2U8J80XffVsfAOdichTfyYd4GxERewKWSbAGiagEUNJiczwE090icYlpibC3xgIl1zI/640?wx_fmt=png&from=appmsg)

OpenClaw重度使用2个月，Hermes Agent深度体验4天，我目前的强烈感受是：
**
每天用OpenClaw 8个小时，4个小时在给它解各种bug；
而Hermes Agent是你可以放心将“后背“交给它的好战友，并且真的越用越聪明。

在Twitter上也看到越来越多和我有类似感受的用户：
**
只翻译第一条吧：

> 我曾经以为OpenClaw是无敌的

> 然而，我试过Hermes Agent之后

> 于是就... (配上了灭霸打响指的视频...请自行脑补）

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGgJj8X8qdIg5siayJWGjzwS1HEFTHmGaicpjIvCoziapwUNRZQFLc1gqVtSysDgzf5pWW9SXSGLl0sNiayty9Cy97icU7ibObJ7Hvse0/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGhKQhQYEEia00afFT4hkPx3mkV90eFdcboibgKeMVIicj7kesTnnQNvmenB8DNTcSPE0nLjM8J8JV6bYJf0LfAffWxbzGwH8E5S8E/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGia9VcpTRSGGBzYmnp7MVt9M3v8GlAabYVH47kp32TEpG8G7T9rm6aXT2VEcPwJh7CLLOu1FnlIJHoXgfib546Oe2Tsibtzs06ufQ/640?wx_fmt=png&from=appmsg)

![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGhEHUBsGsBfT3ic8fG8RgzQGtphiaHeSxlLB8g9M6k8R3H23fjvMnX3t9dN4ZkLPW3ynh3qJUrmXP7NRLr6FnGVPIJQfgJUSNVJU/640?wx_fmt=png&from=appmsg)
OpenClaw最早布道者Alex Finn 最近也在强烈推荐Hermes：

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGiabribHYhqHziaPZsTb9ZUc7dTl5aHf15icJMsmUHtTXTRujHlRelBFKCBYQOp8ffqIgl0d1ibiaskG5SzPlKI7EiaxpoVSeTHtSh9d8/640?wx_fmt=png&from=appmsg)

如果你是第一次看到Hermes Agent这个名字，我敢打赌，你在未来会越来越频繁地看到它。

以下，对Hermes Agent做一个简单的recap：

Hermes Agent项目在2月26日发布，之后就是star飞速上涨的一个月：
**Github repo在: https://github.com/NousResearch/hermes-agent

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGhOrTBOJAFJhQfvib9OKlUqib6MaMqlD1ibcXS0onoSicfUkjAzsmv7eQue2TuOKgYg1zLUIwd8wSODXX3nFjXZxMO3CHPq5zqicPvU/640?wx_fmt=png&from=appmsg)

目前，Hermes Agent在OpenRouter上token消耗量排名已上升到了第5位：
![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGjNUXkib7vQVjcpVVGdOl8WWOF0uZzRcUclrMBibnTwQHn2zmaDzjRNwia5tnLGOXfKowT4c7C3EXL3GMfIo49voiaD1ic3YmeMdiaMc/640?wx_fmt=png&from=appmsg)
**
以目前的速度，大概率一个月内能超过Cline排到第4吧！两三个月超Claude Code感觉也问题不大~

Hermes Agent背后的团队是NOUS Research，旗下有自己的HERMES大模型系列（目前到HERMES4）、有自己的训练框架PSYCHE、API PORTAL，以及今天的主角Hermes Agent；简单讲：综合实力强、Research导向的小而精团队。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGiajVTsvBWAnjCXIF9ylrc7lDn6EvvHvZyXlfXPP9ThaoFvjc8wQibBseOzP8iaeuhw4duBABrdaAk1s9cSY3oDAQK5NdDSeAibKLg/640?wx_fmt=png&from=appmsg)
关于Hermes Agent的定位，NOUS的cofounder Tecknium讲得很清楚：

![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGhbu64V2DRz1VxqYYewWN5uEibzEpwyXOFStXhQUq0icwucAxcAibbx4pxAyO2xVPsCmFZdnCIkpG6rVnQvZ1ia33q59Pd9s0iapgRU/640?wx_fmt=png&from=appmsg)
**
Hermes混合了Claude Code和OpenClaw（以前叫Clawdbot）的优点。

在使用过程中这种感受的确非常强烈：Hermes Agent既有打通各种常用聊天软件的强大Gateway（国内常用的飞书、企微、钉钉都支持），又有类似Claude Code一口气run几个小时任务的Harness强度（比如我今天做的工具网站的skill化改造任务）

其中最突出的优点是：Hermes Agent内置了skill_manage系统，在使用过程中会不断创建、优化、重构自己的skills；这是Hermes的预置天赋树，让人感知到“越用越聪明”的内功心法！
**
并且，这是截止此刻OpenClaw尚无法提供的体验。

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGjpZKb1K3ChP6LibbBguuhCLzJrlCwNYc2R86ic9ibQtqicicUhB3teyyAmhLUm9iaKNU36VmSq5jTaKosYkia5uzZgicQchicb1ponZeSM/640?wx_fmt=png&from=appmsg)
此外，Hermes Agent直接把市面上最好的记忆框架都做成可选项了：Honcho、OpenViking、Mem0、Hindsight、Holographic、RetainDB、ByteRover...

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGiaicyU8ZuOfAGBIDW2awESvGrr7Ou2ic5Qiaorz7uF6EsDzBFWJGIpe7wSUTh8sZJ88sZf3MoSOEicRVWI1QNtNtw031Ctt0wFRO34/640?wx_fmt=png&from=appmsg)
Hermes Agent有自己的官方文档站： https://hermes-agent.nousresearch.com/docs/

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGgTD7t0EuaqhrbxMYCYaqsJrGbPa8pdX2QlOic9ibx84ga8KtIMcmnysVH0Xm80h9kzciakCBvBicWWcTpEJSCVyDfAE7WpjpY7C1k/640?wx_fmt=png&from=appmsg)
我复刻了一个文档汉化站： https://hermes-doc.aigc.green/

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/0m9F5vC1OGj8rPf2DC24FXF1BCibicNLHicNDoT2L9QBccWC8qLbauqibfkfRkuxm7ozc7BLiaC8ib30t4aNpyhBh0WhnJWkXggT0QwibYicC2OCoqY/640?wx_fmt=png&from=appmsg)
**
有想尝试Hermes Agent的同学，可以直接按照文档中的方式进行尝试

此外，我还创建了一个飞书群，欢迎想一起探索Hermes Agent的同学加入讨论~

![image](https://mmbiz.qpic.cn/mmbiz_png/0m9F5vC1OGg8POGwkfMr3suruVmDibcG63OKibOQ4AHg3RpjMO5ytqbClCJUnRgD8b4XTDTBibbSibyt8gbwesaBrgMEBthCp6SDib73ekH6Wmqg/640?wx_fmt=png&from=appmsg)