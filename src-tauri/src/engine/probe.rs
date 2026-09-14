//! ffprobeでメディア情報(尺・コーデック等)を取得する。

use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaInfo {
    pub duration_secs: f64,
    pub format_name: String,
    pub bit_rate: Option<u64>,
}

pub fn probe(path: &str) -> Result<MediaInfo, String> {
    let output = resolve_tool("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration,format_name,bit_rate",
            "-of", "json",
            path,
        ])
        .output()
        .map_err(|e| format!("ffprobeの起動に失敗しました(未インストールの可能性): {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("ffprobe出力の解析に失敗しました: {e}"))?;

    let format = &json["format"];
    let duration_secs: f64 = format["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let format_name = format["format_name"].as_str().unwrap_or("unknown").to_string();
    let bit_rate = format["bit_rate"].as_str().and_then(|s| s.parse().ok());

    Ok(MediaInfo { duration_secs, format_name, bit_rate })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// `scripts/fetch-ffmpeg-sidecars.sh`で取得した実バイナリ(ソース側は
    /// `<name>-<target-triple>[.exe]`という`externalBin`規約の名前)を、
    /// 実際にインストール後のアプリで確認した命名規則(bareな`<name>
    /// [.exe]`、`sidecar.rs`のモジュールdoc「実機検証で発見・修正した
    /// 実装ミス」参照)でテスト実行ファイルの隣へ配置し、`probe()`が
    /// 本当にそのバイナリを見つけて実行・正しい結果を返すことを検証する
    /// (2026-09-14追加、sidecar同梱化の実機E2E検証)。`src-tauri/binaries/`
    /// はgit管理外(README.md参照)なので、未取得の環境ではスキップする
    /// (このプロジェクトの既存方針: フェイクな成功にしない、
    /// `convert.rs`の`ffmpeg_available()`と同じ考え方)。
    #[test]
    fn probe_actually_executes_a_real_sidecar_binary_when_one_is_bundled() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let binaries_dir = manifest_dir.join("binaries");
        // `scripts/fetch-ffmpeg-sidecars.sh`が置くファイル名は
        // `ffmpeg-<target-triple>[.exe]`(externalBinのソース側規約)だが、
        // このテストはターゲットトリプルの正確な文字列を知る必要が無いよう
        // 単純にプレフィックス一致で探す。
        let find_by_prefix = |prefix: &str| -> Option<std::path::PathBuf> {
            std::fs::read_dir(&binaries_dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(prefix) && n.ends_with(std::env::consts::EXE_SUFFIX))
            })
        };
        let (Some(ffmpeg_src), Some(ffprobe_src)) = (find_by_prefix("ffmpeg-"), find_by_prefix("ffprobe-")) else {
            eprintln!("src-tauri/binaries/にffmpeg/ffprobeが無いためスキップ / skipping: run scripts/fetch-ffmpeg-sidecars.sh first");
            return;
        };

        let exe = std::env::current_exe().unwrap();
        let dir = exe.parent().unwrap().to_path_buf();
        // 実行時に探すのはbareな名前(sidecar::find_sidecarと同じ規約)。
        let ffprobe_sidecar = dir.join(format!("ffprobe{}", std::env::consts::EXE_SUFFIX));
        let ffmpeg_sidecar = dir.join(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&ffprobe_src, &ffprobe_sidecar).expect("failed to place ffprobe sidecar next to the test binary");
        std::fs::copy(&ffmpeg_src, &ffmpeg_sidecar).expect("failed to place ffmpeg sidecar next to the test binary");

        let cleanup = || {
            let _ = std::fs::remove_file(&ffprobe_sidecar);
            let _ = std::fs::remove_file(&ffmpeg_sidecar);
        };

        // 同梱ffmpeg(=ffprobe_sidecarの隣に置いたffmpeg_sidecar自体では
        // なく、上でコピーしたffmpeg_sidecarパス)でごく短いテスト動画を
        // 生成し、それを同梱ffprobeでprobeする。
        let tmp = std::env::temp_dir().join(format!("make_disk_sidecar_e2e_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let test_file = tmp.join("sidecar_test.mp4");
        let gen_status = Command::new(&ffmpeg_sidecar)
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=64x64:rate=5", "-c:v", "libx264", "-pix_fmt", "yuv420p", test_file.to_str().unwrap()])
            .output();
        if gen_status.is_err() || !gen_status.as_ref().unwrap().status.success() {
            cleanup();
            let _ = std::fs::remove_dir_all(&tmp);
            panic!("failed to generate test fixture with the bundled ffmpeg sidecar: {gen_status:?}");
        }

        let result = probe(test_file.to_str().unwrap());
        cleanup();
        let _ = std::fs::remove_dir_all(&tmp);

        let info = result.expect("probe() should succeed using the bundled ffprobe sidecar");
        assert!((info.duration_secs - 2.0).abs() < 0.5, "expected ~2s duration from the bundled sidecar, got {}", info.duration_secs);
    }
}
