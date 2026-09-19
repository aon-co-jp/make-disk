//! 外部ツール(ffmpeg/ffprobe/xorriso)の実行ファイル解決(2026-09-14新設)。
//!
//! ## 経緯・正直な開示
//!
//! これまで`Command::new("ffmpeg")`のようにPATH頼みで外部ツールを起動して
//! おり、ユーザーが別途インストールする必要があった。ユーザー指示
//! 「必要なリポジトリを同梱してインストーラー付きアプリとして完成させて」
//! への対応として、Tauriの[sidecar機構](https://tauri.app/develop/sidecar/)
//! (`bundle.externalBin`)で実行ファイルをインストーラーへ同梱できるように
//! する。
//!
//! Tauriの公式な使い方は`tauri_plugin_shell::ShellExt::sidecar`
//! (`AppHandle`経由、非同期API)だが、このアプリの`engine/*.rs`は同期関数
//! として設計されており(`#[tauri::command]`側も同期)、全体を非同期化して
//! `AppHandle`を全呼び出し経路に配線するのは影響範囲が広い大規模な変更に
//! なる。そこで、Tauriのsidecar機構が実際にバイナリを配置する場所を
//! `std::env::current_exe()`から自前で解決する、より軽量な実装にした。
//!
//! **2026-09-14: 実機検証で発見・修正した実装ミス**——当初は
//! `<name>-<target-triple>[.exe]`という、`externalBin`に置くソース側の
//! ファイル名規則がそのままインストール後も使われると誤って想定して
//! 実装していた。実際に`npm run tauri build`でMSI/NSISインストーラーを
//! 生成し、NSISインストーラーをサイレントインストール(`/S /D=...`)して
//! インストール先ディレクトリの中身を確認したところ、Tauriのバンドラーは
//! **ターゲットトリプル部分を落として`ffmpeg.exe`/`ffprobe.exe`という
//! bareな名前**で実行ファイルの隣に配置することが判明した(ビルド対象は
//! 常に単一ターゲットなので曖昧さが無く、サフィックスを付ける理由が
//! そもそも無い)。当初の実装のままだと同梱バイナリを一切見つけられず、
//! 常にPATHへフォールバックするだけの無意味な変更になっていた——
//! この節はその教訓の記録であり、今は実際にインストールした実行ファイルで
//! 検証した通りの規約(bareな名前)に修正済み。
//!
//! 同梱バイナリが見つからない場合(開発時の`cargo test`/`npm run tauri dev`、
//! またはこのセッションではまだ同梱していないmacOS/xorriso)は、従来通り
//! PATH上の`name`をそのまま呼ぶフォールバックになる——既存の動作・
//! テストは一切壊さない設計。

use std::path::PathBuf;
use std::process::Command;

/// `name`(例: `"ffmpeg"`)を実行するための`Command`を返す。
/// 同梱sidecarバイナリ(`<name>[.exe]`、実行ファイルと同じディレクトリ、
/// 実際にインストール後のファイルを確認して検証済みの命名規則)が
/// 見つかればそれを、無ければPATH上の`name`を使う。
pub fn resolve_tool(name: &str) -> Command {
    // rs-ffmpeg/rs-xorrisoはバージョン管理付きプラグインフォルダを最優先で探す(engine::plugins)。
    if let Some(plugin) = crate::engine::plugins::installed_plugin_path(name) {
        return Command::new(plugin);
    }
    if let Some(sidecar) = find_sidecar(name) {
        return Command::new(sidecar);
    }
    Command::new(name)
}

fn find_sidecar(name: &str) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let filename = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let candidate = dir.join(filename);
    candidate.is_file().then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_sidecar_returns_none_when_no_bundled_binary_exists() {
        // このテスト実行バイナリの隣に`nonexistent-tool`は存在しない
        // はずなので、Noneが返り`resolve_tool`はPATHへフォールバックする
        // (既存の`Command::new("...")`と同じ挙動)。
        assert!(find_sidecar("make-disk-test-nonexistent-tool-xyz").is_none());
    }

    #[test]
    fn resolve_tool_falls_back_to_bare_name_when_no_sidecar_present() {
        let cmd = resolve_tool("make-disk-test-nonexistent-tool-xyz");
        // `Command`はプログラム名を直接読み出すAPIが無いため、
        // Debug表示に含まれることで間接的に確認する。
        assert!(format!("{cmd:?}").contains("make-disk-test-nonexistent-tool-xyz"));
    }

    /// `find_sidecar`の「見つかる」経路を実際にファイルを配置して検証する
    /// (上の2件は「見つからない」経路のみだった)。テスト実行バイナリ
    /// (`current_exe()`)と同じディレクトリへ、実際にインストールした
    /// アプリで確認した命名規則(bareな名前)通りのダミーファイルを
    /// 実際に作成・削除する——実ファイルシステムを使った実機に近い検証
    /// (モックに頼らない、このプロジェクト全体の検証方針に合わせる)。
    #[test]
    fn find_sidecar_detects_a_bundled_binary_placed_next_to_the_executable() {
        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap();
        let name = "make-disk-test-fake-sidecar-tool";
        let sidecar_path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));

        std::fs::write(&sidecar_path, b"not a real binary, just needs to exist for is_file()").unwrap();
        let found = find_sidecar(name);
        let cmd = resolve_tool(name); // ファイルがまだ存在するうちに呼ぶこと(削除後だとフォールバックしてしまう)
        let _ = std::fs::remove_file(&sidecar_path); // 掃除は最後に必ず行う

        assert_eq!(found.as_deref(), Some(sidecar_path.as_path()), "find_sidecar should locate the file placed next to the current executable");
        // 見つかった場合はbareな名前ではなく、フルパスが使われているはず。
        assert!(!format!("{cmd:?}").contains(&format!("\"{name}\"")), "resolve_tool should use the full sidecar path, not the bare name, once a sidecar is found");
    }
}
