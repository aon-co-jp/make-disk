//! FFmpegによるフォーマット変換・ビットレート制御・時間トリミング。

use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitrateMode {
    /// 固定ビットレート(kbps)
    Fixed(u64),
    /// ディスク容量から自動算出した最大平均ビットレート(kbps)
    AutoMaxForCapacity(u64),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrimRange {
    /// 開始位置(秒)。省略時は先頭から。
    pub start_secs: Option<f64>,
    /// 長さ(秒)。省略時は末尾まで。
    pub duration_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertJob {
    pub input_path: String,
    pub output_path: String,
    /// 出力コンテナ/コーデックのffmpeg引数(例: "-c:v libx264 -c:a aac")は
    /// フォーマットプリセット側で組み立てて渡す。
    pub codec_args: Vec<String>,
    pub bitrate: Option<BitrateMode>,
    pub trim: Option<TrimRange>,
}

pub fn run_convert(job: &ConvertJob) -> Result<(), String> {
    let mut args: Vec<String> = Vec::new();

    if let Some(trim) = &job.trim {
        if let Some(start) = trim.start_secs {
            args.push("-ss".into());
            args.push(start.to_string());
        }
        args.push("-i".into());
        args.push(job.input_path.clone());
        if let Some(dur) = trim.duration_secs {
            args.push("-t".into());
            args.push(dur.to_string());
        }
    } else {
        args.push("-i".into());
        args.push(job.input_path.clone());
    }

    args.extend(job.codec_args.clone());

    match &job.bitrate {
        Some(BitrateMode::Fixed(kbps)) | Some(BitrateMode::AutoMaxForCapacity(kbps)) => {
            args.push("-b:v".into());
            args.push(format!("{kbps}k"));
        }
        None => {}
    }

    args.push("-y".into());
    args.push(job.output_path.clone());

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました(未インストールの可能性): {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}
