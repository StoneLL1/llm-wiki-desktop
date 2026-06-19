---
title: "零成本搭建企业版 ChatGPT！星标 23K 的开源 AI RAG 平台 Onyx 自部署完整指南！"
url: "https://mp.weixin.qq.com/s/h3rKzP39xKvLWeSi8330jA"
source: "微信公众号"
fetched: 2026-04-18
sha256: bfd36601245d881d
---

大家好！这里是 `AI开源提效指南`！

最近公司打算搞知识库，我在 Github 上搜到了这个非常火的、开源的企业版 ChatGPT 平台! Onyx 是开源企业级 RAG 平台 + 知识库管理 + 企业 ChatGPT（AI 助手）**三者的结合体：**核心是面向团队的**一体化 AI 知识对话平台**。

它支持任意 LLM，能够无缝集成企业内部知识和应用，提供 AI 聊天、智能搜索、自定义 Agent、自动化操作等核心功能。

如果说 ChatGPT 是一个通用的超级大脑，那么 Onyx 的目标就是把这个大脑装进公司的身体里， 让它读懂所有的内部文档。

我这里把文档整理出来，分享给大家！文档内容有点长，有需要的可以仔细看看这篇文章！也可以收藏备用！

## 🎯 核心价值

![image](https://mmbiz.qpic.cn/mmbiz_png/F1MjIPU9X0OfF3cmdzKwjtr85L3KQicClm2BWkrNmDmYFXAPKTNNjVTSkn7X93iclzOQEO2QFQpS0rGsOlnE02mu35JMGS3U4EpWLAaZRM4ic4/640?wx_fmt=png&from=appmsg)

Onyx 是企业团队的 AI 中枢平台，类似"自部署的 ChatGPT 团队版"，但更强大：

- 🔒 **数据隐私**: 完全自托管，数据不出企业内网

- 🧠 **内部知识集成**: 连接企业内部文档、数据库、应用系统

- 🤖 **自定义 Agent**: 为不同场景创建专属 AI 助手

- 🔌 **开放生态**: 支持任意 LLM（OpenAI、Anthropic、Ollama 等）

- 💼 **企业级功能**: 用户管理、权限控制、审计日志

## 🎨 核心功能详解

### 1. 智能聊天（Chat）
**
Onyx 的主界面是强大的聊天 UI，支持多种高级功能。

##### 1.1 支持与文件/URL 聊天 :

- 支持上传 PDF、DOCX、TXT 等文档

- 可直接粘贴 URL 自动抓取内容

- 支持复用历史文件或者URL

##### 1.2 内置 4 大动作选择器 :
Action功能说明内部搜索搜索企业知识库基于已连接的 Connector网络搜索搜索互联网最新信息需配置搜索 API 密钥代码执行运行 Python 代码沙箱环境，安全执行图像生成AI 生成图片需配置图像生成 API
##### 1.3 支持深度研究（Deep Research）:

- 开启后 AI 会进行多轮思考、研究、行动

- 适合复杂问题，需要整合多个来源

- ⚠️ 深度研究可能需要几分钟，且成本可能是普通推理的数倍（>10倍）

##### 1.4 支持模型选择器:

- 支持所有主流 LLM 提供商

- 支持自部署模型（Ollama、VLLM 等）

- 可调节创造力和推理水平

##### 1.5 侧边栏功能：

**左侧边栏:**

- **新会话** 按钮：清空历史，开始新对话

- **项目**：组织聊天和文档集合

- **Agent**：快速启动自定义 AI 助手

- **会话**：最近对话历史列表

**右侧边栏:**

- 显示引用的来源和引用

- 来源可来自内部搜索或网络搜索

- 可选择文档加入下一轮对话

##### 1.6 支持项目（Projects）

**项目是指令（提示词）和文件的集合，用于：**

- 组织正在进行的工作

- 复用常用指令和文件

- 避免重复创建 Agent

**适用场景:**

- 长期项目管理

- 特定主题的持续研究

- 团队协作的知识库

### 2. 自定义代理（Agent）
**
Onyx 代理可以单独使用，也可以与 Onyx 内的其他用户或用户组共享

代理 = `指令 + 知识 + 动作`，可以把他们看作是针对特定任务优化的AI团队成员。

**1. 指令（Instructions）:**

`你是一个高度专业、细致、精准的专家。
目标：深度理解用户意图，必要时提出澄清问题，
逐步思考复杂问题，提供清晰准确的答案，
并主动预测有用的后续信息。
始终优先保证真实、细致、有洞察力且高效。

当前日期：[[CURRENT_DATETIME]]

使用规范：
- 使用粗体、表情符号（适度）、引用块等格式增强可读性
- 使用 Markdown 和 LaTeX 格式化数学、科学公式
- 使用表格呈现数据和结构化信息
- 使用水平线（---）分隔不同章节
`
```

**指令示例:**

- "始终以表格形式提供结果"

- "尽量直接引用文档原文，不要改写"

- "标记超过 3 个月的文档信息"

**2. 知识（Knowledge）:**

- 来自 Connector（外部数据源）

- 来自 文件上传

- Connector 的知识会自动同步更新

**最佳实践:** 仅包含必要知识，范围越窄，性能越可靠

**3. 动作（Actions）:**

Actions 让 Agent 能通过 API 与外部应用交互：
场景Action 示例客服系统根据对话更新工单状态实时监控查询服务状态给出实时答复销售管理根据通话记录推进 CRM 商机
### 3. 内部搜索
**
Onyx 支持企业知识搜索引擎

**Onyx 使用 LLM 原生技术构建知识索引：**

- 🔍 **混合搜索**: 结合语义搜索和关键词搜索

- 🧠 **上下文检索**: 智能理解查询意图

- 🕸️ **AI 生成知识图谱**: 发现知识关联

- 📊 **高级 RAG**: 减少幻觉，提高准确性

**已经支持的 Connector（数据源）如下:**

- Google Drive、Confluence、Slack

- GitHub、GitLab、Jira

- SharePoint、Notion、Salesforce

- 自定义 API、数据库、文件服务器

### 4. 网络搜索
**
Onyx 可以访问互联网，解答需要最新信息或LLM不确定的细分领域信息。 这个很重要，比如文档更新了，如果还是使用老的知识就会出现语法问题，这问题写代码的应该都知道。 用户可以随时切换“Web Search Action”来覆盖LLM的决定。

- 🔌 集成搜索提供商 API（如 Google、Bing）

- 🌐 实时获取互联网最新数据

- 📰 弥补训练数据时效性限制

- ⚙️ 需管理员配置 API 密钥

### 5. Actions & MCP
**
让 AI 执行实际的操作，动作赋予代理与外部系统交互的能力，并允许用户通过 OpenAPI 和模型上下文协议（MCP）配置更多动作

**动作可以让 Agent 拥有以下能力：**

- 通过 API 与外部应用交互

- 执行具体任务（不仅是回答问题）

- 自动化工作流程

**内置动作包含下面 4 种，这个上面我们在智能聊天部分也提到了（更详细的信息大家看官方文档吧）:**

- 内部搜索

- 网络搜索

- 代码执行

- 图像生成

**支持自定义动作:**

- 连接企业内部系统

- 调用 REST API、GraphQL

- 支持 MCP（Model Context Protocol）标准

### 6.代码执行
**
代码执行动作赋予 Onyx 的LLM编写和执行Python代码的能力，解锁了执行复杂计算、对上传文件进行数据分析以及生成或修改文档等能力。

**内置 Python 沙箱环境:**

- 🐍 直接在聊天中执行 Python 代码

- 📊 复杂计算和数据分析

- 🔒 沙箱隔离，确保安全

- 📈 支持数据可视化（图表生成）

**使用场景:**

- 数据统计和分析

- 图表生成和可视化

- 复杂数学计算

- 自动化脚本执行

## 🏗️ 系统架构

**技术栈**
层级技术前端Next.js、React、TypeScript后端Python、FastAPI数据库PostgreSQL向量库Qdrant / Weaviate / Milvus搜索引擎Vespa / Elasticsearch部署Docker、Docker Compose、Kubernetes
**部署架构**

`┌─────────────────────────────────────────────┐
│              Onyx Platform                  │
├─────────────────────────────────────────────┤
│  ┌─────────┐  ┌─────────┐  ┌─────────────┐  │
│  │  Web UI │  │  API    │  │  Background │  │
│  │(Next.js)│  │(FastAPI)│  │   Workers   │  │
│  └────┬────┘  └────┬────┘  └──────┬──────┘  │
│       │            │              │         │
│  ┌────┴────────────┴──────────────┴────┐    │
│  │         PostgreSQL                  │    │
│  │    (用户数据、会话、配置)              │    │
│  └─────────────────────────────────────┘    │
│  ┌─────────────────────────────────────┐    │
│  │      Vector Database (Qdrant)       │    │
│  │         (文档向量索引)                │    │
│  └─────────────────────────────────────┘    │
│  ┌─────────────────────────────────────┐    │
│  │      Search Engine (Vespa)          │    │
│  │         (全文搜索引擎)                │    │
│  └─────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────┐
│          External Integrations              │
│  LLM APIs | Connectors | Search APIs        │
└─────────────────────────────────────────────┘
`
```

## 💡 实际应用场景

### 场景 1: 企业知识库问答

**痛点:** 公司文档分散在多个系统（Confluence、Google Drive、Slack），员工找不到信息。

**Onyx 解决方案:**

- 配置 Connector 连接所有数据源

- 员工用自然语言提问

- Onyx 自动搜索相关知识并生成答案

- 提供引用来源，方便追溯

**效果:**

- 查找信息时间从 30 分钟缩短到 2 分钟

- 新员工入职效率提升 50%

### 场景 2: 客服工单自动化

**痛点:** 客服团队重复回答类似问题，工单处理效率低。

**Onyx 解决方案:**

- 创建"客服助手"Agent

- 连接产品文档、历史工单、FAQ

- 配置 Actions 自动更新工单状态

- AI 自动回答常见问题，复杂问题转人工

**效果:**

- 60% 工单自动处理

- 客服响应时间从 2 小时缩短到 5 分钟

### 场景 3: 研发知识库

**痛点:** 研发团队技术文档分散，新人上手慢，重复问题多。

**Onyx 解决方案:**

- 创建"工程师副驾驶"Agent

- 连接 GitHub、Jira、内部 Wiki

- 配置代码执行功能辅助调试

- 支持技术文档搜索和代码示例生成

**效果:**

- 新人上手时间从 2 周缩短到 3 天

- 重复问题减少 70%

### 场景 4: 销售支持助手

**痛点:** 销售团队需要快速了解产品信息、竞品对比、方案建议。

**Onyx 解决方案:**

- 创建"销售助手"Agent

- 连接产品文档、竞品分析、历史方案

- 配置 CRM Actions 自动更新商机

- 支持生成定制化方案和报价

**效果:**

- 方案准备时间从 1 天缩短到 1 小时

- 商机转化率提升 25%

## 🆚 竞品对比
特性OnyxChatGPT EnterpriseMicrosoft CopilotGlean自部署✅ 支持❌ 仅云端❌ 仅云端❌ 仅云端开源✅ 完全开源❌ 闭源❌ 闭源❌ 闭源LLM 选择✅ 任意 LLM⚠️ 仅 OpenAI⚠️ 仅 Azure OpenAI⚠️ 有限支持数据隐私✅ 完全可控⚠️ 依赖厂商⚠️ 依赖厂商⚠️ 依赖厂商定制开发✅ 可深度定制❌ 有限定制❌ 有限定制❌ 有限定制成本💰 免费（自托管）💰💰💰 $60/用户/月💰💰💰 $30/用户/月💰💰💰 定制报价Agent 支持✅ 完整⚠️ 有限⚠️ 有限❌ 不支持代码执行✅ 内置❌ 不支持⚠️ 有限❌ 不支持
## 🛠️ 部署指南

**前置要求**

**硬件要求:**
部署模式CPU内存存储Lite2 核4GB20GBStandard4 核8GB50GB
**软件要求:**

- Docker 20.10+

- Docker Compose 2.0+

- Git（用于拉取配置文件）

**详细部署步骤**

**步骤 1: 下载安装脚本**

`# Linux / macOS
curl -fsSL https://onyx.app/install_onyx.sh -o install_onyx.sh
chmod +x install_onyx.sh

# Windows PowerShell
irm https://onyx.app/install_onyx.ps1 -Outfile install_onyx.ps1
`
```

**步骤 2: 运行安装脚本**

`# Linux / macOS
bash install_onyx.sh

# Windows
.\install_onyx.ps1
`
```

**步骤 3: 选择部署模式**

`Select deployment type:
  1) Lite (适合个人/小团队)
  2) Standard (适合生产环境)
Enter choice [1-2]: 
`
```

**步骤 4: 配置版本和参数**

`Select Onyx version:
  1) Latest (推荐)
  2) Stable
  3) Specific version
Enter choice [1-3]:
`
```

**步骤 5: 等待部署完成**

脚本会自动：

- 拉取 Docker 镜像

- 创建配置文件

- 启动容器

- 初始化数据库

**步骤 6: 访问 Onyx**

部署完成后，访问 `http://localhost:3000` 即可使用。

**开发环境搭建**

`# 克隆仓库
git clone https://github.com/onyx-dot-app/onyx.git
cd onyx
# 安装依赖
pip install -r requirements.txt
# 启动开发环境
docker compose -f docker-compose.dev.yml up
# 运行测试
pytest tests/
`
```

## 🔧 配置指南

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/F1MjIPU9X0Md3SicLExLYZ1MLeKvh0zmGRjOvNcQ36pG5dZPsz0ocRnaRYmia9kXSIZeGYF4SjmiaxY8XwD9gqNfb6ZzicAjELic0NyQLZDewUAw/640?wx_fmt=png&from=appmsg)

**LLM 配置**

**支持的主流 LLM:**
提供商模型示例配置方式OpenAIGPT-4, GPT-3.5API KeyAnthropicClaude 3, Claude 2API KeyGoogleGemini Pro, Gemini UltraAPI KeyOllamaLlama 3, Mistral自部署VLLM任意开源模型自部署AzureAzure OpenAIAPI Key + Endpoint
**配置步骤:**

- 登录 Onyx 管理后台

- 进入 Settings → Models

- 添加 LLM 提供商和 API Key

- 测试连接

- 设置为默认模型

**Connector 配置**

![image](https://mmbiz.qpic.cn/sz_mmbiz_png/F1MjIPU9X0POY3tFQc09pgoCdFuDGbqLRWym6tEibhFXGiaHZOxkgauvtDJYCC8YrbcS9n8hUyCbpAEQibH19VyA0WmGVecKcrGETB3sTn8ZfA/640?wx_fmt=png&from=appmsg)

**常见 Connector 配置:**

**Google Drive:**

`connector_type: google_drive
credentials: service_account_key.json
include_files: true
include_folders: true
`
```

**Confluence:**

`connector_type: confluence
wiki_base: https://your-company.atlassian.net
username: your-email@company.com
api_token: your_api_token
`
```

**Slack:**

`connector_type: slack
bot_token: xoxb-your-bot-token
channels: ["general", "engineering", "support"]
`
```

## 📚 最佳实践

### 1. 知识管理

**✅ 推荐做法:**

- 按部门/主题组织 Connector

- 定期更新和清理过期文档

- 为敏感数据设置访问权限

- 使用标签和元数据增强搜索

**❌ 避免做法:**

- 一次性导入所有文档（应分批）

- 忽略文档质量（垃圾进，垃圾出）

- 不设置权限（导致信息泄露风险）

### 2. Agent 设计

**✅ 推荐做法:**

- 明确 Agent 的职责范围

- 提供详细的指令和示例

- 限制知识范围（越精准越好）

- 充分测试后再共享给团队

**❌ 避免做法:**

- 指令过于宽泛（导致行为不可控）

- 知识过多（影响性能和准确性）

- 不测试就直接使用

### 3. 性能优化

**✅ 推荐做法:**

- Standard 模式部署生产环境

- 使用 SSD 存储提升搜索速度

- 定期清理旧会话和日志

- 监控资源使用情况

**❌ 避免做法:**

- Lite 模式用于多用户生产环境

- 忽略监控和告警

- 不定期备份数据

## 🔒 安全与合规

### 内置安全特性:

- 🔐 数据加密存储（AES-256）

- 🔑 基于角色的访问控制（RBAC）

- 📝 完整的审计日志

- 🔒 支持 SSO（SAML、OIDC）

- 🛡️ 支持 VPC 部署

### 支持的合规标准:

- SOC 2 Type II

- GDPR

- HIPAA（需额外配置）

- ISO 27001

## 📚 参考资源

`- GitHub 仓库: https://github.com/onyx-dot-app/onyx
- 官方文档: https://docs.onyx.app
- 云服务: https://cloud.onyx.app
- 社区论坛: https://community.onyx.app
`
```

**一句话推荐**: 如果你需要自部署的企业级 AI 平台，Onyx 是最佳选择！

**🎯 ****觉得这份工具干货有用？不妨这样做**

- ⭐ 星标 / 置顶公众号，**第一时间解锁最新工具分享！**

- ✅ **点赞**「**推荐**」，让更多技术伙伴发现优质干货！

- 🔗 **转发**给团队小伙伴，一起高效提效！

- 💬 **底部留言区**，告诉我你想找的工具/项目方向！

**📬 长期追踪优质开源工具**

- 关注「**AI 开源提效指南**」｜日更开源神器，玩转技术提效！

- 回复 **【容器加速器】**，即刻开启你的高效探索之旅～