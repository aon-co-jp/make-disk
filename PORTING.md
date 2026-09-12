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

## 関連リポジトリ / Related repositories

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 本体 / this repo
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) — FFmpegのRustリスペクト版
- [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — xorrisoのRustリスペクト版
