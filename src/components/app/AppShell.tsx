import {
  Bot,
  BookOpenText,
  FileOutput,
  LayoutDashboard,
  MessageSquare,
  Network,
  Settings,
  ShieldCheck,
  Upload,
} from "lucide-react";

const navItems = [
  { label: "Dashboard", icon: LayoutDashboard },
  { label: "Wiki", icon: BookOpenText },
  { label: "Chat", icon: MessageSquare },
  { label: "Graph", icon: Network },
  { label: "Agent", icon: Bot },
  { label: "Import", icon: Upload },
  { label: "Lint", icon: ShieldCheck },
  { label: "Exports", icon: FileOutput },
  { label: "Settings", icon: Settings },
];

export function AppShell() {
  return (
    <div className="flex h-full min-w-[1120px] flex-col bg-[var(--background)] text-[var(--foreground)]">
      <header className="flex h-12 items-center gap-3 border-b border-[var(--border)] bg-[var(--surface)] px-4">
        <strong className="text-sm font-semibold">LLM Wiki Desktop</strong>
        <div className="h-7 flex-1 rounded-md border border-[var(--border)] bg-white px-3 text-sm leading-7 text-[var(--text-muted)]">
          Search current wiki
        </div>
        <span className="rounded-full bg-[var(--accent-soft)] px-2 py-1 text-xs font-medium text-[var(--accent-hover)]">
          Local
        </span>
      </header>

      <div className="flex min-h-0 flex-1">
        <aside className="w-60 border-r border-[var(--border)] bg-[var(--surface)] p-3">
          <nav aria-label="Primary" className="space-y-1">
            {navItems.map((item) => {
              const Icon = item.icon;
              const active = item.label === "Dashboard";

              return (
                <button
                  key={item.label}
                  className={`flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm ${
                    active
                      ? "bg-[var(--accent-soft)] text-[var(--accent-hover)]"
                      : "text-[var(--text-secondary)] hover:bg-[var(--surface-muted)]"
                  }`}
                  type="button"
                >
                  <Icon aria-hidden="true" size={16} />
                  <span>{item.label}</span>
                </button>
              );
            })}
          </nav>
        </aside>

        <main className="min-w-0 flex-1 p-4">
          <section className="h-full rounded-lg border border-[var(--border)] bg-[var(--surface-raised)] p-4">
            <h1 className="m-0 text-xl font-semibold">Dashboard</h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-[var(--text-secondary)]">
              Scaffold workspace for project health, recent wiki pages, imports, Agent state, and background tasks.
            </p>
          </section>
        </main>

        <aside className="w-80 border-l border-[var(--border)] bg-[var(--surface)] p-3">
          <h2 className="m-0 text-sm font-semibold">Context</h2>
          <p className="mt-2 text-sm leading-6 text-[var(--text-muted)]">
            Metadata, citations, task logs, diffs, and export previews will live here.
          </p>
        </aside>
      </div>

      <footer className="flex h-7 items-center justify-between border-t border-[var(--border)] bg-[var(--surface)] px-3 text-xs text-[var(--text-muted)]">
        <span>No project open</span>
        <span>Agent: not detected · Tasks: idle · Wiki pages: 0</span>
      </footer>
    </div>
  );
}

