# SenseVoiceSmall capability pack

This source declaration is the release input for the default local ASR pack.
The published pack is a signed, target-specific ZIP containing a native
JSON-RPC runner, sherpa-onnx, the pinned SenseVoiceSmall int8 model, and an
LGPLv3-or-later FFmpeg audio decoder. CPU execution is mandatory; CUDA or CoreML is
selected when the target-specific runtime is present and falls back to CPU if
provider initialization fails.

For media containers, the runner first asks the verified local FFmpeg binary
for the first embedded subtitle stream. A valid SRT conversion is emitted as
`authorized-local-embedded-subtitle`; the SenseVoice model is loaded only when
that subtitle path is absent or unusable.

The fixed runner commands initiate no network access and accept media only
from the current import staging tree. Installed capability packs are
maintainer-signed trusted application code; the desktop process does not claim
to provide a cross-platform hostile-code OS sandbox around them.

The repository intentionally does not contain generated binaries, the model,
signatures, or a fake development payload. Release CI must fill the manifest
inventory and capability catalog only after qualification succeeds.

## Local desktop development

`npm run tauri dev` prepares the pinned Windows/macOS/Linux pack on first use,
qualifies the real runner, and stores it under the ignored
`.dev-capabilities/` directory. The small Node runtime cache is retained, while
the large source cache and unused Windows CUDA/TensorRT provider payloads are
removed after CPU qualification to keep local disk use bounded. The preparation
step creates an ephemeral Ed25519 key pair, persists only the public key plus
the signed manifest, and never writes the private key to disk.

Debug builds accept that public key only from the repository-local development
directory. Release builds continue to use the embedded maintainer trust store
and never load the development key. Set `LLM_WIKI_SKIP_DEV_CAPABILITY=1` only
when intentionally testing the missing-capability UI.
