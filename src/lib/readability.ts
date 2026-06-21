import { Readability } from "@mozilla/readability";

export interface ReadabilityResult {
  title: string;
  content: string;
  textContent: string;
  excerpt: string;
  length: number;
  siteName: string | null;
  byline: string | null;
  dir: string | null;
}

export function extractArticle(doc: Document): ReadabilityResult | null {
  const reader = new Readability(doc);
  const parsed = reader.parse();
  if (!parsed) return null;

  return {
    title: parsed.title ?? "",
    content: parsed.content ?? "",
    textContent: parsed.textContent ?? "",
    excerpt: parsed.excerpt ?? "",
    length: parsed.length ?? 0,
    siteName: parsed.siteName ?? null,
    byline: parsed.byline ?? null,
    dir: parsed.dir ?? null,
  };
}

export function extractArticleFromHtml(html: string, sourceUrl?: string): ReadabilityResult | null {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, "text/html");
  if (sourceUrl) {
    const base = doc.createElement("base");
    base.href = sourceUrl;
    doc.head.prepend(base);
  }
  return extractArticle(doc);
}

function absoluteUrl(value: string, sourceUrl: string): string {
  try {
    return new URL(value, sourceUrl).toString();
  } catch {
    return value;
  }
}

function renderNode(node: Node, sourceUrl: string, listDepth = 0): string {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? "";
  if (!(node instanceof HTMLElement)) return "";

  const tag = node.tagName.toLowerCase();
  if (tag === "script" || tag === "style" || tag === "noscript") return "";
  if (tag === "br") return "\n";
  if (tag === "img") {
    const src = node.getAttribute("src");
    if (!src) return "";
    return `![${node.getAttribute("alt") ?? ""}](${absoluteUrl(src, sourceUrl)})`;
  }

  const children = Array.from(node.childNodes)
    .map((child) => renderNode(child, sourceUrl, tag === "ul" || tag === "ol" ? listDepth + 1 : listDepth))
    .join("");
  if (/^h[1-6]$/.test(tag)) return `\n\n${"#".repeat(Number(tag[1]))} ${children.trim()}\n\n`;
  if (tag === "p" || tag === "article" || tag === "section" || tag === "div") {
    return `\n\n${children.trim()}\n\n`;
  }
  if (tag === "strong" || tag === "b") return `**${children.trim()}**`;
  if (tag === "em" || tag === "i") return `*${children.trim()}*`;
  if (tag === "code") return `\`${children.trim()}\``;
  if (tag === "pre") return `\n\n\`\`\`\n${node.textContent ?? ""}\n\`\`\`\n\n`;
  if (tag === "blockquote") {
    return `\n\n${children.trim().split("\n").map((line) => `> ${line}`).join("\n")}\n\n`;
  }
  if (tag === "a") {
    const href = node.getAttribute("href");
    return href ? `[${children.trim()}](${absoluteUrl(href, sourceUrl)})` : children;
  }
  if (tag === "li") return `\n${"  ".repeat(Math.max(0, listDepth - 1))}- ${children.trim()}`;
  if (tag === "ul" || tag === "ol") return `\n${children.trim()}\n`;
  return children;
}

function yamlString(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\r?\n/g, " ")}"`;
}

export function articleToMarkdown(article: ReadabilityResult, sourceUrl: string): string {
  const doc = new DOMParser().parseFromString(`<main>${article.content}</main>`, "text/html");
  const body = renderNode(doc.body, sourceUrl)
    .replace(/[ \t]+\n/g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  const frontmatter = [
    "---",
    `title: ${yamlString(article.title || sourceUrl)}`,
    `source_url: ${yamlString(sourceUrl)}`,
    ...(article.byline ? [`author: ${yamlString(article.byline)}`] : []),
    "---",
  ];
  const heading = `# ${article.title || sourceUrl}`;
  return `${frontmatter.join("\n")}\n\n${heading}\n\n${body}\n`;
}
