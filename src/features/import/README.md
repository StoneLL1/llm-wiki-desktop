# Import Feature

The Import V2 workbench keeps source entry, the queue, preview, recovery, and
raw-source commit confirmation in one workflow. History and capability-pack
readiness are separate header sections; they do not duplicate the source-entry
surface or expose a second commit action.

File, platform, and extraction-ability badges come from the backend readiness
DTO. Bilibili imports prefer verified platform subtitles, preserve normalized
transcript segments as evidence, and otherwise pause at
`waiting_authorization` before local ASR. The UI must never infer ASR consent
from pack availability.
