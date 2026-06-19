---
title: "WPS365 CLI"
created: 2026-05-25
updated: 2026-05-27
type: entity
tags: [tool, open-source, enterprise, engineering]
sources:
  - raw/GitHub/wps365-open-cli.md
---

# WPS365 CLI

**WPS365 CLI** 是 WPS 365 官方开源的命令行工具（wps365-open/cli，71 ⭐），面向开发者与 AI Agent，提供 WPS 365 全部 API 的命令行入口。

## 核心能力

覆盖 WPS 365 七大模块：

| 模块 | 能力 |
|------|------|
| 📅 日历 | 日程 CRUD、参会人管理、忙闲查询、请假日程、批量操作 |
| 💬 即时通讯 | 消息收发回复撤回、群聊增删改查、成员管理、加急、书签 |
| 👤 通讯录 | 用户搜索（姓名/邮箱/手机号）、批量查询、部门与离职人员管理 |
| 📧 邮箱 | 邮件收发搜索、文件夹浏览、草稿管理、邮件组与通讯录管理 |
| 📁 云文档 | 驱动器管理、文件上传下载搜索、批量操作、权限管理、版本管理、分享链接 |
| 📋 多维表 | 数据表/字段/视图管理、记录增删改查与搜索、仪表盘、Webhook、附件 |
| 🎥 会议 | 在线会议管理、参会人管理、预约会议、会议纪要与录制、会议室与层级管理 |

## 双轨命令体系

WPS365 CLI 提供两种粒度的调用方式，这一设计与 [[skill-engineering|Skill 工程化]] 中「确定性事务交给 CLI」的理念一致：

1. **精装命令** — 语义化参数、智能默认值、auth 约束自动校验
   ```bash
   wps365-cli user me
   wps365-cli calendar events create primary --name "周会" \
     --from "2024-01-15T14:00:00+08:00" --to "2024-01-15T15:00:00+08:00"
   wps365-cli im messages send --to u1 --to u2 --text "hello"
   ```
2. **通用 API 调用** — 直接调用任意 WPS 365 开放平台端点
   ```bash
   wps365-cli api get "/v7/users/current"
   wps365-cli api post "/v7/calendars/create" --data '{"summary": "项目日历"}'
   ```

## 认证体系

### 认证模式

| 模式 | 说明 | 获取方式 |
|------|------|----------|
| `delegated` | 用户授权身份，适用于用户态接口 | `auth login --scopes "..."` |
| `app` | 应用身份，适用于服务端调用 | `auth login --app` |

### 认证命令

| 命令 | 说明 |
|------|------|
| `auth setup` | 配置 OAuth 客户端凭证（交互式引导） |
| `auth login` | 登录授权（`--scopes` 或 `--app`） |
| `auth status` | 查看认证状态 |
| `auth token` | 输出当前 access token |
| `auth refresh` | 手动刷新 token |
| `auth logout` | 删除本地 token |
| `auth clean` | 清理所有认证数据 |

### 安全存储

- **钥匙串**（macOS/Windows）：系统 Keychain / Credential Manager
- **加密文件**（Linux）：AES-256-GCM 加密
- Token 过期前 10 秒主动刷新，401 响应时透明刷新并重试

## 进阶用法

### 输出格式
```bash
-o json      # JSON（默认）
-o yaml      # YAML
-o table     # 易读表格
-o tsv       # Tab 分隔（适合管道处理）
```

### Dry Run
```bash
wps365-cli --dry-run user me
wps365-cli --dry-run api get "/v7/users/current"
```

### 环境变量

| 变量 | 用途 |
|------|------|
| `WPS365_CLIENT_ID` | OAuth 客户端 ID |
| `WPS365_CLIENT_SECRET` | OAuth 客户端密钥 |
| `WPS365_AUTH` | 默认认证模式（app/delegated） |
| `WPS365_ACCESS_TOKEN` | 直接注入 access token |
| `WPS365_API_BASE` | API 基础地址 |
| `WPS365_KEYRING_BACKEND` | 凭证存储后端（keychain/file） |

## 与 AI Agent 生态的关系

WPS365 CLI 作为 CLI 工具天然适合被 AI Agent 调用，与 [[feishu|飞书]] 在企业协作场景中形成互补：

- **飞书**: 字节跳动的企业协作平台，API 丰富，国际版为 Lark
- **WPS 365**: 金山办公的企业套件，在国内政企市场占有率高

两者都是 AI Agent 接入企业办公场景的重要通道。WPS365 CLI 的「精装命令 + 通用 API」双轨设计，与 [[mcp|MCP]] 暴露工具的思路相似——提供高层语义接口同时保留底层 API 直通能力。

## 技术细节

- **语言**: PowerShell（跨平台）
- **安装**: `curl` 一键安装（macOS/Linux）、PowerShell 一键安装（Windows）
- **许可证**: MIT
- **前置条件**: 需在 WPS 365 开放平台完成应用创建与权限配置
