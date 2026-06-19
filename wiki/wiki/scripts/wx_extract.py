#!/usr/bin/env python3
"""
微信公众号文章提取工具 v2 (curl + readability + html2text)

用法: curl -s -L URL | python3 wx_extract.py
输出: JSON {title, account, author, pub_date, url, images, content_b64, content_len}

v2 改进:
  1. 图片保留：微信 img 只有 data-src（懒加载），html2text 只认 src →
     readability 之后注入独立 src 属性
  2. 代码块保留：微信 <pre><code> 无 lang class → 注入 class 让 html2text 生成 ```fenced```
  3. 图片元信息：单独输出 images 数组（url + alt），方便后续下载
  4. JS 残留清理：移除 <script> 标签避免模板字符串污染
"""
import sys
import re
import json
import datetime
import base64

from readability import Document
import html2text


# ---------------------------------------------------------------------------
# 元信息提取
# ---------------------------------------------------------------------------

def extract_meta(html):
    """从微信 HTML 提取元信息"""
    meta = {}

    # title: 优先 og:title
    m = re.search(r'property="og:title"\s+content="([^"]*)"', html)
    if m:
        meta["title"] = m.group(1).strip()
    else:
        m = re.search(r'<h1[^>]*class="rich_media_title"[^>]*>.*?<span[^>]*>(.*?)</span>', html, re.DOTALL)
        if m:
            meta["title"] = re.sub(r'<[^>]+>', '', m.group(1)).strip()

    # account: data-nickname 属性
    m = re.search(r'data-nickname="([^"]*?)"', html)
    meta["account"] = m.group(1).strip() if m else ""

    # author: meta author
    m = re.search(r'name="author"\s+content="([^"]*)"', html)
    meta["author"] = m.group(1).strip() if m else ""

    # publish_time: 优先 var ct（时间戳），其次 publish_time 元素
    m = re.search(r'var\s+ct\s*=\s*"(\d+)"', html)
    if m and m.group(1).isdigit():
        meta["pub_date"] = datetime.datetime.fromtimestamp(int(m.group(1))).strftime("%Y-%m-%d")
    else:
        m = re.search(r'id="publish_time"[^>]*>(.*?)</em>', html)
        if m and m.group(1).strip():
            meta["pub_date"] = m.group(1).strip()
        else:
            meta["pub_date"] = ""

    # source URL
    m = re.search(r'property="og:url"\s+content="([^"]*)"', html)
    meta["url"] = m.group(1).strip() if m else ""

    return meta


# ---------------------------------------------------------------------------
# 收集图片信息（从原始 HTML）
# ---------------------------------------------------------------------------

def collect_images(html):
    """从原始 HTML 收集所有图片的 URL 和 alt 文本"""
    images = []
    seen = set()
    for m in re.finditer(r'<img\b[^>]*>', html):
        tag = m.group(0)
        ds = re.search(r'data-src="(https?://[^"]+)"', tag)
        src = re.search(r'(?<![a-z-])src="(https?://[^"]+)"', tag)
        alt = re.search(r'alt="([^"]*)"', tag)

        # 优先 data-src（完整 URL），其次独立 src
        url = (ds.group(1) if ds else None) or (src.group(1) if src else None)
        if url and url not in seen and 'mmbiz' in url:
            seen.add(url)
            images.append({"url": url, "alt": alt.group(1) if alt else ""})
    return images


# ---------------------------------------------------------------------------
# readability 之后的 HTML 修复
# ---------------------------------------------------------------------------

def fix_summary_for_html2text(summary_html):
    """
    readability 提取后的 HTML 需要修复：
    1. 微信 img 只有 data-src，html2text 只认独立的 src → 注入 src 属性
    2. 微信 <pre><code> 无 class → 注入 language class 生成 fenced code block

    关键陷阱：data-src 中的子串 "src" 不能被误判为独立 src 属性。
    用 negative lookbehind (?<![a-z-]) 确保匹配的是独立的 src。
    """
    # 修复图片：在 img tag 末尾注入独立 src 属性
    def fix_img(m):
        tag = m.group(0)
        ds = re.search(r'data-src="(https?://[^"]+)"', tag)
        # 用 negative lookbehind 排除 data-src 中的 src
        has_standalone_src = bool(re.search(r'(?<![a-z-])src="(https?://[^"]+)"', tag))
        if ds and not has_standalone_src:
            # 在闭合 > 前插入独立 src 属性
            return tag[:-1] + ' src="' + ds.group(1) + '">'
        return tag

    summary_html = re.sub(r'<img\b[^>]*>', fix_img, summary_html)

    # 修复代码块：给 <pre><code> 注入 class
    summary_html = re.sub(
        r'<pre><code>',
        '<pre><code class="language-bash">',
        summary_html
    )

    return summary_html


# ---------------------------------------------------------------------------
# 内容清理
# ---------------------------------------------------------------------------

def clean_content(markdown):
    """清理微信页面噪音 + 格式修复"""
    # 1. 将 html2text 的 [code]...[/code] 标记转为 ```fenced code blocks```
    #    html2text mark_code=True 输出格式:
    #      有语言时: "bash\n[code]\n...\n[/code]"
    #      无语言时: "[code]\n...\n[/code]"
    #    都要变成: "```bash\n...\n```" 或 "```\n...\n```"
    markdown = re.sub(
        r'^(?:(\w+)\s*\n)?\[code\]\s*\n(.*?)\n\[/code\]',
        lambda m: '```' + (m.group(1) or '') + '\n' + m.group(2) + '\n```',
        markdown,
        flags=re.MULTILINE | re.DOTALL,
    )

    # 2. 移除微信页面噪音
    noise = [
        r'\*?\s*戳上方蓝字.*?关注我\s*\*?',
        r'点击.*?关注.*',
        r'扫码.*?加入.*?群\s*',
        r'获得更多技术支持.*',
        r'关注「.*?」公众号\s*',
        r'与AI时代更靠近一点\s*',
        r'分享有价值的开源项目.*',
        r'^\s*\*?\s*\d+篇原创内容\s*\*?\s*$',
    ]
    for pat in noise:
        markdown = re.sub(pat, '', markdown, flags=re.MULTILINE)

    # 3. 压缩多余空行
    markdown = re.sub(r'\n{3,}', '\n\n', markdown)
    return markdown.strip()


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------

def extract_article(html):
    """从微信 HTML 提取结构化文章内容"""
    meta = extract_meta(html)
    images = collect_images(html)

    # 清理 <script> 标签
    html_clean = re.sub(r'<script[^>]*>.*?</script>', '', html, flags=re.DOTALL)

    # readability 提取正文
    doc = Document(html_clean)
    summary_html = doc.summary()

    # 修复 readability 输出中的图片和代码块
    summary_html = fix_summary_for_html2text(summary_html)

    # html2text 转 Markdown
    h = html2text.HTML2Text()
    h.ignore_links = False
    h.body_width = 0
    h.protect_links = True
    h.ignore_images = False    # 保留图片
    h.images_to_alt = False    # 保留图片 URL
    h.mark_code = True         # 标记代码块

    markdown = h.handle(summary_html)
    markdown = clean_content(markdown)

    return {
        "title": meta.get("title", ""),
        "account": meta.get("account", ""),
        "author": meta.get("author", ""),
        "pub_date": meta.get("pub_date", ""),
        "url": meta.get("url", ""),
        "images": images,
        "image_count": len(images),
        "content": markdown,
    }


def main():
    html = sys.stdin.read()
    if not html:
        print(json.dumps({"error": "empty input"}, ensure_ascii=False))
        sys.exit(1)

    result = extract_article(html)
    content_b64 = base64.b64encode(result["content"].encode("utf-8")).decode("ascii")

    output = {
        "title": result["title"],
        "account": result["account"],
        "author": result["author"],
        "pub_date": result["pub_date"],
        "url": result["url"],
        "images": result["images"],
        "image_count": result["image_count"],
        "content_b64": content_b64,
        "content_len": len(result["content"]),
    }
    print(json.dumps(output, ensure_ascii=False))


if __name__ == "__main__":
    main()
