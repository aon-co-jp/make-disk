# セッション引き継ぎメモ / Session Handoff Notes

このファイルは、複数セッションにまたがる作業の到達点・次回再開ポイントを
記録する(`open-raid-z`等の他リポジトリのPORTING.md運用に準じる)。
詳細な技術的発見・方針決定は[`CLAUDE.md`](CLAUDE.md)にあるので、
ここでは「今どこまで進んでいて、次に何をするか」だけを簡潔に記す。

This file tracks progress and the next resume point across sessions
(following the same `PORTING.md` convention used in other repos like
`open-raid-z`). Detailed technical findings and decisions live in
[`CLAUDE.md`](CLAUDE.md); this file only summarizes where things stand
and what's next.

## 🔁 再開用メッセージ(2026-09-12時点) / Resume message (as of 2026-09-12)

### 完了したこと / Done

1. Tauri + Rustでのデスクトップ版スケルトン一式(UI・ffmpeg/xorriso
   ラッパー・容量ベース自動ビットレート・4段階品質警告)。
   Desktop scaffold complete: UI, ffmpeg/xorriso wrappers,
   capacity-based auto-bitrate, 4-stage quality warning.
2. `https://easy-web.tokyo/make-disk/`に紹介・ダウンロードページを公開
   (VPSの`open-web-server`にルート追加、`easy-web.tokyo`トップページにも
   紹介カードを追加してWASM再ビルド込みで反映済み)。
   Landing/download page live at `https://easy-web.tokyo/make-disk/`
   (route added to the VPS's `open-web-server`; also linked from the
   `easy-web.tokyo` top page, WASM rebuilt and deployed).
3. 姉妹リポジトリ`rs-FFmpeg`・`rs-xorriso`(Rustでのリスペクト版)を新規
   作成。実際のmake-disk呼び出し引数で検証し、見つかった重大な非互換
   (無音で壊れた出力を書く等)を修正済み。両リポジトリとも現時点では
   本家の一部機能のみ対応。
   Created sibling repos `rs-FFmpeg`/`rs-xorriso` (Rust tributes).
   Verified against make-disk's actual invocation args and fixed real
   incompatibilities found there (silently writing broken output,
   etc). Both still cover only a subset of the originals' features.
4. Android実機(OnePlus A401OP, Android 15)での動作確認に成功。
   その過程で**全プラットフォームに影響する重大バグ**
   (`main.js`のベアESモジュールインポートが原因で全ボタンが無反応に
   なっていた)を発見・修正した。
   Verified on a real Android device (OnePlus A401OP, Android 15).
   In the process, found and fixed a **bug affecting every platform**
   (bare ES module imports in `main.js` silently broke every button).

### 未完了・次回やること / Not done yet — next steps

1. **最優先**: Android向けSAFフォルダ選択の独自Tauriプラグイン実装
   (`CLAUDE.md`の「発見2」に具体的な設計を記載済み・未着手)。
   Kotlin側の`ACTION_OPEN_DOCUMENT_TREE`ハンドリング、Rust側の
   `pick_output_tree`コマンド、`main.js`でのAndroid分岐、の3点セット。
   **Top priority**: implement the custom Tauri plugin for Android SAF
   folder picking (concrete design already written in `CLAUDE.md`
   under "Finding 2" — not started). Three pieces: Kotlin-side
   `ACTION_OPEN_DOCUMENT_TREE` handling, a Rust `pick_output_tree`
   command, and the Android branch in `main.js`.
2. `rs-FFmpeg`/`rs-xorriso`のAndroidクロスコンパイル・Rustライブラリ
   直接リンク化の検討(外部プロセスシェルアウトをやめる方向)。
   Investigate cross-compiling `rs-FFmpeg`/`rs-xorriso` for Android and
   linking them as an in-process Rust library instead of shelling out.
3. このPC(Windows)でのAndroid Developer Mode有効化が反映されない問題
   ([`CLAUDE.md`](CLAUDE.md)のプラットフォーム範囲節に詳細記録)は
   未解決のまま。手動`.so`コピー+`gradlew`直叩きの回避策で当面は
   進められるが、`tauri android dev`によるホットリロード開発はできない
   状態が続いている。
   Windows Developer Mode still won't take effect on this dev machine
   (details in `CLAUDE.md`'s platform-scope section) — unresolved. The
   manual `.so`-copy + direct `gradlew` workaround unblocks progress,
   but hot-reload development via `tauri android dev` remains
   unavailable.
4. 実機でのディスク書き込み検証(CD/DVD/Blu-ray)は光学ドライブが
   無いため未実施。Windows/macOS/Linux各インストーラーの実ビルドも
   未確認。
   No real-device disc-burn testing yet (no optical drive on this
   machine). Per-OS installer builds are also unverified.

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.1) / Resume message continued (v0.1.1)

### 完了したこと(追加分) / Done (additional)

5. v0.1.0をGitHub Actions(全プラットフォームビルド)+GitHub Releaseで
   公開。https://github.com/aon-co-jp/make-disk/releases/tag/v0.1.0
   Published v0.1.0 via GitHub Actions (all-platform build) + GitHub
   Release.
6. 複数区間の動画カット機能(`CutRange`)を実装。最初・途中・最後、
   いくつでも指定可能。抽出は`-c copy`(無劣化・高速)、フォーマット
   変換が必要な場合のみ結合時に1回だけエンコード。単体テスト4件で
   区間計算ロジックを検証済み。
   Implemented multi-range video cutting (`CutRange`) — any number of
   cuts (start/middle/end). Extraction uses `-c copy` (lossless, fast);
   encoding (if needed for format conversion) happens once at the
   final concat step. 4 unit tests cover the range-math logic.
7. フレーム精度カットモード(`frame_accurate`)を追加。GPUハードウェア
   エンコーダ(NVENC/QuickSync/AMF)を自動検出して使用し、無ければ
   CPU(libx264、AVX2/AVX512はlibx264自身が自動活用)にフォールバック。
   Added frame-accurate cut mode: auto-detects a GPU hardware encoder
   (NVENC/QuickSync/AMF) and falls back to CPU (libx264, which already
   auto-uses AVX2/AVX512 on its own) when none is found.
8. `open-cpu`をCargo依存として統合、CPUフォールバック時の速度目安を
   `estimate_cpu_encode_speed`コマンドでUIに提供。
   Integrated `open-cpu` as a Cargo dependency; `estimate_cpu_encode_speed`
   surfaces a CPU-fallback speed hint to the UI.
9. Blu-ray 4層/BDXL(128GB, `DiscType::Bd128`)対応を追加。
   Added Blu-ray quad-layer/BDXL support (128GB, `DiscType::Bd128`).
10. フロントエンドに動画プレビュー付き区間カットエディタを実装
    (`<video>`要素+`convertFileSrc`、マウスでシーク→「現在位置」ボタン、
    または時:分:秒を直接数字入力、の両対応)。これに伴い
    `tauri.conf.json`の`assetProtocol`を有効化し、`tauri`クレートに
    `protocol-asset`フィーチャを追加した(無いとビルド時エラーになる
    ことを実際に確認済み)。
    Implemented a video-preview cut-range editor in the frontend
    (`<video>` + `convertFileSrc`; mouse-seek "current position"
    buttons or direct H:M:S numeric entry, either works). This required
    enabling `assetProtocol` in `tauri.conf.json` and adding the
    `protocol-asset` feature to the `tauri` crate dependency (verified
    the build fails without it).
11. `open-cuda`に`yuv_to_rgb_cpu`(YUV420p→RGB24変換カーネル、CPU/rayon)
    を追加、スカラー参照実装と全画素一致を検証(2ユニットテスト+
    3072画素のend-to-end検証)。動画コーデック自体(H.264等)は
    非現実的なスコープのため対象外と明記。`open-directx`での実GPU
    ディスパッチ(既存の狭いDXBC→SPIR-Vデコーダの拡張が必要)は
    次段階として保留。
    Added `yuv_to_rgb_cpu` to `open-cuda` (YUV420p→RGB24 conversion
    kernel, CPU/rayon), verified pixel-for-pixel against a scalar
    reference (2 unit tests + a 3072-pixel end-to-end check). A real
    video codec (H.264, etc.) is explicitly out of scope. Real GPU
    dispatch via `open-directx` (needs extending its narrow existing
    DXBC→SPIR-V decoder) is deferred as the next step.

### 既知の未検証事項(v0.1.1時点) / Known unverified items (as of v0.1.1)

- **このPCにffmpegが入っていないため、複数区間カット機能(`CutRange`)の
  実際のffmpeg実行を伴うテストは未実施**(単体テストは区間計算ロジック
  のみをカバーしており、実際の`-c copy`抽出→concat結合の動作は
  未検証)。次回、ffmpegが入った環境での実ファイルによる検証が必要。
  **This dev machine has no ffmpeg installed, so the multi-range cut
  feature has not been tested against a real ffmpeg run** (unit tests
  only cover the pure range-math logic — the actual `-c copy`
  extraction + concat behavior is unverified). Needs testing against
  real files on a machine with ffmpeg next.
- GPUハードウェアエンコーダ検出(`detect_hw_video_encoder`)も、
  実際にNVENC/QSV/AMF搭載環境での動作は未検証(このマシンの
  GPU/ffmpegビルド構成に依存)。
  GPU hardware-encoder detection (`detect_hw_video_encoder`) is also
  unverified on real NVENC/QSV/AMF hardware.
- 動画プレビューエディタ(`<video>` + `convertFileSrc`)の実機での
  表示・シーク動作は未検証(ビルド成功とロジックレビューのみ)。
  The video-preview editor (`<video>` + `convertFileSrc`) has not been
  verified live in a running window — only that it builds and the
  logic reviews correctly.

## 関連リポジトリ / Related repositories

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 本体 / this repo
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) — FFmpegのRustリスペクト版
- [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — xorrisoのRustリスペクト版
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU命令セット検出(依存として使用)
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU計算抽象化層(`yuv_to_rgb_cpu`を追加)
