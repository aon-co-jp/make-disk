# 開発方針＆開発環境ルール(make-disk)

全リポジトリ共通の開発ルール(自動継続・検証徹底等)は
[`open-raid-z`](https://github.com/aon-co-jp/open-raid-z)の`CLAUDE.md`を
正本として参照すること。この節では本リポジトリ固有の事項のみ記す。

## リポジトリの役割

CD/DVD/Blu-ray書き込み・音声/動画フォーマット変換・ISO出力を行う
Windows/macOS/Linux共通コードのGUIアプリ(Rust + Tauri)。
プラットフォームごとの差分はインストーラー(bundle)のみに閉じ込め、
アプリ本体(`src-tauri/src`・`src`)は単一コードベースとする。

## アーキテクチャ

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

## 外部依存(実行時に必要、同梱はしない)

- `ffmpeg` / `ffprobe` — フォーマット変換・ビットレート制御
- `xorriso` — ISO生成・ディスク書き込み(cdrtools/cdrdao/libburn/libisofs相当)

インストーラー側の課題として、これらのバイナリをOSごとにどう同梱/
案内するか(Windows: 同梱バイナリ配布、macOS: Homebrew案内、
Linux: パッケージマネージャー案内、等)は未確定・要検討。

## プラットフォーム範囲(2026-09-12更新)

- **現在**: Windows/macOS/Linuxのデスクトップ3プラットフォームが対象。
- **Android**: `npm run tauri android init`でプロジェクト生成済み
  (`src-tauri/gen/android`)、Rustのandroidターゲット(aarch64/armv7/
  i686/x86_64)へのクロスコンパイルも成功を確認済み。ただしこの開発機
  (Windows)でのAPKパッケージングは、jniLibsへのシンボリックリンク作成で
  管理者権限相当が必要な状態のため未完了。2026-09-12にユーザーが
  設定でON操作を実施したが、レジストリキー
  (`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock\
  AllowDevelopmentWithoutDevLicense`)が依然存在せず反映されていない
  ことを確認(グループポリシーによる明示的ブロックの形跡は無し)。
  再起動後の再確認待ち、保留中。
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

## Android実機検証で発見した問題と対応方針(2026-09-12)

### 発見1: main.jsのベアインポートが全プラットフォームで動かないバグ(修正済み)
`import { invoke } from "@tauri-apps/api/core"`のようなベア指定子は
バンドラー無しのWebViewでは解決できず、モジュール読み込みが例外で
止まり**main.js内の全イベントリスナーが登録されない**(=全ボタンが
無反応になる)という重大バグがあった。`window.__TAURI__.core.invoke`
経由に変更して修正済み(実機で「ファイルを追加」ボタンがネイティブ
ピッカーを開くことまで確認)。デスクトップ版でもこのバグは(検証は
していなかったが)理論上同じ影響を受けていたはずで、静的ファイル
プレビューだけでは検出できなかった教訓が大きい。

### 発見2: Tauri dialogプラグインはモバイルでフォルダ選択が未実装
実機で`open({directory:true})`を呼ぶと
`"Folder picker is not implemented on mobile"`で例外になることを
確認。ユーザー指示により「まとめて1フォルダに出力」の仕様を
モバイルでも維持する方針とし、Android向けにはSAF
(`ACTION_OPEN_DOCUMENT_TREE`)を直接扱う**独自Tauriプラグインの
新規実装が必要**という結論に至った(標準dialogプラグインの範囲では
実現不可)。

**実装計画(次回セッション向け、未着手)**:
1. `src-tauri/`配下に小さなカスタムTauriプラグインを追加
   (例: `tauri-plugin-android-folder`、Kotlin側で
   `ACTION_OPEN_DOCUMENT_TREE`のIntentを発行しActivity Resultを
   受け取り、`takePersistableUriPermission`で永続化)。
2. Rust側に`pick_output_tree() -> Result<String, String>`のような
   コマンドを追加し、返ったtree URI文字列をJS側で保持。
3. JS側(`main.js`)でAndroid判定時にこのコマンドを呼ぶよう分岐。

### 発見3(重要・優先度確定): ffmpeg/xorrisoはAndroidに存在しないため、
### SAF実装だけでは変換機能は動かない
make-diskは`std::process::Command::new("ffmpeg"/"xorriso")`で外部
バイナリをシェルアウトする設計。Android端末にはこれらのバイナリが
存在せず、同梱もしていないため、**SAFフォルダ選択を実装しても
「実行」ボタンを押した時点で確実に失敗する**(コマンドが見つからない
エラー)。

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

## 既知の未実装・要検証事項(2026-09-12時点)

- 実機での書き込み検証(CD/DVD/Blu-rayいずれも)は未実施。
  この開発機に光学ドライブが無いため、`xorriso -devices`の実際の
  出力形式やドライブ列挙の挙動は未検証。
- Windows/macOS/Linux各インストーラー(bundle target)の実ビルド・
  実行確認は未実施(この開発機ではWindows向けのみ`cargo build`成功)。
- 5分/10分等の固定時間トリミングUIは秒数入力のみの簡易実装。
  プリセットボタン(5分/10分)や波形プレビューは未実装。
