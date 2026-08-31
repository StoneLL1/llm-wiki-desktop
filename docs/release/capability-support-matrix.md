# Import capability support matrix

This matrix is explanatory. `capabilities/product-manifest.json` is the executable authority, and `node scripts/capability-release-plan.mjs` must reproduce its exact `published definition × supported target` matrix.

| Capability | Routes | Formats / surfaces | Runtime evidence |
| --- | --- | --- | --- |
| browser-runtime-lite | readability, WeChat, Zhihu | generic web, articles | bundled Node, locked parser dependencies, offline fetched-page runner |
| browser-runtime | browser, WeChat, X post | generic dynamic web, public X status pages | bundled Node + Playwright Chromium; public/login/restricted/endpoint policy qualification |
| media-metadata | Bilibili metadata | Bilibili video | bundled Node, target-bound metadata/subtitle policy |
| document-standard | MarkItDown fallback | DOC/DOCX/XLS/XLSX/PPT/PPTX/PDF | relocatable Python + hash-locked MarkItDown wheels |
| document-layout | PDF layout | PDF | relocatable Python + hash-locked Docling + signed offline model inventory |
| office-legacy | LibreOffice conversion | DOC/XLS/PPT | target LibreOffice 26.2.4, locked source digest, isolated disposable profile |
| ocr-basic | basic OCR | PDF/PNG/JPEG/WebP/TIFF/BMP | relocatable Python + signed RapidOCR CPU runtime |
| ocr-cjk-accurate | accurate CJK OCR | PDF/PNG/JPEG/WebP/BMP/TIFF/HEIC/HEIF | RapidOCR PP-OCRv5, Pillow HEIF decoder, PDFium renderer, signed models |
| media-runtime | subtitle/keyframes | GIF/WMA/WMV/SRT/VTT/ASS/SSA/LRC, remote media | bundled Node + audited LGPL FFmpeg |
| asr-sensevoice-small | local ASR | declared audio/video containers including WMA/WMV | SenseVoiceSmall + sherpa-onnx + FFmpeg; accelerator with CPU fallback |
| asr-whisper | accurate local ASR | declared audio/video containers including WMA/WMV | whisper.cpp 1.8.3 + verified model + audited LGPL FFmpeg |

Every published row targets Windows x64, macOS arm64, macOS x64, and Linux x64. The signed `CAPABILITY-CONTRACT.json` inside each archive must exactly match the product definition's routes, extensions, platform content types, protocol, entrypoint, runtime permissions, target, and license before activation.
