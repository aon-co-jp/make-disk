//! FFmpegによるフォーマット変換・ビットレート制御・時間トリミング。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
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

/// 「カットしたい範囲」(1本の動画に対して複数指定可能)。
/// 例: 「最初の0:00〜2:00をカット」「途中の1:00:00〜1:05:00をカット」
/// 「最後の4:50:00〜末尾をカット」の3つを同時に指定できる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CutRange {
    pub start_secs: f64,
    /// 省略時はファイル末尾までカットする(「最後の◯◯から末尾まで」用)。
    pub end_secs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertJob {
    pub input_path: String,
    pub output_path: String,
    /// 出力コンテナ/コーデックのffmpeg引数(例: "-c:v libx264 -c:a aac")は
    /// フォーマットプリセット側で組み立てて渡す。
    pub codec_args: Vec<String>,
    pub bitrate: Option<BitrateMode>,
    /// 単一区間の簡易トリミング(開始+長さ)。`cut_ranges`指定時は無視される。
    pub trim: Option<TrimRange>,
    /// 複数区間のカット指定。指定時は`trim`より優先する。
    ///
    /// 実装方針(数時間規模の動画でも実用的な速度で処理するため):
    /// 1. 既定(`frame_accurate: false`)では、残す各区間を`-c copy`
    ///    (ストリームコピー、エンコード無し)で高速に抜き出す
    ///    (キーフレーム単位の精度になる点はffmpegの一般的な制約)。
    /// 2. `frame_accurate: true`の場合は、区間の境界をフレーム単位で
    ///    正確に切るため抽出時にエンコードが必要になるが、CPUソフト
    ///    エンコード(libx264等)ではなく、GPU(NVENC/QuickSync/AMF)の
    ///    ハードウェアエンコーダが検出できればそちらを優先して使う
    ///    ことで、5時間級の動画でも実用的な速度で処理する
    ///    (`hw_encoder`参照)。
    /// 3. 抜き出した区間をconcat demuxerで結合する際に、フォーマット
    ///    変換・ビットレート指定が必要な場合のみ1回だけエンコードする
    ///    (区間ごとに毎回エンコードし直さない)。
    pub cut_ranges: Option<Vec<CutRange>>,
    /// trueならカット境界をフレーム単位で正確に切る(要エンコード、
    /// ただしGPUハードウェアエンコーダを自動検出して使用)。
    /// falseなら`-c copy`でキーフレーム単位の高速カットを行う。
    #[serde(default)]
    pub frame_accurate: bool,
}

/// カットしたい範囲の一覧から、残す(keepする)区間の一覧を求める。
/// 戻り値の各要素は(開始秒, 終了秒。Noneならファイル末尾まで)。
fn keep_segments_from_cuts(cuts: &[CutRange]) -> Vec<(f64, Option<f64>)> {
    let mut sorted: Vec<&CutRange> = cuts.iter().collect();
    sorted.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap_or(std::cmp::Ordering::Equal));

    let mut keep = Vec::new();
    let mut prev_end: f64 = 0.0;
    let mut reached_eof_cut = false;

    for cut in sorted {
        if reached_eof_cut {
            break; // 既に末尾までカット済みなので、それ以降のカット指定は無視
        }
        if cut.start_secs > prev_end {
            keep.push((prev_end, Some(cut.start_secs)));
        }
        match cut.end_secs {
            Some(e) => prev_end = prev_end.max(e),
            None => reached_eof_cut = true,
        }
    }
    if !reached_eof_cut {
        keep.push((prev_end, None));
    }
    keep
}

pub fn run_convert(job: &ConvertJob) -> Result<(), String> {
    if let Some(cuts) = &job.cut_ranges {
        if !cuts.is_empty() {
            return run_convert_with_cut_ranges(job, cuts);
        }
    }
    run_convert_simple(job)
}

fn run_convert_simple(job: &ConvertJob) -> Result<(), String> {
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
    push_bitrate_args(&mut args, &job.bitrate);

    args.push("-y".into());
    args.push(job.output_path.clone());

    run_ffmpeg(&args)
}

fn run_convert_with_cut_ranges(job: &ConvertJob, cuts: &[CutRange]) -> Result<(), String> {
    let keep = keep_segments_from_cuts(cuts);
    if keep.is_empty() {
        return Err(
            "指定したカット範囲により、残る区間がありません / no footage remains after applying the given cut ranges".to_string(),
        );
    }

    let output_path = Path::new(&job.output_path);
    let ext = output_path.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let tmp_dir = output_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".make-disk-tmp-{}", std::process::id()));
    fs::create_dir_all(&tmp_dir).map_err(|e| format!("一時フォルダの作成に失敗しました: {e}"))?;

    let cleanup = |tmp_dir: &Path| {
        let _ = fs::remove_dir_all(tmp_dir);
    };

    let hw_encoder = if job.frame_accurate { detect_hw_video_encoder() } else { None };

    let mut segment_paths: Vec<std::path::PathBuf> = Vec::new();
    for (i, (start, end)) in keep.iter().enumerate() {
        let seg_path = tmp_dir.join(format!("segment_{i}.{ext}"));
        let mut args: Vec<String> = vec!["-ss".into(), start.to_string()];
        if let Some(e) = end {
            args.push("-to".into());
            args.push(e.to_string());
        }
        args.push("-i".into());
        args.push(job.input_path.clone());

        if job.frame_accurate {
            // カット境界をフレーム単位で正確に切るにはエンコードが要る
            // (`-c copy`はキーフレーム単位の精度しか出せない)。
            // CPUソフトエンコード(libx264等)は5時間級の動画では非現実的な
            // 時間がかかるため、GPUハードウェアエンコーダが検出できれば
            // それを優先して使う(NVENC/QuickSync/AMFはCPUソフトエンコード
            // に比べて大幅に高速)。
            args.push("-c:v".into());
            args.push(hw_encoder.unwrap_or("libx264").to_string());
            args.push("-c:a".into());
            args.push("aac".into());
        } else {
            args.push("-c".into());
            args.push("copy".into());
        }
        args.push("-y".into());
        args.push(seg_path.to_string_lossy().to_string());

        if let Err(e) = run_ffmpeg(&args) {
            cleanup(&tmp_dir);
            return Err(format!("区間抽出に失敗しました({start}s〜{end:?}s): {e}"));
        }
        segment_paths.push(seg_path);
    }

    let list_path = tmp_dir.join("concat_list.txt");
    let list_content: String = segment_paths
        .iter()
        .map(|p| format!("file '{}'\n", p.to_string_lossy().replace('\'', "'\\''")))
        .collect();
    if let Err(e) = fs::write(&list_path, list_content) {
        cleanup(&tmp_dir);
        return Err(format!("結合リストの作成に失敗しました: {e}"));
    }

    let mut concat_args: Vec<String> = vec![
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.to_string_lossy().to_string(),
    ];

    // フォーマット変換・ビットレート指定が無ければ、結合も-c copyで
    // 完全に再エンコード無しにする(最速・無劣化)。
    if job.codec_args.is_empty() && job.bitrate.is_none() {
        concat_args.push("-c".into());
        concat_args.push("copy".into());
    } else {
        concat_args.extend(job.codec_args.clone());
        push_bitrate_args(&mut concat_args, &job.bitrate);
    }

    concat_args.push("-y".into());
    concat_args.push(job.output_path.clone());

    let result = run_ffmpeg(&concat_args);
    cleanup(&tmp_dir);
    result
}

/// このffmpegビルドが対応しているGPUハードウェアエンコーダを検出する。
/// NVIDIA(NVENC)→Intel(QuickSync)→AMD(AMF)の順で優先し、
/// どれも無ければNoneを返す(呼び出し側はlibx264にフォールバックする)。
///
/// 「open-cuda」「open-directx」等の自前GPU抽象化層は動画コーデックの
/// 実装を持たないため、ここでは既存のffmpegビルドが持つハードウェア
/// エンコーダをそのまま活用する方針にしている(実際に動作し、CPUソフト
/// エンコードより大幅に高速な、現実的な解決策のため)。
fn detect_hw_video_encoder() -> Option<&'static str> {
    let output = Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);

    for candidate in ["h264_nvenc", "h264_qsv", "h264_amf"] {
        if text.contains(candidate) {
            return Some(candidate);
        }
    }
    None
}

fn push_bitrate_args(args: &mut Vec<String>, bitrate: &Option<BitrateMode>) {
    match bitrate {
        Some(BitrateMode::Fixed(kbps)) | Some(BitrateMode::AutoMaxForCapacity(kbps)) => {
            args.push("-b:v".into());
            args.push(format!("{kbps}k"));
        }
        None => {}
    }
}

fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました(未インストールの可能性): {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cuts_keeps_whole_file() {
        let keep = keep_segments_from_cuts(&[]);
        assert_eq!(keep, vec![(0.0, None)]);
    }

    #[test]
    fn single_middle_cut_produces_two_keep_segments() {
        let cuts = vec![CutRange { start_secs: 100.0, end_secs: Some(200.0) }];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(0.0, Some(100.0)), (200.0, None)]);
    }

    #[test]
    fn start_middle_and_end_cuts_leave_two_middle_segments() {
        // 「最初の0〜120をカット」「途中の600〜660をカット」
        // 「最後の1000〜末尾をカット」の3つを同時指定するケース。
        let cuts = vec![
            CutRange { start_secs: 0.0, end_secs: Some(120.0) },
            CutRange { start_secs: 600.0, end_secs: Some(660.0) },
            CutRange { start_secs: 1000.0, end_secs: None },
        ];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(120.0, Some(600.0)), (660.0, Some(1000.0))]);
    }

    #[test]
    fn overlapping_cuts_are_merged() {
        let cuts = vec![
            CutRange { start_secs: 0.0, end_secs: Some(300.0) },
            CutRange { start_secs: 200.0, end_secs: Some(400.0) },
        ];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(400.0, None)]);
    }

    #[test]
    fn cuts_after_an_eof_cut_are_ignored() {
        let cuts = vec![
            CutRange { start_secs: 500.0, end_secs: None },
            CutRange { start_secs: 100.0, end_secs: Some(200.0) },
        ];
        let keep = keep_segments_from_cuts(&cuts);
        assert_eq!(keep, vec![(0.0, Some(100.0)), (200.0, Some(500.0))]);
    }
}
