import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown from "react-markdown";
import rehypeHighlight from "rehype-highlight";
import rehypeKatex from "rehype-katex";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";

const CITATION_SCHEME = "citation://";

/**
 * Rewrite bare numeric source markers (`[1]`) into clickable citation
 * references, mirroring the Wiki reader so chat answers stay consistent.
 * Footnote-style `[^1]` references are left untouched (no definitions in chat).
 */
function preprocessCitations(body: string): string {
  return body.replace(/(?<!\])\[(\d+)\](?!\s*(?:\(|:))/g, (_match, index: string) => {
    return `[${index}](${CITATION_SCHEME}${index})`;
  });
}

interface MessageContentProps {
  /** Raw markdown body. */
  content: string;
  /** Citation count, used to clamp reference indices to valid citations. */
  citationCount: number;
  /** Jump to a citation by its 1-based index (opens the wiki page). */
  onCitationClick: (index: number) => void;
}

/**
 * Renders a chat message body as GitHub-flavored markdown with math/code
 * highlighting, and turns `[N]` markers into clickable citation references.
 */
export function MessageContent({ content, citationCount, onCitationClick }: MessageContentProps) {
  const { t } = useTranslation();
  const processed = useMemo(() => preprocessCitations(content), [content]);

  return (
    <div className="msg__content chat-prose">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex, rehypeHighlight]}
        components={{
          a({ href, children, ...props }) {
            if (href?.startsWith(CITATION_SCHEME)) {
              const index = Number.parseInt(href.slice(CITATION_SCHEME.length), 10);
              if (Number.isFinite(index) && index >= 1 && index <= citationCount) {
                return (
                  <sup className="citation-ref">
                    <button
                      type="button"
                      className="citation-ref__btn"
                      aria-label={t("chat.thread.citationRef", { index })}
                      onClick={(event) => {
                        event.preventDefault();
                        onCitationClick(index);
                      }}
                    >
                      {index}
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
