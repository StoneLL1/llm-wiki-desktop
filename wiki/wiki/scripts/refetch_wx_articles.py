#!/usr/bin/env python3
"""
微信文章重抓脚本 — 用 v2 wx_extract.py 重新抓取并评估是否需要更新

策略：
  1. 用 v2 脚本重新抓取每篇文章
  2. 与旧文件对比：
     - 只替换正文内容（保留原 frontmatter 的元信息不变）
     - 新增 images 字段（如果旧文件没有）
     - 只有当新内容确实更完整（更大/有图/有代码）时才覆盖
  3. 输出 JSON 报告

用法: python3 refetch_wx_articles.py /tmp/wx_refetch_clean.json
"""
import sys
import os
import re
import json
import subprocess
import base64
import hashlib
from datetime import datetime

WIKI_RAW = os.path.expanduser("~/wiki/raw/articles")
WX_EXTRACT = os.path.expanduser("~/wiki/scripts/wx_extract.py")
PROXY = "http://127.0.0.1:7897"
UA = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/135.0.0.0 Safari/537.36"

# 每批抓取间隔（秒），避免被限流
DELAY = 2


def curl_and_extract(url):
    """用 curl 下载 + wx_extract.py 提取"""
    try:
        r = subprocess.run(
            ["curl", "-s", "-L", "--proxy", PROXY,
             "-H", f"User-Agent: {UA}",
             "--max-time", "30",
             url],
            capture_output=True, timeout=35
        )
        if r.returncode != 0 or not r.stdout:
            return None, f"curl failed (rc={r.returncode})"

        # pipe 给 wx_extract.py
        r2 = subprocess.run(
            ["python3", WX_EXTRACT],
            input=r.stdout, capture_output=True, timeout=15
        )
        if r2.returncode != 0:
            return None, f"extract failed: {r2.stderr.decode('utf-8', errors='replace')[:200]}"

        data = json.loads(r2.stdout.decode('utf-8'))
        return data, None
    except subprocess.TimeoutExpired:
        return None, "timeout"
    except Exception as e:
        return None, str(e)


def read_existing(fpath):
    """读取已有文件，返回 (frontmatter_str, body_str, full_content)"""
    with open(fpath, 'r', encoding='utf-8') as f:
        content = f.read()

    fm_match = re.match(r'^---\s*\n(.*?)\n---\s*\n', content, re.DOTALL)
    if fm_match:
        frontmatter = fm_match.group(1)
        body = content[fm_match.end():]
        return frontmatter, body, content
    return "", content, content


def update_frontmatter(old_fm, new_data):
    """更新 frontmatter：添加 images 和 images_count，保留其他字段不变"""
    lines = old_fm.split('\n')
    
    # 移除旧的 images 和 image_count 字段（如果有）
    lines = [l for l in lines if not l.startswith('images:') and not l.startswith('image_count:')]
    
    # 添加新的 images 和 image_count
    if new_data.get('images'):
        lines.append(f"image_count: {len(new_data['images'])}")
    
    return '\n'.join(lines)


def should_replace(old_body, old_size, new_content, new_data):
    """
    判断是否应该替换：
    - 新内容比旧内容更长（说明抓到更多东西了）
    - 或者新内容有图/代码而旧的没有
    """
    old_img = len(re.findall(r'!\[.*?\]\(.*?\)', old_body))
    old_code = len(re.findall(r'```[\s\S]*?```', old_body))
    new_img = len(re.findall(r'!\[.*?\]\(.*?\)', new_content))
    new_code = len(re.findall(r'```[\s\S]*?```', new_content))
    new_len = len(new_content)
    
    # 如果新内容有更多图片或代码块，或内容更长，则替换
    improved = (new_img > old_img) or (new_code > old_code) or (new_len > old_size * 1.05)
    
    return improved, {
        "old_size": old_size,
        "new_size": new_len,
        "old_imgs": old_img,
        "new_imgs": new_img,
        "old_code": old_code,
        "new_code": new_code,
    }


def main():
    list_file = sys.argv[1]
    with open(list_file) as f:
        articles = json.load(f)

    print(f"[REFETCH] Starting: {len(articles)} articles to evaluate")
    print(f"[REFETCH] Started at: {datetime.now().isoformat()}")

    results = {"replaced": [], "unchanged": [], "failed": []}

    for i, article in enumerate(articles):
        fname = article['file']
        url = article['url']
        fpath = os.path.join(WIKI_RAW, fname)

        print(f"\n[{i+1}/{len(articles)}] {fname}")
        print(f"  URL: {url[:80]}...")

        # 读取旧文件
        if not os.path.exists(fpath):
            print(f"  SKIP: file not found")
            results["failed"].append({"file": fname, "error": "file_not_found"})
            continue

        old_fm, old_body, old_full = read_existing(fpath)
        old_size = len(old_body)

        # 抓取新版本
        new_data, err = curl_and_extract(url)
        if err:
            print(f"  FAIL: {err}")
            results["failed"].append({"file": fname, "url": url, "error": err})
            continue

        new_content = base64.b64decode(new_data['content_b64']).decode('utf-8')
        new_title = new_data.get('title', '')

        # 判断是否需要替换
        should, diff = should_replace(old_body, old_size, new_content, new_data)

        print(f"  Old: {diff['old_size']}b, {diff['old_imgs']} imgs, {diff['old_code']} code")
        print(f"  New: {diff['new_size']}b, {diff['new_imgs']} imgs, {diff['new_code']} code")

        if not should:
            print(f"  KEEP: old version is sufficient")
            results["unchanged"].append({"file": fname, **diff})
            continue

        # 替换：保留旧 frontmatter，只更新正文
        updated_fm = update_frontmatter(old_fm, new_data)
        
        # 更新 fetched 日期
        today = datetime.now().strftime('%Y-%m-%d')
        updated_fm = re.sub(r'fetched:\s*\S+', f'fetched: {today}', updated_fm)
        
        # 更新 sha256
        new_sha = hashlib.sha256(new_content.encode('utf-8')).hexdigest()[:16]
        if 'sha256:' in updated_fm:
            updated_fm = re.sub(r'sha256:\s*\S+', f'sha256: {new_sha}', updated_fm)
        else:
            updated_fm += f'\nsha256: {new_sha}'

        new_file = f"---\n{updated_fm}\n---\n\n{new_content}\n"

        # 写入
        with open(fpath, 'w', encoding='utf-8') as f:
            f.write(new_file)

        # 验证
        verified = os.path.exists(fpath) and os.path.getsize(fpath) > 0

        print(f"  REPLACE: {diff['old_size']}b -> {diff['new_size']}b (imgs {diff['old_imgs']}->{diff['new_imgs']}, code {diff['old_code']}->{diff['new_code']}) verified={verified}")

        results["replaced"].append({
            "file": fname,
            "url": url,
            **diff,
            "verified": verified,
        })

        # 限流
        import time
        time.sleep(DELAY)

    # 输出报告
    print(f"\n{'='*60}")
    print(f"[REFETCH] Summary:")
    print(f"  Replaced: {len(results['replaced'])}")
    print(f"  Unchanged: {len(results['unchanged'])}")
    print(f"  Failed: {len(results['failed'])}")
    print(f"  Finished at: {datetime.now().isoformat()}")

    report_path = "/tmp/wx_refetch_report.json"
    with open(report_path, 'w', encoding='utf-8') as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    print(f"  Report saved to: {report_path}")


if __name__ == "__main__":
    main()
