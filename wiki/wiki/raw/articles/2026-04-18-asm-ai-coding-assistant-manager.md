---
title: "asm多种AI编程助手技能统一管理神器"
url: "https://mp.weixin.qq.com/s/RXMG_qEMalZvWfvDv1iFYg"
source: "微信公众号"
fetched: 2026-05-27
sha256: d92533aeea1a0189
image_count: 8
---

大家好！这里是`AI开源提效指南`！

你是否同时使用 Claude Code、Codex、Cursor、OpenClaw 等多个 AI 编程助手？是否为技能分散在各个隐藏目录而头疼？asm（agent-skill-manager）来了！一个工具统一管理所有 AI 助手的技能，让技能管理从未如此简单！

**技能散落各地** ：

  *   *   *   * 项目级的 `.claude/skills/`... 同一个技能安装了三次，却不知道哪个版本在哪里！

**没有全局视野** ：无法快速查看已安装的技能、重复的技能或过时的技能！

**安装繁琐危险** ：手动克隆仓库、复制文件夹、希望 SKILL.md 格式正确、担心安装了会泄露代码的恶意技能！

**使用的 AI 助手越多，这个问题就越严重！**

**agent-skill-manager（asm）** 是一个统一的命令行工具，帮你管理所有 AI 编程助手的技能。一个 TUI 界面，一个 CLI 命令，搞定所有助手！

* * *

## 🎯 为什么选择 asm？

### 🎯 核心特性

#### 1\. 全局视野，一目了然

从统一的仪表板中列出、搜索和过滤所有提供商和范围的技能。再也不用在各个隐藏目录间用 `ls` 命令来回切换了！

#### 2\. 一键安装 GitHub 技能
```
    asm install github:user/repo  
    
```

自动处理克隆、验证和放置。支持：

  *   *   *   * 

#### 3\. 安全扫描，防患未然

内置安全扫描功能，在安装前标记危险模式：

  *   *   *   *   * 

#### 4\. 创建、测试和发布技能

完整的本地开发工作流：
```
    # 创建新技能  
    asm init my-skill  
    # 符号链接实时开发  
    asm link ./my-skill -p claude  
    # 安全审计  
    asm audit security my-skill  
    # 发布到 ASM Registry  
    asm publish ./my-skill  
    
```

#### 5\. 支持 17+ 个主流 AI 助手

内置支持：

  *   *   *   *   *   *   *   *   *   *   *   *   *   *   *   *   * 

还支持通过配置文件秒级添加自定义提供商！

#### 6\. 双界面，一种工具

  * 🖥️ **完整交互式 TUI** ：键盘导航、搜索、详情视图
  * 💻 **CLI 模式** ：支持 `--json` 输出，适合脚本和自动化

* * *

## 🚀 快速开始（30 秒上手）

### 安装方式

**方式 1：npm**
```
     npm install -g agent-skill-manager  
    
```

**方式 2：Bun**
```
     bun install -g agent-skill-manager  
    
```

**方式 3：curl**
```
     curl -fsSL https://raw.githubusercontent.com/luongnv89/asm/main/install.sh | bash  
    
```

### 立即使用
```
    # 运行 TUI 界面  
    asm  
    # 搜索技能  
    asm search code-review  
    # 安装技能  
    asm install github:user/repo  
    # 查看统计信息  
    asm stats  
    
```

* * *

## 💡 核心使用场景

### 场景 1：浏览技能目录

不想安装？直接在浏览器中探索：

👉 **ASM Catalog**

  *   *   * 🔗 分享过滤视图（如 `?q=code-review&cat=development`）
  * 

**无需注册、无需后端、无追踪！**

###  场景 2：查找已安装的代码审查技能
```
    asm search code-review  
    
```

输出示例：
```
    ✅ 已安装：code-review (Claude Code)  
    📦 可安装：github:anthropic/code-review-skill  
    📦 可安装：github:openclaw/code-review  
    
```

### 场景 3：查看全局统计
```
    asm stats  
    
```

显示：

  *   *   *   * 

* * *

## 🛠️ 本地开发工作流：从零到发布

### 步骤 1：创建技能

**交互模式（选择目标工具）：**
```
     asm init my-skill  
    
```

**直接指定工具：**
```
     asm init my-skill -p claude  
    
```

**自定义目录：**
```
     asm init my-skill --path ./skills  
    
```

自动创建 `my-skill/SKILL.md`，包含有效的 YAML frontmatter 和 Markdown 模板。

### 步骤 2：符号链接实时开发

`asm link` 创建从本地技能目录到 AI 助手技能文件夹的符号链接。每次编辑都立即可见，无需重新安装！
```
    # 链接到 Claude Code  
    asm link ./my-skill -p claude  
    # 链接到 Codex  
    asm link ./my-skill -p codex  
    # 一次链接多个技能  
    asm link ./skill-a ./skill-b ./skill-c -p claude  
    # 覆盖现有链接  
    asm link ./my-skill -p claude --force  
    
```

### 步骤 3：安全审计
```
    # 审计已安装的技能  
    asm audit security my-skill  
    # 审计本地目录  
    asm audit security ./path/to/my-skill  
    # 审计所有技能  
    asm audit security --all  
    
```

安全扫描器会标记：

  *   *   *   *   * 

### 步骤 4：验证元数据
```
    # 检查名称、版本、描述、文件数  
    asm inspect my-skill  
    # 机器可读输出（适合 CI）  
    asm inspect my-skill --json  
    
```

### 步骤 5：测试安装流程
```
    # 模拟用户安装  
    asm install github:you/awesome-skill  
    # 安装到指定工具  
    asm install github:you/awesome-skill -p claude  
    # 从多技能仓库安装指定技能  
    asm install github:you/skills --path skills/awesome-skill  
    # 强制重新安装（测试升级）  
    asm install github:you/awesome-skill --force  
    # 非交互模式（适合 CI）  
    asm install github:you/awesome-skill -p claude --yes --json  
    
```

### 步骤 6：发布到 ASM Registry
```
    asm publish ./my-skill  
    
```

发布流程：

  1. ✅ 验证 SKILL.md frontmatter（名称、描述、版本）
  2.   3. 📝 生成包含当前 commit SHA 的 manifest
  4. 

Registry CI 会验证：

  *   *   *   *   * 

合并后，任何人都可以通过名称安装：
```
    # 无需 GitHub URL，直接按名称安装  
    asm install code-review  
    asm install luongnv89/code-review  
    
```

* * *

## ✅ 技能验证机制

asm 会自动评估技能的验证标准，通过的技能获得 `verified` 徽章。

### 检验标准

  1. **有效的 frontmatter** — SKILL.md 必须包含 `name` 和 `description` 字段
  2. **有意义的内容** — Markdown 正文至少 20 个字符的指令文本
  3.      * `atob()` 调用（运行时 base64 解码/混淆）
     *      *      * 硬编码凭证（`API_KEY`、`SECRET_KEY`、`PASSWORD`）
  4. **正确的结构** — 技能目录必须存在且包含 `SKILL.md`

### 本地验证
```
    # 索引仓库（自动运行验证）  
    asm index ingest github:your-user/your-repo  
    # 检查验证结果  
    asm index search "your-skill" --json  
    
```

* * *

## 📸 界面预览

### TUI 仪表板

交互式终端界面，支持键盘导航和实时搜索

### 搜索功能

查找已安装和可用的技能，智能推荐相关技能

### 统计信息

展示技能总数、磁盘使用量和各 AI 助手的分布情况

* * *

## 📚 参考资源
```
    - GitHub 仓库: https://github.com/luongnv89/asm  
    - 官方文档: https://luongnv.com/asm  
    - ASM Registry: https://github.com/luongnv89/asm-registry  
    
```

* * *

**🎯****觉得这份工具干货有用？不妨这样做**

  * ⭐ 星标 / 置顶公众号，**第一时间解锁最新工具分享！**
  *   *   * 

**📬 长期追踪优质开源工具**

  * 关注「**AI 开源提效指南** 」｜日更开源神器，玩转技术提效！
  *
