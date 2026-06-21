import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Bold,
  Code2,
  Heading2,
  Italic,
  Link2,
  LoaderCircle,
  Quote,
  Redo2,
  Save,
  Undo2,
} from "lucide-react";
import { commandsCtx, defaultValueCtx, Editor, rootCtx } from "@milkdown/kit/core";
import {
  commonmark,
  toggleEmphasisCommand,
  toggleInlineCodeCommand,
  toggleLinkCommand,
  toggleStrongCommand,
  wrapInBlockquoteCommand,
  wrapInHeadingCommand,
} from "@milkdown/kit/preset/commonmark";
import { gfm } from "@milkdown/kit/preset/gfm";
import { history, redoCommand, undoCommand } from "@milkdown/kit/plugin/history";
import { listener, listenerCtx } from "@milkdown/kit/plugin/listener";
import { Milkdown, MilkdownProvider, useEditor } from "@milkdown/react";
import { nord } from "@milkdown/theme-nord";
import "@milkdown/theme-nord/style.css";

import type { SaveState } from "./wikiStore";

interface WikiEditorProps {
  draft: string;
  saveState: SaveState;
  onDraftChange: (draft: string) => void;
  onSave: () => void;
  onCancel: () => void;
  onReload: () => void;
  onReviewConflict?: () => void;
}

/**
 * WYSIWYG Markdown editor backed by Milkdown (ProseMirror).
 *
 * The editor is uncontrolled: `draft` seeds the initial document on mount, and
 * Milkdown emits the serialized markdown back through `onDraftChange` on every
 * change. The parent remounts this component (via a path key) when the page
 * switches, so each page starts from a clean editor state.
 *
 * Wikilinks `[[Target]]` are not first-class Markdown, so they render as
 * literal bracketed text in the WYSIWYG view but roundtrip losslessly to the
 * stored bytes — the reader (MarkdownReader) is responsible for resolving them
 * into clickable links.
 */
function MilkdownEditor({
  initialMarkdown,
  onChange,
}: {
  initialMarkdown: string;
  onChange: (markdown: string) => void;
}) {
  // Keep the latest onChange without churning the editor's deps array.
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;

  const { loading, get } = useEditor(
    (root) =>
      Editor.make()
        .config((ctx) => {
          ctx.set(rootCtx, root);
          ctx.set(defaultValueCtx, initialMarkdown);
          ctx.get(listenerCtx).markdownUpdated((_, markdown) => {
            onChangeRef.current(markdown);
          });
        })
        .config(nord)
        .use(commonmark)
        .use(gfm)
        .use(history)
        .use(listener),
    // Re-create the editor only when the seed markdown changes (i.e. a new
    // page). Normal edits flow through the listener, not through here.
    [initialMarkdown],
  );

  const call = (
    command:
      | typeof toggleStrongCommand
      | typeof toggleEmphasisCommand
      | typeof wrapInHeadingCommand
      | typeof toggleLinkCommand
      | typeof toggleInlineCodeCommand
      | typeof wrapInBlockquoteCommand
      | typeof undoCommand
      | typeof redoCommand,
    payload?: unknown,
  ) => {
    const editor = get();
    if (!editor) return;
    editor.action((ctx) => {
      const commands = ctx.get(commandsCtx);
      commands.call(command.key, payload as never);
    });
  };

  const requestLink = () => {
    const href = window.prompt("https://");
    if (href?.trim()) call(toggleLinkCommand, { href: href.trim() });
  };

  return (
    <>
      <EditorToolbar
        disabled={loading}
        onBold={() => call(toggleStrongCommand)}
        onItalic={() => call(toggleEmphasisCommand)}
        onHeading={() => call(wrapInHeadingCommand, 2)}
        onLink={requestLink}
        onCode={() => call(toggleInlineCodeCommand)}
        onQuote={() => call(wrapInBlockquoteCommand)}
        onUndo={() => call(undoCommand)}
        onRedo={() => call(redoCommand)}
      />
      <Milkdown />
    </>
  );
}

interface EditorToolbarProps {
  disabled: boolean;
  onBold: () => void;
  onItalic: () => void;
  onHeading: () => void;
  onLink: () => void;
  onCode: () => void;
  onQuote: () => void;
  onUndo: () => void;
  onRedo: () => void;
}

function EditorToolbar(props: EditorToolbarProps) {
  const { t } = useTranslation();
  const actions = [
    ["bold", Bold, props.onBold],
    ["italic", Italic, props.onItalic],
    ["heading", Heading2, props.onHeading],
    ["separator"],
    ["link", Link2, props.onLink],
    ["code", Code2, props.onCode],
    ["quote", Quote, props.onQuote],
    ["separator"],
    ["undo", Undo2, props.onUndo],
    ["redo", Redo2, props.onRedo],
  ] as const;

  return (
    <div className="editor__toolbar" role="toolbar" aria-label={t("wiki.editor.toolbar.label")}>
      {actions.map((action, index) => {
        if (action[0] === "separator") {
          return <span className="sep" role="separator" key={`separator-${index}`} />;
        }
        const [name, Icon, onClick] = action;
        const label = t(`wiki.editor.toolbar.${name}`);
        return (
          <button
            type="button"
            key={name}
            aria-label={label}
            title={label}
            disabled={props.disabled}
            onMouseDown={(event) => event.preventDefault()}
            onClick={onClick}
          >
            <Icon size={14} strokeWidth={1.7} />
          </button>
        );
      })}
    </div>
  );
}

export function WikiEditor({
  draft,
  saveState,
  onDraftChange,
  onSave,
  onCancel,
  onReload,
  onReviewConflict,
}: WikiEditorProps) {
  const { t } = useTranslation();
  const saving = saveState === "saving";
  // Seed the WYSIWYG doc once per mount; the parent remounts us (keyed by page
  // path) when the page changes. Live edits flow out via onDraftChange and must
  // NOT feed back into initialMarkdown, or the editor re-creates itself on
  // every keystroke and loses the cursor.
  const [seed] = useState(draft);

  return (
    <div className="flex h-full flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-[var(--border-subtle)] px-1 pb-2 pt-1">
        <span
          className={`inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-medium ${
            saveState === "saved"
              ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
              : saveState === "conflict" || saveState === "error"
                ? "bg-[var(--warning-soft)] text-[var(--warning)]"
                : "bg-[var(--surface-muted)] text-[var(--text-muted)]"
          }`}
        >
          <span
            className={`inline-block h-[6px] w-[6px] rounded-full ${
              saveState === "saved"
                ? "bg-[var(--accent)]"
                : saveState === "conflict" || saveState === "error"
                  ? "bg-[var(--warning)]"
                  : "bg-[var(--text-muted)]"
            }`}
          />
          {t(`wiki.editor.saveState.${saveState}`)}
        </span>
        <div className="ml-auto flex items-center gap-2">
          <button
            type="button"
            className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
            onClick={onCancel}
            disabled={saving}
          >
            {t("wiki.editor.cancel")}
          </button>
          {saveState === "conflict" ? (
            <>
              {onReviewConflict ? (
                <button
                  type="button"
                  className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
                  onClick={onReviewConflict}
                >
                  {t("wiki.editor.reviewConflict")}
                </button>
              ) : null}
              <button
                type="button"
                className="h-[26px] rounded-[var(--radius-sm)] border border-[var(--border)] bg-[var(--surface-raised)] px-3 text-[12px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-muted)]"
                onClick={onReload}
              >
                {t("wiki.editor.reload")}
              </button>
            </>
          ) : null}
          <button
            type="button"
            className="inline-flex h-[26px] items-center gap-1.5 rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 text-[12px] font-medium text-white hover:bg-[var(--accent-hover)] disabled:opacity-60"
            onClick={onSave}
            disabled={saving}
          >
            {saving ? (
              <LoaderCircle size={13} className="animate-spin" />
            ) : (
              <Save size={13} />
            )}
            {t("wiki.editor.save")}
          </button>
        </div>
      </div>
      {saveState === "conflict" ? (
        <div className="shrink-0 border-b border-[var(--warning)] bg-[var(--warning-soft)] px-3 py-2 text-[11.5px] text-[var(--warning)]">
          {t("wiki.editor.conflictCopy")}
        </div>
      ) : null}
      <div
        className="wiki-editor min-h-0 flex-1 overflow-y-auto px-2 pb-2 text-[13px] leading-[1.7] text-[var(--text-primary)]"
        onKeyDown={(event) => {
          if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
            event.preventDefault();
            if (!saving) onSave();
          }
        }}
      >
        <MilkdownProvider>
          <MilkdownEditor initialMarkdown={seed} onChange={onDraftChange} />
        </MilkdownProvider>
      </div>
    </div>
  );
}
