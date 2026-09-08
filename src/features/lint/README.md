# Lint Feature

Local deterministic lint, Agent deep lint, issue review, and guarded repair
flows live here. Restricted projects may run bounded local read-only checks.
Deep external checks require trust; repair requires trusted writable access,
backend revalidation, and the required checkpoint/confirmation policy.

## Health reports

Health reports share this finding surface. Their optional `execution` evidence records the actual input generation, freshness and deep-check coverage; old reports remain readable with unknown freshness. Viewing recalculates freshness without rewriting the saved report digest. A failed or cancelled Complete task can still open its completed local portion. Persistence follows project app-state access, while Agent repair eligibility remains a separate current-report contract.

Workflows is the only check launcher and task owner. `LintTaskStatus` reads its progress and cancellation state from the global task store. Legacy deep reports remain readable, but Lint does not maintain a second deep-task lifecycle. A local refresh keeps the previous report visible, replaces the deterministic findings and retains AI evidence with stale freshness. Such a combined view is not eligible for Agent repair until a new complete report is opened.

The finding list groups once and mounts at most 100 findings per page. Group totals and the visible range remain explicit; repair selection spans pages, and changing reports or filters returns to the first page. This bounds rendering without measuring or fixing row heights.

## Decision Gate H status

The H5 repair entry point selects only eligible findings from the current persistent Agent Health report. Semantic findings remain manual unless an existing deterministic lint recheck proves resolution; there is no semantic lint engine or BYOK repair fallback. H6 keeps Decision Gate H and Batch 7 blocked until the full gate and remaining validation matrix are green.
