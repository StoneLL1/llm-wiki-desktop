import {
  memo,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ImgHTMLAttributes,
} from "react";
import { useTranslation } from "react-i18next";
import ReactMarkdown, {
  defaultUrlTransform,
  type Components,
} from "react-markdown";
import { invoke } from "@tauri-apps/api/core";
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
  projectId?: string;
  projectRootPath?: string;
  pagePath?: string;
}

interface WikiAssetContent {
  contentType: string;
  bytes: number[];
}

const WIKILINK_SCHEME = "wikilink://";
const CITATION_SCHEME = "citation://";
const REMARK_PLUGINS = [remarkGfm, remarkMath];
const REHYPE_PLUGINS = [rehypeKatex, rehypeHighlight];
// React StrictMode replays mount effects in development. Share only active
// requests so replay and duplicate images do not duplicate backend I/O.
const wikiAssetRequests = new Map<string, Promise<WikiAssetContent>>();

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

function isLocalWikiAsset(src: string): boolean {
  const path = src.split(/[?#]/, 1)[0].replace(/^\.\//, "");
  return path.startsWith("assets/");
}

function wikiUrlTransform(url: string): string {
  return url.startsWith(WIKILINK_SCHEME) || url.startsWith(CITATION_SCHEME)
    ? url
    : defaultUrlTransform(url);
}

function loadWikiAsset(
  projectId: string,
  projectRootPath: string,
  pagePath: string,
  assetPath: string,
): Promise<WikiAssetContent> {
  const key = JSON.stringify([projectId, projectRootPath, pagePath, assetPath]);
  const existing = wikiAssetRequests.get(key);
  if (existing) return existing;

  const request = invoke<WikiAssetContent>("read_wiki_asset", {
    request: {
      projectId,
      projectRootPath,
      pagePath,
      assetPath,
    },
  });
  wikiAssetRequests.set(key, request);
  const clear = () => {
    if (wikiAssetRequests.get(key) === request) {
      wikiAssetRequests.delete(key);
    }
  };
  void request.then(clear, clear);
  return request;
}

type WikiImageProps = ImgHTMLAttributes<HTMLImageElement> & {
  projectId?: string;
  projectRootPath?: string;
  pagePath?: string;
};
type LocalWikiImageProps = Omit<
  WikiImageProps,
  "pagePath" | "projectId" | "projectRootPath" | "src"
> & {
  src: string;
  projectId: string;
  projectRootPath: string;
  pagePath: string;
};

function LocalWikiImage({
  src,
  alt,
  projectId,
  projectRootPath,
  pagePath,
  ...props
}: LocalWikiImageProps) {
  const imageRef = useRef<HTMLImageElement>(null);
  const [loadRequested, setLoadRequested] = useState(
    () => typeof IntersectionObserver === "undefined",
  );
  const [resolvedSrc, setResolvedSrc] = useState<string | null>(null);

  useEffect(() => {
    if (loadRequested) return;
    const image = imageRef.current;
    if (!image) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setLoadRequested(true);
          observer.disconnect();
        }
      },
      { rootMargin: "600px" },
    );
    observer.observe(image);
    return () => observer.disconnect();
  }, [loadRequested]);

  useEffect(() => {
    let disposed = false;
    let objectUrl: string | null = null;
    setResolvedSrc(null);
    if (!loadRequested) {
      return () => {
        disposed = true;
      };
    }

    void loadWikiAsset(projectId, projectRootPath, pagePath, src)
      .then((asset) => {
        if (disposed) return;
        const blob = new Blob([new Uint8Array(asset.bytes)], {
          type: asset.contentType,
        });
        objectUrl = URL.createObjectURL(blob);
        setResolvedSrc(objectUrl);
      })
      .catch(() => {
        if (!disposed) setResolvedSrc(null);
      });

    return () => {
      disposed = true;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [loadRequested, pagePath, projectId, projectRootPath, src]);

  return (
    <img
      {...props}
      ref={imageRef}
      src={resolvedSrc ?? undefined}
      alt={alt ?? ""}
      loading={props.loading ?? "lazy"}
      decoding={props.decoding ?? "async"}
      data-wiki-asset-state={resolvedSrc ? "ready" : "loading"}
    />
  );
}

function WikiImage({
  src,
  alt,
  projectId,
  projectRootPath,
  pagePath,
  ...props
}: WikiImageProps) {
  if (
    src &&
    isLocalWikiAsset(src) &&
    projectId &&
    projectRootPath &&
    pagePath
  ) {
    return (
      <LocalWikiImage
        {...props}
        src={src}
        alt={alt}
        projectId={projectId}
        projectRootPath={projectRootPath}
        pagePath={pagePath}
      />
    );
  }

  return (
    <img
      {...props}
      src={src}
      alt={alt ?? ""}
      loading={props.loading ?? "lazy"}
      decoding={props.decoding ?? "async"}
    />
  );
}

export const MarkdownReader = memo(function MarkdownReader({
  bodyMarkdown,
  frontmatterYaml,
  pages,
  onOpenPage,
  projectId,
  projectRootPath,
  pagePath,
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
  const components = useMemo<Components>(
    () => ({
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
      img({ src, alt, ...props }) {
        return (
          <WikiImage
            {...props}
            src={src}
            alt={alt}
            projectId={projectId}
            projectRootPath={projectRootPath}
            pagePath={pagePath}
          />
        );
      },
    }),
    [
      onOpenPage,
      pagePath,
      projectId,
      projectRootPath,
      resolver,
      t,
    ],
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
        remarkPlugins={REMARK_PLUGINS}
        rehypePlugins={REHYPE_PLUGINS}
        urlTransform={wikiUrlTransform}
        components={components}
      >
        {processed}
      </ReactMarkdown>
    </article>
  );
});
