import { Readability } from "@mozilla/readability";

export function extractArticle(document, platform) {
  if (!["generic", "wechat", "zhihu"].includes(platform)) return null;
  const copy = document.cloneNode(true);
  const title = platform === "wechat"
    ? copy.querySelector("#activity-name")?.textContent?.trim()
    : platform === "zhihu" ? copy.querySelector(".Post-Title,.QuestionHeader-title,h1")?.textContent?.trim() : null;
  const byline = copy.querySelector(platform === "wechat" ? "#js_name" : ".AuthorInfo-name")?.textContent?.trim();
  copy.querySelectorAll("script,style,nav,header,footer,aside,form,button,template,noscript,[role=navigation]")
    .forEach((node) => node.remove());
  const selector = platform === "wechat" ? "#js_content"
    : platform === "zhihu" ? ".RichContent-inner,.Post-RichText,.RichText"
      : 'article,main,[itemprop="articleBody"]';
  const root = copy.querySelector(selector);
  const hasBodyText = (element) => {
    const body = element?.cloneNode(true);
    body?.querySelectorAll("h1,h2,h3,h4,h5,h6").forEach((node) => node.remove());
    return Boolean(body?.textContent?.trim());
  };
  if (hasBodyText(root)) {
    return {
      title: title || copy.querySelector('meta[property="og:title"]')?.getAttribute("content") || document.title,
      byline: byline || null,
      content: root.innerHTML,
      textContent: root.textContent.trim(),
    };
  }
  // Platform shells must not become article prose through Readability's body
  // fallback. Generic pages can use Readability after navigation is removed.
  if (platform !== "generic" || root || !hasBodyText(copy.body)) return null;
  const article = new Readability(copy).parse();
  return article?.textContent?.trim() ? article : null;
}
