# RapidOCR PP-OCRv5 capability pack

This source folder defines the release input for local CJK OCR. Published
packs are signed per target and contain a relocatable CPython 3.12 runtime,
hash-locked RapidOCR 3.8.1 and ONNX Runtime 1.23.2 dependencies, and the
official PP-OCRv5 mobile detection, recognition, and orientation models.

The runner is CPU-only, blocks Python socket APIs, accepts only an image
inside the current Import v2 staging workspace, verifies the signed model
inventory and upstream model hashes, and emits labeled OCR evidence plus
coordinates and confidence metadata. It never downloads a model or uses a
cloud OCR fallback.

Installed capability packs are maintainer-signed trusted application code.
The socket guard and fixed local model paths are privacy defenses, but the
desktop process does not claim to provide a cross-platform hostile-code OS
sandbox around native dependencies.

The aggregate release license also covers the LGPL components redistributed
inside the pinned binary wheels: FFmpeg in opencv-python, Qt/related libraries
on Linux and macOS, and GEOS in Shapely. Their upstream license files remain
in the signed runtime and are indexed by the generated NOTICE and SPDX SBOM.

Source declarations are not installable. Release CI must fetch every pinned
archive, install only hash-locked wheels, run the official Chinese fixture,
sign the complete file inventory, and publish a catalog entry before the
desktop app can install the pack.
