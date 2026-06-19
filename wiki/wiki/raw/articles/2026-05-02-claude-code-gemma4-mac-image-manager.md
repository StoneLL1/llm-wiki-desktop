---
title: "舒服了！Claude Code + Gemma 4 一键理清Mac 30000+图片"
url: "https://mp.weixin.qq.com/s/Qf2GcVpAcAoAcXyU8kC6BA"
source: "微信公众号"
author: "字节笔记本"
account: "字节笔记本"
fetched: 2026-05-27
sha256: 26e3bea1438fe949
image_count: 7
---

五一假期得空，终于有时间来整理一下硬盘空间告急的Mac Pro了。

其中大头就是图片，散落在本地各处目录的有近30000+图片，光近期的截图文件就有2000多。

像截图这种东西，直接删吧，不知道哪张里面藏着一个还没处理的快递单、一张还没转账的账单、一个还没记录的密码。

不删吧，2000张图片一张张翻根本不现实，等你需要找某张的时候，大概率已经记不得自己截过它了。

总之，食之无味，弃之可惜。

我平时用的截图习惯是保存到本地，使用的工具是 Shottr。

这里顺便提一下，这个软件是我近两年用过最顺手的截图应用之一，标注、截图、自动文字提取一条龙，截完直接用，基本上不需要额外操作。

但即便用了很好的截图工具，截图的管理问题依然没有解决。

截图只负责存进来，不负责找出去。

怎么办？自己做一个！

用Claude Code大概花了 10 分钟，写了一个本地的图片管理应用。

其中最核心的是接了 Gemma 4 的多模态能力。

Gemma 4是谷歌今年发布的开源模型，是本地部署，是用来Vibe Coding 的首选。

推荐结合PI开始用，具体链接：[Token自由！目前本地Coding Agent的最佳拍档](<https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247515862&idx=1&sn=3a43ec50a2913a069801762b2da73bc4&scene=21#wechat_redirect>)

它的多模态能力也非常的强，对图片的理解能够多维度的识别出图片的具体内容、颜色构成、文字信息、场景类型，可以说是降维的打击了OCR这类领域。

而且本地搭配Ollama加载Gemma 4使用，完全零成本，而且图片分析全程不出本地网络，对于追求隐私保护的用户非常的实用。

应用跑起来之后，指定完目录就能一键分析处理。

把 2000 多张截图导入，让模型逐张分析，生成语义描述和标签。

这个过程需要一些时间，但不需要人盯着，等分析完成，就可以用自然语言检索了输入。

输入类似 ”AutoCaw界面”、“二维码”、”微信对话“、“账单“这样的自然语言，直接出结果，活脱脱的一个离线版本Google Photos。

![a8ca665d-344e-4e3a-bb78-5533beb9a0e6.png](https://mmbiz.qpic.cn/mmbiz_png/iacaCWlP1x1zTQebV8Tq3nWD15HgkExLzMSkcxoXQyib9enp8RRkbs8YHUxaRS0uk9ZO9f5ejgmfUIeU9M8uFgGs0WQ9JK9ce1g9dIjqUOjRE/640?wx_fmt=png)

更有用的是批量清理。

把识别出来的纯黑屏截图重复内容临时验证码一次性筛出来，确认之后批量删除。

2000 多张截图，有效清理掉将近一半。

对于识别结果不准确的图片，可以单独触发二次识别，重新生成描述。

除了以上的功能，AI图片管家还可以自动的生成标签和分类，通过这些标签快速的定位到具体的图片，再就是提供了包括颜色区分、日期等多维度的搜索模式。

这里不得不感慨一下，以前我们用软件，是因为没有别的选择，需要某个功能，只能去找有没有现成的应用，找不到就放弃。

现在这个逻辑在悄悄发生变化。

如果一个需求足够具体、足够个人化，直接让 AI 帮你写一个，10 分钟到半小时，能用的东西就出来了。

软件开始变得日抛。

不是说质量差，而是说门槛低了，低到想到了就能做，低到用完了可以扔掉。

这次的图片管理工具，我可能不一定会长期维护它，但它能完成一个特定具体的任务。

这就够了。

而这件事可以往下延伸的方向很多，比如批量处理 PDF 文档、给语音备忘录自动生成文字摘要、对一个文件夹里的视频做内容索引...

原来很多需要特定的、特殊的某项功能，都会去想有哪些软件可以实现的事情，现在的答案变成了自己写一个。

随着AI的越来越强大，软件开发这个工作真的已经变天了！

AI图片管家源码下载地址：

https://link.bytenote.net/note

screenshot-ai-manager文件，不仅仅是源码，里面还包含了之前发的[Claude Code 长程任务的记忆管理方案](<https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247516009&idx=1&sn=397712c1681433f6f60c1456e62fd9b5&payreadticket=HBqLMrGbGSM2emxowPAtnTGnbgVscNOpvZdG8q4VgkHog75jy8cxRjFGYnJ04pS3hnR54TQ&scene=21#wechat_redirect>)所生成的本地知识库管理文件，通过这些文件，可以有效的管控AI编程过程中的上下文，可做参照。

[把Claude Code塞进微信！实现车上厕上床上优雅Vibe Coding](<https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247516033&idx=1&sn=6b3acef0499d6d5f75836f9a30dff494&payreadticket=HLs8ZyH1idfbd9r20nyHgDl5kFZjb0GmB5zaY1HhyHuFPwu5_x_NfcrnAebEEUJEe7XKXls&scene=21#wechat_redirect>)  

[让人馋哭了Claude Design终于有了开源替代！](<https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247516020&idx=1&sn=14b79b4c147e45e713af46a08e889502&scene=21#wechat_redirect>)  

[Claude Code 长程任务的记忆管理方案](<https://mp.weixin.qq.com/s?__biz=MzIzMzQyMzUzNw==&mid=2247516009&idx=1&sn=397712c1681433f6f60c1456e62fd9b5&payreadticket=HCfNE6KTF2oDMXJkwQNcthapXK9mM0c8JVKL-WTquclt8xtbE0Nu-KncH6wEwxamn6qbCEk&scene=21#wechat_redirect>)
