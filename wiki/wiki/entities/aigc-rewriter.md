---
title: AIGC Rewriter
created: 2026-05-19
updated: 2026-05-23
type: entity
tags:
  - tool
  - nlp
  - open-source
sources:
  - https://github.com/h5box/aigc-rewriter
  - raw/articles/2026-04-22-aigc-rewriter-open-source-model.md
---

# AIGC Rewriter

## Overview

AIGC Rewriter 是一个开源的 AI 生成内容改写工具（GitHub: [h5box/aigc-rewriter](https://github.com/h5box/aigc-rewriter)），用于消除文本中的 AI 写作痕迹。由"格式工坊"团队开发，基于微调的 Qwen3 模型（qwen3-merged-aigc_zhv3-Q4_K_M.gguf），本地运行。

与 [[stop-slop]] 互补：后者是规则驱动的写作风格约束，AIGC Rewriter 则通过模型重写来去除 AI 特征。

## 核心优势

1. **AI 味移除**：实现对文本的 AI 写作风格消除
2. **轻量本地运行**：模型量化后适配各种 Windows 系统电脑，不需要高配置
3. **开源免费**：支持二次微调

## 使用方法

1. 下载压缩包并解压（提供夸克网盘和 GitHub 两种下载方式）
2. 右键管理员身份运行 `启动.bat`（自动配置环境和加载模型）
3. 浏览器地址栏输入 `http://127.0.0.1:8181`
4. 粘贴需要改写的文本，点击改写即可

## 技术细节

- 底层模型：基于 Qwen3 微调的 GGUF 量化模型
- 推理引擎：llama.cpp（支持 Vulkan GPU 加速和 CPU 回退）
- 前端：本地 Web 界面（端口 8181）
- 免费 AIGC 检测：[geshigongfang.com/aigc-check](https://www.geshigongfang.com/aigc-check)

## 故障排查要点

- 缺少 `llama-server.exe`：需补齐 `llama-b8721-bin-win-vulkan-x64`
- 模型文件缺失：确认 `qwen3-merged-aigc_zhv3-Q4_K_M.gguf` 在根目录
- 端口占用：查进程或修改脚本端口参数
- Vulkan 异常：可切回 CPU 版验证链路

## Relationships

- 与 [[stop-slop]] 互补，共同构成 [[anti-slop-writing]] 工具链
- 基于 Qwen3 模型架构

## See Also

- [[stop-slop]] — 规则驱动的写作风格约束
- [[anti-slop-writing]] — 反 AI 味写作的完整方法论
