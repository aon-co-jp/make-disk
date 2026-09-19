# make-disk

[日本語](#日本語) | [English](#english)

---

## 日本語

プラットフォーム共通コード(Rust + Tauri)によるCD/DVD/Blu-ray書き込み・
音声/動画フォーマット変換GUIアプリ。インストーラーのみをOSごとに分ける
方針で、アプリ本体は単一コードベース。

### 機能

- **入力/出力**: 複数のソースファイル(音声/動画/PDF)と出力先フォルダを選択。
  変換・ISOイメージ出力・ディスク書き込みを、チェックボックスで複数同時に選択可能
  (複数フォーマット・複数ファイル・複数ディスク種別は非同期・並列実行)。
- **音声**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3(5.1〜7.1)** /
  **DSD64・128・256・512・1024(DSF)**。DSDはffmpegが書き出せないため、PCM→1bitのΔΣ変調
  (5次、チャンネル並列)とDSF書き出しを自前実装(実ffmpegのDSFデコーダで往復検証済み)。
- **動画**: MP4 / MKV / AVI / MOV / WebM / **AV1**(libsvtav1優先・無ければlibaom) /
  **HEVC 10bit(HDR10)** / **無変換コピー**(Dolby Vision・Atmos・5.1/7.1サラウンドをそのまま保持)。
  解像度(720×480〜8K・PAL・カスタム・AI最適化)とFPS(24/30/60/120・カスタム)を指定可能。
  ディスク変換の方向(BD→DVD: 通常DVD解像度またはフルHD、DVD→BD: フルHDまたは4K)で選択肢を絞り込み。
- **高解像度PCM(R-2R等マルチビットDAC向け)**: 352.8kHz/384kHz(24bit・32bit)、705.6kHz/768kHz(32bit)のWAV。使える最高品質のリサンプラ
  (soxr、無ければswresampleの高精度設定)+TPDFディザ。実測SNR(厳密な基準正弦波との比較): 352.8kHz/24bit=141.2dB、384kHz/32bit=150.2dB。
  DSDは仕様上1bitのため、DSDと同時にPCM版(FLAC 24bit/352.8kHz)も並べて出力できる(DSD非対応機器向け。ファイル内の自動フォールバックではなく再生側で選択)。
  DSDの実測往復SNR: DSD64=99.6dB、DSD128=132.4dB。
- **AI超解像(映像、GPU)**: Real-ESRGAN(NCNN-Vulkan、MIT)をオンデマンドのプラグインとして利用(DVD→4K等)。
- **AIノイズ除去**: 本物の学習済みニューラルネット(RNNoise)を同梱。実モデルでノイズ低減を検証済み。
  ただし主に人の声で学習されており、音楽では効果が控えめ。
- **ビットレート/容量**: 固定・ディスク容量から自動算出・「最高音質・最高画質」モード(未選択時は
  自動でロスレスWAV+ISO化)・サイズ指定・時間指定・AI判断(無音検出)自動カット。
  自動ビットレートは元ファイルのビットレートで頭打ち。音声のみの出力には`-b:a`を正しく適用。
- **編集**: 複数区間カット(プレビュー付き)、複数ファイルの結合、等間隔/サイズ指定分割
  (あまりはディスクいっぱいに自動フィット)。
- **PDF**: 見開き画像化(右綴じ/左綴じ、最大4K)、綴じ方向の一括変換保存(ページ順反転)。
- **書き込み(Windows)**: 光学ドライブを自動検出し、IMAPI2でISO作成(日本語ファイル名を保持)・
  書き込み。実機(BD-REドライブ+CD-R)で3.5時間の動画→CD容量いっぱいの音声→ISO→書き込みまで成功確認済み。
- **その他**: 起動時の自動アップデート確認(日英ダイアログ)、rs-ffmpeg/rs-xorrisoの
  バージョン管理付きプラグイン(同じ版は上書きしない)。

### できないこと・制限(正直な開示)

- **著作権保護(CSS/AACS等)の回避は実装しません**(違法となり得るため)。保護のないディスクのみ対象。
- **Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX の新規生成は不可**(ライセンス制)。保持(無変換コピー)と互換下位形式のみ。
- **AI映像超解像(Real-ESRGAN)は非常に遅く、短いクリップ向け**(720×480の1フレーム: 高速モデルはCPU 32スレッド/AVX2で約1.2秒、GT 730 GPUで約4.6秒、高品質モデルはGT 730で約110秒)。
  CPU版は高速モデルのみ(高品質モデルはGPU必須)。**音声AI超解像は未実装**: 評価した音声用モデル(LavaSR)は音楽素材で元信号との対数スペクトル距離が悪化した(下記CLAUDE.md参照)ため、
  音質向上の機能としては採用していない。「AI最適化」表記の解像度/FPS設定は簡易ヒューリスティックで、AI超解像とは別物。
- DSDは巨大(ステレオ1分あたりDSD64≈42MB〜DSD1024≈678MB)。SACD規格ディスクではなくDSFファイル。
- Linux/macOSの書き込みは本家xorriso前提(未同梱)。データCDとして書き込み(音楽CD=CD-DAは未対応)。
- iOSは実機がなく未対応。

### 実行時の外部依存

Windows/Linux版インストーラーにはffmpeg/ffprobeとrs-ffmpeg/rs-xorrisoを同梱済み
(`src-tauri/src/engine/sidecar.rs`・`plugins.rs`参照)。同梱が無ければPATH上のものへフォールバック。
macOSのffmpeg・本家xorrisoは未同梱のため別途インストールが必要。

### 開発

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

### ダウンロード / インストーラー

Windows(.msi/.exe)・macOS(.dmg、Intel/Apple Silicon)・Linux(.deb/.rpm/.AppImage)・Android(universal APK)を
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest)で公開。`v*`タグのpushで
`.github/workflows/release.yml`が全プラットフォームを自動ビルド。紹介ページ: <https://easy-web.tokyo/make-disk/>

### ライセンス

MIT(同梱のRNNoiseモデルは作者が著作権対象外と明記、詳細は`CLAUDE.md`)

---

## English

A cross-platform (Rust + Tauri) GUI for burning CD/DVD/Blu-ray and converting audio/video.
One codebase; only the installers differ per OS.

### Features

- **I/O**: pick multiple sources (audio/video/PDF) and an output folder; convert, build ISO images and burn
  discs at once (multiple formats, files and disc types run asynchronously in parallel).
- **Audio**: MP3 / WAV / FLAC / AAC / OGG / **Opus** / **AC-3 / E-AC-3 (5.1–7.1)** /
  **DSD64·128·256·512·1024 (DSF)**. ffmpeg cannot write DSD, so PCM→1-bit delta-sigma modulation
  (5th order, per-channel threads) and DSF writing are implemented in Rust and verified by round-tripping through ffmpeg's DSF decoder.
- **Video**: MP4 / MKV / AVI / MOV / WebM / **AV1** (libsvtav1 preferred, libaom fallback) /
  **HEVC 10-bit (HDR10)** / **lossless stream copy** (keeps Dolby Vision, Atmos, 5.1/7.1 surround).
  Resolution (720x480 up to 8K, PAL, custom, AI-optimized) and FPS (24/30/60/120, custom) are selectable;
  choices narrow by disc direction (BD→DVD: standard DVD resolution or Full HD; DVD→BD: Full HD or 4K).
- **High-resolution PCM (for R-2R / multi-bit DACs)**: 352.8 kHz / 384 kHz (24- and 32-bit) and 705.6 kHz / 768 kHz (32-bit) WAV, using the best
  available resampler (soxr, else a high-precision swresample setup) with TPDF dither. Measured SNR against an exact reference sine:
  352.8 kHz/24-bit = 141.2 dB, 384 kHz/32-bit = 150.2 dB. Since DSD is 1-bit by definition, a PCM companion (FLAC 24-bit/352.8 kHz) can be
  written next to it for devices without DSD (chosen at playback, not an in-file fallback). Measured DSD round-trip SNR: DSD64 = 99.6 dB, DSD128 = 132.4 dB.
- **AI super-resolution (video)**: Real-ESRGAN (MIT) as an on-demand plugin (e.g. DVD→4K). Uses the GPU (NCNN-Vulkan) when a Vulkan device works, otherwise our own **Rust CPU implementation** (AVX2+FMA used automatically; output PSNR 42.0 dB against the official GPU implementation) with automatic switching.
- **AI noise reduction**: bundles a real trained neural network (RNNoise), verified with the real model.
  It is trained mostly on speech, so the effect on music is modest.
- **Bitrate / capacity**: fixed, auto from disc capacity, a "maximum quality" mode (lossless WAV + ISO when no format is
  chosen), target size, target duration, and silence-based auto-cut. Auto bitrate is capped at the source bitrate,
  and audio-only outputs correctly use `-b:a`.
- **Editing**: multi-range cutting with preview, concatenating files, equal-interval / size-based splitting
  (the remainder is auto-fitted to fill the disc).
- **PDF**: spread images (right/left binding, up to 4K) and bulk binding-direction conversion (page order reversal).
- **Burning (Windows)**: auto-detects optical drives, builds ISOs with IMAPI2 (keeps Japanese filenames) and burns.
  Verified on real hardware (BD-RE drive + CD-R): a 3.5-hour video → disc-filling audio → ISO → burn succeeded.
- **Other**: automatic update check on launch (bilingual dialog); versioned rs-ffmpeg / rs-xorriso plugins
  (an identical version is not overwritten).

### Not supported / limitations (honest disclosure)

- **Circumventing copy protection (CSS/AACS, etc.) is not implemented** (it can be illegal). Unprotected discs only.
- **Newly creating Dolby Vision / Atmos / Dolby Cinema / IMAX / 4DX is not possible** (licensed). Only preservation
  (stream copy) and compatible lower formats.
- **AI video super-resolution (Real-ESRGAN) is very slow — short clips only** (per 720x480 frame: fast model ~1.2 s on a 32-thread AVX2 CPU, ~4.6 s on a GT 730 GPU;
  high-quality model ~110 s on the GT 730). The CPU build supports the fast model only (the high-quality model needs a GPU). **Audio AI super-resolution is not implemented**: the
  speech-trained model we evaluated (LavaSR) worsened the log-spectral distance to the original on music (see CLAUDE.md), so it is not offered as a quality feature.
  The "AI-optimized" resolution/FPS options are simple heuristics, not AI super-resolution.
- DSD files are huge (per stereo minute: DSD64 ≈ 42 MB … DSD1024 ≈ 678 MB) and are DSF files, not a Super Audio CD disc.
- Burning on Linux/macOS relies on real xorriso (not bundled). Discs are written as data discs (audio CD / CD-DA is unsupported).
- iOS is unsupported (no test device).

### External dependencies

The Windows/Linux installers bundle ffmpeg/ffprobe and rs-ffmpeg/rs-xorriso (see `src-tauri/src/engine/sidecar.rs`
and `plugins.rs`); if absent, the tools on `PATH` are used. macOS ffmpeg and real xorriso are not bundled.

### Development

```bash
npm install
npm run tauri dev
cd src-tauri && cargo test --lib -- --test-threads=1
```

### Downloads / installers

Windows (.msi/.exe), macOS (.dmg, Intel/Apple Silicon), Linux (.deb/.rpm/.AppImage) and Android (universal APK) are on
[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest). Pushing a `v*` tag builds all platforms via
`.github/workflows/release.yml`. Landing page: <https://easy-web.tokyo/make-disk/>

### License

MIT (the bundled RNNoise model is stated by its author to be outside copyright; see `CLAUDE.md`).
