---
title: "AI + Vercel 一键部署网站实战"
url: "https://mp.weixin.qq.com/s/ITgHVne1Rc8f0nqXQs-vIg"
source: "微信公众号"
author: "程序员鱼皮"
account: "程序员鱼皮"
pub_date: 2026-05-26
fetched: 2026-05-27
image_count: 34
sha256: a098c386921ff5c2
---

大家好，我是程序员鱼皮，今天分享一个提升 AI 编程效率的神技巧。

以前，我们开发一个网站，从写代码到部署上线，每个环节都得自己动手。

但是现在 AI 写代码已经溜飞边子了，在我心里已经取代了古法编程，谁能想到这是短短 1 年就发生的巨变。

而且 AI 的能力不止于此，利用 Skills 技能或 MCP 扩展，AI 甚至可以直接帮我们把网站部署上线！

好好好，合着我 **只要提个一句话需求，写代码和部署都不用我来干了** ，让我退化是吧？

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1GSUMnV1ykJuQBJKqibDEiaAJ8eIb0hd3J1xhopurWUAtibMibrGg5JRJEFtcSAM69Tkt0HRBqyzmQ1GwWH5ibKJdLXRQFEGQsEYJdg/640?wx_fmt=png&from=appmsg)

这篇文章我就来手把手演示一下，怎么让 AI 帮你一键部署网站。全程实操，看完就能上手，建议收藏~

## 有哪些免费部署平台？

目前能免费部署网站的平台还挺多的。

国内的话，推荐 EdgeOne Pages，这是腾讯云的全球 CDN 加速 + 网站托管平台，国内访问速度很快，目前提供了免费额度，个人项目基本够用。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GDRO4bAQee6x4RMZhtfdASNtX4KMF9OmChDOgulffYtiaSenN4FnxHXXbHicEre0XqHC0iaSK2yNicVibmNLhN2V7fl5JdJkfGvdsA/640?wx_fmt=png&from=appmsg)

国外主流的平台有 3 个，都是响当当的产品。

1）Vercel 是 Next.js 框架官方推荐的部署平台，海外生态最成熟，几乎是前端开发者的标配。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1H10CN6siafib2tRaplVmareib38D8qjkzmvvkFJibvaOkibIRvz0Adj0fX0ZVSUGoVYDRO1CW7CFXGLsRkmchBJ9kQfWnKkbamQ4CM/640?wx_fmt=png&from=appmsg)

2）Netlify 是一个老牌的前端托管平台，内置了表单处理、身份验证等开箱即用的功能，免费额度也挺大方。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GMcTQKPjgA3y2EekvvXTUjnhDR6Q8RgDOWw69LmNs6HCLc6hylNDT2tJjdWIlRZXEIZo3pkzzAYVELrxZwEBIXreBN5uVsianU/640?wx_fmt=png&from=appmsg)

3）Cloudflare Pages 背靠全球最大的 CDN 厂商 Cloudflare，全球访问速度快，而且免费额度夯爆了，每天 10 万次请求都不收费！

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1HxUMibLBIgOkvnWEIyoVow0YTJKic57hbTzs8ibzaoYSPMX43MibMUXN5DcK5PdKOK39rNsKKv5cV3nfI4juMOs5UOK2YqRAHNtqU/640?wx_fmt=png&from=appmsg)

这些平台都支持从 GitHub 仓库导入项目并一键部署，也都提供了 Skills 或 MCP 来让 AI 编程工具自动完成部署操作。而且不只能部署纯静态页面，Next.js、Nuxt、Astro 这些全栈框架的网站也能直接丢上去，支持 SSR 服务端渲染。

下面我以 Vercel 为例，带大家完整走一遍 AI 自动部署的流程。

假设我已经用 AI 开发好了一个「编程宝典」文档网站，基于 VuePress 构建，代码也推到了 GitHub 上开源。这是一个精简版的模板项目，大家可以直接 fork 拿来练手。

> 开源指路：https://github.com/liyupi/codefather （template 分支）

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FXUjODFiaJthxBwqnJlz7b6jrEOaTPUVxhFjuldbadgjibzv8xTuGMmba1q7klerKKJYpMrV9jFm8zG0JhYibQ3Z7dm4cQEM0A0I/640?wx_fmt=png&from=appmsg)

## AI + Vercel 自动部署网站

### 什么是 Vercel？

先简单介绍一下 Vercel。它是目前海外最主流的网站部署平台，你把代码丢给它，它能帮你构建、托管、CDN 加速，还自动配好 HTTPS，完全不需要自己搭建服务器。而且跟 GitHub 深度集成，之后每次推送代码都会自动重新部署，非常省心。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1EV3BxBE261lSX7EYB6DfyLK8X97mdN6DIJlibrkpGShkUZCK1MAo9m9uwDHibAS4jgKqgxM3iaUN0QViaWX7zzw7ysAQdBSTQYwm0/640?wx_fmt=png&from=appmsg)

为了让 AI 编程工具能帮我们操作 Vercel，Vercel 官方提供了 3 样东西：

1）Vercel CLI 命令行工具：在终端里执行 `vercel` 命令就能完成部署，可以让 AI 帮你调用。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1HuEIaBRdclLmv96viaMup52wGicrSrlazFHa3LuvR72efepRkJA9XuJ78ib2UyRCKRQmKdxAqZbIicRiaaOBUR5PTesIl7vfUWJGiaM/640?wx_fmt=png&from=appmsg)

2）Skills 技能包：Skill 是一种标准化的指令文件，相当于给 AI 看了一遍 Vercel 的部署教程。装了之后 AI 就知道完整的操作流程和最佳实践，遇到问题也知道怎么处理。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FOyOhjn7OTpVnFLgVTVSu6SshbOxFqubjDlRsve0JXIQPKvx5IhmsFib8xwqwaLoeNu9VQ2KAAEDq3efQ4UglWjXzblvuJbaOY/640?wx_fmt=png&from=appmsg)

3）MCP 模型上下文协议：让 AI 直接调用 Vercel 的 API 来管理项目，比如创建部署、配置环境变量等。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1E8rFugGW9X6jyugRSpIS6PkC6oVECGusA7arumPNPrXS7cWGZOxI66DPnk9gFFYnL73opbOF5HDH1MZwG55FOODzu0C4FQRdI/640?wx_fmt=png&from=appmsg)

我们这里主要用 **CLI + Skills** 的组合来演示。相比 MCP，Skills 不需要额外配置服务端，而且一次安装后，各种 AI 编程工具都能直接用。

### 快速实战

#### 1、注册 Vercel 账号

首先到 Vercel 官网 注册账号，建议直接用 GitHub 账号登录，这样后面导入项目会很方便：

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1ErhKeoFXbZP0msPKJzs1ax3S5GIHibtDlbBskiaU7kI8sfSFia9m6MhvUMibMyzibXFvAqIsrH4lOhUtjicGRtN8E6Qfsicq1xEwodU8/640?wx_fmt=png&from=appmsg)

#### 2、安装 Vercel CLI

Vercel 提供了命令行工具，可以让我们通过终端完成部署。

先打开终端，用 npm 全局安装：
```
    npm i -g vercel@latest  
    
```

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1ELhrNCvTFQyy2J07eQDCLkhlOCOyA2ACeTbP5dCAQIS7qe50qSARlvcOwUPsah6MRrVMbq6uolu0M7enWYKJhRh0ialmBVUN90/640?wx_fmt=png&from=appmsg)

安装完之后，在终端执行 `vercel login`，并且在弹出的网页中完成登录认证：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HadmN90LfmiarAS5wSwdHDVt7apicCicF0TSIomRH1Y6Dzz3M38hBgIgaYw5LynBZxGcOOFjUUQE7u9W1hCvBLekIOuKD44SZK1I/640?wx_fmt=png&from=appmsg)

工具很贴心地问我要不要安装给 Claude Code 用的 Vercel 插件，这里我就先拒绝了，还是手动给大家演示一遍自主安装的全流程。

#### 3、安装 Vercel Skills

接下来给 AI 编程工具装上 Vercel 的部署技能 `deploy-to-vercel`。装了之后，AI 在帮你部署的时候就能自动检测项目状态、选择最佳的部署方式、生成预览链接。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1Gr5yGIjhiafvJKKT1LFe2tc1PqSoqf8UB4yqzN5YqJcu4FHoHaNsia2qe3RdFC0MJehrF8ZzQXB6dKSGkulPtEFUwLKicH4ibaL5c/640?wx_fmt=png&from=appmsg)

打开终端，输入下列命令，可以安装 Vercel 团队提供的多个技能：
```
    npx skills add vercel-labs/agent-skills  
    
```

你可以自己选择要安装哪些技能，这里我就单选一个部署技能：

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1Gtk9nqcpeQCvVfv0xiaEBMl2HWicxwSTrxJEGJ9bkuQVia3kzqibdkZwKooB7ibX80QGADPaROB3r5JqQict3IC5RngWjia6fTk71yGA/640?wx_fmt=png&from=appmsg)

技能默认会被安装到大多数 AI 编程工具都支持的 `.agents/skills` 目录下，你也可以安装在特定 AI 编程工具的技能目录下，比如安装到 Claude Code 支持的 `.claude/skills` 目录：

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GOGP7UlKKia1EYNkXuqcsUWRMuZAZfBhZkVQIkaGElVe3CNz3ZyIfscXjibAyOAwfPkdhib5pda7N1nQUULl79ZKhA3DNnbabOAk/640?wx_fmt=png&from=appmsg)

最后，选择「全局安装」还是「当前项目下安装」，我一般选择全局，省的每个项目都要单独装一遍。

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HsHKkAGnUntj3BicoeIKAnF5TxwFol2PCcc5jd75JCgiaq0ZTVeSMlKrNLEEmltNibyZclic4Qno9qSLlrmaSob4wpcOpQZluIKjQ/640?wx_fmt=png&from=appmsg)

安装成功后，你的 AI 编程工具（Cursor、Claude Code、Codex 等）就会自动加载这些技能，在 AI 需要的时候帮你用上。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1EAtliaibz7SJfLePbGYSCWYUPXzvMxDJb0KicaB3Tk7ncbxU6EibkMhzuTQDyk1aWh59XJHNdduEbf84zGY5gLqvqo4zgobGKkWH4/640?wx_fmt=png&from=appmsg)

除了技能之外，你也可以配置 Vercel 的 MCP 服务，二选一就好。

直接进入 Vercel MCP 的官方文档，点击「Add to Cursor」按钮：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HiayqHibiaJxURQhibLV7icsKbnPz32RsSUiaicf5AqRSVFnITtKvoBN8xRJiaPFJiblgCKMC4G73gibV3JPXJufdf0MeZYlCwFjyH8bCIQ/640?wx_fmt=png&from=appmsg)

会自动跳转到 Cursor 的设置页面，点击 Install 就能完成安装了，然后完成授权即可，非常傻瓜式：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1HRic5qqa9xibFy1tOk40H9iaiczAyTrOddbrElXFmSageXH1dxUgRicxQ8X7NHKj0TKdLMQP5y6rp5tDIpqia80EWgHcAKpZ1EncKd4/640?wx_fmt=png&from=appmsg)

#### 4、让 AI 帮你部署

准备工作搞定，下面在 AI 编程工具中打开你的项目目录，就可以开始部署了。

这里我选择使用 Claude Code 来演示，但 Cursor、Codex 等其他 AI 编程工具的用法都是一样的。

安装 Skills 之后，会多出一个 `/deploy-to-vercel` 斜杠命令。直接在跟 AI 对话时输入这个命令，就能触发部署流程，比手打提示词更方便、成功率也更高。
```
    /deploy-to-vercel 帮我把当前项目部署到 Vercel  
    
```

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1FnyjYqdnZbbgRrhYJPgy1XUDSuJj1VRiaI0udKXTCSf6PFW6R2QjjKvyTA2VqcL8WYWgqsKhQCt8vmKZSeJH0qLJk0d89dHE3s/640?wx_fmt=png&from=appmsg)

AI 会自动帮你完成以下操作：

  1. 检测当前项目类型（比如识别出这是一个 VuePress 项目）
  2.   3. 如果是第一次部署，AI 会自动帮你创建 Vercel 项目并完成关联
  4. 等待构建完成，最后把访问链接给你（中间遇到报错还会自主修复）

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1FIOwViaa4J6VGuazuQdjwc8o5TwiatEicE4qBRp8T9A0DD3OII9F96gxcrTJIX6yAkARLEbJ4m1EZ437QKVFophXEvkMdDLKGaLI/640?wx_fmt=png&from=appmsg)

很快就部署完成了，AI 会返回一个类似 `xxx.vercel.app` 的链接：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1G9qxI98iapAudNuCzUyBvdbG4ic7aH3YyCbX3wZfKIvyzmtOnzqn3dwmZSls0EpyDNtCUkzRx5IJuLFddfTKswjIdeb4vHwjsf8/640?wx_fmt=png&from=appmsg)

直接打开网址就能看到效果了，不要太方便！

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1G6xZQHGJ2281QJNDlVqjpGAwHvWic7LHjzHxnKhia3iaiac1icjJw0lqfaF9j0YIuClH9VEaVRaSOLUseeLnRk1hsHRPdTZND2zzJE/640?wx_fmt=png&from=appmsg)

还可以在 Vercel 后台查看和管理已经部署的网站：

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1GWABDKa52yFsqLNh8RaJMf7Eic74hIvQ5ibVxyYIEmsTSxCVXdSscN6K7huqmBoGxvOVAGt16vdIgtgx6puEjENzXdbIXDDlsyQ/640?wx_fmt=png&from=appmsg)

对了，如果你的项目需要配置环境变量（比如 AI 模型的 API Key），在部署前告诉 AI 就行。

比如：帮我设置环境变量 OPENROUTER_API_KEY 值是 sk-xxx。

AI 会通过 Vercel CLI 帮你配好，部署时自动生效。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1EaOKBLxadqkO3IbsdicENhibemNDKeKia1K2TOVzR7geFicpCmEBTW65UNIDvDmzrJGKKJFa0GJ4NsaxILpJDTTib17ibGKsqJM2Tp0/640?wx_fmt=png&from=appmsg)

你也可以在 Vercel 网页控制台手动添加，效果是一样的。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1FZqDU75U4CuczcQOXd3S89Eeyny9BFiaYGYKjXQdpiaY1VG7qtvw4bYB38KRicysIBMYDAoX21xPR4gL90njUicKZxBAib4tcibZHdQ/640?wx_fmt=png&from=appmsg)

#### 5、后续更新

之后每次你修改了代码，只需要再跟 AI 说一句「帮我重新部署」就行。或者你也可以直接把代码推到 GitHub，Vercel 检测到代码更新后会自动触发重新部署，完全不用手动操作。

如果你想发布到正式的生产环境（带正式域名那种），直接跟 AI 说，让它帮你执行 `vercel --prod` 命令就好。

![](https://mmbiz.qpic.cn/mmbiz_png/LlSQOKIxJ1Gl0PdMx9rtvvA831picVh6p88pABiaRNAQWt5nSUaY8SICjI6WRG7Cl3z22zNLAmWXHntQan5x749naaXgqnoq2bTKxNB579DxY/640?wx_fmt=png&from=appmsg)

#### 6、配置自定义域名（可选）

Vercel 默认给你的是 `.vercel.app` 结尾的域名，不太优雅。

如果你有自己的域名，可以在 Vercel 控制台绑定，比如我之前给自己做的「互联网数字墓园项目 - 挂了么」绑定了个域名：

![](https://mmbiz.qpic.cn/sz_mmbiz_png/LlSQOKIxJ1GbVrBGGgicm1vghvqpzXTLaAacQOacEha6xwwdkyXufJGPN2eibS9mQuYxLEAvYfqMtN0KFbNibQ7JLiaHcxvU7qnmurZ60DI0lick/640?wx_fmt=png&from=appmsg)

添加域名后，AI 会引导你完成 DNS 配置，绑定成功后 Vercel 还会自动帮你配好 HTTPS 证书。

## 部署完整的前后端项目

前面演示的主要是前端和静态网站的部署方式。如果你的项目包含 Java / Python 后端，或者用到了 WebSocket 实时通信、定时任务、关系型数据库这些需要持久运行的服务，那上面这些托管平台就不够用了，还需要配合服务器或容器来部署后端部分。

最常见的做法是买一台阿里云或腾讯云的服务器，用宝塔面板或 1Panel 这种可视化工具来管理，前端放 Nginx、后端跑 Java 或 Node 进程，最传统但也最灵活，啥项目都能部署。

还有更灵活的方式，可以用 Docker 把应用打包成容器镜像，然后部署到 Railway、Render 这类 Serverless 容器托管平台上，不用自己管理服务器了。

我在 编程导航 的项目教程里几乎每种部署方式都手把手带大家做过，感兴趣的同学可以看看。
