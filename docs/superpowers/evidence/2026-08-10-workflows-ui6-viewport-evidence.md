# Workflows UI-6 viewport evidence

Date: 2026-08-10

## Method

- Ran the real Tauri v2 application against the Vite development server and inspected its WebView2 DOM through the local DevTools protocol.
- Staged deterministic overview and history summaries directly in the frontend Zustand stores after the real Workflows surface mounted. No workflow command was launched and no project content was written.
- Used WebView device metrics for the viewport matrix. The 1181 px request reported `innerWidth = 1182` because of WebView device-pixel rounding; it remained above the 1180 px dock/overlay breakpoint.
- The current project is in recovery mode, so its existing recovery banner appears in the captures. That shared banner is not changed by UI-6.

## Captures

- [1440 × 900, English overview](2026-08-10-workflows-ui6-1440x900.png)
- [820 × 900, English stress reflow](2026-08-10-workflows-ui6-820x900.png)
- [1181 × 900 request, maximum persisted panes, English history](2026-08-10-workflows-ui6-1181x900-max-panes-history.png)
- [1440 × 900, Chinese overview](2026-08-10-workflows-ui6-1440x900-zh.png)

## Measured results

| Scenario | Measured Workflows width | Row geometry | Overflow / status result |
| --- | ---: | --- | --- |
| 1440 × 900 overview | 971 px | available rows `88 / 88 / 88`; recent rows `54 / 54 / 54 / 54 / 53` | no horizontal overflow; Attention has one status icon and its workflow-kind icon does not spin |
| 820 × 900 overview | 758 px | container query active; available rows `154 / 154 / 154`; grid `30px 664px` | no horizontal overflow; right panel is an overlay, not a docked pane |
| 1181 × 900 request + sidebar 360 + right panel 520 | 290 px | five virtualized History rows are all `88px`; inner History grid reflows to two columns | no horizontal overflow in row or run controls |
| 1440 × 900 Chinese overview | 971 px | same desktop geometry | no horizontal overflow; visible status labels include `运行中 / 可运行 / 已是最新 / 已完成 / 失败` |

## Loading and reduced motion

- Loading skeleton renders the final section shapes: three workflow rows and five recent rows.
- With `prefers-reduced-motion: reduce` emulated in the same DevTools session, both the skeleton pulse and running-status icon report computed `animation-name: none`.
- The Workflows-only narrow context overlay, stage disclosure, and result transition are also covered by the CSS contract test that requires their reduced-motion override.

## Acceptance conclusion

UI-6 meets the viewport acceptance matrix for stable row geometry, container-aware stress reflow, non-duplicated icon-and-label statuses, English/Chinese fit, and reduced-motion behavior. No Health route availability or copy was changed.
