# Lint Feature

Local deterministic lint, Agent deep lint, issue review, and guarded repair
flows live here. Restricted projects may run bounded local read-only checks.
Deep external checks require trust; repair requires trusted writable access,
backend revalidation, and the required checkpoint/confirmation policy.

## Health reports

Health reports share this finding surface. Their optional `execution` evidence records the actual input generation, freshness and deep-check coverage; old reports remain readable with unknown freshness. Viewing recalculates freshness without rewriting the saved report digest. A failed or cancelled Complete task can still open its completed local portion. Persistence follows project app-state access, while Agent repair eligibility remains a separate current-report contract.

## Decision Gate H status

The H5 repair entry point selects only eligible findings from the current persistent Agent Health report. Semantic findings remain manual unless an existing deterministic lint recheck proves resolution; there is no semantic lint engine or BYOK repair fallback. H6 keeps Decision Gate H and Batch 7 blocked until the full gate and remaining validation matrix are green.
