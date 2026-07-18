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
  const protectedParts: string[] = [];
  let nonce = 0;
  let markerPrefix = "";
  do {
    markerPrefix = `\uE000chat-protected-${body.length}-${nonce++}-`;
  } while (body.includes(markerPrefix));
  const escapedMarkerPrefix = markerPrefix.replace(/[.*+?^${}()|[\\]\\]/g, "\\$&");
  const markerPattern = new RegExp(`${escapedMarkerPrefix}(\\d+)\\uE001`, "g");
  const protect = (match: string) => {
    const index = protectedParts.push(match) - 1;
    return `${markerPrefix}${index}\uE001`;
  };
  // Citation-looking text in fenced/inline code and existing Markdown links
  // is literal content, not a model citation. Mask it before rewriting bare
  // markers, then restore the original bytes after the replacement.
  const masked = body
    .replace(/(`{3,}[\s\S]*?`{3,}|~{3,}[\s\S]*?~{3,})/g, protect)
    .replace(/`[^`\n]*`/g, protect)
    .replace(/\[[^\]]+\]\([^\n)]*\)/g, protect);
  const rewritten = masked
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
  return rewritten.replace(markerPattern, (_match, index: string) => {
    return protectedParts[Number(index)] ?? _match;
  });
}

interface MessageContentProps {
  /** Raw markdown body. */
  content: string;
  /** Citation count, used to clamp reference indices to valid citations. */
  citationCount: number;
  /** Valid model source ids (`S1`, `S2`, ...), used for `[S#]` markers. */
  citationIds?: string[];
  /** Streaming text has no finalized citation contract yet. */
  enableCitations?: boolean;
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
  enableCitations = true,
  onCitationClick,
}: MessageContentProps) {
  const { t } = useTranslation();
  const processed = useMemo(
    () => (enableCitations ? preprocessCitations(content) : content),
    [content, enableCitations],
  );
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
              try {
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
              } catch {
                // Treat malformed citation URLs as plain text below.
              }
              return <span {...props}>{children}</span>;
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
