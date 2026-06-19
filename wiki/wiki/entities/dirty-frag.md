---
title: Dirty Frag
created: 2026-05-27
updated: 2026-05-27
type: entity
tags: [tool, security]
sources:
  - raw/articles/2026-05-12-10-new-open-source-github-projects.md
---

# Dirty Frag

## Overview

Dirty Frag 是一个影响几乎所有主流 Linux 发行版的本地提权漏洞链。它利用 Linux 内核网络子系统中的两个漏洞，组合起来通杀 Ubuntu、RHEL、CentOS、Fedora、openSUSE 等发行版。一行命令从普通用户提权到 root。

## 技术细节

### 两个互补漏洞

- **xfrm-ESP 变种**：提供任意 4 字节写入原语，在 RHEL/CentOS/Fedora/openSUSE 上有效
- **RxRPC 变种**：不需要 namespace 权限，在 Ubuntu 上有效

### 利用手法

- **ESP 变种**：修改 `/usr/bin/su` 的页缓存，用 192 字节微型 ELF 替换前 192 字节，绕过 PAM 直接拿 root shell
- **RxRPC 变种**：修改 `/etc/passwd` 第一行，清空密码字段，利用 PAM 的 nullok 配置无密码 su

### 影响范围

- 漏洞有效生命周期约 9 年（从 2017 年存在）
- 确定性漏洞，不需要竞争条件，成功率极高
- 开源地址：https://github.com/V4bel/dirtyfrag

## 安全启示

对 [[multi-agent-collaboration]] 场景中的安全设计有重要参考价值：Agent 运行环境的安全性是基础保障。

## 相关链接

- [[multi-agent-collaboration]] — 多 Agent 安全考虑
