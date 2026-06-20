import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown, { defaultUrlTransform } from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

import type { WikiPageMeta } from "../../types/wiki";

interface MarkdownReaderProps {
  bodyMarkdown: string;
  frontmatterYaml: string | null;
  pages: WikiPageMeta[];
  onOpenPage: (path: string) => void;
}

const WIKILINK_SCHEME = "wikilink://";

/** Build a case-insensitive wikilink target → page path index. */
function buildResolver(pages: WikiPageMeta[]): Map<string, string> {
  const index = new Map<string, string>();
  for (const page of pages) {
    const stem = page.path.split("/").pop()?.replace(/\.md$/i, "").toLowerCase();
    if (stem) index.set(stem, page.path);
    if (page.title) {
      index.set(page.title.toLowerCase(), page.path);
    }
    for (const alias of page.aliases) {
      if (alias) index.set(alias.toLowerCase(), page.path);
    }
  }
  return index;
}

/** Rewrite `[[Target]]` / `[[Target|Alias]]` into markdown links the reader can intercept. */
function preprocessWikilinks(body: string): string {
  return body.replace(/\[\[([^\]]+)\]\]/g, (match, inner: string) => {
    const [targetRaw, aliasRaw] = inner.split("|");
    const target = (targetRaw ?? "").split("#")[0].trim();
    const alias = (aliasRaw ?? target).split("#")[0].trim();
    if (!target) return match;
    return `[${alias}](${WIKILINK_SCHEME}${encodeURIComponent(target)})`;
  });
}

export function MarkdownReader({
  bodyMarkdown,
  frontmatterYaml,
  pages,
  onOpenPage,
}: MarkdownReaderProps) {
  const { t } = useTranslation();
  const resolver = useMemo(() => buildResolver(pages), [pages]);
  const processed = useMemo(() => preprocessWikilinks(bodyMarkdown), [bodyMarkdown]);

  return (
    <article className="wiki-prose">
      {frontmatterYaml ? (
        <pre className="wiki-frontmatter">{frontmatterYaml}</pre>
      ) : null}
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex, rehypeHighlight]}
        urlTransform={(url) => (url.startsWith(WIKILINK_SCHEME) ? url : defaultUrlTransform(url))}
        components={{
          a({ href, children, ...props }) {
            if (href && href.startsWith(WIKILINK_SCHEME)) {
              const target = decodeURIComponent(href.slice(WIKILINK_SCHEME.length));
              const resolved = resolver.get(target.toLowerCase());
              return (
                <a
                  href="#"
                  onClick={(event) => {
                    event.preventDefault();
                    if (resolved) onOpenPage(resolved);
                  }}
                  className={resolved ? "wikilink" : "wikilink wikilink--missing"}
                  title={resolved ?? t("wiki.reader.missingLink")}
                >
                  {children}
                </a>
              );
            }
            return (
              <a href={href} target="_blank" rel="noreferrer" {...props}>
                {children}
              </a>
            );
          },
        }}
      >
        {processed}
      </ReactMarkdown>
    </article>
  );
}
