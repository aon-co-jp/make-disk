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
  - `convert.rs` — ffmpegでフォーマット変換・ビットレート制御・トリミング・
    複数区間カット(後述)
  - `capacity.rs` — CD/DVD/Blu-ray容量からの自動最大ビットレート算出、
    および基準ビットレートに対する低下度合いを4段階(下がります→
    少し下がります→かなり下がります→画質/音質が落ちます)で警告する
    `quality_warning`(下限は設けず、常に容量に収まる値を返す仕様)
  - `cpu.rs` — `open-cpu`によるCPU命令セット検出。CPUフォールバック時の
    速度目安表示と、`-preset`自動選択の両方に使う(後述)
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

## 複数区間カット・フレーム精度モード・open-cpu統合(2026-09-12)

- `CutRange`(`convert.rs`): 1本の動画に対して「最初のA〜Bをカット」
  「途中のC〜Dをカット」「最後のE〜末尾をカット」を同時にいくつでも
  指定できる。残す区間は`-c copy`(無劣化・高速)で抽出し、フォーマット
  変換が必要な場合のみ結合(concat demuxer)時に1回だけエンコードする。
- `frame_accurate`フラグ: カット境界をフレーム単位で正確に切りたい場合、
  GPUハードウェアエンコーダ(NVENC→QuickSync→AMFの順)を実際に1フレーム
  試しエンコードして本当に動くか検証してから採用する(`ffmpeg -encoders`
  の一覧に載っているかだけでは不十分——古いNVIDIAドライバでは
  `h264_nvenc`がリストには出るのに実行すると失敗する実バグを統合テストで
  発見・修正済み)。GPUが無ければCPU(`libx264`)にフォールバックする。
- `open-cpu`(エコシステム共通のCPU命令セット検出ライブラリ)をCargo
  依存として統合。CPUフォールバック時に:
  1. UIのログ欄に速度目安(fast/moderate/slow/very_slow、AVX2/AVX-512
     BW/VL/FMA等の検出状況に基づく4段階)を表示するだけでなく、
  2. `recommended_x264_preset()`で実際のffmpeg`-preset`引数を自動選択する
     (非力なCPUには`ultrafast`/`veryfast`、強力なCPUには`slow`など、
     速度目安を実際のコマンドライン引数へ反映する——表示のみでなく
     実際の分岐・制御に使っている)。
  3. `main.js`側では、フレーム精度モードのチェックボックスをONにした際、
     速度目安がslow/very_slowなら`window.confirm()`で続行確認を挟む。
- ffmpeg自身(libx264)がAVX2/AVX-512を使うかどうかは実行時にx264が
  自動判定するものであり、open-cpuの検出結果でその判定自体を上書き
  することはできない(そうする必要も無い)。open-cpuの役割は
  あくまで「-presetの選択」と「事前警告」という、実際にffmpegの
  コマンドライン引数・UIの分岐に反映される制御である。

## プラットフォーム範囲(2026-09-12更新)

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
1. **完了(2026-09-12)**: SAFフォルダ選択のUI・権限取得部分を実装した。
   `src-tauri/plugins/tauri-plugin-android-folder/`に自前Tauriプラグインを
   新規作成(Rust側`pick_output_tree`コマンド、Kotlin側
   `ACTION_OPEN_DOCUMENT_TREE`+`takePersistableUriPermission`)。
   実機(OPPO Reno11 A, Android)で「フォルダを選択」→ネイティブの
   フォルダツリーピッカー→アクセス許可ダイアログ→
   `content://com.android.externalstorage.documents/tree/primary%3ADocuments`
   というURIが実際に「出力先フォルダ」欄に表示されるところまで
   エンドツーエンドで確認済み。実装は`tauri-plugin-dialog`
   (v2.7.3)自身のAndroidソース(`DialogPlugin.kt`の
   `saveFileDialog`/`ACTION_CREATE_DOCUMENT`パターン)を正確なAPI
   リファレンスとして参照した。`Cargo.toml`では
   `[target.'cfg(target_os = "android")'.dependencies]`でAndroid限定の
   依存とし、デスクトップ版のビルド・テスト(11件)には影響しないことを
   確認済み。
   なお、返るのはcontent:// URIであり実ファイルシステムパスではないため、
   実際のffmpeg/xorriso連携(後述の2)は別途URI→実処理の橋渡しが必要。
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
- Windows/macOS/Linux各インストーラー(bundle target)はGitHub Actions
  (`v*`タグpushで発火)で全プラットフォームのビルドに成功済み
  (v0.1.0〜v0.1.2のリリースで確認)。ただし実機での動作確認は
  Windows版のみ実施済みで、macOS/Linux版は未実施。
- 5分/10分等の固定時間トリミングは、複数区間カット機能(`CutRange`)の
  「終了」欄に秒数指定で対応可能になったが、プリセットボタン
  (5分/10分ワンクリック)や波形プレビューは未実装。
- 動画プレビューエディタ(`<video>` + `convertFileSrc`)の実機での
  表示・シーク動作は未検証(ビルド成功とロジックレビューのみ)。

## HANDOFF追記(2026-09-14) Android CI追加・installerフォルダ新設・紹介ページ更新・v0.1.4リリース / Follow-up: added Android CI, new installer/ folder, updated the landing page, released v0.1.4

ユーザー指示「installerフォルダーを作ってWindows/Mac/Linux/Android
スマホ・タブレット用などのインストーラー付きアプリをリリース公開して」
への対応。

**確認した既存状態**: Windows/macOS/Linuxのデスクトップ3プラットフォーム
は既にv0.1.0〜v0.1.3で`.github/workflows/release.yml`(`v*`タグpushで
発火、`tauri-apps/tauri-action`)経由でGitHub Releaseへ公開済みだった
(`gh release view v0.1.3`で`.msi`/`.exe`/`.dmg`×2/`.deb`/`.rpm`/
`.AppImage`の全アセットを実際に確認)。Androidのみリリースパイプライン
未配線だった(この開発機ではWindows Developer Mode有効化の壁で
`tauri android build`が直接動かせず、手動`.so`コピー+`gradlew`直叩きの
回避策でローカル実機確認〈OnePlus A401OP〉のみ済んでいた)。

**今回追加した3点**:
1. `.github/workflows/release.yml`へ`release-android`ジョブを追加
   (`ubuntu-latest`、`android-actions/setup-android@v3`でSDK、
   `sdkmanager`でNDK 27.0.12077973を明示インストール、Rust側は
   aarch64/armv7/i686/x86_64のAndroidターゲットをクロスコンパイル、
   `npm run tauri android build -- --apk ... --debug`でuniversal APK
   〈スマホ・タブレット共通、署名鍵未設定のためサイドロード専用〉を
   ビルドし`gh release upload`で同じReleaseへ添付)。この開発機の
   Developer Mode制約はGitHub Actionsのubuntu-latestランナーには
   無いため、公式コマンドをそのまま使える設計にした。
   **正直な開示**: このCIジョブ自体はこのセッションでは実際に
   グリーンになるまで検証していない(NDKバージョン27.0.12077973は
   Tauri 2の一般的な要求バージョンからの推測であり、実際にCIを
   走らせてみないと確定できない——次回セッションで`gh run watch`
   により実際の成否を確認し、失敗する場合はログを見てNDK
   バージョン等を調整すること)。
2. `installer/`フォルダを新設(`installer/README.md`)。バイナリ自体は
   コミットせず(リポジトリ肥大化を避ける)、プラットフォームごとの
   ファイル形式・ビルド元・正直な制限(未署名APK・実機書き込み未検証・
   iOS保留)をまとめた対応表を置いた。
3. `webpage/index.html`(VPS `easy-web.tokyo/make-disk/`で公開中の
   紹介ページ、`make-disk-web.service`が`/root/repository/make-disk/
   webpage/`を`python3 -m http.server 8108`で配信、nginx経由で公開)の
   「現在は初期開発段階のため近日公開予定」という**既に古くなっていた
   文言**を、実際に公開済みの全プラットフォーム一覧+プレリリース扱い
   である旨の正直な開示へ更新。Androidバッジも追加。VPS側は
   `/root/repository/make-disk`で`git pull`するだけで反映される
   (ビルド不要、静的ファイルをそのまま配信する設計のため)。

`package.json`/`src-tauri/Cargo.toml`/`src-tauri/tauri.conf.json`の
バージョンを0.1.3→0.1.4へ統一し、`v0.1.4`タグをpushしてリリースを
発火させた。

## HANDOFF追記(2026-09-14続き) 次の開発増分: ffmpeg/xorrisoのsidecarバイナリ同梱化(ユーザー指示、設計を記録) / Next increment: bundle ffmpeg/xorriso as Tauri sidecar binaries (user request, design recorded)

ユーザーから「必要性のある全てのリポジトリを同梱してのインストーラー
付きアプリとして完成させて」との指示を受けた。現状、実行には`ffmpeg`/
`xorriso`が別途インストール済みでPATHが通っている必要があり(README/
webpage双方で明記)、これをインストーラーへ同梱する(ユーザーが別途
インストールしなくて済む)ことが求められている。**拙速な実装で壊すより、
今回スコープ済みの増分〈Android CI・installerフォルダ・紹介ページ更新・
リリース〉を確実に完成させることを優先し、この機能は正式な次の増分
として設計だけ記録した**(実装は次回セッション)。

**設計方針**:
- Tauriの[`bundle.externalBin`](https://tauri.app/develop/sidecar/)
  機構(通称sidecar)を使い、`ffmpeg`/`ffprobe`/`xorriso`の実行可能
  ファイルをプラットフォームごとにビルド成果物へ同梱する。
  `src-tauri/tauri.conf.json`の`bundle.externalBin`にバイナリの
  ベースパスを列挙し、実行時は`Command::sidecar("ffmpeg")`のように
  呼び出す(現状は`std::process::Command`で`PATH`上の`ffmpeg`を
  直接呼んでいる`convert.rs`/`iso.rs`/`burn.rs`の呼び出し方を変更する
  必要がある)。
- **バイナリの入手元**: 本家ffmpeg/xorrisoの静的ビルド済みバイナリ
  (Windows: 公式ffmpeg.orgの静的ビルド配布・xorriso公式は無いため
  MSYS2/Chocolatey等の配布物を調査要。macOS/Linux: Homebrew/apt等の
  パッケージではなく、CI内で静的リンクビルドするか、事前ビルド済み
  バイナリをダウンロードして`externalBin`用ディレクトリへ配置する
  ビルドステップをrelease.ymlへ追加する形になる見込み)。
  `rs-FFmpeg`/`rs-xorriso`(このリポジトリの姉妹Rustリスペクト版)は
  README/webpageの既存の正直な開示の通り**現時点で本家の一部機能しか
  対応しておらず**(rs-xorrisoはISO生成のみ・長いファイル名切り詰め・
  書き込み機能無し、rs-ffmpegは無圧縮WAVのprobe/変換のみ)、これらを
  そのままsidecarとして同梱してもmake-disk本体の実要件(任意フォーマット
  変換・実ディスク書き込み)を満たせない——**本家バイナリの同梱が
  本命、rs-FFmpeg/rs-xorrisoは将来両者が本家相当の機能を持った時点での
  代替候補**という位置づけを維持する。
- **ライセンス上の注意**: FFmpeg・xorrisoともGPL系ライセンス
  (xorrisoはGPLv2/v3、FFmpegはビルド構成によりLGPL/GPL)。バイナリを
  同梱配布する場合、ライセンス全文の同梱・ソース入手先の明記が
  必要(webpage/README/インストーラー内のライセンス表示への追記が
  要る)。
- **Android/iOSでの扱い**: モバイル版は光学ドライブアクセス自体が
  無いため書き込み機能はそもそも対象外(既存方針通り)。フォーマット
  変換機能のみをモバイルでも提供するなら、ffmpegのAndroid/iOS向け
  クロスコンパイル(NDK/Xcodeツールチェーン)が別途必要——これも
  本家バイナリの静的クロスコンパイルが前提になるため、まずデスクトップ
  3プラットフォームでの同梱を先に完成させ、モバイルは次々回以降の
  増分とするのが妥当と判断する。

**次回セッションでの着手順序案**: (1) デスクトップ3プラットフォーム
向けの本家ffmpeg/xorriso静的バイナリの入手元を確定させる調査、
(2) `tauri.conf.json`の`externalBin`配線+`convert.rs`/`iso.rs`/
`burn.rs`の呼び出し変更、(3) release.ymlへバイナリダウンロード
ステップを追加、(4) ライセンス表示の追加、(5) 実機での動作確認
(sidecar経由でも既存の統合テストが通ることを確認)。

## HANDOFF追記(2026-09-14) ffmpeg/ffprobeのsidecar同梱化(Windows/Linux)実装完了、v0.1.6リリース / Follow-up: implemented ffmpeg/ffprobe sidecar bundling for Windows/Linux, released v0.1.6

前回のHANDOFFで設計だけ記録していた同梱化(ユーザー指示への対応)を、
このセッションで実装・実機検証・リリースまで完了させた。

**実装したもの**:
- `engine/sidecar.rs`新設。`resolve_tool(name)`が、実行ファイルと
  同じディレクトリにある同梱バイナリを探し、見つかればそれを、
  無ければ従来通りPATH上の`name`を使う`Command`を返す。
  `Command::new("ffmpeg")`だった8箇所(`probe.rs`・`convert.rs`
  〈非テスト部分の2箇所〉・`iso.rs`・`burn.rs`)を全て置き換えた。
- **Tauri公式の`tauri_plugin_shell::ShellExt::sidecar`(AppHandle経由の
  非同期API)は採用しなかった**——採用すると`engine/*.rs`の同期関数
  全てを非同期化し`AppHandle`を全呼び出し経路(`lib.rs`の
  `#[tauri::command]`群含む)へ配線する必要があり、影響範囲が広い
  大規模な変更になる。かわりに、Tauriのsidecar機構が実際にバイナリを
  配置する場所を`std::env::current_exe()`から自前で解決する軽量な
  実装にした——既存の同期設計・既存テストを一切壊さずに済む
  トレードオフとして採用。
- `tauri.windows.conf.json`・`tauri.linux.conf.json`(新設、Tauriの
  プラットフォーム別config上書きの仕組み)に`bundle.externalBin`を
  設定。macOSは対象外(理由は後述)。
- `scripts/fetch-ffmpeg-sidecars.sh`新設。
  [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds)の
  静的ビルドをダウンロードし、`src-tauri/binaries/`へ`externalBin`の
  命名規則(`<name>-<target-triple>[.exe]`)で配置する。
  `release.yml`のWindows/Linuxビルドジョブがビルド前に自動実行する。
- `src-tauri/binaries/`はgit管理外(`.gitignore`、バイナリでリポジトリを
  肥大化させないため)、`README.md`だけをコミットして対応表・入手方法・
  正直な制限を記載。

**実機検証で発見・修正した実装ミス(重要な教訓)**: 当初、sidecar
バイナリは`externalBin`のソース側と同じ`<name>-<target-triple>[.exe]`
という名前のままインストール後も配置されると想定して`resolve_tool`を
実装していた。実際に`npm run tauri build`でMSI/NSISインストーラーを
ビルドし(このセッションで実際にビルド成功、`.msi`が約2.8MB→約130MB、
`.exe`が約1.9MB→約95MBへ増加=バイナリが実際に同梱されたことを確認)、
NSISインストーラーを`/S /D=<dir>`でサイレントインストールして
インストール先の中身を実際に確認したところ、**Tauriのバンドラーは
ターゲットトリプル部分を落として`ffmpeg.exe`/`ffprobe.exe`という
bareな名前で配置する**ことが判明した(ビルド対象は常に単一ターゲット
なので、サフィックスを付ける理由がそもそも無い)。当初の実装のままだと
同梱バイナリを一切見つけられず、常にPATHへフォールバックするだけの
無意味な変更になっていた——`resolve_tool`をbareな名前で探す実装へ
修正し、修正後に改めて全テスト(15本、うち新規4本)が通ることを確認。
**この発見は「ビルドが通る」「テストが通る(自分で作ったモックが
自分の想定と一致するだけ)」だけでは不十分で、実際にインストーラーを
作りインストールして中身を見るという実機検証まで行って初めて防げた
バグだった**——このプロジェクト・このエコシステム全体の検証方針
(フェイクな成功にしない)が実際に効いた具体例として記録する。

**新規テスト4本**(`engine::sidecar`3本、`engine::probe`1本)、
うち`probe_actually_executes_a_real_sidecar_binary_when_one_is_bundled`
は実際にダウンロードしたffmpeg/ffprobeバイナリをテスト実行ファイルの
隣へ実際に配置し、`probe()`がそれを検出・実行して正しい動画長を返す
ことを検証する実機E2Eテスト(`src-tauri/binaries/`が無い環境では
スキップ)。クレート全体15本成功、clippy警告は既存の無関係な1件のみ。

**正直な開示(誇張しない、今回のスコープ外)**:
- **macOSは未対応**——BtbN/FFmpeg-BuildsはmacOSビルドを配布しておらず、
  Intel/Apple Silicon両対応の信頼できる単一の入手元を今回確定できな
  かった。
- **xorrisoは未対応**——本家に信頼できる静的クロスプラットフォーム
  ビルド配布が見当たらなかった。GPLライセンス表示整備も別途必要。
- **モバイル(Android/iOS)は未対応**——ffmpegのNDK/iOS向けクロス
  コンパイルが別途必要、かつモバイル版はそもそもディスク書き込み
  機能の対象外(既存方針通り)。
- いずれも`resolve_tool`の仕組み自体は汎用的なので、静的バイナリの
  入手元さえ確定すればコード変更なしで追加できる設計にしてある。

v0.1.6としてタグpush、CI(`release.yml`)で全プラットフォーム
ビルド・GitHub Release公開・VPS紹介ページへの反映まで実施予定
(このHANDOFF記載時点でCI実行前——次回このメッセージを読む人は
実際の成否を`gh run list`で確認すること)。

## HANDOFF追記(2026-09-16) 自動アップデート確認・rs-FFmpeg/rs-xorriso追加同梱・最高音質モード・変換の並列化、v0.1.7リリース / Follow-up: startup update-check, rs-FFmpeg/rs-xorriso bundling, max-quality mode, parallel conversion, v0.1.7

ユーザーからの複数の指示に対応した、今回のセッションでの主な変更点。

### 1. 自動アップデート確認機能

アプリ起動時に一度だけGitHub Releasesの最新版を確認し、新しいバージョンが
あれば「新しいバージョン`<version>`があります。バージョンアップします
か？」(日英併記)のダイアログを出し、選択実行できる機能を追加した
(ユーザー指示通りの文言)。

- `tauri-plugin-updater`・`tauri-plugin-process`をデスクトップのみ
  (`#[cfg(not(any(target_os = "android", target_os = "ios")))]`)で
  追加。モバイルは対象外(ストア/APKサイドロードでの更新が前提)。
- 署名鍵ペアを`npx tauri signer generate`で生成
  (`src-tauri/updater.key`はgit管理外、`src-tauri/updater.key.pub`は
  `tauri.conf.json`の`plugins.updater.pubkey`へ埋め込み済み)。
  秘密鍵はGitHub Secrets(`TAURI_SIGNING_PRIVATE_KEY`)へ登録済み。
- `tauri.conf.json`に`plugins.updater.endpoints`
  (`https://github.com/aon-co-jp/make-disk/releases/latest/download/latest.json`)
  と`bundle.createUpdaterArtifacts: true`を追加。
- **正直な開示・重要な設計変更**: GitHubの`/releases/latest/download/...`
  エイリアスは**prerelease扱いのリリースを除外する**仕様のため、
  `release.yml`の`prerelease`を`true`→**`false`**へ変更した——これに
  より`v0.1.7`以降のリリースはGitHub上で通常のリリース(pre-release
  ラベル無し)として表示されるようになる。アプリ自体の完成度が
  変わったわけではなく、あくまでアップデーター機構を正しく動かす
  ための技術的な必要変更である点に注意。
- `main.js`に`checkForUpdatesOnStartup()`を追加、起動時に自動実行。
- **実機検証で発見した2つの罠**: (1) `plugins.updater`を設定しただけ
  では`.sig`署名アーティファクトは生成されない——`bundle.
  createUpdaterArtifacts: true`を明示的に設定する必要があった
  (最初のローカルビルドでは`.sig`が生成されずハマった)。(2) ローカル
  ビルドでは`TAURI_SIGNING_PRIVATE_KEY`環境変数が無いと
  `failed to build app`という手掛かりの薄いエラーで失敗する
  (署名ステップの失敗が汎用エラーメッセージに丸められる)——
  実際に`export TAURI_SIGNING_PRIVATE_KEY=$(cat src-tauri/updater.key)`
  してから再ビルドし、`.sig`ファイル生成まで実機確認して解決した。

### 2. rs-FFmpeg/rs-xorrisoの追加同梱

ユーザー指示「作成したRust版(rs-FFmpeg・rs-xorriso、共にMITライセンス・
新規作成は不要で既存)も同梱して」への対応。`scripts/
build-rs-tribute-sidecars.sh`新設(隣にcloneされた`rs-FFmpeg`/
`rs-xorriso`をソースからビルドし`src-tauri/binaries/`へ配置)、
`tauri.windows.conf.json`/`tauri.linux.conf.json`の`externalBin`へ
`binaries/rs-ffmpeg`・`binaries/rs-xorriso`を追加。実際に
`npm run tauri build`でインストーラーへ同梱され、`target/release/`に
`rs-ffmpeg.exe`/`rs-xorriso.exe`(bareな名前、既存の`sidecar.rs`の
命名規則通り)として配置されることを実機確認済み。`release.yml`にも
rs-FFmpeg/rs-xorrisoをcheckout+ビルドするステップを追加。

**正直な開示**: これらのバイナリは同梱されるが、アプリ本体のロジックは
これらを自動選択・呼び出さない(既存のCLAUDE.md「早期WIPで本家の
完全な代替にはならない」という開示は変わらず——あくまで「試したい人が
使える実験的な追加バイナリ」の位置づけ)。

### 3. 「最高音質・最高画質で記録する」モード

ユーザー指示「音声や画像フォーマットを選択する代わりに最高音質＆最高
画質で記録するを選択可能として」「未選択時はMP4などの動画をCDのISOに
変換する場合は、自動でロスレスWAVでかつISO同時変換化」への対応。

- `bitrate-mode`ラジオボタンに`max_quality`を追加。
- 音声/動画フォーマットが1つも選択されていない場合、自動的にWAV
  (ロスレスPCM)を選択し、常にISO化する。
- `capacity.rs`に`estimate_lossless_audio_fit`(CD品質ロスレスWAVで
  指定ディスクに収まるかどうか、収まらない場合は代わりに何秒までなら
  収まるかを算出)を新設——ユーザー指示「必要な時間やデータサイズを
  自動で割り出す」に対応。新規テスト3本(短い尺は収まる/長すぎる尺は
  収まらず代替秒数を返す/より大きいディスクなら収まる、の3パターン)。
- 実行時に「[cd700] 収録時間(...)はロスレスWAVで収まります(必要:
  ...MB / 容量: ...MB)」のような見積もりを日英併記でログ表示する。

### 4. 変換処理の並列化(非同期・マルチスレッド)

ユーザー指示「MP4をWAVでかつISOに同時変換などいくつかの代表的な
組み合わせも非同期でマルチスレッドで同時に行える様に」への対応。
`main.js`の`convertAll`を、逐次`await`のfor文から
`runWithConcurrencyLimit`(単純なワーカープール、外部ライブラリ不要、
`navigator.hardwareConcurrency`の半分を目安の同時実行数にする)へ
変更。音声変換・動画変換自体も`Promise.all`で互いを待たず並行に開始
するようにした。**設計メモ**: `convert_media`は非asyncな
`#[tauri::command]`のため、Tauri v2は内部で`spawn_blocking`相当の
スレッドプール実行にしており、JS側で複数の`invoke`を逐次`await`せず
同時に発行すれば、複数のffmpegプロセスが実際に並列実行される
(Rust側の追加変更は不要だった)。

### バージョン

`package.json`/`src-tauri/Cargo.toml`/`src-tauri/tauri.conf.json`を
0.1.6→0.1.7へ統一。クレート全体テスト18本成功(新規4本:
`estimate_lossless_audio_fit`系3本+既存の再検証)、clippy警告は
既存の無関係な1件のみ。実機で`npm run tauri build`成功・`.msi`/
`.exe`インストーラー生成・`.sig`署名アーティファクト生成・全4種の
sidecarバイナリ(ffmpeg/ffprobe/rs-ffmpeg/rs-xorriso)の同梱を確認済み。

**次回への引き継ぎ**: (1) `v0.1.7`タグpush後のCI実際の成否確認
(特に`prerelease: false`への変更・`TAURI_SIGNING_PRIVATE_KEY`
シークレットを使った署名付きビルドがCI環境でも成功するか)。
(2) 実機での自動アップデート機能そのもののエンドツーエンド確認
(v0.1.7インストール後、v0.1.8のような次のリリースを出して実際に
「アップデートがあります」ダイアログが出て更新できるか)。
(3) ユーザーから追加指示のあった「open-directx/open-cuda/aruaru-llmの
マルチCPU・マルチコア・非同期対応」はこのリポジトリではなく各リポジトリ
側での横断的な調査・改修が必要な別課題として保留。

## HANDOFF追記(2026-09-16続き) v0.1.7 CI実際の結果確認・Android CI修正・複数ドライブ並列書き込み、v0.1.8 / Follow-up: verified v0.1.7 CI results, fixed Android CI, parallel multi-drive burning, v0.1.8

**v0.1.7の実際のCI結果(前回HANDOFFで「次回確認」としていた項目)**:
デスクトップ4ジョブ(Windows/macOS×2/Linux)は全て成功
(`gh release view v0.1.7`で`.sig`署名ファイル・`latest.json`
アップデーターマニフェスト・`prerelease: false`を実際に確認済み)。
Linuxジョブは`tauri-action`ステップだけで約24分かかった(v0.1.6の
約27分と同様の傾向——ffmpeg/ffprobe同梱+LTOビルドのため。ハングでは
なく正常な所要時間と判断)。VPS(`easy-web.tokyo/make-disk/`)への
反映も`git pull`+`curl`でのライブ確認まで完了。

**release-androidジョブは失敗**——今回のセッションの変更とは無関係な
外部要因: `android-actions/setup-android@v3`が
`Warning: Failed to find package 'tools'`で失敗するようになっていた
(Googleが非推奨の`tools`パッケージをSDKリポジトリから削除したためと
見られる)。**修正**: GitHub Actionsの`ubuntu-latest`ランナーには
元々Android SDK(`cmdline-tools`込み、`ANDROID_HOME=/usr/local/lib/
android/sdk`)がプリインストールされていることを確認済みだったため、
`android-actions/setup-android@v3`のステップ自体を削除し、
プリインストール済みの`sdkmanager`でNDKだけを追加インストールする
形に簡略化した(次回のタグpushで実際の成否を確認すること)。

**複数ドライブ並列書き込み**: ユーザー指示「ディスクへの書き込みを
選んで実行すると...書き込みも同時に行なって」への対応。`main.js`の
書き込みループを、物理ドライブが複数検出された場合はドライブ単位で
並行実行するよう変更(`Promise.all`)——1台のドライブへ同時に2つの
書き込みストリームは送れないため、同じドライブへ割り当てられた
ディスク種別同士は順番に、異なるドライブへの書き込みは互いを待たずに
並行実行する設計(ドライブ数より種別数が多い場合はラウンドロビンで
割り当て)。Rust側(`burn.rs`)は無変更——並列度の制御はJS側の
呼び出しタイミングだけで実現できる(`convertAll`の並列化と同じ設計
パターン)。

バージョンを0.1.7→0.1.8へ統一。

**次回への引き継ぎ**: (1) Android CI修正(setup-android削除)が
実際にタグpushで成功するか確認。(2) 複数物理ドライブでの並列書き込みは
実機検証未実施(この開発機には光学ドライブが1台も無い——既存の
「実機での書き込み検証は未実施」という制約がここでも該当する)。

## HANDOFF追記(2026-09-16続き2) v0.1.9 CI障害3件の切り分け・修正、v0.1.10 / Follow-up: diagnosed and fixed 3 CI failures for v0.1.9, v0.1.10

v0.1.9のタグpushで、デスクトップ4ジョブ全てが
`Resource not accessible by integration`(リリース作成権限エラー)で
一斉に失敗するという新しい問題が発生した。原因を`gh api`で調査した
ところ、**このリポジトリのActions設定`default_workflow_permissions`が
`"read"`になっていた**ことが判明(`gh api repos/aon-co-jp/make-disk/
actions/permissions/workflow`で確認)——ワークフローYAML側の
`permissions: contents: write`はリポジトリ設定の上限を超えて昇格
できないため、リポジトリ側の既定値が"read"のままだと個々のワークフローの
宣言は無視される。`gh api -X PUT .../actions/permissions/workflow
-f default_workflow_permissions=write`で修正した。

**教訓その1**: `gh run rerun --failed`は、実行中のrunに紐づいた
トークン設定をそのまま引き継ぐらしく、権限設定を直してから
再実行しても直らなかった(実際に確認)。真に新しい設定を反映させるには
新しいタグをpush(=完全に新規のワークフロー実行)する必要がある。

Android修正(前回HANDOFF記載のsdkmanagerパス修正)も同じv0.1.9で
検証したところ、さらに2つの実バグが連続して見つかった:
- **教訓その2**: `install NDK`ステップの`env:`ブロックで設定した
  `ANDROID_HOME`は、そのステップの中でしか有効ではない——後続の
  `build Android universal APK`ステップで`${{ env.ANDROID_HOME }}`
  (ワークフロー式の`env`コンテキスト)を参照しても空文字列にしか
  ならなかった(`env`コンテキストは`$GITHUB_ENV`へ明示的に書き出した
  変数だけを見る)。結果`NDK_HOME: /ndk/27.0.12077973`という壊れた値に
  なり、`tauri android build`が「Android NDK invalid」で失敗した。
  修正: `NDK_HOME`は`run:`スクリプト内で`export NDK_HOME="$ANDROID_HOME/
  ndk/..."`のようにシェル変数展開で組み立てる(`$ANDROID_HOME`自体は
  ubuntu-latestランナーにOS標準で設定済みのjob全体で有効な環境変数)。

これら3件は全て、CI環境特有の設定・スコープの落とし穴であり、
ローカルの`cargo build`/`npm run tauri build`では検出できない種類の
問題だった(実際にCIで実行して初めて発見できた)——このプロジェクトの
実機検証重視の方針が、ローカルビルドだけでは終わらない理由の実例として
記録する。

バージョンを0.1.9→0.1.10へ統一(Android修正の検証用)。

**ユーザーから新たに2件の大きな機能追加指示があった**(まだ未着手、
CI安定化を優先したため):
1. サイズ指定/時間指定/ディスクいっぱい/「AI判断で自動カット」の
   いずれかを選択可能にする機能(日英表示)。**正直な開示の方針**:
   「AI判断」は実際のML/LLM分析モデルを統合するものではなく、
   ffmpegの無音検出(`silencedetect`)等のヒューリスティックによる
   自動カットとして実装する予定(誇張しない)。
2. 音声・静止画・動画に加え、最大4Kの見開きPDF対応(右綴じ/左綴じ選択、
   選択に応じた左右ページ入れ替え)。Windows/macOS/Linux/Android/
   iPhone/iPad対応。**正直な開示**: iOS/iPadOSは既存の制約(テスト
   実機を保有していないため対応保留、本ファイル「プラットフォーム
   範囲」節参照)がそのまま当てはまる。PDF処理は音声/動画変換とは
   別の技術領域(PDFライブラリ選定・見開きページレイアウト処理)で、
   別途まとまった設計が必要。
3. (さらに)将来的なゲーム機対応の要望も受けた——現時点では具体的な
   対象機種・実現方式とも未確定のため、方向性のみ記録し設計は次回以降。

**次回への引き継ぎ(優先順位)**: (1) v0.1.10でAndroid CI修正の最終
検証(NDK_HOMEのシェル変数展開修正)。(2) 上記機能1(サイズ/時間/
ディスクいっぱい/自動カット選択)の設計・実装。(3) 上記機能2(PDF
見開き対応)は独立した大きな増分として別途設計セッションが必要。
(4) ゲーム機対応は要件が未確定のため、まずユーザーへの追加ヒアリングが
必要(対象機種・用途を明確にしてから着手すべき)。

## HANDOFF追記(2026-09-16続き3) v0.1.10でNDK_HOME修正は成功・Androidは4件目の実バグ(レースコンディション)、v0.1.11 / Follow-up: NDK_HOME fix worked in v0.1.10, Android hit a 4th real bug (race condition), v0.1.11

v0.1.10で前回のNDK_HOME修正(シェル変数展開)は**実際に効いた**——
`install NDK`・`build Android universal APK`ステップの両方が成功し、
universal APKのビルド自体は完走した。デスクトップ4ジョブ(Windows/
macOS×2/Linux)も全て成功(`.sig`・`latest.json`込みで実際に確認、
VPSへも反映済み)。

**Androidは4件目の実バグ(今度はレースコンディション)で失敗**:
`upload APK to release`ステップが`release not found`で失敗した。
原因: `release-android`ジョブと`release`(デスクトップ)ジョブは
ワークフロー内で並行実行される独立ジョブで、実際のGitHub Release
本体を作成するのは`release`ジョブ(`tauri-action`経由)側——
Androidのビルドはデスクトップより速く終わるため(6分程度)、
デスクトップ側がまだリリースを作っていない時点でAPKのアップロードを
試みてしまっていた。**修正**: `release-android`に`needs: release`を
追加し、デスクトップ側(全マトリクス)の完了を待ってから実行する
ようにした。

**このセッションでAndroid CIだけで合計4件の異なる実バグを発見・修正した
(いずれもローカルビルドでは検出不可能、CI環境特有の問題)**:
(1) `--target x86`という誤ったCLI引数値(正しくは`i686`)、
(2) `android-actions/setup-android@v3`の非推奨パッケージ取得失敗、
(3) `sdkmanager`のPATH未設定+`NDK_HOME`の`env`コンテキストスコープの
誤解、(4) 今回のジョブ間レースコンディション。加えてデスクトップ側でも
リポジトリの`default_workflow_permissions`設定という、コードとは
無関係な環境要因の障害が1件あった。**教訓**: CI/CDパイプラインの
複雑さ(並行ジョブ・環境変数スコープ・外部アクションの非推奨化・
リポジトリ設定)は、アプリケーションコード自体の複雑さとは独立した、
別の種類の「実機」であり、同じ「実際に動かして確認する」という
検証哲学がそのまま当てはまる。

バージョンを0.1.10→0.1.11へ統一(Android修正の最終検証用)。

**次回への引き継ぎ**: v0.1.11でAndroidジョブが最終的に成功するか確認
すること。成功すれば、Android CIに関する既知の実バグは今回のセッションで
全て解消されたことになる。その後、ユーザーから積み上がっている機能
要望(サイズ/時間/自動カット選択、PDF見開き対応、複数動画合成編集・
等間隔分割、ゲーム機対応)へ順番に着手する。

## HANDOFF追記(2026-09-16続き4) 「収まらない分の自動調整方法」選択機能を実装、v0.1.12 / Follow-up: implemented the "how to fit content that's too long" selector, v0.1.12

v0.1.11でAndroidジョブが最終的に成功したことを確認済み(過去の
HANDOFFで記録済みの4件のAndroid実バグは全て解消)。これを受けて、
CI安定化を優先して積み上げていた機能要望の1番目に着手した:

> 「DISKいっぱいのビットレートに変換して書き込みも良いですが、画質や
> 音質に、影響が少ない範囲で英語と日本語で表示して　サイズか、
> 時分秒か、DISKいっぱいか、AI判断で自動カット　のいずれかを選択可能
> として」

**実装内容**:
- バックエンド(`src-tauri/src/engine/convert.rs`): `detect_silence_ranges()`
  (ffmpegの`silencedetect`フィルタで無音区間を検出)と
  `bitrate_for_target_size_kbps()`(目標サイズ・総尺からビットレートを
  逆算)を新設。**正直な開示**: 「AI判断」は実際には音量ベースの無音
  検出であり、意味的なシーン解析(真のAI/ML判断)ではない。この点は
  コードのdocコメント・UI双方に日英併記で明記した。ffmpeg実バイナリを
  使った統合テスト(無音区間を含むテスト用音声を実際に生成し検出させる)
  を含め、全21テストがパス。
- `src-tauri/src/lib.rs`: 上記2関数を`#[tauri::command]`として
  Android/非Android両方のinvoke_handlerに登録。
- `src/index.html`: 「7.5. 収まらない分の自動調整方法」セクションを
  新設。`fit-strategy`ラジオボタン(disk_full[既定]/target_size/
  target_duration/ai_auto_cut)、サイズ入力・時分秒入力を追加。
- `src/main.js`: run-btnハンドラ内に分岐ロジックを追加。
  - `target_size`: 総尺から`calc_bitrate_for_target_size_kbps`で
    ビットレートを算出し上書き。
  - `target_duration`: 手動カット未設定のファイルに対し、先頭から
    指定秒数までの`CutRange`を自動設定。
  - `ai_auto_cut`: 手動カット未設定のファイルごとに
    `detect_silence_ranges`を呼び、検出した無音区間をそのまま
    `CutRange`として設定(結果は既存の「編集...」UIで確認・調整可能)。
  - `disk_full`(既定): 既存のディスク容量ベースの自動ビットレート
    算出をそのまま維持(無変更)。
  - いずれの自動編集も、ユーザーが既に手動でカット区間を設定済みの
    ファイルは上書きしない(既存の編集作業を尊重)。

バージョンを0.1.11→0.1.12へ更新。

**次回への引き継ぎ**: 残る積み上げ要望は (2) 音声・静止画・動画に
加えPDF見開き対応(右綴じ/左綴じ、最大4K、Windows/Mac/Linux/Android/
iPhone/iPad対応——iOS/iPadOSは実機無しの制約が既存)、(3) 複数の
音声・静止画・動画・PDFの合成編集・等間隔/サイズ指定分割+余りの
自動ディスクフィット、(4) 将来的なゲーム機対応(対象プラットフォーム
の要確認)。この順で着手予定。また、自動アップデート機能自体は
ビルド・署名基盤の検証は済んでいるが、実際にインストール済みの
旧バージョンが新バージョンを検知・適用するE2E動作確認はまだ未実施。

## HANDOFF追記(2026-09-16続き5) PDF見開き対応(右綴じ/左綴じ、最大4K)を実装、v0.1.13 / Follow-up: implemented PDF spread support (right/left binding, up to 4K), v0.1.13

積み上げ要望の2番目に着手した:

> 「音声、静止画、動画の他に、最大4Kの見開きPDF対応で、右綴じ、
> 左綴じ選択可能で、選択によって左右も入れ替える機能も搭載させて
> Windows、MAC、LINUC、Android、iPhone、iPAD対応して」

**実装内容**:
- 新規モジュール`src-tauri/src/engine/pdf.rs`: `BindingDirection`
  (RightToLeft/LeftToRight)、`compose_spread()`(2ページを横に並べ、
  綴じ方向に応じて左右を入れ替え、4K上限〈3840px〉を超える場合は
  縦横比を保って縮小)、`render_pdf_as_spreads()`(PDF全体を見開き画像
  群として書き出す)。
- **正直な開示・設計判断**: 見開き合成そのもの(`compose_spread`)は
  純Rustの`image` crateのみで実装し、外部ネイティブ依存が無いため
  Android/iOSを含む全ターゲットでコンパイル・実行できる。実際に
  合成ロジックを5件の単体テストで検証済み(左綴じ/右綴じでの左右配置、
  高さの異なるページの拡縮、4K超過時の縮小、4K以内では無変更)——
  全26テストがパス、clippyも(既存の無関係な1件を除き)クリーン。
  一方、PDFのラスタライズ(ページ→画像)自体はpoppler-utils
  (`pdftoppm`/`pdfinfo`)をffmpeg/xorrisoと同じsidecarパターンで
  呼ぶ設計にしたが、**poppler-utilsはまだこのリポジトリのCIで
  sidecarバイナリとして同梱・検証されていない**(現時点では実行環境の
  PATHに別途インストールされている前提。ffmpeg同様の取得スクリプト
  〈`scripts/fetch-ffmpeg-sidecars.sh`に相当するもの〉は次の増分で
  追加予定)。iOS/iPadOSは既存の「実機無し」の制約により、そもそも
  Tauriモバイルビルド自体がこのセッションでは未検証のまま。
- `src-tauri/src/lib.rs`: `convert_pdf_to_spreads`を`#[tauri::command]`
  として追加・両invoke_handlerへ登録。4K超過指定は常に4Kへ丸める
  (ユーザー指示「最大4K」の厳密な適用)。
- `src/index.html`: 「4.5. PDF見開き変換」セクションを新設
  (右綴じ/左綴じのラジオボタン、poppler-utils未同梱である旨の
  日英併記の注記)。
- `src/main.js`: run-btnハンドラの先頭でソース内のPDFファイルを検出し、
  見開き変換を実行。ソースが全てPDFの場合は音声/動画フォーマット
  必須のバリデーションをスキップしてPDF変換のみで完了とする。
  音声/動画変換(`convertAll`)からはPDFファイルを除外(ffmpegへ渡さない)。

バージョンを0.1.12→0.1.13へ更新。

**次回への引き継ぎ**: (1) poppler-utils(pdftoppm/pdfinfo)の
Windows/Linux向け静的ビルド取得スクリプトを追加し、真に「同梱・
単体で動く」状態にする、(2) 実際のPDFファイルを使ったE2E動作確認
(このセッションでは合成ロジックの単体テストのみ、実PDFでの
pdftoppm呼び出し経路は未検証)、(3) Android/iOS向けのUI導線確認
(PDF選択→見開き変換→出力先確認)、(4) 残る積み上げ要望
(複数の音声・静止画・動画・PDFの合成編集・等間隔/サイズ指定分割+
余りの自動ディスクフィット、将来的なゲーム機対応)への着手。

## HANDOFF追記(2026-09-16続き6) PDF綴じ方向の一括変換・保存機能を追加、v0.1.14 / Follow-up: added bulk PDF binding-direction conversion & save, v0.1.14

ユーザー指示への追加対応:

> 「make-disk の機能として pdf が左綴じを右綴じや右綴じを左綴じなどに
> 一括編集して保存も可能にして」

これは前段(v0.1.13)の「見開き画像として書き出す」機能とは別の要望——
**PDF自体(画像化せず)のページ順序を編集して、PDFのまま保存する**機能。

**実装内容**:
- `src-tauri/src/engine/pdf.rs`に`reverse_pdf_page_order()`を追加。
  綴じ方向の変換はページの並び順を反転することと等価(反転操作は
  どちら向きにも効く自己逆元)なので、変換先の方向を毎回指定する
  必要が無い設計にした。純Rustの`lopdf`(ネイティブ依存無し、
  Android/iOSクロスコンパイルも問題無い)でPDFの`/Pages`の`/Kids`配列を
  直接反転して保存する。
- **既知の制限(正直な開示)**: ページツリーがフラット(直接ページ
  オブジェクトのみ)であることが前提。スキャナ出力の単純なPDFは
  ほぼこの構造だが、章ごとに入れ子のページツリーを持つ複雑なPDFには
  未対応で、その場合は黙って壊れたPDFを作らず明確なエラーを返す。
- テスト: lopdfでメモリ上に最小限のテスト用PDF(ページごとに異なる
  MediaBox幅を持たせて順序を判別可能にした)を実際に構築し、
  (1) フラットなページツリーが正しく反転されること、(2) 入れ子の
  ページツリーが検出されて明確なエラーになること、の2つを実際に
  検証(モック無し、本物のPDF読み書きを経由)。全28テストがパス、
  clippyも既存の無関係な1件を除きクリーン。
- `src-tauri/src/lib.rs`: `rebind_pdfs`を`#[tauri::command]`として
  追加(複数PDFの一括処理に対応、`Vec<Result<String, String>>`を
  返し1件の失敗で全体を止めない)、両invoke_handlerへ登録。
- `src/index.html`: PDF見開き変換セクション内に「選択したPDFの
  綴じ方向を一括変換して保存」ボタンと日英併記の注記を追加。
- `src/main.js`: 専用のクリックハンドラを追加、ソース内のPDF全てを
  対象に`rebind_pdfs`を呼び、結果(成功/エラー)をファイルごとに
  ログ表示する。

バージョンを0.1.13→0.1.14へ更新。

**次回への引き継ぎ**: (1) 実際の複雑なPDF(入れ子ページツリー・
暗号化・フォーム等)での動作確認(このセッションでは合成テスト用の
最小PDFのみで検証)、(2) 入れ子ページツリー対応の拡張(現状は
明確なエラーで止まるのみ)、(3) 前回同様に残る積み上げ要望
(複数の音声・静止画・動画・PDFの合成編集・等間隔/サイズ指定分割+
余りの自動ディスクフィット、将来的なゲーム機対応)への着手。
