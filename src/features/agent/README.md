# Agent Feature (Historical UI Boundary)

The user-facing Agent page, right panel, generic Run Agent dialog, and their controller were retired in Workflows Batch 8. This folder is retained only as a historical ownership marker; it contains no active React surface.

Target behavior is defined by [`../../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md`](../../../docs/superpowers/specs/2026-07-30-workflows-panel-redesign.md), tracked in [`../../../SPEC/roadmap/agent.md`](../../../SPEC/roadmap/agent.md), and executed through the [batched implementation plan](../../../docs/superpowers/plans/2026-07-30-workflows-panel-implementation.md).
Project trust and access behavior is defined by [`../../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md).

- Active product UI lives in `src/features/workflows/`.
- Agent CLI detection, execution, logs, cancellation, types, and backend services remain reusable technical capabilities and keep their Agent names.
- Agent, BYOK, model, and Provider configuration remains in Settings; the sidebar Agent status foot remains.
- Do not restore a generic launcher, arbitrary prompt/Skill selection, or cross-project task surface here.
