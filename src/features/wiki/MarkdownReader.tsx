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
const CITATION_SCHEME = "citation://";

interface FrontmatterRow {
  key: string;
  value: string;
}

/**
 * Parse frontmatter for presentation without mutating or re-serializing YAML.
 * The backend remains the source of truth for the raw bytes. This intentionally
 * small parser handles the flat metadata shape used by Wiki pages and keeps
 * nested/unknown syntax visible as continuation text instead of dropping it.
 */
function parseFrontmatterRows(yaml: string): FrontmatterRow[] {
  const rows: FrontmatterRow[] = [];

  for (const rawLine of yaml.split(/\r?\n/)) {
    if (rawLine.trim() === "" || rawLine.trim() === "---") continue;

    const field = rawLine.match(/^([^\s:#][^:]*):(?:\s*(.*))?$/);
    if (field) {
      rows.push({ key: field[1].trim(), value: field[2] ?? "" });
      continue;
    }

    if (/^\s+/.test(rawLine) && rows.length > 0) {
      const last = rows[rows.length - 1];
      last.value = [last.value, rawLine.trim()].filter(Boolean).join(" · ");
      continue;
    }

    rows.push({ key: "", value: rawLine.trim() });
  }

  return rows;
}

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

/** Convert simple numeric source markers (`[1]`) into reader-owned anchors. */
function preprocessCitations(body: string): string {
  return body
    .replace(/\[\^(\d+)\](?!\s*:)/g, (_match, index: string) => {
      return `[${index}](${CITATION_SCHEME}${index})`;
    })
    .replace(/(?<!\])\[(\d+)\](?!\s*(?:\(|:))/g, (_match, index: string) => {
      return `[${index}](${CITATION_SCHEME}${index})`;
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
  const processed = useMemo(
    () => preprocessCitations(preprocessWikilinks(bodyMarkdown)),
    [bodyMarkdown],
  );
  const frontmatterRows = useMemo(
    () => (frontmatterYaml ? parseFrontmatterRows(frontmatterYaml) : []),
    [frontmatterYaml],
  );

  return (
    <article className="wiki-prose" role="article">
      {frontmatterRows.length > 0 ? (
        <div className="frontmatter">
          {frontmatterRows.map((row, index) => (
            <div className="frontmatter__row" key={`${row.key}-${index}`}>
              <span className="frontmatter__k">{row.key ? `${row.key}:` : ""}</span>
              <span className="frontmatter__v">{row.value}</span>
            </div>
          ))}
        </div>
      ) : null}
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex, rehypeHighlight]}
        urlTransform={(url) =>
          url.startsWith(WIKILINK_SCHEME) || url.startsWith(CITATION_SCHEME)
            ? url
            : defaultUrlTransform(url)
        }
        components={{
          a({ href, children, ...props }) {
            if (href?.startsWith(CITATION_SCHEME)) {
              const index = href.slice(CITATION_SCHEME.length);
              return (
                <a
                  href={`#citation-${index}`}
                  className="citation-ref"
                  aria-label={t("wiki.reader.citation", { index })}
                  onClick={(event) => {
                    event.preventDefault();
                    document.getElementById(`citation-${index}`)?.scrollIntoView({
                      behavior: "smooth",
                      block: "center",
                    });
                  }}
                >
                  {index}
                </a>
              );
            }
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
