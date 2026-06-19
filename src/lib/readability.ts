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

export function extractArticleFromHtml(html: string): ReadabilityResult | null {
  const parser = new DOMParser();
  const doc = parser.parseFromString(html, "text/html");
  return extractArticle(doc);
}
