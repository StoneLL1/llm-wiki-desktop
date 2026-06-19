---
title: "推荐 10 个本周 GitHub 上最火火火的开源项目，收藏一波"
source: https://mp.weixin.qq.com/s/loJB2RIvH8yowq9f_HE4Uw
author: 逛逛GitHub
date: 2026-04-18
type: article
tags: [open-source, github, weekly]
sha256: 2b9690b8a28aa50a
---

# 推荐 10 个本周 GitHub 上最火火火的开源项目，收藏一波。

> 来源：微信公众号「逛逛GitHub」| 2026-04-18

---

## 01 多 Agent 系统的可视化 IDE

Rowboat 是 YC 孵化的一个开源项目，目前 1.2 万 Star。

简单说，就是一个用来搭建多 agent 系统的可视化 IDE，还带一个 Copilot 辅助你生成 agent。

你不用自己写一行代码，丢一段自然语言描述过去，Copilot 就会帮你把多 agent 工作流搭起来。

搭完之后还能在 AI 模拟场景里测试，确认没问题再接入 MCP server 和各种工具。

底层用的是 OpenAI 的 Agents SDK，对接了 Slack、Linear、Jira、GitHub、ElevenLabs、Exa 这些常用服务。

Python SDK 和 HTTP API 两种方式都能集成到你自己的产品里。

如果你想做 AI 客服、自动化调研或者内部工作流，又不想从零啃多 agent 框架，这个项目能帮你省下不少时间。

开源地址：https://github.com/rowboatlabs/rowboat

---

## 02 把编程 Agent 变成真正的队友

Multica 的思路挺有意思，Linear 加 AI Agent 的组合。

现在大家用 Claude Code、Codex 这些编码 agent，基本都是对着终端一轮一轮复制粘贴提示词，干完一个任务人还得盯着。

Multica 想把这套改掉。

它把 agent 做成了团队成员，你像给同事派活一样在看板上分配任务，agent 自己去执行、报告进度、更新状态、遇到问题还能上报 blocker。

目前已经拿下 1.47 万 Star，4 月份刚更新完。

核心能力包括任务全生命周期管理、WebSocket 实时进度流、每个 workspace 独立隔离，支持本地 daemon 和云端 runtime 混用。

兼容 Claude Code、Codex、OpenCode、Gemini、Cursor Agent 等一大堆 CLI。

而且解决方案会自动沉淀成可复用的 skill，团队能力随着使用越来越强。

如果你的团队已经开始用 AI agent 写代码，这个项目能让协作井井有条。

开源地址：https://github.com/multica-ai/multica

---

## 03 AI 编码工程纪律包

这个项目作者是 Addy Osmani，Google Chrome 团队的工程 leader，写过 Learning JavaScript Design Patterns 那本书的大佬。

Agent Skills 做的事情很直接，把资深工程师的开发规范封装成 AI 可以直接执行的 skill。

目前 1.66 万 Star。

核心是 7 个 Skill 贯穿全流程：/spec 先写需求、/plan 拆解任务、/build 增量实现、/test 验证、/review 质量门禁、/code-simplify 简化、/ship 部署。

里面打包了 20 个 skill，按阶段分得很清楚。

基本覆盖了从定义需求到上线的完整生命周期。

AI 写代码容易走捷径跳过规范，这个项目就是把大厂工程师那套纪律硬塞给 agent。

如果你用 Claude Code 或者类似工具搞实际项目，这套 skill 能显著提升交付质量。

开源地址: https://github.com/addyosmani/agent-skills

---

## 04 让 AI 编程可重复的工作流引擎

Archon 是 coleam00 开源的一个项目，定位是 AI 编程的 harness builder，目前 1.84 万 Star。

它要解决的问题挺痛的。

现在 AI 编程 agent 跑起来，每次结果都不一样，同样一个任务它今天跳过了 plan 阶段，明天忘了写测试，后天又无视了 PR 模板。

Archon 干的事就是用 YAML 把流程固定下来，让 AI 编码变得确定可重复。

几个设计挺讲究的：

每次工作流运行都会开一个独立的 git worktree，多个任务并行跑也不会互相污染。

节点是可组合的，确定性的 bash 脚本、测试和 AI 的规划、代码生成节点都能往里塞。

自带 17 个默认工作流，feature 开发、issue 修复、PR review、重构都有模板。

它也不只是命令行工具，CLI、Web UI、Slack、Telegram、Discord、GitHub 都能触发。

团队只要把 .archon/workflows/ 目录下的 YAML 文件提交到仓库，所有人用的流程就对齐了。

有点像 n8n 在通用自动化里的角色，Archon 想在 AI 编码这件事上干这件事。

开源地址：https://github.com/coleam00/Archon

---

## 05 港大开源的 AI 家教

DeepTutor 是香港大学 Data Intelligence Lab 开源的 AI 学习助手，目前拿下 1.88 万 Star。

它有 5 大学习模式：

带工具增强的聊天(RAG + 联网 + 代码执行)、多 agent 拆解复杂问题的 Deep Solve、基于知识库自动出题的 Quiz Generation。

还有并行 agent 深度调研的 Deep Research、专做数学可视化的 Math Animator。

最有意思的是它做了一个叫 TutorBot 的东西，每个 TutorBot 有独立工作区和人设，可以自主运行，比那种简单的聊天机器人高一个档次。

还有 Co-Writer 这个 Markdown 编辑器把 AI 做成了第一公民，Persistent Memory 让学习者画像在所有功能里共享。

开源地址: https://github.com/HKUDS/DeepTutor

---

## 06 让 Claude Code 变聪明的 CLAUDE.md

andrej-karpathy-skills 的核心就是一个 CLAUDE.md 文件，灵感来自 Karpathy 在推上吐槽大模型写代码的那几个毛病，目前已经狂揽 5 万 Star。

Karpathy 当时吐槽了啥？

模型做了错误假设不澄清就硬往下写、代码和 API 过度设计搞一堆没用的抽象、顺手改了它没完全理解的代码、困惑了也不说就装懂继续。

作者把这些观察转化成了 4 条原则：

思考再写：明确假设，必要时推回、简洁优先：只写必要代码，别搞推测性功能、手术式修改：只碰必要的代码，匹配现有风格、目标驱动执行：先定验证标准再跑。

安装有两种方式，推荐走 plugin:

```
/plugin marketplace add forrestchang/andrej-karpathy-skills
/plugin install andrej-karpathy-skills@karpathy-skills
```

也能直接把 CLAUDE.md 下到项目根目录用。

如果你觉得 Claude Code 最近写代码有点自作主张，试试这个文件，会老实很多。

开源地址：https://github.com/forrestchang/andrej-karpathy-skills

---

## 07 给 Claude Code 装上长期记忆

claude-mem 解决的是 Claude Code 每次开新会话就失忆的问题，目前已经 6 万 Star。

它的工作原理非常直接，会话进行时自动捕捉 Claude 做的所有事，用 Claude 的 agent-sdk 做 AI 语义压缩，等下次启动新会话再把相关上下文注入回去。

你不用手动记录，不用手动召回，整个过程是自动的。

功能上还挺全：

检索用的是 skill-based 自然语言搜索，还做了渐进式披露，每层记忆调用的 token 成本都标出来了，不会默默烧你的 API 费。

自带一个本地 Web Viewer 跑在 localhost:37777，用浏览器能直接看历史记录。

隐私方面，你可以用 `<private>` 标签圈住不想被记住的内容。

底层存储是 SQLite 加 Chroma 向量库，都是本地的。

顺带也支持 Gemini CLI。

安装一行命令搞定:

```
npx claude-mem install
```

如果你每天和 Claude Code 打交道，这个工具几乎是刚需。装完之后那种接续上次的感觉，挺舒服的。

开源地址：https://github.com/thedotmack/claude-mem

---

## 08 中国教材 PDF 全套打包

ChinaTextbook 这个项目挺硬核的，把小学、初中、高中、大学的教材 PDF 全收了，目前 6.97 万 Star。

作者的动机写得很直白，想推动义务教育资源平权，让没条件的家庭也能拿到正版教材，也方便海外华人家庭让孩子接触中文教育资源。

内容覆盖人教版、五·四学制等多种课纲，数学部分从一年级一直拉到大学，特别全。

所有文件都是 PDF，完全免费。

部分超过 50MB 的文件被切成 35MB 一段，仓库里提供了合并工具。

开源地址：https://github.com/TapXWorld/ChinaTextbook

---

## 09 万物转 Markdown 神器

MarkItDown 是微软官方出的 Python 工具，目前 11.1 万 Star，妥妥的顶流。

干的事情很简单，把各种格式的文件转成 Markdown。

支持的格式一长串：PDF、Word、PPT、Excel、图片、音频、HTML啥的，甚至连 YouTube 链接都能直接拽进去。

为啥转 Markdown 呢？

因为现在的 LLM 对 Markdown 是原生友好，token 利用率最高，结构也能完整保留下来，标题、列表、表格、链接都在。

几个实用的点：内置 LLM 集成，图片可以调 OpenAI 模型做描述，音频能转录。有 Azure 文档智能的对接，也支持第三方插件扩展，比如 markitdown-ocr。

安装一行搞定:

```
pip install 'markitdown[all]'
```

命令行和 Python API 都能用:

```
markitdown path-to-file.pdf -o document.md
```

不管是给 Claude、GPT 喂文档，做知识库，还是做数据清洗，这个工具都应该放进常用工具箱。

开源地址：https://github.com/microsoft/markitdown

---

## 10 端侧跑得动的 TTS 大模型

VoxCPM 是面壁智能开源的一个语音合成大模型，上线没多久就已经拿下 1.38 万 Star。

说白了，这是一个 2B 参数的 TTS 模型，训练数据用了 200 多万小时的多语种语音，支持 30 种语言自动识别切换，输出的音频是 48kHz 录音棚级别。

它有两个功能还挺顶的：

一个是 Voice Design，你给一段文字描述，它能直接生成一个符合描述的音色，不用提供参考音频。

另一个是可控声音克隆，克隆完之后还能加风格引导，让同一个声音表现出不同情绪。

实时流式推理方面，一张 RTX 4090 上 RTF 能跑到 0.3 左右，基本算端侧可用了。

如果你在做播客、视频配音、智能客服或者有声书，这个模型可以拉下来试试。

开源地址: https://github.com/OpenBMB/VoxCPM
