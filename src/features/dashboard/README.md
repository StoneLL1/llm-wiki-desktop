# Dashboard Feature

Project health, recent pages, Source/Wiki/Graph readiness, import state,
execution-route availability, and task summaries live here.

Native, compatible, read-only, and recovery projects use the same Dashboard
information structure. The backend supplies layout, trust, access, capability,
Git, scan, and recovery state; React must not infer them from file names.
Restricted or deep-scanning projects may show bounded/partial local results and
the truthful next action without persisting a cache or implying that the
project is healthy.

With no open knowledge base, Dashboard is not rendered as a fake project. The
persistent app shell instead shows the exact two actions defined by
[`../../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md).
