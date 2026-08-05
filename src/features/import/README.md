# Import Feature

Product authority: [`../../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md`](../../../docs/superpowers/specs/2026-07-24-import-source-media-flow-design.md). Project creation/opening authority: [`../../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md`](../../../docs/superpowers/specs/2026-07-30-first-run-project-open-workbench-design.md).

The Import V2 workbench keeps source entry, the queue, preview, recovery, and
raw-source commit confirmation in one workflow. History and capability-pack
readiness are separate header sections; they do not duplicate the source-entry
surface or expose a second commit action.

Every successful commit produces both immutable evidence and a readable Source;
Import never starts Update Wiki automatically. Discovery and preview may run
locally in restricted/read-only mode, but commit requires backend-verified
trust, writable filesystem access, and layout-provided app/evidence/Source
write roots. Source AI organization additionally requires trust before external
execution; applying its candidate revalidates writability, hashes, and Git
policy.

Import always targets the currently open knowledge base. A selected directory is
a batch source input to preview and copy; this feature does not open a knowledge
base, initialize a directory in place, move original materials, enable a
compatibility layer, or repair a project. Those decisions stay in the typed
project-open flow. After creating a knowledge base, the app navigates here
without automatically opening a system picker.

File, platform, and extraction-ability badges come from the backend readiness
DTO. Bilibili imports prefer verified platform subtitles, preserve normalized
transcript segments as evidence, and otherwise pause at
`waiting_authorization` before local ASR. The UI must never infer ASR consent
from pack availability.

Batch processing is operation-owned: one backend operation task carries
cancellation and aggregate progress, while Import session/item JSON remains the
per-item source of truth. The frontend consumes bounded
`import://session-patch` cohorts with one bulk store update and one terminal
summary refresh; it does not synthesize one global task per item. Saved file
scans and aggregate confirmation thresholds are backend-owned. React may show
their totals and submit an accept/discard token, but it never rescans paths,
decides thresholds, mutates source files, or treats app-state persistence as
content write authority. The legacy per-item start IPC remains an explicit
small-cohort compatibility boundary, not the primary workflow.

Aggregate and per-spreadsheet risk are separate saved-scan acknowledgements:
aggregate acceptance admits only ordinary files and keeps large CSV/XLSX (or
an XLS with an incomplete output estimate) pending. Both stages retain the
same scan token and are revalidated under the backend project-write authority
critical section. A discovery hard-limit error never becomes a partial scan.
