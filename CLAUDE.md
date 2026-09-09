# CLAUDE.md

Read [AGENTS.md](AGENTS.md) for the shared repository policy, product/safety constraints, design authorities, check levels, review workflow, and local logging rules. Apply its working agreement to Claude Code as well. Keep shared rules in that file rather than mirroring them here.

Load only the feature documents needed for the current task, using `.codex/skills/llm-wiki-desktop-context/references/project-map.md` as a navigation aid. Current product authorities take precedence over historical plans and implementation drift.

Additional implementation pointers (verify against the relevant specification):

- Project-owned content and derived state remain local files; global recents and trust state belong in app configuration, and secrets belong in OS credential storage.
- Source-only Graph/Chat is supported; compilation into Wiki is a separate explicit workflow. Do not restore compile-after-import behavior.
- The selected Agent/BYOK execution route must not silently switch when unavailable. BYOK is not an Import parser or recovery route.
- Thin IPC uses typed DTOs and `BackendError`; high-risk knowledge-base actions use the backend confirmation contract (`PendingAction`) where applicable.
- All project file access is validated against `ProjectContext`; frontend disabled controls do not establish authorization.

These describe application behavior, not extra approval gates for routine repository development. A coding task must continue after setup, checks, and reviews until the requested outcome is delivered or a concrete blocker is reported.
