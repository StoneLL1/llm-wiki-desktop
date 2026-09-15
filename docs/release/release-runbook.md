# 桌面发布流程

稳定版 `app-vX.Y.Z` 与 RC `app-vX.Y.Z-rc.N` 共用 [Desktop release](../../.github/workflows/desktop-release.yml)。只有三个阶段：预检 → 四平台构建与安装启动检查 → 汇总并发布。能力资源通过独立 [Capability resources](../../.github/workflows/capability-release.yml) 构建，不与 App 发版绑定。

## 准备与触发

1. 更新 package、Cargo、Tauri 的版本和发布说明，通过源码 CI。RC 可以使用相同基础版本的源码，构建时自动覆盖 App 版本为完整 RC 版本；稳定版必须与源码版本一致。
2. 提交完整 `capabilities/install-catalog.json`，资源实际托管在其中声明的 HTTPS 地址。资源首次上传或内容修改后，运行完整检查：

   ```sh
   node scripts/verify-published-capability-assets.mjs --catalog capabilities/install-catalog.json
   ```

3. 将 tag 指向准备好的提交，推送 tag；或在 Actions 的 Desktop release 中从 `master` 手动运行，填写已有 `release_tag`。不需要历史构建 run ID、能力包签名密钥、相同版本的能力包 Release 或手动审批记录。

源码完整测试由 CI 负责，发布流程不再重复执行 `npm run check`，也不轮询其他 workflow 的状态。维护者应在源码 CI 通过后打 tag。此调整不改变既有分支保护或三平台 CI 检查名称。

## 三个阶段

- **预检**：检查版本、App 身份、updater 公钥与请求 tag 的实际提交；检查能力目录完整性，并匿名请求资源前 4 KiB，发现缺失文件、错误长度或 HTML 登录页。这里是可达性检查，不是整包哈希验收；无需在每次 App 发版时重新下载所有大模型。下载源不可达会在构建前明确失败。
- **构建**：Windows x64、macOS arm64/x64、Linux x64 使用同一份提交中的 catalog。设置 `LLM_WIKI_CAPABILITY_CATALOG_MODE=distributable`，以提交内 `capabilities/` 作为 staging 输入；Rust 构建与成品嵌入检查保证实际使用该目录。构建安装包后验证 updater 签名，并保留原生安装/启动检查。
- **发布**：读取四个平台的产物声明，汇总需要公开的文件和校验和。稳定版生成 `latest.json`；RC 附带 `.sig`，不生成稳定更新清单。最后检查远程 tag 仍对应构建提交，创建或恢复草稿，上传缺失或变更文件，再公开发布。

普通 App 构建不传递重复的 catalog artifact，不生成“OS 签名不要求”的临时证明文件，也不依赖固定附件总数。程序平台、实际文件、updater 签名和清单一致性检查仍保留。

## 重试与已发布版本

优先使用 **Re-run failed jobs**。这样可以沿用成功的原生构建及其 artifact，继续未完成的草稿上传，不必移动 tag 或重新构建所有平台。

- 草稿已有的相同文件跳过；不完整或不同文件重新上传。
- 草稿中的其他附件保留，不因多一份说明或报告而阻止发布。
- GitHub 提供 SHA-256 时直接比较；缺少摘要元数据时仅下载该文件核对实际字节，不把“没有元数据”当作损坏。
- 已公开且字节相同的发布重试直接成功，不写任何资产；已有公开文件与新构建不同则明确拒绝覆盖。完整重建可能产生不同安装包字节，不能据此重写已公开版本；代码修复使用新版本。
- 仅在最后发布稳定版时查询当前 latest。较新稳定版可以接管更新入口；较旧稳定版可以发布，但 `latest=false`。RC 永远不接管稳定更新入口。
- 权限、认证或网络错误不会被伪装成“Release 不存在”。失败保留草稿，供重试。

发布后的 CDN 清单探测仅用于提示传播问题，失败不会撤销或删除已发布版本。

## 权限、签名与网络

只有发布 job 有 `contents: write`；构建从 `desktop-release` 环境读取 updater 签名密钥。App/updater 的签名保持原合同，Windows Authenticode 和 Apple Developer ID/notarization 仍非必需。环境没有人工 reviewer；已有分支/tag 范围约束不变。

国内用户能否下载取决于实际托管位置，可使用国内 HTTPS 静态存储或完整离线资源目录；见 [能力资源分发](../../capabilities/RELEASE.md)。空目录或不存在的下载地址不能靠放宽发布检查变成可用功能。App 内置 catalog 应来自已经公开并完成下载核验的资源；发布前提交这份真实目录。

本地维护验证使用 `npm run check`、`npm run check:release-config`；已有 `actionlint` 时可检查 Actions 语法。历史验收清单与审查报告是证据，不是每次发版要重新填写的审批步骤。密钥轮换见 [发布身份与访问](release-identity-and-access.md#updater-signing-key-operations)。
