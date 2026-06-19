---
title: "GitHub 33k 星：这个开源项目，把你的知识库变成了会思考的 AI 大脑"
url: "https://mp.weixin.qq.com/s/DxMRDek6q2WcCEw_wiW-Pg"
source: "微信公众号"
fetched: 2026-04-18
sha256: e5ceb8eb737543e1
---

# GitHub 33k 星：这个开源项目，把你的知识库变成了会思考的 AI 大脑
**
当你的笔记、文档、邮件都能被 AI 理解并随时检索，还能在任何设备上用自然语言对话——这不是科幻，是 Khoj 正在做的事。

## 一、它是什么

Khoj（发音同 "knowledge" 的前两音节）是一个**开源的个人 AI 第二大脑**。

GitHub：khoj-ai/khoj ⭐ 33,642 | 语言：Python + TypeScript

简单来说，它把**本地文档 + 在线大模型 + 语义搜索**三者结合，让你的所有笔记、PDF、Markdown、Notion 都能被 AI 理解，变成一个可以随时对话的私人知识库。

## 二、为什么值得关注

市面上的 AI 助手很多，但有三个痛点 Khoj 精准命中：

**1. 私有化部署，数据不离手**ChatGPT、Claude 的数据会经过第三方，而 Khoj 完全本地运行，文档不上云，隐私有保障。

**2. 支持本地模型，离线可用**不联网也能跑——Llama3、Qwen、Gemma、Mistral 随便接，Mac M 芯片电脑上跑起来毫无压力。

**3. 多端入口，不挑工具**支持 Obsidian 插件、Emacs、Web、桌面客户端、手机 App、WhatsApp——在你习惯的地方直接问，不需要专门开网页。

## 三、核心功能

- **与文档对话**：上传 PDF、Markdown、Notion、Word、Org-mode，直接问 AI 相关问题，AI 基于你的文档回答，而不是泛泛而谈

- **自定义 Agent**：创建专属角色 AI（带知识库、人设、工具集），让 AI 按你的逻辑工作

- **自动化研究**：设置定时任务，AI 自动抓取新闻、竞品动态，生成每日简报推送到邮箱

- **全模型支持**：GPT、Claude、Gemini、DeepSeek、Llama3、Qwen、Mistral，随便切换，随便混用

- **语义搜索**：不只是关键词匹配，AI 理解你的搜索意图，在海量文档中找到最相关的那段

- **图像生成 + 语音**：AI 可以生成图片、朗读回答，手机上用语音向知识库提问

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/MEMowKW5K0NmDV7aLLmkprmV8FNQP2iaAuVaEmcb7vQfETIiaRZeoQ4PBR2MJEwa9k4sUpjZicfvM4qVRjFvpYVJttjLOCzuIpaibP7GmexC8po/640?wx_fmt=png&from=appmsg)
Khoj Agent 界面
## 四、快速上手（3 分钟跑起来）

**方式一：直接用云端（零配置）**

打开 https://app.khoj.dev，注册即可使用，无需安装任何东西。

**方式二：Docker 一键部署（推荐）**

`git clone https://github.com/khoj-ai/khoj.git
cd khoj

# 生成配置
make config

# 启动（需要 Docker）
docker compose up
`
```

访问 `http://localhost:42110`，按引导配置模型即可。

**方式三：本地 Python 安装**

`pip install khoj
khoj
`
```

## 五、效果截图

![image](https://mmbiz.qpic.cn/mmbiz_png/MEMowKW5K0PY1CbXOJFIpqiaGtZooMpqA53ufsTXJ5rnaF2piaMXI18Wghvb9pZvVJdvbQU6iccdtialuamqyicVFC91ibTHd4egyNQjO1kgrV8Uc/640?wx_fmt=png&from=appmsg)
Khoj Chrome PWA
从浏览器到手机 App，Khoj 提供了统一的 AI 对话体验，所有对话历史跨设备同步。

## 六、谁适合用
人群用 Khoj 做什么科研人员上传论文 PDF，用 AI 快速检索研究背景独立开发者让 AI 理解自己项目的代码和文档，随时回答技术问题知识工作者把所有笔记汇总，用自然语言随时查询企业团队自托管部署，在内网搭建私有 AI 知识库AI 爱好者体验各种大模型，在本地实验 Agent 玩法
## 七、总结

Khoj 不是一个普通的 AI 聊天工具，它解决的是**"我的知识在那里，但我找不到、用不上"**这个根本问题。

当你把几年积累的笔记、论文、项目文档全部接入 Khoj，问它任何问题，它都能跨越所有文档给你一个综合答案——这种体验，是传统搜索完全无法提供的。
**
**项目地址**：https://github.com/khoj-ai/khoj**官方文档**：https://docs.khoj.dev**在线体验**：https://app.khoj.dev