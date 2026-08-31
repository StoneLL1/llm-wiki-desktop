# LLM Wiki Desktop

A local-first desktop app for building a personal knowledge base from raw
material — import documents, web pages, audio, and images; curate them into a
Markdown wiki; explore it as a graph; and ask questions of your own notes with
AI you configure yourself.

**Status: pre-release (0.2.0 RC).** The application is feature-complete for
its first supported journeys and covered by an extensive local test suite, but
no stable public release exists yet. Expect rough edges, and read
[Known limitations](#known-limitations) before trusting it with important
data.

## What it does

- **Local-first, no database.** A knowledge base is a plain folder of
  Markdown, JSON, and media files. Your notes work in Obsidian and any text
  editor; nothing is uploaded, and there is no server-side account.
- **Import → Sources → Wiki.** Raw imports become immutable evidence plus
  readable *Source* pages; the wiki is a derived layer you explicitly update
  (the Karpathy LLM Wiki pattern: Raw Sources → Wiki → Schema). The app opens
  knowledge bases laid out in its native format, legacy LLM Wiki /
  `nashsu/llm_wiki` conventions, and Obsidian-compatible Markdown vaults
  (see `SPEC/SPEC.md`).
- **All-format ingestion.** Documents (PDF, Office), web pages (URL
  readability extraction), audio/video (ASR transcripts), and images (OCR)
  — the heavier engines ship as optional, separately downloadable
  *capability packs* that are signature-verified before use.
- **Knowledge graph.** Every readable page becomes a node (sigma.js +
  ForceAtlas2 + Louvain communities); works without any prior "compilation"
  step and stays read-only in restricted project modes.
- **Chat with your wiki.** Natural-language Q&A grounded in readable Sources
  or wiki pages, driven by your own AI configuration (see below). Plain
  keyword search never calls a model.
- **Workflows, not background magic.** AI-assisted updates, deep lint, and
  content generation run as explicit, cancelable, observable workflows with
  confirmation gates for destructive steps.
- **Git as the safety net.** Destructive operations (bulk replace, raw-source
  removal, agent-driven fixes) require a Git checkpoint first.

## AI providers

The app never bundles model access. You choose the execution path in Settings:

- **Agent CLI** (an external agent binary you install and authorize), and/or
- **BYOK API** keys for OpenAI / Anthropic / Google / Ollama / any
  OpenAI-compatible endpoint.

Keys are stored only in the OS credential store (Windows Credential Manager,
macOS Keychain, Linux Secret Service) — never in project files or logs. No
telemetry is collected.

## Platforms

| Platform | Installer |
| --- | --- |
| Windows 10/11 x64 | NSIS `.exe` |
| macOS 13+ (Apple Silicon) | `.dmg` / `.app` |
| macOS 13+ (Intel) | `.dmg` / `.app` |
| Linux x64 | `.AppImage` |

Installers are published from [Releases](../../releases) once the first
stable version passes real-machine acceptance. In-app updates are
signature-verified (minisign).

## Developing

Prerequisites: Node.js 20+, Rust stable, platform Tauri v2 dependencies
([Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)).

```bash
npm install
npm run dev        # dev app with hot reload
npm run check      # full gate: lint, frontend build+tests, Rust tests
```

Shorter loop during normal development: `npm run check:quick`.

The frontend is React 19 + TypeScript + Vite + Tailwind v4 + shadcn/ui; the
backend is Rust behind thin Tauri IPC commands. Architecture boundaries are
specified in `SPEC/TECH_STACK.md` and `SPEC/BACKEND_STRUCTURE.md` — please
read them before non-trivial changes. Contribution rules live in
[CONTRIBUTING.md](CONTRIBUTING.md).

## Repository layout

```
src/                  React frontend (features, stores, components)
src-tauri/            Rust backend (commands -> services -> local files)
capabilities/         Capability-pack manifests, runners, release tooling
skills/               Agent skill definitions (wiki-lint, html-*, …)
SPEC/                 Product/architecture specifications
docs/                 Architecture decisions, release runbooks, maintainers
UI-Frontend-design/   Authoritative UI design reference
release/              Release contracts and machine-checked checklists
```

## Privacy and data

Everything the app writes stays inside your knowledge-base folder (plus app
state under `.app/`). AI features send only the content you explicitly submit
to the provider you configured. See [SECURITY.md](SECURITY.md) for how to
report vulnerabilities — please do not open public issues for suspected
security problems.

## Known limitations

- No stable release yet; upgrade/rollback acceptance is still being completed
  per platform.
- macOS binaries are not notarized and Windows binaries are not
  Authenticode-signed; first launch requires the OS manual-override path
  (documented in the release notes of each version).
- Deep AI lint and some workflows require project *trust* escalation; opening
  a folder read-only is always available.
- The UI is bilingual (English / 简体中文); AI-generated content follows your
  language preference.

## License

[Apache-2.0](LICENSE). The application license covers this repository's code.
Optional capability packs bundle their own third-party components (for
example FFmpeg under LGPLv3+) and ship with their own per-pack license
notices, SBOMs, and signatures — see `capabilities/` and each release's
manifest.

---

中文说明：本项目是一个本地优先的个人知识库桌面应用（导入 → Source → Wiki →
图谱/对话），详细产品与架构规格见 `SPEC/`（中文）。当前处于 0.2.0 候选阶段，
尚无稳定版本发布。
