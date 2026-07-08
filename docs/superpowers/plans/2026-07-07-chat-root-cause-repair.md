# Chat Root Cause Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the Chat page and per-page Wiki AI sidebar so conversations remain scrollable, visually aligned with the design system, and correctly scoped to the conversation/page the user is asking about.

**Architecture:** Keep chat data as JSON under `.app/chats/{id}.json`. Repair the frontend layout first, then make the composer send contract explicit, then add typed page-scoped chat metadata so Wiki page sidebars do not share one global active session. Backend remains thin Tauri commands -> typed DTOs -> services -> local JSON files.

**Tech Stack:** React 19, TypeScript, Zustand, Tailwind CSS v4 tokens in `src/styles.css`, Lucide React, Tauri v2 Rust services, Vitest + Testing Library.

## Global Constraints

- Project content stays Markdown + JSON + local files; do not introduce a database.
- Chat sessions stay in `.app/chats/{id}.json`.
- React UI must not own filesystem, Git, Agent process, or secret-storage logic.
- Use structured DTOs and typed interfaces; avoid ad hoc string protocols.
- `raw/sources/` is immutable by default and is not touched by this work.
- High-risk file operations need Git checkpoints first; this plan does not require destructive user-content writes.
- Match `UI-Frontend-design/chat.html` and `UI-Frontend-design/assets/app.css`; do not modify `UI-Frontend-design/`.
- Use absolute px font sizes and CSS variables from `src/styles.css`; do not hardcode hex colors in components.
- After implementation run `npm run test`, `npm run lint`, confirm no unintended `console.log`, and verify imports resolve.

---

## Root Cause Findings

1. **Chat scroll/composer disappearance is a layout boundary bug.**  
   `ChatView.tsx` renders the conversation column as `className="flex min-w-0 flex-col"` without `min-h-0`/`overflow-hidden` on the grid item. In a fixed-height shell with `overflow: hidden`, a flex/grid child with default `min-height: auto` can grow to its content, preventing the intended inner `overflow-y-auto` region from owning scroll. The design spec uses `chat-stream-wrap { display:flex; flex-direction:column; overflow:hidden }`, a fixed toolbar, a scrollable `chat-stream`, and a fixed composer.

2. **Reopened Chat sessions can lose usable composer state because session loading and sending have no explicit active-session contract.**  
   `chatStore.loadSessions()` only loads summaries; it does not select the newest/last session. `ChatView.handleSend()` returns early when `activeSessionId` is null. `ChatComposer.submit()` calls `onSend(trimmed)` and immediately clears the textarea even if the parent did nothing or the backend rejected the send.

3. **Wiki page AI sidebar uses one global active chat session.**  
   `navigationStore` tracks `wikiAssistantPagePath`, but `RightContextPanel` passes the current `wikiStore.page` and `PageChatPanel` reads global `activeSessionId`/`activeSession`. `PageChatPanel.handleSend()` only creates a session when no global active session exists. Opening another Wiki page with the assistant open therefore reuses the previous page's chat.

4. **Page sidebar cannot intentionally start a new conversation.**  
   `PageChatPanel` has no New Chat control. The only creation path is implicit and only when `activeSessionId` is null.

5. **Several Chat controls drift from the design system.**  
   `ChatSessionList.tsx` uses corrupted text glyphs (`鉁?`, `脳`) instead of Lucide icons, lacks session search/meta/footer from the design, and exposes rename/delete mostly on hover. `ChatComposer.tsx` uses plain text Send/Cancel buttons instead of compact icon-led controls and has no `aria-label` on the textarea.

6. **Adjacent audit defects should be fixed in the same pass.**  
   Page sidebar message citations and save actions are no-ops. `RightContextPanel.tsx` reads `saveStatus` and `saveAnswer` via `useChatStore.getState()` during render, so the right panel is not subscribed to later save-status changes. Chat tests cover happy paths but not reopening sessions, failed/no-op sends, page switches, or scroll contract.

## File Structure

- Modify `src/features/chat/ChatView.tsx`: split fixed toolbar, scroll region, and composer; make send async/boolean-aware; use repaired layout classes.
- Modify `src/features/chat/ChatComposer.tsx`: return-aware send contract, icon-led controls, `aria-label`, design-aligned composer structure.
- Modify `src/features/chat/ChatSessionList.tsx`: search, meta row, footer, Lucide icon buttons.
- Modify `src/features/chat/PageChatPanel.tsx`: page-scoped session selection/creation, New Chat button, real citation/save handlers, repaired internal scroll.
- Modify `src/stores/chatStore.ts`: auto-select newest session, support page-scoped session metadata, add helper for ensuring a page chat session, make send return observable success.
- Modify `src/types/chat.ts`: add optional `contextPagePath` to session DTOs and create request flow.
- Modify `src-tauri/src/models/chat.rs`: add optional `context_page_path` to session and summary DTOs with serde defaults for existing JSON.
- Modify `src-tauri/src/services/chat_service.rs`: persist and summarize page-scoped session metadata.
- Modify `src-tauri/src/commands/chat_commands.rs`: pass `context_page_path` through create command.
- Modify `src/components/app/RightContextPanel.tsx`: remove outer scroll wrapper around `PageChatPanel`, subscribe to chat save state.
- Modify `src/styles.css`: add design-aligned `.chat-stream-wrap`, `.chat-scroll-region`, `.composer`, `.composer__inner`, `.composer__foot`, `.chat-sessions__*`, and page-chat layout rules.
- Test `src/features/chat/chatView.test.tsx`, `src/features/chat/PageChatPanel.test.tsx`, `src/stores/chatStore.test.ts`, Rust model/service tests, and `src/test/ui-css-contracts.test.ts`.

---

### Task 1: Repair Chat Scroll Boundaries

**Files:**
- Modify: `src/features/chat/ChatView.tsx`
- Modify: `src/components/app/RightContextPanel.tsx`
- Modify: `src/styles.css`
- Test: `src/features/chat/chatView.test.tsx`
- Test: `src/test/ui-css-contracts.test.ts`

**Interfaces:**
- Consumes: existing `ChatView`, `SessionToolbar`, `ChatComposer`, `PageChatPanel`.
- Produces: stable layout classes `chat-stream-wrap`, `chat-scroll-region`, `chat-conversation`, `page-chat-shell`.

- [ ] **Step 1: Write the layout contract tests**

Add assertions to `src/features/chat/chatView.test.tsx`:

```tsx
it("keeps the session toolbar outside the scrollable message region", () => {
  seedActiveSession();
  render(<ChatView />);

  const log = screen.getByRole("log");
  expect(log).toHaveClass("chat-scroll-region");
  expect(log.querySelector(".view-toolbar")).toBeNull();
  expect(document.querySelector(".chat-stream-wrap .view-toolbar")).toBeInTheDocument();
});
```

Add CSS contract assertions to `src/test/ui-css-contracts.test.ts`:

```ts
it("defines chat scroll containers with shrink-safe overflow boundaries", () => {
  const css = readStyles();
  expect(css).toContain(".chat-stream-wrap");
  expect(css).toMatch(/\.chat-stream-wrap\s*\{[^}]*overflow:\s*hidden/s);
  expect(css).toMatch(/\.chat-scroll-region\s*\{[^}]*overflow-y:\s*auto/s);
  expect(css).toMatch(/\.chat-conversation\s*\{[^}]*min-height:\s*0/s);
});
```

- [ ] **Step 2: Run the focused tests and confirm they fail**

Run: `npm run test -- src/features/chat/chatView.test.tsx src/test/ui-css-contracts.test.ts`  
Expected: FAIL because toolbar is currently inside the log region and CSS classes do not exist.

- [ ] **Step 3: Refactor the ChatView layout**

In `ChatView.tsx`, change the conversation column to:

```tsx
<div className="chat-conversation flex min-h-0 min-w-0 flex-col overflow-hidden">
  {error ? <div role="alert" className="...">{error}</div> : null}
  {activeSession ? (
    <SessionToolbar ... />
  ) : null}
  <div className="chat-scroll-region min-h-0 flex-1 overflow-y-auto px-6 py-4" role="log" aria-live="polite">
    ...
  </div>
  <ChatComposer ... />
</div>
```

Move `SessionToolbar` out of the scroll region. Keep only empty state, messages, streaming bubble, and overwrite prompt inside `chat-scroll-region`.

- [ ] **Step 4: Fix the Wiki assistant panel wrapper**

In `RightContextPanel.tsx`, replace the Wiki assistant wrapper:

```tsx
<div className="min-h-0 flex-1 overflow-hidden">
  <ViewErrorBoundary>
    <Suspense fallback={<ViewFallback />}>
      <PageChatPanel ... />
    </Suspense>
  </ViewErrorBoundary>
</div>
```

The scroll owner must be `PageChatPanel`'s message body, not an outer right-panel wrapper.

- [ ] **Step 5: Add CSS**

In `src/styles.css`:

```css
.chat-conversation,
.chat-stream-wrap,
.page-chat-shell {
  min-height: 0;
  overflow: hidden;
}

.chat-stream-wrap,
.chat-conversation {
  display: flex;
  flex-direction: column;
}

.chat-scroll-region {
  min-height: 0;
  flex: 1;
  overflow-y: auto;
}
```

- [ ] **Step 6: Verify**

Run: `npm run test -- src/features/chat/chatView.test.tsx src/test/ui-css-contracts.test.ts`  
Expected: PASS.

---

### Task 2: Make Composer Sends Explicit And Non-Destructive

**Files:**
- Modify: `src/features/chat/ChatComposer.tsx`
- Modify: `src/features/chat/ChatView.tsx`
- Modify: `src/features/chat/PageChatPanel.tsx`
- Modify: `src/stores/chatStore.ts`
- Test: `src/features/chat/chatView.test.tsx`
- Test: `src/features/chat/PageChatPanel.test.tsx`

**Interfaces:**
- Produces: `ChatComposerProps.onSend: (content: string) => boolean | Promise<boolean>`.
- Produces: no textarea clearing unless a send was accepted.

- [ ] **Step 1: Write failing composer behavior tests**

In `chatView.test.tsx`:

```tsx
it("does not clear the composer when no session can accept the send", async () => {
  useChatStore.setState({
    activeSessionId: null,
    activeSession: null,
    sessions: [],
    createSession: vi.fn(async () => null) as never,
    loadSessions: async () => {},
  });
  render(<ChatView />);

  const box = screen.getByPlaceholderText(/Ask about this wiki/i);
  fireEvent.change(box, { target: { value: "Will this vanish?" } });
  fireEvent.click(screen.getByRole("button", { name: /send/i }));

  await waitFor(() => expect(box).toHaveValue("Will this vanish?"));
});
```

In `PageChatPanel.test.tsx`:

```tsx
it("keeps the draft when page chat session creation fails", async () => {
  useChatStore.setState({
    activeSessionId: null,
    activeSession: null,
    createSession: vi.fn(async () => null) as never,
  });
  render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

  const box = screen.getByPlaceholderText("Ask about this page...");
  fireEvent.change(box, { target: { value: "Do not clear me" } });
  fireEvent.click(screen.getByRole("button", { name: /send/i }));

  await waitFor(() => expect(box).toHaveValue("Do not clear me"));
});
```

- [ ] **Step 2: Run tests and confirm failure**

Run: `npm run test -- src/features/chat/chatView.test.tsx src/features/chat/PageChatPanel.test.tsx`  
Expected: FAIL because `ChatComposer` clears immediately.

- [ ] **Step 3: Update the composer contract**

In `ChatComposer.tsx`:

```tsx
interface ChatComposerProps {
  ...
  onSend: (content: string) => boolean | Promise<boolean>;
}

const [submitting, setSubmitting] = useState(false);

const submit = async () => {
  const trimmed = value.trim();
  if (!trimmed || generating || submitting) return;
  setSubmitting(true);
  try {
    const accepted = await onSend(trimmed);
    if (accepted) setValue("");
  } finally {
    setSubmitting(false);
  }
};
```

Disable the send button with `generating || submitting || !value.trim()`.

- [ ] **Step 4: Return send acceptance from ChatView and PageChatPanel**

In `ChatView.handleSend`, return `false` when there is no active session and session creation fails. Return `true` only after `send(...)` returns a task id.

```tsx
const handleSend = async (content: string): Promise<boolean> => {
  let sessionId = activeSessionId;
  if (!sessionId) {
    const created = await createSession(projectId, rootPath);
    sessionId = created?.id ?? useChatStore.getState().activeSessionId;
  }
  if (!sessionId) return false;
  const taskId = await send(...);
  return Boolean(taskId);
};
```

Apply the same boolean return pattern in `PageChatPanel.handleSend`.

- [ ] **Step 5: Verify**

Run: `npm run test -- src/features/chat/chatView.test.tsx src/features/chat/PageChatPanel.test.tsx`  
Expected: PASS.

---

### Task 3: Restore Or Auto-Select A Chat Session On Open

**Files:**
- Modify: `src/stores/chatStore.ts`
- Test: `src/stores/chatStore.test.ts`

**Interfaces:**
- Produces: `loadSessions(projectId, rootPath)` selects the newest session when no active session is selected.
- Preserves: explicit user-selected session remains selected when still present.

- [ ] **Step 1: Add failing store tests**

In `chatStore.test.ts`:

```ts
it("auto-selects the newest session when loading sessions without an active session", async () => {
  invokeMock
    .mockResolvedValueOnce([
      sessionSummary({ id: "new", title: "Newest", updatedAt: "2026-07-07T10:00:00Z" }),
      sessionSummary({ id: "old", title: "Old", updatedAt: "2026-07-06T10:00:00Z" }),
    ])
    .mockResolvedValueOnce(session({ id: "new", title: "Newest" }));

  await useChatStore.getState().loadSessions(PROJECT.projectId, PROJECT.rootPath);

  expect(useChatStore.getState().activeSessionId).toBe("new");
  expect(invokeMock.mock.calls[1][0]).toBe("load_chat_session");
});
```

```ts
it("does not replace an already selected session during list refresh", async () => {
  useChatStore.setState({ activeSessionId: "old", activeSession: session({ id: "old" }) });
  invokeMock.mockResolvedValueOnce([
    sessionSummary({ id: "new", updatedAt: "2026-07-07T10:00:00Z" }),
    sessionSummary({ id: "old", updatedAt: "2026-07-06T10:00:00Z" }),
  ]);

  await useChatStore.getState().loadSessions(PROJECT.projectId, PROJECT.rootPath);

  expect(useChatStore.getState().activeSessionId).toBe("old");
});
```

- [ ] **Step 2: Run the store tests and confirm failure**

Run: `npm run test -- src/stores/chatStore.test.ts`  
Expected: FAIL because `loadSessions` does not select anything.

- [ ] **Step 3: Implement auto-select**

After setting `sessions` in `loadSessions`, select the first summary only when `activeSessionId` is null:

```ts
set({ sessions, loadingSessions: false });
if (!get().activeSessionId && sessions[0]) {
  await get().selectSession(projectId, rootPath, sessions[0].id);
}
```

Guard project scope after the awaited selection.

- [ ] **Step 4: Verify**

Run: `npm run test -- src/stores/chatStore.test.ts`  
Expected: PASS.

---

### Task 4: Add Page-Scoped Wiki Assistant Sessions

**Files:**
- Modify: `src/types/chat.ts`
- Modify: `src/stores/chatStore.ts`
- Modify: `src/features/chat/PageChatPanel.tsx`
- Modify: `src-tauri/src/models/chat.rs`
- Modify: `src-tauri/src/services/chat_service.rs`
- Modify: `src-tauri/src/commands/chat_commands.rs`
- Test: `src/features/chat/PageChatPanel.test.tsx`
- Test: `src/stores/chatStore.test.ts`
- Test: `src-tauri/src/models/chat.rs`
- Test: `src-tauri/src/services/chat_service.rs`

**Interfaces:**
- Produces: optional `contextPagePath?: string | null` on `ChatSession` and `ChatSessionSummary`.
- Produces: `createSession(projectId, rootPath, title?, contextPagePath?)`.
- Produces: `ensurePageSession(projectId, rootPath, pagePath, pageTitle, forceNew?)`.

- [ ] **Step 1: Write failing frontend tests for page switching and New Chat**

In `PageChatPanel.test.tsx`:

```tsx
it("creates a distinct page-scoped session when the wiki page changes", async () => {
  const ensurePageSession = vi.fn(async () => session({ id: "react-session" }));
  useChatStore.setState({ ensurePageSession: ensurePageSession as never });

  const { rerender } = render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

  const nextPage = {
    ...page,
    meta: { ...page.meta, path: "wiki/concepts/planning.md", title: "Planning" },
  };
  rerender(<PageChatPanel page={nextPage} projectId="project-1" rootPath="/wiki" />);

  await waitFor(() => {
    expect(ensurePageSession).toHaveBeenCalledWith(
      "project-1",
      "/wiki",
      "wiki/concepts/planning.md",
      "Planning",
      false,
    );
  });
});
```

```tsx
it("offers a new page chat action", async () => {
  const ensurePageSession = vi.fn(async () => session({ id: "fresh-session" }));
  useChatStore.setState({ ensurePageSession: ensurePageSession as never });
  render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" />);

  fireEvent.click(screen.getByRole("button", { name: /new chat/i }));

  await waitFor(() => {
    expect(ensurePageSession).toHaveBeenCalledWith(
      "project-1",
      "/wiki",
      page.meta.path,
      page.meta.title,
      true,
    );
  });
});
```

- [ ] **Step 2: Write Rust DTO compatibility tests**

In `src-tauri/src/models/chat.rs`:

```rust
#[test]
fn session_defaults_context_page_path_for_existing_json() {
    let raw = r#"{
      "id":"s1","title":"Old","projectId":"p",
      "createdAt":"2026-07-07T00:00:00Z",
      "updatedAt":"2026-07-07T00:00:00Z",
      "messages":[]
    }"#;
    let session: ChatSession = serde_json::from_str(raw).unwrap();
    assert!(session.context_page_path.is_none());
}
```

- [ ] **Step 3: Run focused tests and confirm failure**

Run: `npm run test -- src/features/chat/PageChatPanel.test.tsx src/stores/chatStore.test.ts`  
Run: `cargo test --lib --no-default-features chat::tests::session_defaults_context_page_path_for_existing_json`  
Expected: FAIL until DTO/store changes exist.

- [ ] **Step 4: Extend TypeScript DTOs**

In `src/types/chat.ts`:

```ts
export interface ChatSession {
  ...
  contextPagePath?: string | null;
}

export interface ChatSessionSummary {
  ...
  contextPagePath?: string | null;
}
```

- [ ] **Step 5: Extend Rust DTOs with backward-compatible serde**

In `src-tauri/src/models/chat.rs`:

```rust
pub struct ChatSession {
    ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_page_path: Option<String>,
    #[serde(default)]
    pub messages: Vec<ChatMessage>,
}

pub struct ChatSessionSummary {
    ...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_page_path: Option<String>,
    pub message_count: usize,
}

pub struct CreateChatSessionRequest {
    ...
    #[serde(default)]
    pub context_page_path: Option<String>,
}
```

- [ ] **Step 6: Persist and summarize page context**

Change `ChatService::create_session` signature to accept `context_page_path: Option<&str>`, normalize empty strings to `None`, and write it into the session. Include `context_page_path: session.context_page_path` in `list_sessions`.

- [ ] **Step 7: Add store helper**

In `chatStore.ts`, add:

```ts
ensurePageSession: (
  projectId: string,
  rootPath: string,
  pagePath: string,
  pageTitle: string,
  forceNew?: boolean,
) => Promise<ChatSession | null>;
```

Implementation:

```ts
const title = `Ask: ${pageTitle}`;
if (!forceNew) {
  const existing = get().sessions.find((s) => s.contextPagePath === pagePath);
  if (existing) {
    await get().selectSession(projectId, rootPath, existing.id);
    return get().activeSession;
  }
}
return get().createSession(projectId, rootPath, title, pagePath);
```

If `sessions` is empty, call `loadSessions(projectId, rootPath)` first, then search again.

- [ ] **Step 8: Use the helper in PageChatPanel**

In `PageChatPanel.tsx`, on `page.meta.path` changes:

```tsx
useEffect(() => {
  if (!page) return;
  void ensurePageSession(projectId, rootPath, page.meta.path, page.meta.title, false);
}, [projectId, rootPath, page?.meta.path, page?.meta.title, ensurePageSession]);
```

Add a compact icon/text New Chat button in the panel header:

```tsx
<button
  type="button"
  className="icon-button shrink-0"
  onClick={() => void ensurePageSession(projectId, rootPath, page.meta.path, page.meta.title, true)}
  aria-label={t("chat.sessions.new")}
  title={t("chat.sessions.new")}
>
  <Plus size={15} aria-hidden="true" />
</button>
```

- [ ] **Step 9: Verify**

Run: `npm run test -- src/features/chat/PageChatPanel.test.tsx src/stores/chatStore.test.ts`  
Run: `cargo test --lib --no-default-features chat`  
Expected: PASS.

---

### Task 5: Align Chat Buttons And Session List With The Design

**Files:**
- Modify: `src/features/chat/ChatComposer.tsx`
- Modify: `src/features/chat/ChatSessionList.tsx`
- Modify: `src/features/chat/ChatView.tsx`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/styles.css`
- Test: `src/features/chat/chatView.test.tsx`

**Interfaces:**
- Consumes: Lucide React icons.
- Produces: icon-led controls with accessible labels and design-token styling.

- [ ] **Step 1: Write UI tests**

In `chatView.test.tsx`:

```tsx
it("renders design-aligned session search and icon controls", () => {
  useChatStore.setState({
    sessions: [{ id: "s1", title: "Agent Memory", createdAt: "x", updatedAt: "2026-07-07T10:00:00Z", messageCount: 2 }],
    loadSessions: async () => {},
  });
  render(<ChatView />);

  expect(screen.getByPlaceholderText(/Search chats/i)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /new chat/i })).toBeInTheDocument();
  expect(screen.queryByText("鉁?")).not.toBeInTheDocument();
  expect(screen.queryByText("脳")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run and confirm failure**

Run: `npm run test -- src/features/chat/chatView.test.tsx`  
Expected: FAIL because search/meta/footer and icons are missing.

- [ ] **Step 3: Replace corrupted glyph controls**

In `ChatSessionList.tsx`, import Lucide icons:

```tsx
import { Pencil, Plus, Search, Trash2 } from "lucide-react";
```

Replace the text/glyph controls with `icon-button`-style buttons using `aria-label` and `title`. Render a searchbar above the session list and filter by lowercase title.

- [ ] **Step 4: Render session meta and footer**

Render:

```tsx
<div className="chat-session__meta">
  {formatTime(session.updatedAt)} · {t("chat.sessions.messageCount", { count: session.messageCount })}
</div>
```

Add footer text matching the design: `.app/chats/ · wiki/queries/`.

- [ ] **Step 5: Polish composer controls**

Use Lucide `SendHorizontal` and `CircleStop` icons. Keep text where it improves clarity, but use icon+label buttons and `aria-label`. Add `aria-label={t("chat.composer.inputLabel")}` to the textarea. Update placeholder to include the send shortcut and future `/` skill affordance without adding nonfunctional skill behavior:

```json
"placeholder": "Ask about this wiki · Enter to send · Shift+Enter for newline"
```

- [ ] **Step 6: Verify**

Run: `npm run test -- src/features/chat/chatView.test.tsx`  
Expected: PASS.

---

### Task 6: Fix Page Sidebar Citation And Save Actions

**Files:**
- Modify: `src/features/chat/PageChatPanel.tsx`
- Modify: `src/components/app/RightContextPanel.tsx`
- Test: `src/features/chat/PageChatPanel.test.tsx`

**Interfaces:**
- Consumes: existing `saveAnswer`, `openWikiPage`, `setActiveView`.
- Produces: page sidebar citations open Wiki pages and Save Answer works from sidebar messages.

- [ ] **Step 1: Write failing tests**

In `PageChatPanel.test.tsx`:

```tsx
it("opens cited wiki pages from page chat messages", () => {
  const openCitation = vi.fn();
  useChatStore.setState({
    activeSessionId: "session-1",
    activeSession: session({
      messages: [{
        id: "a1",
        role: "assistant",
        content: "See [1]",
        createdAt: "2026-07-04T00:00:00Z",
        citations: [{ pagePath: "wiki/a.md", title: "A", score: 1 }],
      }],
    }),
  });
  render(<PageChatPanel page={page} projectId="project-1" rootPath="/wiki" onOpenCitation={openCitation} />);

  fireEvent.click(screen.getByRole("button", { name: /A/i }));

  expect(openCitation).toHaveBeenCalledWith("wiki/a.md");
});
```

- [ ] **Step 2: Run and confirm failure**

Run: `npm run test -- src/features/chat/PageChatPanel.test.tsx`  
Expected: FAIL because handlers are currently no-ops.

- [ ] **Step 3: Wire handlers**

Add optional props to `PageChatPanel`:

```ts
onOpenCitation?: (path: string) => void;
```

Use it for `MessageBubble.onCitationClick` and `onOpenCitation`. Use `saveAnswer(projectId, rootPath, activeSessionId, message.id)` for assistant save buttons.

- [ ] **Step 4: Subscribe in RightContextPanel**

In the Chat right panel branch, replace `useChatStore.getState().saveStatus` and `saveAnswer` reads with selectors:

```tsx
const saveStatusById = useChatStore((state) => state.saveStatus);
const saveAnswer = useChatStore((state) => state.saveAnswer);
```

- [ ] **Step 5: Verify**

Run: `npm run test -- src/features/chat/PageChatPanel.test.tsx`  
Expected: PASS.

---

### Task 7: Final Verification And Review

**Files:**
- Modify as required by fixes from prior tasks.
- Update: `SPEC/progress.txt`
- Optional update: `SPEC/gotchas.txt` only if a subtle recurring trap is discovered during implementation.

**Interfaces:**
- Produces: verified Chat repair with documented results.

- [ ] **Step 1: Run frontend checks**

Run: `npm run test`  
Expected: all tests pass.

Run: `npm run lint`  
Expected: ESLint exits 0 with max warnings 0.

- [ ] **Step 2: Run Rust checks relevant to touched backend files**

Run: `cargo test --lib --no-default-features chat` from `src-tauri` if backend DTO/service code changed.  
Expected: all chat model/service tests pass.

- [ ] **Step 3: Confirm no unintended console logs**

Run: `Select-String -Path 'src\\**\\*.ts','src\\**\\*.tsx','src-tauri\\src\\**\\*.rs' -Pattern 'console\\.log'`  
Expected: no unintended `console.log`.

- [ ] **Step 4: Perform review workflow**

Launch two review subagents if available:

- Subagent A, shared context: review design intent, logic, consistency, and integration with `AGENTS.md`, `SPEC/roadmap/chat.md`, and `UI-Frontend-design/chat.html`.
- Subagent B, fresh context: review without assumptions for blind spots, missing tests, unclear behavior, and regressions.

If subagents are unavailable, perform both reviews manually and report that.

- [ ] **Step 5: Fix valid review findings and rerun all checks**

Rerun from Step 1 after every fix.

- [ ] **Step 6: Update progress log**

Prepend a `SPEC/progress.txt` record in this format:

```text
[2026-07-07] Chat root-cause repair — Fixed scroll/session/page-scoped assistant defects and aligned controls with design — Key decision: page AI sessions are typed `.app/chats` JSON metadata, not a separate database or ad hoc title convention.
```

