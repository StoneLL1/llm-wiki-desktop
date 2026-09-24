# Media text qualification fixtures

Synthetic text-only clips for the media-runtime video-frame OCR route, with no audio or third-party content. Each file contains three seconds of the same readable opening scene at 640px width and 2 fps. Containers use MPEG-4 Part 2 (MP4/M4V/MOV/MKV/AVI), MSMPEG4v3 (WMV), VP8 (WebM), and GIF.

The original generic matrix videos contain no stable text, so successful decoding correctly yields no OCR candidate. These fixtures exercise positive frame extraction; the qualification script also checks extension masquerades, corrupt input, Unicode paths, cancellation, and nonempty contained OCR frame files.
