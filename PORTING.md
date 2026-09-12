# セッション引き継ぎメモ

このファイルは、複数セッションにまたがる作業の到達点・次回再開ポイントを
記録する(`open-raid-z`等の他リポジトリのPORTING.md運用に準じる)。
詳細な技術的発見・方針決定は[`CLAUDE.md`](CLAUDE.md)にあるので、
ここでは「今どこまで進んでいて、次に何をするか」だけを簡潔に記す。

## 🔁 再開用メッセージ(2026-09-12時点)

### 完了したこと

1. Tauri + Rustでのデスクトップ版スケルトン一式(UI・ffmpeg/xorriso
   ラッパー・容量ベース自動ビットレート・4段階品質警告)。
2. `https://easy-web.tokyo/make-disk/`に紹介・ダウンロードページを公開
   (VPSの`open-web-server`にルート追加、`easy-web.tokyo`トップページにも
   紹介カードを追加してWASM再ビルド込みで反映済み)。
3. 姉妹リポジトリ`rs-FFmpeg`・`rs-xorriso`(Rustでのリスペクト版)を新規
   作成。実際のmake-disk呼び出し引数で検証し、見つかった重大な非互換
   (無音で壊れた出力を書く等)を修正済み。両リポジトリとも現時点では
   本家の一部機能のみ対応。
4. Android実機(OnePlus A401OP, Android 15)での動作確認に成功。
   その過程で**全プラットフォームに影響する重大バグ**
   (`main.js`のベアESモジュールインポートが原因で全ボタンが無反応に
   なっていた)を発見・修正した。

### 未完了・次回やること

1. **最優先**: Android向けSAFフォルダ選択の独自Tauriプラグイン実装
   (`CLAUDE.md`の「発見2」に具体的な設計を記載済み・未着手)。
   Kotlin側の`ACTION_OPEN_DOCUMENT_TREE`ハンドリング、Rust側の
   `pick_output_tree`コマンド、`main.js`でのAndroid分岐、の3点セット。
2. `rs-FFmpeg`/`rs-xorriso`のAndroidクロスコンパイル・Rustライブラリ
   直接リンク化の検討(外部プロセスシェルアウトをやめる方向)。
3. このPC(Windows)でのAndroid Developer Mode有効化が反映されない問題
   ([`CLAUDE.md`](CLAUDE.md)のプラットフォーム範囲節に詳細記録)は
   未解決のまま。手動`.so`コピー+`gradlew`直叩きの回避策で当面は
   進められるが、`tauri android dev`によるホットリロード開発はできない
   状態が続いている。
4. 実機でのディスク書き込み検証(CD/DVD/Blu-ray)は光学ドライブが
   無いため未実施。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.1)

### 完了したこと(追加分)

5. v0.1.0をGitHub Actions(全プラットフォームビルド)+GitHub Releaseで
   公開。https://github.com/aon-co-jp/make-disk/releases/tag/v0.1.0
6. 複数区間の動画カット機能(`CutRange`)を実装。最初・途中・最後、
   いくつでも指定可能。抽出は`-c copy`(無劣化・高速)、フォーマット
   変換が必要な場合のみ結合時に1回だけエンコード。単体テスト4件で
   区間計算ロジックを検証済み。
7. フレーム精度カットモード(`frame_accurate`)を追加。GPUハードウェア
   エンコーダ(NVENC/QuickSync/AMF)を自動検出して使用し、無ければ
   CPU(libx264、AVX2/AVX512はlibx264自身が自動活用)にフォールバック。
8. `open-cpu`をCargo依存として統合、CPUフォールバック時の速度目安を
   `estimate_cpu_encode_speed`コマンドでUIに提供。
9. Blu-ray 4層/BDXL(128GB, `DiscType::Bd128`)対応を追加。
10. フロントエンドに動画プレビュー付き区間カットエディタを実装
    (`<video>`要素+`convertFileSrc`、マウスでシーク→「現在位置」ボタン、
    または時:分:秒を直接数字入力、の両対応)。これに伴い
    `tauri.conf.json`の`assetProtocol`を有効化し、`tauri`クレートに
    `protocol-asset`フィーチャを追加した(無いとビルド時エラーになる
    ことを実際に確認済み)。
11. `open-cuda`に`yuv_to_rgb_cpu`(YUV420p→RGB24変換カーネル、CPU/rayon)
    を追加、スカラー参照実装と全画素一致を検証(2ユニットテスト+
    3072画素のend-to-end検証)。動画コーデック自体(H.264等)は
    非現実的なスコープのため対象外と明記。`open-directx`での実GPU
    ディスパッチ(既存の狭いDXBC→SPIR-Vデコーダの拡張が必要)は
    次段階として保留。

### 既知の未検証事項(v0.1.1時点、v0.1.2で一部解消)

- ~~このPCにffmpegが入っていないため~~ → **v0.1.2で解消**。gyan.devの
  公式Windowsビルドをこの開発機にインストールし、実際のffmpegで
  複数区間カット機能を検証した(下記参照)。
- 動画プレビューエディタ(`<video>` + `convertFileSrc`)の実機での
  表示・シーク動作は未検証(ビルド成功とロジックレビューのみ)。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.2)

### 実施したこと

12. この開発機にffmpeg(gyan.dev公式Windowsビルド)をインストールし、
    複数区間カット機能・フレーム精度カットモードの**実際のffmpeg実行を
    伴う統合テストを2件追加**(合成テスト動画を`testsrc`で生成→
    カット適用→`ffprobe`で結果の尺を検証)。
13. **その統合テストで実際のバグを発見・修正した**:
    `detect_hw_video_encoder`は`ffmpeg -encoders`の一覧に載っているか
    (=ffmpegのビルドにそのコーデックがコンパイルされているか)だけを
    見ており、実際にこのマシンのGPUドライバが対応しているかは
    見ていなかった。実機のNVIDIAドライバが古く、`h264_nvenc`は
    リストには出るが実行すると
    `"Driver does not support the required nvenc API version. \
    Required: 13.1 Found: 11.1"`で失敗することを統合テストで実際に
    再現した。修正: 候補ごとに実際に1フレームだけ試しエンコードし、
    本当に成功するものだけを採用するよう変更(`hw_encoder_actually_works`)。
    修正後、10テスト全green。

このv0.1.2の教訓: 「実際に統合テストを書いて実行する」ことでしか
見つからないバグ(実機のドライバ依存の挙動)があった。単体テストと
コードレビューだけでは検出できなかった。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.3予定分)

### 実施したこと

14. `open-cpu`統合を「参考表示のみ」から「実際の分岐・制御」へ強化。
    - `estimate_cpu_encode_speed`をFMA・AVX-512BW/VL等も見た4段階
      (fast/moderate/slow/very_slow)に拡張し、検出できた命令セット
      一覧(`detected_features`)も返すようにした。
    - `recommended_x264_preset()`を新設し、CPUフォールバック時の
      実際のffmpeg`-preset`引数を検出結果に応じて自動選択する
      (非力なCPUには`ultrafast`/`veryfast`、強力なCPUには`slow`)。
      表示だけでなく実際にffmpegのコマンドライン引数へ反映される
      制御になった。
    - `main.js`側: フレーム精度モードのチェックボックスをONにした際、
      速度目安がslow/very_slowなら`window.confirm()`で続行確認を
      挟むようにした。
    - 実際のffmpeg統合テスト(11件)全green(`-preset`引数追加後も
      複数区間カット・フレーム精度カットの結果尺は正しいことを確認)。

### 未完了・次回やること(継続)

- `open-directx`での実GPUディスパッチ(既存の狭いDXBC→SPIR-Vデコーダの
  拡張が必要、次段階として保留中)。
- macOS/Linux版の実機動作確認(GitHub Actionsでのビルド自体は
  v0.1.0〜成功しているが、実機起動確認はWindows版のみ)。
- 動画プレビューエディタの実機での表示・シーク動作の実機確認。
- rusty_h264(ピュアRust H.264実装、調査で発見)は現時点でスター6・
  作成2.5ヶ月と実績が浅く、「ffmpegとビット完全一致」という主張も
  独立検証できていないため、採用は見送り。将来の選択肢として記録のみ。

## 🔁 再開用メッセージ(2026-09-12 続き、v0.1.3、AndroidのSAFプラグイン完了)

### 実施したこと

15. **Android向けSAFフォルダ選択プラグインを実装・実機検証まで完了**。
    `src-tauri/plugins/tauri-plugin-android-folder/`に新規プラグイン作成
    (Rust: `pick_output_tree`コマンド。Kotlin:
    `ACTION_OPEN_DOCUMENT_TREE`+`takePersistableUriPermission`、
    `tauri-plugin-dialog`自身のAndroidソースを正確なAPIリファレンスとして
    参照)。`Cargo.toml`では`target.'cfg(target_os = "android")'`で
    Android限定の依存にし、デスクトップ版のビルド・11テストへの影響
    無しを確認。
    実機(OPPO Reno11 A)で「フォルダを選択」→ネイティブのSAFフォルダ
    ツリーピッカーが開く→フォルダを選んで「このフォルダを使用」→
    アクセス許可ダイアログ→`content://com.android.externalstorage.
    documents/tree/primary%3ADocuments`というURIが実際に「出力先
    フォルダ」欄へ反映される、という一連の流れをエンドツーエンドで
    確認した。
    **残課題**: 返るのはcontent:// URIでありffmpeg/xorrisoにそのまま
    渡せる実ファイルシステムパスではない。実際の変換機能との連携には
    URI→実処理の橋渡しが別途必要(かつffmpeg/xorriso自体がAndroidに
    存在しない問題は解決していない、上記「発見3」参照)。

## 🔁 再開用メッセージ(2026-09-12 続き、open-directx実GPU実行の事前検証)

### 実施したこと(調査のみ、コード変更なし)

16. `open-directx`でyuv_to_rgbカーネルを実GPU(Vulkan)で動かす前段階として、
    リスクを減らすため実装前に3点を検証した。
    - **ツール確認**: `fxc.exe`(HLSLコンパイラ)はこのマシンのWindows SDK
      (`C:\Program Files (x86)\Windows Kits\10\bin\...\x64\fxc.exe`)に
      同梱されており利用可能。
    - **GPU確認**: `open-cuda/examples/vulkan_info`を実行し、このマシンの
      GT 730で実際にVulkan 1.2のcompute queueが使えることを確認済み
      (`api_version: 1.2.175`、`OK: ... compute queue are available`)。
    - **既存デコーダの対応範囲を精査**: `directx-shader-translate`には
      想定より進んだ`translate_chain_shader`(N入力バッファ+逐次2項演算
      〈add/mul/div/sub〉のチェーンに対応、`vector_add_mul_div_sub_...`
      系のテストが多数存在)があった。
    - **重大な制約を発見**: `RegExpr`は`Load(uav)`と`BinOp`のみを表現でき、
      **即値(リテラル定数)オペランドに対応していない**
      (`decode_chain_shape`のソースを直接確認)。YUV→RGB変換の係数
      (1.402, 0.344136, 0.714136, 1.772, オフセット128)は全て定数のため、
      現状のデコーダでは原理的に表現不可能。

### 次にやること(具体的にスコープ確定済み、次回セッション)

- `RegExpr`に`Immediate(f32)`相当のバリアントを追加。
- `decode_chain_shape`で`RegisterType::Immediate32`オペランドを実際の
  fxc.exe出力で確認しながら認識できるよう拡張。
- `emit_chain_spirv`側で対応する`OpConstant`を生成するよう拡張。
- 簡単な定数付きシェーダー(例: `Output[i] = A[i] * 1.402 + 128.0`)を
  実際にfxc.exeでコンパイルし、実GPU(GT730)で動かしてCPU参照実装と
  数値一致することを検証してから、yuv_to_rgb本体の実装に進む。

## 関連リポジトリ

- [aon-co-jp/make-disk](https://github.com/aon-co-jp/make-disk) — 本体
- [aon-co-jp/rs-FFmpeg](https://github.com/aon-co-jp/rs-FFmpeg) — FFmpegのRustリスペクト版
- [aon-co-jp/rs-xorriso](https://github.com/aon-co-jp/rs-xorriso) — xorrisoのRustリスペクト版
- [aon-co-jp/open-cpu](https://github.com/aon-co-jp/open-cpu) — CPU命令セット検出(依存として使用)
- [aon-co-jp/open-cuda](https://github.com/aon-co-jp/open-cuda) — GPU計算抽象化層(`yuv_to_rgb_cpu`を追加)
