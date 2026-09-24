# Session handoff notes (make-disk) — summary

**Languages**: [日本語 (full, authoritative)](../PORTING.md) | English | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> This is a summary of where the work stands and what comes next. The full history of resume notes is the Japanese [`PORTING.md`](../PORTING.md); technical decisions are in [`CLAUDE.md`](../CLAUDE.md).

## Where we are (2026-09-24, v0.1.28)

- Shipped: IMAPI2 burning (real CD-R), AV1/Opus, Dolby/surround preservation, AI noise reduction (RNNoise), DSD64–1024 (DSF) and DoP WAV, high-resolution PCM,
  AI video super-resolution (GPU/CPU), experimental audio bandwidth extension, audio-CD ripping, MKV multiple audio/subtitle tracks.
- v0.1.26: fixed the bug where MP4→CD produced a 0.6-second WAV (cut intermediates are now MKV); ISO/burn is aborted after any failed conversion;
  **fast extract of only the needed range** (input seek; 10 minutes from a 3h33m video in ~1 s); GPU / open-cpu accelerated re-encode of the extract;
  **AI range search** via aruaru-llm (subtitles or transcription → 3 candidates → LLM pick); source/output folders must differ.
- v0.1.27: **upconvert and fill the disc** — DVD 1–2 layer / Blu-ray 1–4 layer × Full HD / 4K; audio subtracted; no source-bitrate cap when filling;
  ceilings of 40 Mbps (Full HD) / 100 Mbps (4K); capacity-derived bitrates use **2-pass** (1-pass overshot 6–7%; 2-pass lands at 96–99%, never over).
- v0.1.28: separate bilingual note that scaling is interpolation (tick 4.3 AI super-resolution for detail).
- Verified on the installed v0.1.28 via WebView2 remote debugging (CDP): the upconvert panel, and the app's own conversion extracting 60 minutes → WAV 3600 s / 635 MB.

## Next

1. Install layout under `%LOCALAPPDATA%\open-easy-web\` (make-disk, aruaru-llm, open-web-server).
2. LLM manager screen: CPU (open-cpu), memory, GPU/VRAM (aruaru-llm `/v1/recommend`), install the recommended / one-size-up / one-size-down LLM (NPU later).
3. Local open-web-server as the gateway to aruaru-llm.
4. Intro and link on easy-web.tokyo; browser control from easy-web.tokyo/make-disk, launching the locally installed make-disk first (`make-disk://` protocol), limited to easy-web.tokyo with a pairing code.
5. Full-size upconvert E2E filling a real disc.

## Related repositories

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — this app
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — Rust takes on FFmpeg / xorriso
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU feature detection
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU compute abstraction
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — hi-res audio pipeline, DSD tools, player
