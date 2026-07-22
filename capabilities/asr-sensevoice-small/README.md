# SenseVoiceSmall capability pack

This source declaration is the release input for the default local ASR pack.
The published pack is a signed, target-specific ZIP containing a native
JSON-RPC runner, sherpa-onnx, the pinned SenseVoiceSmall int8 model, and an
LGPLv3-or-later FFmpeg audio decoder. CPU execution is mandatory; CUDA or CoreML is
selected when the target-specific runtime is present and falls back to CPU if
provider initialization fails.

The fixed runner commands initiate no network access and accept media only
from the current import staging tree. Installed capability packs are
maintainer-signed trusted application code; the desktop process does not claim
to provide a cross-platform hostile-code OS sandbox around them.

The repository intentionally does not contain generated binaries, the model,
signatures, or a fake development payload. Release CI must fill the manifest
inventory and capability catalog only after qualification succeeds.
