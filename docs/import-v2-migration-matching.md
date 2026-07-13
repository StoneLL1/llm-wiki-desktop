# Import V2 migration matching

Migration links are intentionally conservative. The planner never treats a
title, filename, timestamp, or fuzzy text similarity as identity evidence.

| Evidence | Automatic result |
| --- | --- |
| Stable source ID and exact content hash resolve to the same V2 source | `link_existing` |
| Exact original hash resolves uniquely and the legacy destination path is explicit and unique | `link_existing` |
| Exact content hash and exact normalized public URL resolve to the same V2 source | `link_existing` |
| A valid legacy record has no V2 identity evidence | `create_v2_record` proposal when its identity is complete |
| Missing, duplicated, contradictory, or case-colliding evidence | `conflict` or `legacy_unmanaged` |

Low confidence is a safe and expected outcome. A human can resolve a conflict
later; an automatic link cannot be safely undone without changing evidence.
