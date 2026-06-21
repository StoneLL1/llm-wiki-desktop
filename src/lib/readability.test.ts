import { describe, expect, it } from "vitest";
import { articleToMarkdown, extractArticleFromHtml } from "./readability";

describe("URL article import", () => {
  it("converts Readability content to Markdown with source metadata and images", () => {
    const html = `<!doctype html><html><head><title>Test article</title></head><body>
      <article><h1>Test article</h1><p>Hello <a href="/more">world</a>.</p>
      <img src="/cover.png" alt="Cover" /></article></body></html>`;
    const article = extractArticleFromHtml(html, "https://example.com/posts/one");

    expect(article).not.toBeNull();
    const markdown = articleToMarkdown(article!, "https://example.com/posts/one");
    expect(markdown).toContain("source_url: \"https://example.com/posts/one\"");
    expect(markdown).toContain("# Test article");
    expect(markdown).toContain("[world](https://example.com/more)");
    expect(markdown).toContain("![Cover](https://example.com/cover.png)");
  });
});
