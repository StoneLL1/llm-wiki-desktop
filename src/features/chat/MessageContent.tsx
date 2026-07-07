import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

const CITATION_SCHEME = "citation://";

/**
 * Rewrite bare source markers (`[S1]`, `[S1, S2]`, legacy `[1]`) into clickable
 * citation references. Footnote-style `[^1]` references are left untouched.
 */
function preprocessCitations(body: string): string {
  return body
    .replace(/\[(S\d+(?:\s*,\s*S\d+)*)\]/gi, (_match, marker: string) => {
      return marker
        .split(",")
        .map((part) => {
          const id = part.trim().toUpperCase();
          return `[${id}](${CITATION_SCHEME}${id})`;
        })
        .join(", ");
    })
    .replace(/(?<!\])\[(\d+)\](?!\s*(?:\(|:))/g, (_match, index: string) => {
      return `[${index}](${CITATION_SCHEME}${index})`;
    });
}

interface MessageContentProps {
  /** Raw markdown body. */
  content: string;
  /** Citation count, used to clamp reference indices to valid citations. */
  citationCount: number;
  /** Valid model source ids (`S1`, `S2`, ...), used for `[S#]` markers. */
  citationIds?: string[];
  /** Jump to a citation by model source id or legacy 1-based index. */
  onCitationClick: (ref: string) => void;
}

/**
 * Renders a chat message body as GitHub-flavored markdown with math/code
 * highlighting, and turns `[N]` markers into clickable citation references.
 */
export function MessageContent({
  content,
  citationCount,
  citationIds = [],
  onCitationClick,
}: MessageContentProps) {
  const { t } = useTranslation();
  const processed = useMemo(() => preprocessCitations(content), [content]);
  const normalizedCitationIds = useMemo(
    () => new Set(citationIds.map((id) => id.toUpperCase())),
    [citationIds],
  );

  return (
    <div className="msg__content chat-prose">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex, rehypeHighlight]}
        components={{
          a({ href, children, ...props }) {
            if (href?.startsWith(CITATION_SCHEME)) {
              const ref = decodeURIComponent(href.slice(CITATION_SCHEME.length)).toUpperCase();
              const index = Number.parseInt(ref, 10);
              const isKnownSource = normalizedCitationIds.has(ref);
              const isLegacyIndex = Number.isFinite(index) && index >= 1 && index <= citationCount;
              if (isKnownSource || isLegacyIndex) {
                return (
                  <sup className="citation-ref">
                    <button
                      type="button"
                      className="citation-ref__btn"
                      aria-label={t("chat.thread.citationRef", { index: ref })}
                      onClick={(event) => {
                        event.preventDefault();
                        onCitationClick(ref);
                      }}
                    >
                      {children}
                    </button>
                  </sup>
                );
              }
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
    </div>
  );
}
