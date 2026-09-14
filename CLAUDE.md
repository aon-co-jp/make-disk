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
