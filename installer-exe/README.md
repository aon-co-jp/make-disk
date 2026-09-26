# installer-exe/

`make-disk-installer.exe` の実装(単一自己完結インストーラー、Rust製)。
配布物としてのダウンロード先・使い方は [`../installer/README.md`](../installer/README.md)
を参照。ここは実装側のメモ。

## 何をするか

ダブルクリック → インストール先を選ぶ(既定は
`%LOCALAPPDATA%\Programs\make-disk`) → 「open-barも同時にインストールする」の
チェック → 「インストール」ボタン、の数クリックだけで完了する。
`pip install`/`npm install`のようなコマンド操作は一切要求しない。

同梱(すべて`build.rs`が`payload.zip`としてまとめ、`include_bytes!`で
埋め込む。実行時の追加ダウンロードは無い):

- make-disk本体(`src-tauri/target/release/make-disk.exe`をそのまま)
- 本家ffmpeg/ffprobe(`scripts/fetch-ffmpeg-sidecars.sh`が用意したもの)
- 実験的な純Rust版 rs-ffmpeg/rs-xorriso(`scripts/build-rs-tribute-sidecars.sh`)

実行時に取得するもの(埋め込むと肥大化するため):

- WebView2ランタイム: 既にインストール済みならレジストリ確認だけで済ませ、
  無ければ既存のNSISインストーラー(`../installer/installer.nsi`)と同じ
  固定URLから小さなブートストラッパーだけ取得して`/silent /install`する。
- open-bar: 「同時にインストールする」にチェックが入っている時だけ、
  `aon-co-jp/open-bar`のGitHub Releasesから最新のWindows用インストーラー
  (`*_x64-setup.exe`)を取得し`/S`でサイレントインストールする。

アンインストールは自分自身をインストール先へコピーしておき(元の配布exeが
消されても動くように)、「プログラムと機能」からそのコピーを
`--uninstall --dir <インストール先>`付きで呼ぶ形でレジストリ登録する。

## 既知の欠落(正直な開示)

**本家xorriso(GPL)は未同梱。** Windows向けの単純な静的exeが無く
(Cygwin/MSYS依存が強い)、今回のビルドでは調達できていない。展開先へ
`KNOWN_GAPS.txt`として明記され、完了画面でも案内される。ISO書き込み機能を
使うには利用者が別途xorrisoを導入しPATHへ通す必要がある(`engine/sidecar.rs`
が既存の仕組み通りPATH上のxorrisoへフォールバックする)。

## ビルドする

前提: リポジトリルートで`npm run tauri build`(または最低限
`cargo build --release`をsrc-tauri/で)済みで`src-tauri/target/release/make-disk.exe`が
存在すること。`scripts/fetch-ffmpeg-sidecars.sh`と
`scripts/build-rs-tribute-sidecars.sh`も実行し`src-tauri/binaries/`が
揃っていること(rs-ffmpeg/rs-xorrisoは無くてもビルドは通る、警告のみ)。

```bash
cd installer-exe
cargo build --release
# 出力: target/release/make-disk-installer.exe
```

## 動作検証

GUIを経由しない動作検証用に、非公開のフラグを用意している(通常の配布物には
影響しない、開発時の自動テスト用):

```bash
# 展開・WebView2確認・レジストリ登録までを自動実行(open-barは取得しない)
make-disk-installer.exe --test-install <検証用フォルダ>
# --with-open-bar を付けるとopen-barの取得・サイレントインストールも試す

# アンインストール(インストール先へコピーされた方を呼ぶのが正しい使い方)
<インストール先>\make-disk-installer.exe --uninstall --dir <インストール先>
```

2026-09-26に`--test-install`で実機検証済み: 展開(make-disk.exe/ffmpeg.exe/
ffprobe.exe/rs-ffmpeg.exe/rs-xorriso.exe/KNOWN_GAPS.txt)・レジストリ登録・
アンインストール(レジストリ削除+フォルダ削除)まで確認。GUI経由の
クリック操作自体(参照ダイアログ・チェックボックス・完了ダイアログ)と、
open-bar同時インストールの経路は自動操作ツールが無く未検証(次回、実機で
手動クリックして確認する必要がある)。
