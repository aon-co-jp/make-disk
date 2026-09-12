# 開発方針＆開発環境ルール(make-disk)
# Development Policy & Environment Rules (make-disk)

全リポジトリ共通の開発ルール(自動継続・検証徹底等)は
[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の`CLAUDE.md`を
正本として参照すること。この節では本リポジトリ固有の事項のみ記す。

Ecosystem-wide development rules (autonomous continuation, thorough
verification, etc.) live in
[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)'s `CLAUDE.md`
as the canonical source. This file only covers what's specific to
this repository.

## リポジトリの役割 / Repository role

CD/DVD/Blu-ray書き込み・音声/動画フォーマット変換・ISO出力を行う
Windows/macOS/Linux共通コードのGUIアプリ(Rust + Tauri)。
プラットフォームごとの差分はインストーラー(bundle)のみに閉じ込め、
アプリ本体(`src-tauri/src`・`src`)は単一コードベースとする。

A cross-platform (Windows/macOS/Linux, shared code) GUI app (Rust +
Tauri) for CD/DVD/Blu-ray writing, audio/video format conversion, and
ISO export. Platform differences are confined to the installer
(bundle) layer only — the app itself (`src-tauri/src`, `src`) is a
single codebase.

## アーキテクチャ / Architecture

- フロントエンド: `src/`(バニラJS、Tauri IPC経由でRustコマンドを呼ぶ)
- バックエンド: `src-tauri/src/engine/`
  - `probe.rs` — ffprobeでメディア尺・コーデック取得
  - `convert.rs` — ffmpegでフォーマット変換・ビットレート制御・トリミング
  - `capacity.rs` — CD/DVD/Blu-ray容量からの自動最大ビットレート算出、
    および基準ビットレートに対する低下度合いを4段階(下がります→
    少し下がります→かなり下がります→画質/音質が落ちます)で警告する
    `quality_warning`(下限は設けず、常に容量に収まる値を返す仕様)
  - `iso.rs` — xorriso(`-as mkisofs`)でISOイメージ生成
  - `burn.rs` — xorriso(`-as cdrecord`)でディスク書き込み・速度指定・デバイス列挙

xorrisoはlibburn/libisofs/cdrtools(cdrecord)相当の機能をOS非依存の
単一コマンド体系で提供するため、CD/DVD/Blu-rayの書き込み経路を
xorrisoに統一している(OSごとに別ライブラリを直接バインディングしない)。

- Frontend: `src/` (vanilla JS, calls Rust commands over Tauri IPC)
- Backend: `src-tauri/src/engine/`
  - `probe.rs` — media duration/codec info via ffprobe
  - `convert.rs` — format conversion, bitrate control, trimming via ffmpeg
  - `capacity.rs` — auto-max-bitrate calculation from CD/DVD/Blu-ray
    capacity, plus `quality_warning`, which warns in 4 escalating
    stages (reduced → reduced a bit further → reduced considerably →
    quality noticeably drops) as the computed bitrate falls below a
    reference level (no lower bound — it always returns a value that
    fits the capacity, by design)
  - `iso.rs` — ISO image creation via xorriso (`-as mkisofs`)
  - `burn.rs` — disc burning, speed control, device enumeration via
    xorriso (`-as cdrecord`)

xorriso is used as the single cross-platform CLI for the whole
CD/DVD/Blu-ray burning path (rather than binding separate OS-specific
libraries) because it covers libburn/libisofs/cdrtools(cdrecord)
functionality in one OS-independent command set.

## 外部依存(実行時に必要、同梱はしない) / Runtime dependencies (not bundled)

- `ffmpeg` / `ffprobe` — フォーマット変換・ビットレート制御
- `xorriso` — ISO生成・ディスク書き込み(cdrtools/cdrdao/libburn/libisofs相当)

インストーラー側の課題として、これらのバイナリをOSごとにどう同梱/
案内するか(Windows: 同梱バイナリ配布、macOS: Homebrew案内、
Linux: パッケージマネージャー案内、等)は未確定・要検討。

- `ffmpeg` / `ffprobe` — format conversion, bitrate control
- `xorriso` — ISO creation, disc burning (covers
  cdrtools/cdrdao/libburn/libisofs)

Open installer question: how to bundle/guide installation of these
binaries per OS (Windows: bundle the binaries; macOS: point to
Homebrew; Linux: point to the distro's package manager, etc.) — not
yet decided.

## プラットフォーム範囲(2026-09-12更新) / Platform scope (updated 2026-09-12)

- **現在**: Windows/macOS/Linuxのデスクトップ3プラットフォームが対象。
- **Android**: `npm run tauri android init`でプロジェクト生成済み
  (`src-tauri/gen/android`)、Rustのandroidターゲット(aarch64/armv7/
  i686/x86_64)へのクロスコンパイルも成功を確認済み。この開発機
  (Windows)ではWindows開発者モードが有効化できず(2026-09-12に複数回
  試行・再起動後も`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\
  AppModelUnlock\AllowDevelopmentWithoutDevLicense`が作成されないことを
  確認)、`tauri android build`のjniLibsシンボリックリンク作成が失敗する。
  回避策として、Rustクロスコンパイル成功後の`.so`を手動で
  `gen/android/app/src/main/jniLibs/arm64-v8a/`へコピーし、
  `gradlew assembleUniversalDebug -PabiList=arm64-v8a
  -PtargetList=aarch64 -x rustBuildArm64Debug`で直接パッケージングする
  ことでAPK生成・実機インストール・起動まで到達した(実機:
  OnePlus A401OP, Android 15)。
  UIは`styles.css`にタッチ操作向けの調整(44px以上のタップ領域、
  スマホ幅でのレイアウト崩し、iOS自動ズーム防止のinput最小フォント
  16px)を追加済み。
- **iPhone/iOS**: ユーザーは現時点でテスト実機を保有しておらず、
  対応は将来課題として保留。Tauriはmobile_entry_pointを通じてiOSにも
  対応するため、コード側の障壁は大きくない見込みだが未検証。
- **ディスク書き込み機能のモバイル版での扱い**: Android/iOSはOSレベルで
  内蔵光学ドライブへの直接アクセス手段を持たないため、モバイル版では
  当面「音声/動画変換・ISOイメージ生成」までを提供し、実ディスクへの
  書き込みはPC版限定の機能として明記する。将来的にはUSB接続の外付け
  光学ドライブ(Android USB OTG経由等)への対応を検討課題とする
  (現時点では未着手・要実機検証)。

- **Current**: targets the three desktop platforms — Windows, macOS,
  Linux.
- **Android**: project scaffold generated (`src-tauri/gen/android`)
  via `npm run tauri android init`; Rust cross-compiles cleanly to all
  4 Android targets (aarch64/armv7/i686/x86_64). Windows Developer
  Mode could not be enabled on this dev machine (tried repeatedly,
  including after a reboot, on 2026-09-12 — the registry marker
  `AllowDevelopmentWithoutDevLicense` never appears), so `tauri
  android build`'s jniLibs symlink step fails. Workaround: after a
  successful Rust cross-compile, manually copy the `.so` into
  `gen/android/app/src/main/jniLibs/arm64-v8a/` and package directly
  with `gradlew assembleUniversalDebug -PabiList=arm64-v8a
  -PtargetList=aarch64 -x rustBuildArm64Debug`. This got as far as a
  real install + launch on a physical device (OnePlus A401OP, Android
  15). `styles.css` already has touch-friendly adjustments (44px+ tap
  targets, single-column layout under 600px, 16px input font to avoid
  iOS auto-zoom).
- **iPhone/iOS**: deferred — the user has no test hardware right now.
  Tauri supports iOS via the same `mobile_entry_point` mechanism, so
  the code-side barrier is expected to be small, but this is
  unverified.
- **Disc burning on mobile**: Android/iOS have no OS-level access to
  an internal optical drive, so the mobile build will offer
  audio/video conversion and ISO export only for now; actual disc
  burning stays PC-only. Future: USB-attached external drives (e.g.
  via Android USB OTG) — unexplored, needs real hardware to test.

## Android実機検証で発見した問題と対応方針(2026-09-12)
## Issues found via real-device Android testing, and the plan (2026-09-12)

### 発見1: main.jsのベアインポートが全プラットフォームで動かないバグ(修正済み)
`import { invoke } from "@tauri-apps/api/core"`のようなベア指定子は
バンドラー無しのWebViewでは解決できず、モジュール読み込みが例外で
止まり**main.js内の全イベントリスナーが登録されない**(=全ボタンが
無反応になる)という重大バグがあった。`window.__TAURI__.core.invoke`
経由に変更して修正済み(実機で「ファイルを追加」ボタンがネイティブ
ピッカーを開くことまで確認)。デスクトップ版でもこのバグは(検証は
していなかったが)理論上同じ影響を受けていたはずで、静的ファイル
プレビューだけでは検出できなかった教訓が大きい。

### Finding 1: main.js's bare imports broke every platform (fixed)
Bare specifiers like `import { invoke } from "@tauri-apps/api/core"`
can't be resolved by a WebView with no bundler in front of it — the
module script throws at import time, which meant **every event
listener in main.js silently failed to register** (every button was
dead). Fixed by switching to `window.__TAURI__.core.invoke` (verified
on-device: the "Add files" button now opens the native picker). The
desktop build was theoretically hit by the exact same bug, though it
had never actually been exercised through a real running window before
this — a static file preview alone could not have caught it. That's
the big lesson here.

### 発見2: Tauri dialogプラグインはモバイルでフォルダ選択が未実装
実機で`open({directory:true})`を呼ぶと
`"Folder picker is not implemented on mobile"`で例外になることを
確認。ユーザー指示により「まとめて1フォルダに出力」の仕様を
モバイルでも維持する方針とし、Android向けにはSAF
(`ACTION_OPEN_DOCUMENT_TREE`)を直接扱う**独自Tauriプラグインの
新規実装が必要**という結論に至った(標準dialogプラグインの範囲では
実現不可)。

### Finding 2: the Tauri dialog plugin doesn't implement folder picking on mobile
Calling `open({directory:true})` on-device throws `"Folder picker is
not implemented on mobile"`. Per the user's direction, the "output
everything to one shared folder" UX must be preserved on mobile too,
which means a **custom native Tauri plugin wrapping Android's SAF**
(`ACTION_OPEN_DOCUMENT_TREE`) is required — the stock dialog plugin
can't do this.

**実装計画(次回セッション向け、未着手)**:
1. `src-tauri/`配下に小さなカスタムTauriプラグインを追加
   (例: `tauri-plugin-android-folder`、Kotlin側で
   `ACTION_OPEN_DOCUMENT_TREE`のIntentを発行しActivity Resultを
   受け取り、`takePersistableUriPermission`で永続化)。
2. Rust側に`pick_output_tree() -> Result<String, String>`のような
   コマンドを追加し、返ったtree URI文字列をJS側で保持。
3. JS側(`main.js`)でAndroid判定時にこのコマンドを呼ぶよう分岐。

**Implementation plan (next session, not started yet)**:
1. Add a small custom Tauri plugin under `src-tauri/`
   (e.g. `tauri-plugin-android-folder`) whose Kotlin side fires an
   `ACTION_OPEN_DOCUMENT_TREE` intent, handles the Activity Result,
   and persists it via `takePersistableUriPermission`.
2. Add a Rust command like `pick_output_tree() -> Result<String,
   String>` and hold the returned tree URI string on the JS side.
3. Branch in `main.js` to call this command when running on Android.

### 発見3(重要・優先度確定): ffmpeg/xorrisoはAndroidに存在しないため、
### SAF実装だけでは変換機能は動かない
make-diskは`std::process::Command::new("ffmpeg"/"xorriso")`で外部
バイナリをシェルアウトする設計。Android端末にはこれらのバイナリが
存在せず、同梱もしていないため、**SAFフォルダ選択を実装しても
「実行」ボタンを押した時点で確実に失敗する**(コマンドが見つからない
エラー)。

### Finding 3 (important, priority already decided): ffmpeg/xorriso don't exist on Android, so the SAF work alone won't make conversion work
make-disk shells out to external `ffmpeg`/`xorriso` binaries via
`std::process::Command`. Those binaries don't exist on an Android
device and aren't bundled, so **even with SAF folder picking done,
pressing "実行" (Run) will still fail immediately** (command not
found).

**方針(ユーザー承認済み、2026-09-12)**: Android対応のffmpeg/xorriso戦略は
以下の優先順で検討する。
1. まずSAFフォルダ選択のUI・権限取得部分を実装する(上記実装計画)。
   実行結果はエラーになる想定だが、UIフローとしては完成させる。
2. [`rs-FFmpeg`](https://github.com/aon-co-jp/rs-FFmpeg)・
   [`rs-xorriso`](https://github.com/aon-co-jp/rs-xorriso)を
   Android向けにクロスコンパイルし、`jniLibs`同梱の実行可能ファイル
   またはRustライブラリとして直接リンクする方向を検討する
   (外部プロセスのシェルアウトではなくRust関数呼び出しに変更できれば
   Android/iOS双方で動く可能性が高い——ただし両リポジトリとも
   現時点でmake-diskの実引数と非互換な部分が残っている点に注意)。
3. 本家ffmpeg/xorrisoをAndroidバイナリとして同梱する道は、
   ライセンス・バイナリサイズ・クロスコンパイルの複雑さの観点から
   優先度を下げる。

**Plan (approved by the user, 2026-09-12)**, in priority order:
1. Implement the SAF folder-picker UI/permission flow first (plan
   above). Running the actual conversion is expected to fail, but the
   UI flow itself will be complete.
2. Investigate cross-compiling
   [`rs-FFmpeg`](https://github.com/aon-co-jp/rs-FFmpeg) and
   [`rs-xorriso`](https://github.com/aon-co-jp/rs-xorriso) for Android
   and linking them directly as a Rust library (rather than shelling
   out to a separate process) — bundled in `jniLibs` or linked
   in-process. This would likely work on both Android and iOS, but
   both repos currently still have gaps versus the exact args
   make-disk sends them (see their own CLAUDE.md compatibility notes).
3. Bundling the real upstream ffmpeg/xorriso as Android binaries is
   deprioritized given the licensing, binary-size, and
   cross-compilation complexity involved.

## 既知の未実装・要検証事項(2026-09-12時点) / Known gaps / needs verification (as of 2026-09-12)

- 実機での書き込み検証(CD/DVD/Blu-rayいずれも)は未実施。
  この開発機に光学ドライブが無いため、`xorriso -devices`の実際の
  出力形式やドライブ列挙の挙動は未検証。
- Windows/macOS/Linux各インストーラー(bundle target)の実ビルド・
  実行確認は未実施(この開発機ではWindows向けのみ`cargo build`成功)。
- 5分/10分等の固定時間トリミングUIは秒数入力のみの簡易実装。
  プリセットボタン(5分/10分)や波形プレビューは未実装。

- No real-device burn testing yet (CD, DVD, or Blu-ray) — this dev
  machine has no optical drive, so `xorriso -devices`'s actual output
  format and device-enumeration behavior are unverified.
- No real build/run verification yet for the Windows/macOS/Linux
  installer bundle targets (only `cargo build` for Windows has
  succeeded on this dev machine).
- The 5-minute/10-minute style fixed-duration trimming UI is a bare
  seconds-input field for now; preset buttons and a waveform preview
  are not implemented.
