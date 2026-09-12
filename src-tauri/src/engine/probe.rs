//! ffprobeでメディア情報(尺・コーデック等)を取得する。

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize)]
pub struct MediaInfo {
    pub duration_secs: f64,
    pub format_name: String,
    pub bit_rate: Option<u64>,
}

pub fn probe(path: &str) -> Result<MediaInfo, String> {
    let output = Command::new("ffprobe")
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
