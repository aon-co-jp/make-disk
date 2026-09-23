# Session handoff notes (make-disk) — summary

**Languages**: [日本語 (full, authoritative)](../PORTING.md) | English | [简体中文](PORTING.zh-CN.md) | [繁體中文(台灣)](PORTING.zh-TW.md) | [한국어](PORTING.ko.md) | [Deutsch](PORTING.de.md) | [Français](PORTING.fr.md) | [Русский](PORTING.ru.md) | [Українська](PORTING.uk.md) | [فارسی](PORTING.iran%28Perusha%29.md) | [العربية](PORTING.ar.md)

> This is a summary of where the work stands and what comes next. The full history of resume notes is the Japanese [`PORTING.md`](../PORTING.md); technical decisions are in [`CLAUDE.md`](../CLAUDE.md).

## Where we are (2026-09-23)

- Shipped so far: IMAPI2 burning (verified with real CD-R), AV1/Opus, Dolby/surround preservation, AI noise reduction (RNNoise), DSD64–1024 (DSF) and DoP WAV,
  high-resolution PCM, AI video super-resolution (GPU/CPU), experimental audio bandwidth extension, audio-CD ripping (verified with a real disc), MKV multiple audio/subtitle tracks.
- 2026-09-23: fixed the cut editor that could not be operated from section 8; DSD no longer creates PCM alongside it; exclusive YES/NO "cut by size / cut by time" (size by default);
  "fill the disc" and "AI silence cut" as post-processing checkboxes; playback-standard limits (CD / DVD-Video / DVD-Audio / Blu-ray / UHD Blu-ray / PC only); multilingual docs.

## Next

1. Real-file E2E of the 2026-09-23 features: cut positions for size/time, `-ar` / bit depth / bitrate caps per playback standard, fill-the-disc bitrate.
2. Audio-CD (CD-DA) burning (IMAPI2 TrackAtOnce; currently data discs only).
3. Move `rs-*` tools out of the installer and fetch them on demand from the sister repositories' releases.
4. Export "video + DSD audio" as a set (video file + `.dsf` + `.obar.json` for `open-bar`), and split the delta-sigma modulator out into `open-mqa-dsd`.
5. Unprotected BD/DVD ripping (copy-protection circumvention will not be implemented).

## Related repositories

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — this app
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) / [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — Rust takes on FFmpeg / xorriso
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU feature detection
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU compute abstraction
- [aon-co-jp/open-mqa](https://github.com/aon-co-jp/open-mqa) / [aon-co-jp/open-mqa-dsd](https://github.com/aon-co-jp/open-mqa-dsd) / [aon-co-jp/open-bar](https://github.com/aon-co-jp/open-bar) — hi-res audio pipeline, DSD tools, player
