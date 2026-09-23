# Development policy & environment rules (make-disk) — summary

**Languages**: [日本語 (full, authoritative)](../CLAUDE.md) | English | [简体中文](CLAUDE.zh-CN.md) | [繁體中文(台灣)](CLAUDE.zh-TW.md) | [한국어](CLAUDE.ko.md) | [Deutsch](CLAUDE.de.md) | [Français](CLAUDE.fr.md) | [Русский](CLAUDE.ru.md) | [Українська](CLAUDE.uk.md) | [فارسی](CLAUDE.iran%28Perusha%29.md) | [العربية](CLAUDE.ar.md)

> This is a summary. The full text, including the complete development history (HANDOFF entries), is the Japanese [`CLAUDE.md`](../CLAUDE.md).
> Rules shared by all repositories (autonomous continuation, thorough verification, etc.) follow `CLAUDE.md` in [`open-raid-z`](https://github.com/aon-co-jp/open-raid-z).

## Role of the repository

A GUI app (Rust + Tauri) for burning CD/DVD/Blu-ray, converting audio/video and writing ISO images, with one codebase for Windows/macOS/Linux (and Android).
Platform differences are confined to the installers (bundles); the app itself (`src-tauri/src`, `src`) is a single codebase.

## Architecture

- Frontend: `src/` (vanilla JS; calls Rust commands through Tauri IPC via `window.__TAURI__`, because bare imports do not work without a bundler).
- Backend: `src-tauri/src/engine/`
  - `probe.rs` — duration, codecs, sample rate, Dolby traits via ffprobe (rs-ffmpeg fallback)
  - `convert.rs` — ffmpeg conversion, bitrate control, trimming, multi-range cuts (stream copy by default, GPU encoder for frame-accurate cuts)
  - `capacity.rs` — max bitrate from disc capacity and a 4-level quality warning
  - `dsd.rs` — own PCM→1-bit delta-sigma modulator and DSF writer, DoP WAV
  - `cdda.rs` — audio-CD ripping (Windows, simple secure read)
  - `iso.rs` / `burn.rs` / `windows_imapi.rs` — ISO building and burning (IMAPI2 on Windows, xorriso elsewhere)
  - `ai_upscale.rs` / `cpu_sr.rs` / `audio_sr.rs` — Real-ESRGAN (GPU/CPU), audio bandwidth extension
  - `mkv_tracks.rs` — keeping/adding multiple audio and subtitle tracks in MKV
  - `plugins.rs` / `sidecar.rs` — bundled/downloaded tools with version management
  - `cpu.rs` — CPU feature detection via `open-cpu` (speed hints and x264 `-preset` choice)

## Fixed policies

- **No circumvention of copy protection** (CSS/AACS etc.). Unprotected discs only.
- **No new generation of Dolby Vision / Atmos / IMAX / 4DX** (licensed); preservation by stream copy and compatible lower formats only.
- **No MQA** (patents/trade secrets). Hi-res output follows the open-format route of `aon-co-jp/open-mqa`.
- **When creating DSD, no PCM is created alongside it** (players convert DSD to PCM automatically on hardware without DSD). DoP WAV is DSD data, so it is allowed.
- DSD modulation stays **sequential per channel (bit-exact)**; segment-parallel modulation was measured to destroy SNR and was rejected.
- "AI" features are disclosed honestly: silence-based auto-cut and "AI-optimized" resolution/FPS are heuristics; audio bandwidth extension is synthesis.
- Full HD on DVD is outside DVD-Video; set-top players do not downscale automatically. The UI keeps this warning (a DVD-Video + Full HD file combo was declined by the user).
- Documentation languages: Japanese is authoritative; `README/`, `CLAUDE/`, `PORTING/` hold English, Simplified Chinese, Traditional Chinese (Taiwan), Korean, German, French, Russian, Ukrainian, Persian (Iran, file suffix `.iran(Perusha)`) and Arabic (CLAUDE/PORTING as summaries).

## Verification rules

- CI success is not proof of working. Before reporting completion, verify for real: `npm run lint` (eslint `no-undef`), a browser check of the UI with stubbed Tauri APIs, Rust tests (`cargo test --lib -- --test-threads=1`) and, where possible, real-file / real-hardware E2E.
- Honestly state what has not been verified.

## Releases

Pushing a `v*` tag makes `.github/workflows/release.yml` build Windows/macOS/Linux/Android installers and publish them on GitHub Releases.

## Latest state (2026-09-23)

Fixed the cut editor that could not be operated from section 8; stopped creating PCM alongside DSD; redesigned cutting as exclusive YES/NO "cut by size / cut by time" (size by default) with "fill the disc" and "AI silence cut" as post-processing checkboxes;
added playback-standard limits (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / PC only) for max kHz, bit depth and bitrate. Real-file E2E of these features is still pending.
