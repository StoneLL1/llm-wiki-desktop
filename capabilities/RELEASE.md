# 能力资源构建与分发

当前合同是 [ADR 0003：按需下载程序，独立管理模型](../docs/architecture/decisions/0003-on-demand-capability-resources.md)。所有可选外部程序按需安装，不随桌面 App 打包。程序和模型可以放在维护者提供的 HTTPS 静态存储、国内对象存储/CDN 或 GitHub Release；资源版本与桌面版本、GitHub tag、Actions run 无关。

`capabilities/` 中的源码、依赖声明与模板不是可安装资源。用户安装的是应用内置 `install-catalog.json` 指定的程序 ZIP，以及可选的独立模型文件。

## 构建资源

使用独立的 [Capability resources 工作流](../.github/workflows/capability-release.yml)，通过 `workflow_dispatch` 指定：

- `base_url`：程序 ZIP 将被实际托管的 HTTPS 目录，必填。
- `model_base_url`：模型文件的 HTTPS 根目录，可选，默认与程序相同。不同程序版本、平台可以共用同一个模型存储目录。程序内容改变时分配新的资源版本，避免旧安装目录与新字节冲突。
- `publish_github`：默认关闭。开启后在 GitHub runner 上直接发布资源，避免维护者先下载完整资源再上传。此时 `base_url` 必须是本仓库 `https://github.com/OWNER/REPO/releases/download/capabilities-TAG/` 地址，例如 `capabilities-2026-09-14`；模型也转换成同一 Release 的扁平附件，`model_base_url` 不作为最终下载地址。

程序目录应使用新的资源目录名，避免用新字节覆盖旧 catalog 引用的同名文件。模型目录按内容摘要组织，可以长期复用。地址应由维护者提供并维护。仅填写 URL 不会使资源自动公开；GitHub 自动发布需明确开启 `publish_github`。

工作流从 [product-manifest.json](product-manifest.json) 推导每个已发布能力支持的平台，结合 [release-recipes.json](release-recipes.json)、[release-sources.json](release-sources.json) 与 [qualification-corpus.json](qualification-corpus.json) 构建资源。不能用“能力数 × 平台数”的固定乘积代替实际矩阵；部分能力只支持平台子集。

每个平台在对应原生 runner 上执行：

1. `prepare-release-capability.mjs` 下载并检查锁定的依赖，准备程序、runner、许可与构建证据。
2. `capability_release assemble` 生成程序归档及 catalog 片段；新资源无需能力包签名密钥。
3. `split-capability-models.py` 把模型从 ZIP 移出，重新计算程序归档大小和 SHA-256，并写入 `modelFiles`。
4. `capability_release verify-install` 用 App 的真实本地安装器将最终 ZIP 和独立模型安装到新目录，执行路线自检、启用和重启加载检查。
5. 在最终安装目录运行完整路线与格式 qualification，验收用户将收到的文件。

浏览器资源必须通过实际 Chromium 启动及本地行为检查。X/微信的在线样本检查仅在仓库配置对应 `X_PRODUCTION_SAMPLE_URL` / `WECHAT_PRODUCTION_SAMPLE_URL` 时运行；未配置会明确报告在线访问未验证，不会因缺少外部网页样本阻止程序构建。

单项构建产物为 `resource-<capabilityId>-<targetTriple>`。最终 `capability-resources` 产物包含完整程序 ZIP、共享的 `models/` 目录和 `install-catalog.json`。默认只保存这些产物，供 HTTPS 托管或离线分发；工作流不会修改仓库 catalog。

开启 `publish_github` 后，合并 job 还会转换扁平附件、创建独立资源 tag 和草稿，并逐项上传校验。上传中断时重跑失败 job 会复用已上传的同字节附件；草稿中的不完整或变化附件可替换，已公开附件则必须全部匹配，绝不覆盖。上传完整后公开为 prerelease，始终 `latest=false`，不影响 App 更新。随后匿名下载 catalog 中的全部程序和独立模型，逐个校验完整大小和 SHA-256；通过后输出小体积 `public-capability-catalog` 产物。将这里的 catalog 提交到仓库即可准备 App，无需下载全部资源，也不需要绑定 Actions run。

维护者也可以使用同一套命令在相应平台运行。拆分已完成 qualification 的旧完整 ZIP 时，调用方式为：

```sh
python3 scripts/split-capability-models.py \
  --catalog INPUT/install-catalog.json \
  --archives INPUT \
  --output OUTPUT \
  --base-url "$RESOURCE_BASE_URL" \
  --model-base-url "$MODEL_BASE_URL"
```

`OUTPUT` 必须是新目录。两个环境变量需填写实际托管地址；没有单独模型地址时，省略 `--model-base-url`。拆分会检查原 ZIP 摘要，保持引擎既有 `models/` 路径，并重新生成归档摘要，不能继续使用旧 catalog 的 ZIP 哈希。

## 托管资源并发布 App

1. 将产物中的程序 ZIP 与 `models/` 按 catalog URL 所声明的目录结构上传至实际托管位置。完整离线分发则保留相同目录结构。
2. 将完整、经过检查的 `install-catalog.json` 更新到仓库 [install-catalog.json](install-catalog.json)。保留 [trusted-keys.json](trusted-keys.json) 以读取历史已签名包；新安装不依赖其中必须存在密钥。
3. 运行 catalog 校验与实际下载检查：

```sh
node scripts/verify-capability-catalog.mjs \
  --catalog capabilities/install-catalog.json \
  --trusted-keys capabilities/trusted-keys.json --mode release
node scripts/verify-published-capability-assets.mjs \
  --catalog capabilities/install-catalog.json
```

GitHub Release 的附件没有子目录。选择 GitHub 托管时，先用 `stage-capability-downloads.mjs` 把构建目录转换为真实附件布局：

```sh
node scripts/stage-capability-downloads.mjs \
  --input capability-resources --output public-capability-resources \
  --base-url "https://github.com/OWNER/REPO/releases/download/RESOURCE_TAG/"
```

手动托管时上传输出目录的所有文件，并提交其中生成的 catalog；工作流启用 `publish_github` 会自动执行转换与发布。该工具核对本地原始文件摘要，把在线模型地址改为不重名的扁平附件地址，同时提供保留目录结构、可重复生成的 `models.zip`。离线用户下载所需程序 ZIP，把 `models.zip` 解压到旁边，即可选择程序 ZIP 安装。任何附件达到 2 GiB 会明确拒绝，需改用目录托管。转换本身不创建 Release，也不代表国内网络一定可达；有实际国内存储时可直接托管原构建目录。

实际下载检查不携带登录 token，会从 catalog 地址读取文件并验证大小和 SHA-256。模型存在多个候选 URL 时，至少一个候选需返回正确文件。仅有 Actions artifact、未公开的 Release 或对象存储管理后台记录，不能证明用户可以下载。

4. 稳定版与 RC 共用 Desktop release 工作流。预检校验完整 catalog 并请求公开资源前 4 KiB，检查实际可达性与声明大小，避免每次 App 发版重新下载全部模型。构建使用同一提交的 `capabilities/` 作为 `LLM_WIKI_CAPABILITY_STAGING_DIR`，设置 `LLM_WIKI_CAPABILITY_CATALOG_MODE=distributable`，并检查成品二进制包含相同 catalog 字节。完整 SHA 验收仍用于本节的资源首次上传/变更，用户安装时也仍检查 SHA。详见 [桌面发布流程](../docs/release/release-runbook.md)。

桌面发布不重建能力程序，不要求历史资源构建 run、同版本资源 tag、catalog provenance 或能力包签名 secret。App 本身及 updater 的签名流程不变。资源 URL/摘要变更必须更新内置 catalog 并重新构建 App；单独上传资源不会更新已经发布的客户端。

开发构建可以使用空 source catalog；稳定版和 RC 都不允许空或缺项 catalog。当前仓库 catalog 是否具备发布条件，应以上面的 `--mode release` 检查结果为准。

## 用户安装与使用

新安装流程是：下载或选择本地 ZIP → 流式验证归档 SHA-256 → 安全解压 → 准备独立模型 → 一次运行时自检 → 自动注册已知路线。安装器仍拒绝越界路径、链接、特殊文件和超出大小限制的归档；失败或取消保留此前可用版本。

程序归档由可信 App 内置 catalog 的 SHA-256 确认，不再强制第二套 Ed25519 签名。安装器保存 `.installation.json`，记录归档和 manifest 摘要。日常使用读取 manifest、检查协议/平台/入口和必要文件，不重复扫描整个程序或模型计算哈希。未带安装记录的旧包仍可通过历史签名路径读取；旧完整 ZIP 也继续支持。

独立模型由 catalog 的 `modelFiles` 声明 `path`、`bytes`、`sha256` 和候选 `urls`。首次下载或离线导入校验摘要，之后直接使用已安装数据；程序运行时不自动联网补模型。模型路径保持在程序目录的 `models/` 下，底层可共享缓存文件。当前是清单固定的官方模型，不是任意模型架构或可执行插件导入。

离线用户选择程序 ZIP，将模型放在 ZIP 同目录的 `models/<sha256>/<filename>`；兼容原始的 `models/` 相对路径。不要只分发拆分后的 ZIP 而遗漏模型。离线安装缺少模型时会报告缺少文件，不会偷偷切换到在线下载。

国内可达性需要实际维护的国内源，或提供完整离线产物。取消 GitHub 限制不会自动修复 v0.2.1 二进制里已经写入的失效地址。Linux 程序依赖的宿主系统动态库也仍需满足各资源构建记录的运行条件；“按需包”不表示所有平台都是完全静态程序。
