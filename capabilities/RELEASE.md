# Capability pack release

Capability source folders are declarations and runner source, not installable release artifacts. An installable pack is accepted only when all of these anchors agree:

1. The application-embedded `install-catalog.json` names the exact HTTPS URL, ZIP SHA-256, manifest SHA-256, compressed size, installed size, capability/version/target, and license.
2. A schema v2 `manifest.json` is signed with an Ed25519 key already embedded in `trusted-keys.json`.
3. The signed manifest contains the exact sorted file inventory, entrypoint, entrypoint arguments, and the files whose executable permission may be restored.
4. The installer verifies the catalog digest before extraction, rejects traversal/symlinks/special files and expansion beyond declared bounds, verifies the signed runtime inventory, and only then restores signed executable permissions.

Schema v2 deliberately leaves `archiveSha256` empty and both size fields zero inside the manifest. Putting the ZIP digest inside a manifest contained by that ZIP is self-referential and cannot produce a normal cryptographic artifact. ZIP measurements belong to the application catalog; extracted-runtime measurements belong to the signed file inventory.

Capability packs are maintainer-signed trusted application code. `network: false`
means the reviewed runner uses fixed local inputs and does not initiate
network access; it is not a claim that every supported desktop OS supplies an
equivalent hostile-code network/filesystem sandbox. Archive, inventory,
staging-containment, and runner-policy checks remain mandatory.

## Release key provisioning

Generate the Ed25519 key offline. Never commit the private key, put it in a workflow argument, log it, or save it in an application project.

```powershell
openssl genpkey -algorithm Ed25519 -out capability-release.pem
openssl pkcs8 -topk8 -nocrypt -in capability-release.pem -outform DER -out capability-release.pk8
$env:LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX = ([Convert]::ToHexString([IO.File]::ReadAllBytes('capability-release.pk8'))).ToLowerInvariant()
cargo run --manifest-path src-tauri/Cargo.toml --bin capability_release -- public-key
```

Add only the resulting 32-byte public key hex under a stable key ID in `trusted-keys.json`. Configure the same PKCS#8 hex as the protected GitHub Actions secret `LLM_WIKI_CAPABILITY_SIGNING_KEY_PKCS8_HEX`. The release builder refuses a private key that does not match the committed public key.

## Manifest-derived release matrix

`capabilities/product-manifest.json` is the only pack/route/format/target authority. `scripts/capability-release-plan.mjs` joins it with `release-recipes.json`, `release-sources.json`, and `qualification-corpus.json`; release preflight fails unless every published definition has implemented staging and qualification, locked source ownership, a real fixture for every declared extension, and exactly one entry for each supported target. The current matrix is 11 published packs × 4 targets = 44 archives, but workflows and verifiers must derive that count rather than hard-code it.

Every staged payload contains `CAPABILITY-CONTRACT.json`. The release assembler signs it through the ordinary schema-v2 inventory. At startup and before atomic activation, Rust compares that signed contract with the embedded product definition and rejects missing or drifted routes, formats, platform content types, target, protocol, entrypoint/arguments, license, or runtime policy.

## Node/browser packs

The reusable `Capability release` workflow builds `browser-runtime`, `browser-runtime-lite`, and `media-metadata` for Windows x64, macOS arm64, macOS x64, and Linux x64. It:

- downloads the official pinned Node distribution and verifies its committed SHA-256;
- installs locked npm dependencies and the Playwright-pinned Chromium;
- runs browser policy, platform extraction, and real Chromium launch smoke tests on each target;
- stages Node, signed SBOM/NOTICE/license evidence, runners, dependencies, and Chromium without relying on a system Node installation;
- compiles the Rust release tool with locked dependencies before the signing secret enters the environment, so dependency build scripts cannot read the private key;
- signs schema v2 manifests, verifies the finished ZIP member-by-member, and merges catalog fragments while re-verifying every archive digest, manifest digest, Ed25519 signature, trusted key, and exact immutable tag URL.

The workflow never creates or uploads a GitHub release. It is callable through `workflow_call` from the unified desktop release orchestration, and publication happens only in that final pipeline after desktop installers, the updater manifest, and packaged smoke evidence are complete.

- The workflow emits a `capability-install-catalog` application-integration artifact with the exact `capabilities/install-catalog.json`, `capabilities/trusted-keys.json`, and `capabilities/catalog-provenance.json` paths. `catalog-provenance.json` records the immutable release tag, commit SHA, and workflow run that produced the catalog, so a desktop build can reject catalog artifacts from another run, tag, or commit.

Desktop release builds consume that artifact through an auditable staging input instead of overwriting source files: the release job downloads the artifact from the same workflow run, verifies the catalog contract and provenance with `scripts/verify-capability-catalog.mjs`, stages it, and builds with `LLM_WIKI_CAPABILITY_CATALOG_MODE=release` plus `LLM_WIKI_CAPABILITY_STAGING_DIR`. The Rust build script fails closed on an empty release catalog or a missing trusted key, copies the exact staged bytes into the binary, and writes an embed record that in-tree tests cross-check. After the build, `scripts/verify-embedded-capability-catalog.mjs` reverse-verifies that the finished binary embeds the exact staged catalog bytes.

Development builds keep the explicit source fallback (`LLM_WIKI_CAPABILITY_CATALOG_MODE=source` or unset) and may embed the empty placeholder catalog. The source-tree `capabilities/install-catalog.json` and `capabilities/trusted-keys.json` remain the reviewed fallback: after a release catalog is generated and reviewed, those files must still be committed together and the application rebuilt so source builds embed the new trust inputs. Publishing artifacts without rebuilding the application does not make them trusted or discoverable.

On Linux, Node and Chromium application bytes are bundled, but desktop shared libraries are intentionally supplied by the host OS. `BUILD-PROVENANCE.json` contains the exact shared-library support contract. The release workflow qualifies on Ubuntu 24.04 after Playwright installs that dependency baseline; the runtime reports launch failures as missing-host-dependency errors rather than claiming a fully static Linux bundle.

## Document, OCR, media, and ASR packs

The protected workflow also builds document-standard, office-legacy, both OCR profiles, media-runtime, SenseVoice, and Whisper on all four desktop targets. document-layout ships three targets: PyTorch has published no x86_64 macOS wheels since 2.2.2, so `pdf.layout` is unavailable on Intel macOS and document-standard remains installable there. Each capability publishes exactly its product-manifest `supportedTargets` subset, and the release catalog records that per-capability matrix. Source folders and placeholder manifests remain fail-closed; the desktop binary cannot install a capability until the protected workflow succeeds, qualification uses the staged runtime, and the same-run catalog/trust/provenance inputs are embedded in the desktop build.

SenseVoice release jobs:

- verify the exact sherpa-onnx 1.13.4 target runtime, SenseVoiceSmall int8 model, Node 22.17 runtime, and FFmpeg 8.1.2 archive/source build;
- stage an LGPLv3-or-later shared FFmpeg build with GPL/nonfree and runtime networking disabled;
- decode and transcribe both the official Chinese WAV and an AAC-in-M4A derivative through the staged JSON-RPC runner;
- prefer CUDA on Windows/Linux or CoreML on macOS, detect sherpa's silent CPU fallback, and report the provider that actually ran;
- require CPU to complete in the same task whenever an accelerator is absent or fails.

The macOS FFmpeg payload is built from the pinned source archive on the pinned
macOS 15 runner family. Its compiler, make, Xcode, SDK, OS release, configure
recipe, and source digest are recorded in signed build provenance. Apple-hosted
runner images and SDKs can be serviced in place, so these source-built payloads
are not claimed to be bit-for-bit identical across runner-image revisions.

RapidOCR release jobs:

- verify a relocatable CPython 3.12.13 runtime for Windows x64, macOS arm64/x64, and Linux x64;
- install RapidOCR 3.8.1, ONNX Runtime, OpenCV, Pillow HEIF, PDFium, and all transitive dependencies from the committed hash lock using prebuilt wheels only;
- bundle the exact PP-OCRv5 mobile detector, recognizer, orientation classifier, and dictionary;
- run the official `ch_en_num.jpg` fixture plus the repository HEIC/HEIF and scanned-PDF fixtures through the staged offline JSON-RPC runner and require Chinese text, coordinates, confidence, and evidence labeling;
- emit the dependency lock, source provenance, notices, and SPDX SBOM into the signed payload. Cloud OCR and runtime model downloads are forbidden.

Primary implementation references:

- [sherpa-onnx Rust crate installation](https://k2-fsa.github.io/sherpa/onnx/rust-api/install.html)
- [sherpa-onnx SenseVoice model and API](https://k2-fsa.github.io/sherpa/onnx/sense-voice/index.html)
- [Playwright browser installation](https://playwright.dev/docs/browsers)
- [Node.js distribution index](https://nodejs.org/dist/)
