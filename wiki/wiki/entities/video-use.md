---
title: video-use
created: 2026-04-23
updated: 2026-04-23
type: entity
tags: [tool, multimodal, code, open-source]
sources:
  - video-use-claude-code-video-editing
  - raw/articles/2026-04-21-video-use-claude-code-video-editing.md
---

# video-use

## Overview

**video-use** is an open-source [[claude-code]] skill created by the **browser-use team** for AI-driven video editing. It enables users to place raw footage in a folder, converse with Claude Code, and receive a fully edited `final.mp4` — automatically removing filler words, adding subtitles, applying color grading, and more.

GitHub: [browser-use/video-use](https://github.com/browser-use/video-use)

The creator's motivation: "I didn't want to keep paying for video editing software."

## Pipeline

The complete editing pipeline follows six stages:

1. **Transcription** — Each source file processed via ElevenLabs Scribe for word-level timestamps, speaker diarization, and audio events (laughter, applause, sighs)
2. **Packing** — All takes compiled into `takes_packed.md` (~12KB), the LLM's primary reading view
3. **LLM Reasoning** — Claude analyzes the packed transcript to make editing decisions
4. **EDL Generation** — Edit Decision List created with precise cut points at word boundaries
5. **ffmpeg Rendering** — Video assembled with cuts, color grading, subtitles, and transitions
6. **Self-Evaluation** — Each cut point verified via timeline_view for frame jumps, audio pops, subtitle occlusion. Auto-fixes up to 3 cycles before presenting to user

## Core Design: Audio-First Approach

This is the most innovative aspect of video-use. The LLM does **not** watch video frames — it reads video through a two-layer structure:

### Layer 1: Audio Transcription (always loaded)
Each source file produces word-level timestamps in a compact format:
```
## C0103 (duration: 43.0s, 8 phrases)
[002.52-005.36] S0 Ninety percent of what a web agent does is completely wasted.
[006.08-006.74] S0 We fixed this.
```

### Layer 2: Visual Synthesis (on-demand)
`timeline_view` generates a single PNG containing filmstrip thumbnails, waveforms, and word-level annotations — only called at critical decision points (e.g., judging ambiguous pauses, comparing beat-matched segments, verifying cut points).

**Efficiency comparison**: Naive approach = 30,000 frames × 1,500 tokens = 45M tokens of noise. video-use = 12KB text + a few PNGs.

This mirrors browser-use's philosophy of giving LLMs structured DOM rather than screenshots — applied to video.

## Features

- **Filler word removal**: Automatic cutting of "um," "ah," false starts, and inter-clip silence
- **Color grading**: Per-segment grading with presets (warm cinematic, neutral impact) or custom ffmpeg chains
- **Audio fade**: 30ms fade-in/fade-out at every cut point to eliminate pops
- **Subtitle burning**: Default two-word-group all-caps, fully customizable
- **Animation overlays**: Via Manim, Remotion, or PIL — multiple animations processed in parallel
- **Self-evaluation**: Post-render verification at every cut point
- **Session persistence**: `project.md` saves state for resuming across sessions

## Installation

```bash
git clone https://github.com/browser-use/video-use
cd video-use
ln -s "$(pwd)" ~/.claude/skills/video-use
pip install -e .
brew install ffmpeg       # required
brew install yt-dlp       # optional, for downloading online footage
```

## Design Principles

- Text plus on-demand visuals — no frame dumping; transcript is the interface
- Audio first, visuals follow — cut points from speech boundaries and silence gaps
- Strategy confirmation before execution, self-evaluation after, persistence after that
- No assumptions about content type — observe first, ask, then edit
- 12 hard rules for production correctness, artistic freedom for everything else

## Dependencies

- Requires ElevenLabs API key for transcription
- ffmpeg for video/audio processing
- Optional: yt-dlp for downloading source material
- Uses [[remotion]] and [[manim]] frameworks for animation generation

## Relationships

- Created by the browser-use team (also creators of the [[browser-use]] framework)
- Implemented as a [[claude-code]] [[skills|skill]]
- Uses [[ffmpeg]] for rendering
- Compatible with [[claude-md]] project configuration
- Part of the broader AI content creation tools ecosystem alongside [[ppt-master]]

## See Also

- [[claude-code]] — Anthropic's CLI agent that runs video-use
- [[skills]] — the SKILL.md modular capability framework
- [[browser-use]] — browser automation framework by the same team
- [[ppt-master]] — companion tool for AI-generated presentations
