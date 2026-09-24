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

- **2026-09-24 Continued 29 — AI upscaling rebuilt, CPU/GPU auto-selection, RIFE interpolation, fit prediction**:
  - **`engine/ai_video.rs` (new)**: streaming (raw RGB over a pipe), resumable (committed every 240 frames), no frame limit. Interlace/telecine detection (idet), black-bar detection (cropdetect), BT.601->709, DAR preserved, flat/still-frame reuse, ETA, cancel (`progress.rs`, event `make-disk-progress`).
  - **`engine/hw_bench.rs` (new)**: measures CPU / GPU / both on this PC (cached; re-measured if the CPU or build type changes) and picks a method only if it is at least 1.08x faster. **Measured here (GT 730): CPU 1.27 s/frame, GPU 2.76 s + 1.6 s launch, both ~1.3x faster.** `engine/sr_pool.rs` shares the work (the GPU takes several frames per launch to amortize the fixed cost).
    open-cuda has no convolution op, so the AI math runs on our own CPU kernel (open-cpu detects the ISA) and NCNN-Vulkan; open-cuda / open-directx / aruaru-llm do not speed it up (only GPU detection and search help) — disclosed honestly.
  - **`engine/rife.rs` (new)**: RIFE (20221029, rife-v4.6). Only ~12 MB of the ~411 MB zip is fetched by HTTP range requests. Segments are committed for resume. **On the GT 730 the default `-j 1:2:2` silently yields black frames**, so it runs with `-j 1:1:1`, output brightness is checked and broken segments are replaced by frame repetition. Fully static segments skip RIFE. Real test (10 fps -> 20 fps, 60 frames) passes.
  - **`engine/fit_predict.rs` (new)**: rates fit (verdict marks) from disc capacity and bits-per-pixel (scaling with fps^0.6), capped by the standards' bitrate limits (Full HD 40 / 4K 100 Mbps), with static share sampled via blackdetect/freezedetect. Exposed as `ai_fit_predict` behind the "Predict fit" button in section 6.
  - **Bugs found by real tests and fixed**: GPU-only mode leaving the last frame unprocessed (hang; collector ignoring disconnects), black GPU outputs (checked and replaced by bicubic), a debug-build benchmark being cached and mis-selected (build type added to the signature).
  - **Not done / limits**: realesr-general-x4v3 is not bundled; a feature-length run (days) is unverified; RIFE speed at 4K is unmeasured; fit prediction covers only the first file, not the sum of several; `realesrgan-x4plus` needs a GPU.
  - **Next**: open-easy-web install layout (`%LOCALAPPDATA%\open-easy-web\`), LLM manager (NPU postponed), local open-web-server, easy-web.tokyo integration and `make-disk://` launch.
