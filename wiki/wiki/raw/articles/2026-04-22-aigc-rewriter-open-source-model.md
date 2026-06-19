---
title: "格式工坊：AIGC降重降Ai率模型开源！"
url: "https://mp.weixin.qq.com/s/57-G9vMO3gDJ0UsDnMFtYw"
source: "微信公众号"
author: "Dong"
fetched: 2026-04-22
sha256: ea5ad60cc79c66f8
---

# 格式工坊：AIGC降重降Ai率模型开源！

**作者**: Dong

是的，我们开源了，好的技术与知识不该被束之高阁，有价值的知识与模型，更应开放给同学们这是我们开源的第一个AIGC降重的模型（两分钟上手）（后续会有更多模型免费开源给大家）

## 一.模型下载

- 国内夸克：https://pan.quark.cn/s/8c52c120a495
- github项目地址：https://github.com/h5box/aigc-rewriter

## 二.核心优势

1. 可实现对文本的AI味移除
2. 模型轻量，本地运行，适配各种win系统的电脑，不需要多高配置
3. 开源免费支持二次微调

## 三.使用教程及注意事项

为了让新手小白同学可以轻松上手，我们将本地部署环境提前打包好了

1. 下载压缩包并解压
2. 点击启动.bat（一定记得右键用管理员身份运行这个是自动配置环境模型环境加载完成）
3. 浏览器地址栏输入 http://127.0.0.1:8181
4. 将需要改写的文本粘贴，并点击改写即可

免费AIGC 检测地址：https://www.geshigongfang.com/aigc-check

## 故障排查

1. 提示缺少 llama-server.exe The prompt is missing "<". 运行时目录不完整。按"安装与补齐运行环境"补齐 llama-b8721-bin-win-vulkan-x64。
2. 提示缺少模型文件确认根目录存在 qwen3-merged-aigc_zhv3-Q4_K_M.gguf，且文件名与脚本一致。
3. 端口 8181 被占用先查占用进程：`Get-NetTCPConnection -LocalPort 8181 -State Listen | Select-Object LocalAddress,LocalPort,OwningProcess` 关闭占用后重试，或修改启动脚本中的端口参数。
4. Windows 安全策略拦截可执行文件若被 SmartScreen/Defender 拦截，请在你信任来源前提下放行后再运行。
5. Vulkan 环境异常改用 CPU 版预编译包进行验证，确认链路通后再切回 Vulkan 版本。
