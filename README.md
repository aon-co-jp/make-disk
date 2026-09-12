# make-disk

Cross-platform (Windows / macOS / Linux) GUI app for CD / DVD / Blu-ray
writing and audio/video format conversion, built with Rust + Tauri.
プラットフォーム共通コード(Rust + Tauri)によるCD/DVD/Blu-ray書き込み・
音声/動画フォーマット変換GUIアプリ。インストーラーのみをOSごとに分ける
方針で、アプリ本体は単一コードベース。

## Features / 機能

- Select multiple source files (audio/video) and an output folder.
  複数のソースファイル(音声/動画)と出力先フォルダを選択。
- Convert to well-known audio formats (MP3, etc.) and video formats
  (MP4, etc.), or output an ISO image, or burn directly to disc —
  any combination, selectable via checkboxes.
  有名な音声フォーマット(MP3等)・動画フォーマット(MP4等)への変換、
  ISOイメージ出力、ディスクへの直接書き込みを、チェックボックスで
  複数選択可能。
- Bitrate: fixed (kbps) or auto-calculated to the maximum that fits
  the target disc's capacity (CD/DVD/DVD-DL/BD/BD-DL).
  ビットレートは固定値、またはディスク容量(CD/DVD/DVD DL/BD/BD DL)
  から自動算出した最大値を選択可能。
- Per-file start time / duration trimming (e.g. 5-minute or
  10-minute clips), editable per item.
  ファイルごとの開始位置・長さ(5分・10分等)を個別に指定・編集可能。
- Write speed: auto-detect, maximum, or a fixed speed.
  書き込み速度は自動判定・最高速・速度指定から選択可能。

## Requirements / 実行時の外部依存

This app wraps existing open-source engines rather than reimplementing
codecs or disc-burning logic. The following must be installed and on
`PATH`:
コーデックや書き込み処理を自前実装せず、既存のオープンソースエンジンを
ラップする方針。以下が別途インストール済みで`PATH`が通っている必要が
あります。

- [FFmpeg](https://ffmpeg.org/) (`ffmpeg` / `ffprobe`) — format
  conversion and bitrate control / フォーマット変換・ビットレート制御
- [xorriso](https://www.gnu.org/software/xorriso/) — ISO creation and
  disc burning (covers cdrtools/cdrecord, cdrdao, libburn/libisofs
  functionality through one cross-platform CLI) / ISO生成・ディスク
  書き込み(cdrtools/cdrecord・cdrdao・libburn/libisofs相当の機能を
  クロスプラットフォームな単一CLIでカバー)

## Development / 開発

```bash
npm install
npm run tauri dev
```

## Building installers / インストーラーのビルド

```bash
npm run tauri build
```

Produces a native installer for the host OS (`.msi`/`.exe` on Windows,
`.dmg`/`.app` on macOS, `.deb`/`.AppImage` on Linux) via Tauri's bundler.
実行したOS向けのネイティブインストーラー(Windows: `.msi`/`.exe`、
macOS: `.dmg`/`.app`、Linux: `.deb`/`.AppImage`)がTauriのbundlerにより
生成されます。

## Status / 現状

Early scaffold (2026-09-12): core UI and Rust command wiring are in
place; real-device burn testing and per-OS installer builds are not
yet verified. See [`CLAUDE.md`](CLAUDE.md) for details (Japanese).
初期スケルトン段階(2026-09-12時点)。UIとRustコマンドの配線は完了して
いるが、実機での書き込みテスト・各OSインストーラーの実ビルド確認は
未実施。詳細は[`CLAUDE.md`](CLAUDE.md)(日本語)を参照。

## License

MIT
