# make-disk

プラットフォーム共通コード(Rust + Tauri)によるCD/DVD/Blu-ray書き込み・
音声/動画フォーマット変換GUIアプリ。インストーラーのみをOSごとに分ける
方針で、アプリ本体は単一コードベース。

## 機能

- 複数のソースファイル(音声/動画)と出力先フォルダを選択。
- 有名な音声フォーマット(MP3等)・動画フォーマット(MP4等)への変換、
  ISOイメージ出力、ディスクへの直接書き込みを、チェックボックスで
  複数選択可能。
- ビットレートは固定値、またはディスク容量(CD/DVD/DVD DL/BD/BD DL/
  BDXL 4層 128GB)から自動算出した最大値を選択可能。
- 複数区間の動画カット(最初・途中・最後、いくつでも指定可能)。
  マウスでの動画プレビュー確認、または時:分:秒の直接数字入力の
  どちらでも指定できる。フレーム精度カットモードではGPUハードウェア
  エンコーダ(NVENC/QuickSync/AMF)を自動検出して使用し、無ければ
  CPU(open-cpuの検出結果に応じて`-preset`を自動選択)にフォールバック。
- 書き込み速度は自動判定・最高速・速度指定から選択可能。

## 実行時の外部依存

コーデックや書き込み処理を自前実装せず、既存のオープンソースエンジンを
ラップする方針。以下が別途インストール済みで`PATH`が通っている必要が
あります。

- [FFmpeg](https://ffmpeg.org/)(`ffmpeg` / `ffprobe`) — フォーマット
  変換・ビットレート制御。Windows向けにはRustでのリスペクト版
  [rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg)もあります
  (初期WIP、機能は本家の一部のみ)。
- [xorriso](https://www.gnu.org/software/xorriso/) — ISO生成・ディスク
  書き込み(cdrtools/cdrecord・cdrdao・libburn/libisofs相当の機能を
  クロスプラットフォームな単一CLIでカバー)。Windows向けにはRustでの
  リスペクト版[rs-xorriso](https://github.com/aon-co-jp/rs-xorriso)も
  あります(初期WIP、機能は本家の一部のみ)。

## 開発

```bash
npm install
npm run tauri dev
```

## インストーラーのビルド

```bash
npm run tauri build
```

実行したOS向けのネイティブインストーラー(Windows: `.msi`/`.exe`、
macOS: `.dmg`/`.app`、Linux: `.deb`/`.AppImage`)がTauriのbundlerにより
生成されます。`v*`タグをpushするとGitHub Actionsで全プラットフォーム
向けにビルドし、GitHub Releaseへ自動公開されます。

## ダウンロード / インストーラー

Windows(.msi/.exe)・macOS(.dmg、Intel/Apple Silicon)・Linux(.deb/
.rpm/.AppImage)・Android(スマホ・タブレット共通のuniversal APK)向け
インストーラーを[GitHub Releases](https://github.com/aon-co-jp/make-disk/releases/latest)で
公開している。詳細・各ファイルの対応表は[`installer/README.md`](installer/README.md)
参照。紹介ページ: <https://easy-web.tokyo/make-disk/>

`v*`タグをpushすると`.github/workflows/release.yml`が全プラットフォーム
分を自動ビルドし、同じGitHub Releaseへ添付する(デスクトップ3種+
Android)。iOSは実機未保有のため対応保留。

## 現状

2026-09-14時点でv0.1.4まで公開済み。デスクトップ版はUIとRustコマンドの
配線が完了し、実際のffmpegを使った統合テストも整備済み。Android版は
実機(OnePlus A401OP)での動作確認・SAFフォルダ選択プラグインの実装まで
完了し、CI経由でuniversal APKを自動ビルド・公開できるようになった。
実機でのディスク書き込みテスト(CD/DVD/Blu-ray)は未実施。`ffmpeg`/
`xorriso`は別途インストールが必要(インストーラーへの同梱化は次の開発
増分として計画中)。詳細は[`CLAUDE.md`](CLAUDE.md)・
[`PORTING.md`](PORTING.md)を参照。

## ライセンス

MIT
