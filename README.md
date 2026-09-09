<div align="center">
  <img src="src/assets/app-logo.png" alt="LLM Wiki Desktop" width="88" />
  <h1>LLM Wiki Desktop</h1>
  <p><strong>Your sources. Your wiki. Your AI.</strong></p>
  <p>A local-first workspace for turning documents, links, and media into a connected Markdown knowledge base.</p>
  <p>
    <a href="https://github.com/StoneLL1/llm-wiki-desktop/releases/latest"><img src="https://img.shields.io/github/v/release/StoneLL1/llm-wiki-desktop?style=flat-square&color=30363d" alt="Latest release" /></a>
    <a href="https://github.com/StoneLL1/llm-wiki-desktop/actions/workflows/ci.yml"><img src="https://github.com/StoneLL1/llm-wiki-desktop/actions/workflows/ci.yml/badge.svg?branch=master" alt="CI" /></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-30363d?style=flat-square" alt="Apache 2.0 license" /></a>
  </p>
  <p><a href="https://github.com/StoneLL1/llm-wiki-desktop/releases/latest">Download</a> · <a href="#get-started">Get started</a> · <a href="README.zh-CN.md">简体中文</a> · <a href="CONTRIBUTING.md">Contributing</a></p>
</div>

![LLM Wiki Desktop — a Markdown wiki built from connected research notes](docs/images/wiki-workspace.png)

## A workspace that keeps the source in sight

Bring your material together, read it as a **Source**, then explicitly organize it into a **Wiki**. Explore connections, ask questions, and publish what you learn. Your knowledge base stays a folder you can open in a text editor.

| | What you can do |
| :-- | :-- |
| **Collect** | Import documents, web pages, images, audio, and video. Optional engines add OCR, transcription, browser extraction, and document conversion. |
| **Connect** | Read and edit Markdown, follow backlinks, search locally, and explore a knowledge graph. Open native projects or compatible Markdown vaults. |
| **Ask** | Chat with your Sources and wiki using your own API provider, local Ollama service, or supported Agent CLI. |
| **Create** | Update Wiki, run a health check, generate content, and export HTML through explicit Workflows with visible progress and cancellation. |
| **Keep control** | Review sensitive changes and protect supported write operations with local Git version history. Keep original sources alongside derived pages. |

English and 简体中文 interfaces. Light and dark themes. No application account required.

## Download

**[Get v0.2.1 →](https://github.com/StoneLL1/llm-wiki-desktop/releases/tag/app-v0.2.1)**

| Platform | Download |
| :-- | :-- |
| macOS · Apple Silicon | [DMG installer](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/darwin-aarch64-LLM.Wiki.Desktop_0.2.1_aarch64.dmg) |
| macOS · Intel | [DMG installer](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/darwin-x86_64-LLM.Wiki.Desktop_0.2.1_x64.dmg) |
| Windows · x64 | [Setup installer](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/windows-x86_64-LLM.Wiki.Desktop_0.2.1_x64-setup.exe) |
| Linux · x64 | [AppImage](https://github.com/StoneLL1/llm-wiki-desktop/releases/download/app-v0.2.1/linux-x86_64-LLM.Wiki.Desktop_0.2.1_amd64.AppImage) |

On macOS, drag the app into Applications. On Windows, run the setup file. On Linux, make the AppImage executable and launch it; Ubuntu 24.04 is the CI target.

The macOS app is not Apple-notarized and the Windows installer does not carry an Authenticode identity, so the OS may show a first-launch warning. See [installation notes](docs/release/known-limitations.md) for details. App updates and optional capability packs are signature-verified.

The release also contains automatic-update files and `CHECKSUMS.sha256`. Optional engines are downloaded from the app when needed; you do not need to install all the capability packs.

## Get started

1. **Create or open a knowledge base.** Start in a new folder, or open a compatible Markdown vault. Existing vaults keep their layout.
2. **Import a source.** Add a document or link and review the readable Source. Install an optional import engine only when the format requires it.
3. **Choose your AI.** Configure a provider in Settings, connect local Ollama, or select an installed Agent CLI. Reading, editing, graph navigation, and keyword search work without AI.
4. **Build your wiki.** Run **Update Wiki** from Workflows, inspect the result, and explore the connected pages. Use Chat for questions and HTML export for sharing.

### Bring your own AI

API providers include OpenAI, Anthropic, Google, Ollama, and OpenAI-compatible endpoints. API access and external Agent CLIs are configured separately; this app does not include a paid model subscription.

Credentials use the operating system's credential store. AI actions send task content to the provider or Agent you choose; remote models are not offline. Optional import engines may also download runtimes and models on first use.

### Files you own

Native projects use a readable directory structure:

```text
my-knowledge-base/
├── raw/       Original material and readable Sources
├── wiki/      Connected Markdown pages
├── .app/      Project settings and operation state
├── exports/   Generated HTML and other outputs
└── skills/    Project-specific guidance
```

There is no content database or mandatory cloud sync. Compatible vaults retain their Markdown structure, and app guidance stays under `.app/compat/`. Local version history provides recovery for supported operations; it complements your usual backups.

## Development

Built with **Tauri 2 · React 19 · TypeScript · Rust**.

Use Node.js **22.23.1** and Rust **1.92.0** to match CI, plus the [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/). Git is required for version-history features.

```bash
git clone https://github.com/StoneLL1/llm-wiki-desktop.git
cd llm-wiki-desktop
npm ci
npm run tauri -- dev
```

```bash
npm run check:quick   # Short development check
npm run check         # Frontend, tooling, capability runners, and Rust checks
npm run tauri -- build --config '{"bundle":{"createUpdaterArtifacts":false}}'
```

`npm run dev` starts only the frontend server; use `npm run tauri -- dev` for the desktop app. Local builds use the development capability configuration; signed release catalogs are assembled by the release workflow. The local build command disables updater artifact signing; official releases are signed in CI.

| Resource | Purpose |
| :-- | :-- |
| [Contributing](CONTRIBUTING.md) | Development workflow and verification |
| [Architecture](SPEC/TECH_STACK.md) | Frontend, IPC, and Rust service boundaries |
| [Troubleshooting](docs/maintainers/troubleshooting.md) | Known development pitfalls |
| [Release runbook](docs/release/release-runbook.md) | Builds, signing, and release recovery |
| [Changelog](CHANGELOG.md) | User-visible changes |
| [Security](SECURITY.md) | Private vulnerability reporting |

## License

[Apache-2.0](LICENSE). Optional capability packs include their own third-party licenses and notices inside each archive.
