//! FFmpegによるフォーマット変換・ビットレート制御・時間トリミング。

use crate::engine::cpu;
use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BitrateMode {
    /// 固定ビットレート(kbps)
    Fixed(u64),
    /// ディスク容量から自動算出した最大平均ビットレート(kbps)
    AutoMaxForCapacity(u64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// 出力解像度(2026-09-17新設)。ユーザー指示「DVDは通常解像度
    /// 720×480とフルHDを選択可能に、ブルーレイはフルHDと4Kを選べる
    /// ようにして、ビデオ出力は、720X480から4Kや5Kや8Kも指定可能に」
    /// への対応。`None`なら元の解像度のまま(無変換)。
    #[serde(default)]
    pub resolution: Option<Resolution>,
    /// 出力フレームレート(fps、2026-09-17新設)。ユーザー指示
    /// 「FPSは、最低不明、24FPS、30FPS 60FPS 120FPなども選択や指定可能に」
    /// への対応。`None`(「不明」=元のフレームレートのまま)なら無変換。
    #[serde(default)]
    pub fps: Option<u32>,
    /// AIノイズ除去(2026-09-19新設)。本物のニューラルネット(RNNoise、ffmpegの
    /// `arnndn`フィルタ)で音声のノイズを低減する。`None`なら無効。
    #[serde(default)]
    pub ai_denoise: Option<AiDenoise>,
}

/// AIノイズ除去の設定。`mix`は原音とのブレンド(-1.0〜1.0、1.0で完全適用、
/// 負値は除去した「ノイズ成分」側)。RNNoiseは主に音声で学習されたモデルで、
/// 音楽では効果が控えめ・高域が鈍る場合があるため、既定は弱め。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiDenoise {
    pub mix: f32,
}

/// 同梱のRNNoise学習済みモデル(GregorR/rnnoise-models の marathon-prescription、
/// 作者が「著作権の対象外」と明記)。バイナリへ埋め込み、使用時に一時ファイルへ書き出す
/// (ffmpegの`arnndn`はファイルパスを要求するため。Tauriのリソース配置に依存せず
/// 全OSで同じ動作にできる)。
static RNNOISE_MODEL: &[u8] = include_bytes!("../../models/rnnoise-general.rnnn");

fn rnnoise_model_path() -> Result<std::path::PathBuf, String> {
    let path = std::env::temp_dir().join(format!("make-disk-rnnoise-general-{}.rnnn", RNNOISE_MODEL.len()));
    if fs::metadata(&path).map(|m| m.len() as usize).ok() != Some(RNNOISE_MODEL.len()) {
        fs::write(&path, RNNOISE_MODEL).map_err(|e| format!("AIモデルの展開に失敗しました: {e}"))?;
    }
    Ok(path)
}

/// `-af arnndn=...`を追加する。フィルタ文字列内のパスでは`:`をバックスラッシュで
/// エスケープする必要があり、`\`はパス区切りと紛らわしいため`/`へ置き換える。
fn push_ai_denoise_args(args: &mut Vec<String>, denoise: &Option<AiDenoise>) -> Result<(), String> {
    if let Some(d) = denoise {
        let p = rnnoise_model_path()?.to_string_lossy().replace('\\', "/").replace(':', "\\\\:");
        args.push("-af".into());
        args.push(format!("arnndn=m={p}:mix={}", d.mix.clamp(-1.0, 1.0)));
    }
    Ok(())
}

/// 出力動画の解像度(幅×高さ、ピクセル)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
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

/// 使えるAV1エンコーダを選ぶ(2026-09-19新設)。高速なlibsvtav1が使えればそれを、
/// 無ければlibaom-av1を、どちらも無ければNone。ffmpegのビルドごとに含まれる
/// エンコーダが違うため、実際に`-encoders`の一覧を見て判断する。
fn detect_av1_encoder() -> Option<(&'static str, Vec<&'static str>)> {
    static CACHE: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
    let name = CACHE.get_or_init(|| {
        let out = resolve_tool("ffmpeg").args(["-hide_banner", "-encoders"]).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        ["libsvtav1", "libaom-av1"].into_iter().find(|e| text.contains(e))
    });
    name.map(|n| (n, if n == "libaom-av1" { vec!["-cpu-used", "6", "-row-mt", "1"] } else { vec!["-preset", "8"] }))
}

/// codec_args内の疑似コーデック`-c:v av1`を、実際に使えるAV1エンコーダへ置き換える。
fn resolve_av1_codec_args(codec_args: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < codec_args.len() {
        if codec_args[i] == "-c:v" && codec_args.get(i + 1).map(String::as_str) == Some("av1") {
            let (enc, extra) = detect_av1_encoder().ok_or("このffmpegにはAV1エンコーダ(libsvtav1/libaom-av1)がありません / this ffmpeg build has no AV1 encoder")?;
            out.push("-c:v".to_string());
            out.push(enc.to_string());
            out.extend(extra.into_iter().map(String::from));
            i += 2;
        } else {
            out.push(codec_args[i].clone());
            i += 1;
        }
    }
    Ok(out)
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

    if is_audio_only_output(&job.output_path) {
        args.push("-vn".into()); // 動画入力から音声だけを取り出す場合に映像ストリームを含めない
    }
    args.extend(resolve_av1_codec_args(&job.codec_args)?);
    if codec_args_are_stream_copy(&job.codec_args) {
        // 無変換コピー(Dolby Vision/Atmos等の保持)では、再エンコード系の指定
        // (ビットレート・拡縮・fps)は矛盾するので付けない。
    } else {
        push_bitrate_args(&mut args, &job.bitrate, &job.output_path);
        push_resolution_args(&mut args, &job.resolution);
        push_fps_args(&mut args, &job.fps);
        push_ai_denoise_args(&mut args, &job.ai_denoise)?;
    }

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
            if hw_encoder.is_none() {
                // GPUが無くCPU(libx264)にフォールバックする場合のみ、
                // open-cpuの検出結果(AVX2/AVX-512の有無)で-presetを
                // 自動選択する。x264自身はAVX2/AVX-512の使用可否を実行時に
                // 自動判定するが、「どれだけ探索を頑張るか」を決める
                // presetはこちらで明示的に選ぶ必要がある——非力なCPUには
                // 軽いpreset、強力なCPUにはより圧縮効率の良いpresetを
                // 割り当てることで、open-cpuの検出結果を実際にffmpegの
                // 挙動制御へ反映する。
                args.push("-preset".into());
                args.push(cpu::recommended_x264_preset().to_string());
            }
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

    // フォーマット変換・ビットレート・解像度・フレームレートいずれの
    // 指定も無ければ、結合も-c copyで完全に再エンコード無しにする
    // (最速・無劣化)。
    if job.codec_args.is_empty() && job.bitrate.is_none() && job.resolution.is_none() && job.fps.is_none() && job.ai_denoise.is_none() {
        concat_args.push("-c".into());
        concat_args.push("copy".into());
    } else {
        match resolve_av1_codec_args(&job.codec_args) {
            Ok(a) => concat_args.extend(a),
            Err(e) => {
                cleanup(&tmp_dir);
                return Err(e);
            }
        }
        push_bitrate_args(&mut concat_args, &job.bitrate, &job.output_path);
        push_resolution_args(&mut concat_args, &job.resolution);
        push_fps_args(&mut concat_args, &job.fps);
        if let Err(e) = push_ai_denoise_args(&mut concat_args, &job.ai_denoise) {
            cleanup(&tmp_dir);
            return Err(e);
        }
    }

    concat_args.push("-y".into());
    concat_args.push(job.output_path.clone());

    let result = run_ffmpeg(&concat_args);
    cleanup(&tmp_dir);
    result
}

/// このマシンで実際に使えるGPUハードウェアエンコーダを検出する。
/// NVIDIA(NVENC)→Intel(QuickSync)→AMD(AMF)の順で優先し、
/// どれも無ければNoneを返す(呼び出し側はlibx264にフォールバックする)。
///
/// 「open-cuda」「open-directx」等の自前GPU抽象化層は動画コーデックの
/// 実装を持たないため、ここでは既存のffmpegビルドが持つハードウェア
/// エンコーダをそのまま活用する方針にしている(実際に動作し、CPUソフト
/// エンコードより大幅に高速な、現実的な解決策のため)。
///
/// 重要: `ffmpeg -encoders`の一覧に載っているかどうかだけでは不十分
/// (実機検証で発見した実バグ)。それはそのエンコーダが「ffmpegの
/// ビルドにコンパイルされているか」を示すだけで、実際にこのマシンの
/// GPUドライバが対応しているかは別問題——古いNVIDIAドライバでは
/// 「h264_nvencはリストに出るが実行すると
/// "Driver does not support the required nvenc API version" で失敗する」
/// ことを実際に確認した。そのため、候補ごとに実際に1フレームだけ
/// 試しエンコードしてみて、本当に成功するものだけを採用する。
fn detect_hw_video_encoder() -> Option<&'static str> {
    for candidate in ["h264_nvenc", "h264_qsv", "h264_amf"] {
        if hw_encoder_actually_works(candidate) {
            return Some(candidate);
        }
    }
    None
}

fn hw_encoder_actually_works(encoder: &str) -> bool {
    resolve_tool("ffmpeg")
        .args([
            "-f", "lavfi", "-i", "color=black:size=64x64:rate=1",
            "-frames:v", "1", "-c:v", encoder, "-f", "null", "-",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// `-c copy`(全ストリーム無変換コピー)指定か。Dolby Vision/Atmos等を壊さず保持するモード。
fn codec_args_are_stream_copy(codec_args: &[String]) -> bool {
    codec_args.windows(2).any(|w| w[0] == "-c" && w[1] == "copy")
}

/// 出力が音声専用のコンテナ/拡張子か。
fn is_audio_only_output(output_path: &str) -> bool {
    let ext = std::path::Path::new(output_path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "mp3" | "wav" | "flac" | "aac" | "m4a" | "ogg" | "opus" | "ac3" | "eac3" | "mka")
}

/// ビットレート指定を追加する。音声専用出力には`-b:a`、動画には`-b:v`を使う
/// (以前は常に`-b:v`で、音声のみの出力ではビットレート指定が無視されていた実バグ)。
fn push_bitrate_args(args: &mut Vec<String>, bitrate: &Option<BitrateMode>, output_path: &str) {
    match bitrate {
        Some(BitrateMode::Fixed(kbps)) | Some(BitrateMode::AutoMaxForCapacity(kbps)) => {
            args.push(if is_audio_only_output(output_path) { "-b:a" } else { "-b:v" }.into());
            args.push(format!("{kbps}k"));
        }
        None => {}
    }
}

/// 出力解像度の指定があれば`-vf scale=W:H`を追加する(2026-09-17新設)。
fn push_resolution_args(args: &mut Vec<String>, resolution: &Option<Resolution>) {
    if let Some(r) = resolution {
        args.push("-vf".into());
        args.push(format!("scale={}:{}", r.width, r.height));
    }
}

/// フレームレート指定があれば`-r <fps>`を追加する(2026-09-17新設)。
/// 「不明」(未指定)の場合は元のフレームレートのまま変換しない。
fn push_fps_args(args: &mut Vec<String>, fps: &Option<u32>) {
    if let Some(f) = fps {
        args.push("-r".into());
        args.push(f.to_string());
    }
}

/// 「AI判断で自動カット」モード(2026-09-16新設)で使う、無音区間の
/// 自動検出結果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilenceRange {
    pub start_secs: f64,
    pub end_secs: f64,
}

/// ffmpegの`silencedetect`フィルタで無音区間を検出し、[`CutRange`]として
/// 使える形で返す(2026-09-16新設)。
///
/// **正直な開示(誇張しない)**: これはユーザー指示にある「AI判断で
/// 自動カット」の実装だが、実際にはLLM/画像認識モデルによる意味的な
/// 判断ではなく、ffmpeg内蔵の**音量ベースの無音検出**という
/// ヒューリスティックである。無音部分(既定: -30dB未満が0.5秒以上
/// 続く区間)を「重要度が低く、削っても画質・音質への影響が少ない
/// 部分」とみなして自動的にカット候補にする、という単純だが実用的な
/// 近似——本当の意味でのシーン重要度判定(退屈な場面の検出等)は
/// 行っていない。
pub fn detect_silence_ranges(path: &str, silence_threshold_db: f64, min_silence_secs: f64) -> Result<Vec<SilenceRange>, String> {
    let output = resolve_tool("ffmpeg")
        .args([
            "-i",
            path,
            "-af",
            &format!("silencedetect=noise={silence_threshold_db}dB:d={min_silence_secs}"),
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました(未インストールの可能性): {e}"))?;

    // silencedetectはstderrへログを出す(ffmpegの一般的な挙動、
    // exit codeは正常終了する)。
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut ranges = Vec::new();
    let mut pending_start: Option<f64> = None;
    for line in stderr.lines() {
        if let Some(idx) = line.find("silence_start: ") {
            let rest = &line[idx + "silence_start: ".len()..];
            if let Some(v) = rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok()) {
                pending_start = Some(v);
            }
        } else if let Some(idx) = line.find("silence_end: ") {
            let rest = &line[idx + "silence_end: ".len()..];
            if let (Some(start), Some(end)) = (pending_start.take(), rest.split_whitespace().next().and_then(|s| s.parse::<f64>().ok())) {
                ranges.push(SilenceRange { start_secs: start, end_secs: end });
            }
        }
    }
    Ok(ranges)
}

/// 「サイズ指定」モード(2026-09-16新設)向け: 目標ファイルサイズ
/// (バイト)と総尺(秒)から、そのサイズに収まる平均ビットレート(kbps)を
/// 算出する。ディスク容量ではなく任意の目標サイズを指定できる点が
/// `capacity::max_bitrate_for_capacity`(ディスク種別限定)との違い。
pub fn bitrate_for_target_size_kbps(target_bytes: u64, total_duration_secs: f64) -> u64 {
    if total_duration_secs <= 0.0 {
        return 0;
    }
    ((target_bytes as f64 * 8.0) / total_duration_secs / 1000.0) as u64
}

/// 「等間隔分割」(2026-09-16新設): `total_secs`を`segment_count`個の
/// 等しい長さの区間に分割する。各区間は既存の`TrimRange`(開始+長さ)
/// として返すため、呼び出し側は`convert_media`を区間ごとに呼ぶだけで
/// 分割出力できる(新しい抽出処理を実装する必要が無い)。
pub fn equal_interval_segments(total_secs: f64, segment_count: u32) -> Vec<TrimRange> {
    if segment_count == 0 || total_secs <= 0.0 {
        return Vec::new();
    }
    let segment_len = total_secs / segment_count as f64;
    (0..segment_count)
        .map(|i| TrimRange {
            start_secs: Some(segment_len * i as f64),
            duration_secs: Some(segment_len),
        })
        .collect()
}

/// 「サイズ指定分割」(2026-09-16新設): `segment_secs`ごとに区切る。
/// 割り切れない最後の区間は、その分だけ短い「あまり」として返す
/// (呼び出し側で、この最後の区間だけディスク容量いっぱいに
/// ビットレートを自動調整することを想定——ユーザー指示「あまりは、
/// DISKいっぱいにビットレートを自動変更して自動編集して」への対応)。
pub fn fixed_length_segments(total_secs: f64, segment_secs: f64) -> Vec<TrimRange> {
    if segment_secs <= 0.0 || total_secs <= 0.0 {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let mut start = 0.0;
    while start < total_secs {
        let remaining = total_secs - start;
        let len = remaining.min(segment_secs);
        segments.push(TrimRange { start_secs: Some(start), duration_secs: Some(len) });
        start += segment_secs;
    }
    segments
}

/// 複数の音声/動画ファイルを結合(合成)する(2026-09-16新設)。
/// ユーザー指示「複数の音声・静止画・動画・PDFの合成編集」のうち、
/// 静止画・PDFの合成は別機能(PDF見開き対応)で扱うため、ここでは
/// 音声/動画同士の結合を担う。ffmpegの`concat`フィルタを使うため、
/// 入力同士のコーデック・解像度が揃っていなくても(再エンコードで)
/// 結合できる。`has_video`は呼び出し側(フロントエンド)が入力の
/// 拡張子から判定して渡す(全入力が動画か、全て音声かのどちらかを
/// 前提とする——動画と音声の混在結合は現時点で未対応)。
pub fn concat_media(input_paths: &[String], output_path: &str, has_video: bool) -> Result<(), String> {
    if input_paths.len() < 2 {
        return Err("結合には2つ以上のファイルが必要です / concatenation needs at least 2 files".to_string());
    }

    let mut cmd = resolve_tool("ffmpeg");
    for p in input_paths {
        cmd.args(["-i", p]);
    }

    let n = input_paths.len();
    let mut filter = String::new();
    for i in 0..n {
        if has_video {
            filter.push_str(&format!("[{i}:v][{i}:a]"));
        } else {
            filter.push_str(&format!("[{i}:a]"));
        }
    }
    if has_video {
        filter.push_str(&format!("concat=n={n}:v=1:a=1[outv][outa]"));
    } else {
        filter.push_str(&format!("concat=n={n}:v=0:a=1[outa]"));
    }
    cmd.args(["-filter_complex", &filter]);
    if has_video {
        cmd.args(["-map", "[outv]", "-map", "[outa]"]);
    } else {
        cmd.args(["-map", "[outa]"]);
    }
    cmd.args(["-y", output_path]);

    let output = cmd.output().map_err(|e| format!("ffmpegの起動に失敗しました(未インストールの可能性): {e}"))?;
    if !output.status.success() {
        return Err(format!("ffmpegによる結合が失敗しました: {}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

/// 本家ffmpegを優先して実行し、起動自体に失敗した場合(PATH上に無い場合)
/// のみ同梱の`rs-ffmpeg`へフォールバックする(2026-09-17新設、
/// `engine::iso`のxorriso→rs-xorrisoと同じパターン)。**正直な開示**:
/// `rs-ffmpeg`は非圧縮WAVの単純なサンプルレート/チャンネル変換専用
/// (`-i in.wav [-ar rate] [-ac channels] out.wav`のみ)で、コーデック
/// 指定・トリミング・ビットレート指定等の引数は自身で明確に拒否する
/// 設計になっている(黙って無視して壊れたファイルを作らない)ため、
/// このフォールバックは「本家ffmpegが無く、かつ単純なWAV処理」の
/// 場合のみ実際に成功し、それ以外は分かりやすいエラーで終わる。
fn run_ffmpeg(args: &[String]) -> Result<(), String> {
    match resolve_tool("ffmpeg").args(args).output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(String::from_utf8_lossy(&output.stderr).to_string()),
        Err(_) => {
            let output = resolve_tool("rs-ffmpeg")
                .args(args)
                .output()
                .map_err(|e| format!("ffmpeg・rs-ffmpegともに起動に失敗しました(いずれも未インストール/未同梱の可能性): {e}"))?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).to_string());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_for_target_size_kbps_computes_expected_value() {
        // 10MB(=80,000,000ビット)を100秒に収めるなら800kbps。
        let kbps = bitrate_for_target_size_kbps(10_000_000, 100.0);
        assert_eq!(kbps, 800);
    }

    #[test]
    fn bitrate_for_target_size_kbps_returns_zero_for_zero_duration() {
        assert_eq!(bitrate_for_target_size_kbps(10_000_000, 0.0), 0);
    }

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

    // --- ここから実ffmpegを使う統合テスト ---
    // ffmpegが無い環境ではeprintln!してスキップする(open-cuda/open-directx
    // の実機テストと同じ方針: fakeな成功にしない)。

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg").arg("-version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    /// (幅, 高さ, フレームレート)を実ffprobeで取得する
    /// (解像度/FPS指定〈2026-09-17新設〉の実機E2E検証用)。
    fn probe_video_dimensions_and_fps(path: &Path) -> (u32, u32, f64) {
        let output = Command::new("ffprobe")
            .args([
                "-v", "error",
                "-select_streams", "v:0",
                "-show_entries", "stream=width,height,r_frame_rate",
                "-of", "csv=p=0",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffprobe should run");
        let text = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = text.trim().split(',').collect();
        let width: u32 = parts[0].parse().expect("width should parse");
        let height: u32 = parts[1].parse().expect("height should parse");
        let fps = parts[2]
            .split('/')
            .map(|s| s.parse::<f64>().unwrap())
            .reduce(|num, den| num / den)
            .expect("r_frame_rate should parse as a fraction");
        (width, height, fps)
    }

    fn probe_duration_secs(path: &Path) -> f64 {
        let output = Command::new("ffprobe")
            .args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", path.to_str().unwrap()])
            .output()
            .expect("ffprobe should run");
        String::from_utf8_lossy(&output.stdout).trim().parse().expect("ffprobe should print a duration")
    }

    fn make_test_video(dir: &Path, name: &str, duration_secs: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-f", "lavfi", "-i", &format!("testsrc=duration={duration_secs}:size=320x240:rate=10"),
                "-c:v", "libx264", "-g", "10", "-pix_fmt", "yuv420p",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "test fixture generation failed: {}", String::from_utf8_lossy(&status.stderr));
        path
    }

    /// 音声の途中(2〜4秒)だけ無音にした6秒のテスト音声ファイルを作る
    /// (`detect_silence_ranges`の実機E2Eテスト用)。
    fn make_test_audio_with_silence_gap(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-y", "-f", "lavfi", "-i", "sine=frequency=1000:duration=6",
                "-af", "volume=enable='between(t,2,4)':volume=0",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "test fixture generation failed: {}", String::from_utf8_lossy(&status.stderr));
        path
    }

    #[test]
    fn real_ffmpeg_silence_detection_finds_the_expected_gap() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_silence_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_audio_with_silence_gap(&tmp, "source_with_gap.wav");

        let ranges = detect_silence_ranges(source.to_str().unwrap(), -30.0, 0.5).expect("detect_silence_ranges should succeed");
        let _ = fs::remove_dir_all(&tmp);

        assert_eq!(ranges.len(), 1, "expected exactly one silence range, got {ranges:?}");
        let r = &ranges[0];
        assert!((r.start_secs - 2.0).abs() < 0.2, "silence should start around 2.0s, got {}", r.start_secs);
        assert!((r.end_secs - 4.0).abs() < 0.2, "silence should end around 4.0s, got {}", r.end_secs);
    }

    #[test]
    fn real_ffmpeg_multi_range_cut_produces_expected_duration() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video(&tmp, "source.mp4", 20);
        let output = tmp.join("output.mp4");

        // カット: 最初の0-3秒、途中の8-10秒、最後の16-20秒(末尾まで)。
        // 残る区間: 3-8秒(5秒) + 10-16秒(6秒) = 合計11秒のはず。
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec![],
            bitrate: None,
            trim: None,
            cut_ranges: Some(vec![
                CutRange { start_secs: 0.0, end_secs: Some(3.0) },
                CutRange { start_secs: 8.0, end_secs: Some(10.0) },
                CutRange { start_secs: 16.0, end_secs: None },
            ]),
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
        };

        run_convert(&job).expect("run_convert with cut_ranges should succeed");

        let result_duration = probe_duration_secs(&output);
        assert!(
            (result_duration - 11.0).abs() < 2.0,
            "expected ~11s after cuts (stream-copy is keyframe-aligned so some slack is expected), got {result_duration}s"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_ffmpeg_frame_accurate_cut_is_close_to_exact() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_fa_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video(&tmp, "source.mp4", 10);
        let output = tmp.join("output.mp4");

        // 単一区間(2秒〜7秒、5秒分)のフレーム精度カット。
        // GPUエンコーダはこのCI/開発機には無い想定なのでlibx264
        // フォールバック経路を検証する。
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec![],
            bitrate: None,
            trim: None,
            cut_ranges: Some(vec![
                CutRange { start_secs: 0.0, end_secs: Some(2.0) },
                CutRange { start_secs: 7.0, end_secs: None },
            ]),
            frame_accurate: true,
            resolution: None,
            fps: None,
            ai_denoise: None,
        };

        run_convert(&job).expect("run_convert with frame_accurate should succeed");

        let result_duration = probe_duration_secs(&output);
        assert!(
            (result_duration - 5.0).abs() < 0.5,
            "frame-accurate cut should be close to exact (expected ~5s), got {result_duration}s"
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn real_ffmpeg_applies_resolution_and_fps() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_res_fps_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video(&tmp, "source.mp4", 2);
        let output = tmp.join("output.mp4");

        // DVDフルHD相当(1920x1080)・30fpsへの変換を指定(2026-09-17新設の
        // 解像度/FPS指定機能)。
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".to_string(), "libx264".to_string(), "-pix_fmt".to_string(), "yuv420p".to_string()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 1920, height: 1080 }),
            fps: Some(30),
            ai_denoise: None,
        };

        run_convert(&job).expect("run_convert with resolution/fps should succeed");

        let (width, height, fps) = probe_video_dimensions_and_fps(&output);
        let _ = fs::remove_dir_all(&tmp);

        assert_eq!((width, height), (1920, 1080), "指定した解像度(1920x1080)に変換されているはず");
        assert!((fps - 30.0).abs() < 0.1, "指定したフレームレート(30fps)に変換されているはず、実際: {fps}");
    }

    #[test]
    fn real_ffmpeg_applies_audio_bitrate_to_audio_only_output() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_abr_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video_with_audio(&tmp, "src.mp4", 6);
        let output = tmp.join("out.mp3");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "libmp3lame".into()],
            bitrate: Some(BitrateMode::Fixed(64)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
        };
        run_convert(&job).expect("audio-only conversion from a video input should succeed");
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "format=bit_rate", "-of", "default=nw=1:nk=1", output.to_str().unwrap()]).output().unwrap();
        let kbps: f64 = String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().unwrap() / 1000.0;
        let _ = fs::remove_dir_all(&tmp);
        assert!((kbps - 64.0).abs() < 8.0, "指定した64kbpsが音声出力に効いているはず(実際: {kbps} kbps)");
    }

    #[test]
    fn real_ffmpeg_encodes_av1_video_with_opus_audio() {
        if !ffmpeg_available() || detect_av1_encoder().is_none() {
            eprintln!("ffmpegまたはAV1エンコーダが無いためスキップ");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_av1_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video_with_audio(&tmp, "src.mp4", 2);
        let output = tmp.join("out.webm");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:v".into(), "av1".into(), "-c:a".into(), "libopus".into()],
            bitrate: Some(BitrateMode::Fixed(200)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
        };
        run_convert(&job).expect("AV1+Opus conversion should succeed");
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0", output.to_str().unwrap()]).output().unwrap();
        let codecs = String::from_utf8_lossy(&out.stdout).to_string();
        let _ = fs::remove_dir_all(&tmp);
        assert!(codecs.contains("av1") && codecs.contains("opus"), "出力はAV1+Opusのはず(実際: {codecs})");
    }

    #[test]
    fn real_ffmpeg_encodes_opus_audio_only() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_opus_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_test_video_with_audio(&tmp, "src.mp4", 3);
        let output = tmp.join("out.opus");
        let job = ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: output.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "libopus".into()],
            bitrate: Some(BitrateMode::Fixed(96)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: None,
        };
        run_convert(&job).expect("Opus conversion should succeed");
        let out = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_name", "-of", "csv=p=0", output.to_str().unwrap()]).output().unwrap();
        let codecs = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(codecs, "opus");
    }

    fn make_surround_video(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let st = Command::new("ffmpeg")
            .args(["-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10", "-f", "lavfi", "-i", "sine=frequency=440:duration=2", "-filter_complex", "[1:a]pan=5.1|c0=c0|c1=c0|c2=c0|c3=c0|c4=c0|c5=c0[a]", "-map", "0:v", "-map", "[a]", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "ac3", path.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stderr));
        path
    }

    #[test]
    fn real_ffmpeg_preserves_surround_with_eac3_and_stream_copy() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_surround_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let source = make_surround_video(&tmp, "src.mkv");
        let mk = |out: &Path, codec: Vec<&str>| ConvertJob {
            input_path: source.to_string_lossy().to_string(),
            output_path: out.to_string_lossy().to_string(),
            codec_args: codec.into_iter().map(String::from).collect(),
            bitrate: Some(BitrateMode::Fixed(500)),
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: Some(Resolution { width: 640, height: 480 }),
            fps: Some(30),
            ai_denoise: None,
        };
        let channels = |p: &Path| -> String {
            let o = Command::new("ffprobe").args(["-v", "error", "-select_streams", "a:0", "-show_entries", "stream=channels,codec_name", "-of", "csv=p=0", p.to_str().unwrap()]).output().unwrap();
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        let eac3 = tmp.join("out.eac3");
        run_convert(&mk(&eac3, vec!["-c:a", "eac3"])).expect("E-AC-3");
        let copy = tmp.join("out.mkv");
        run_convert(&mk(&copy, vec!["-map", "0", "-c", "copy"])).expect("stream copy must ignore bitrate/resolution/fps");
        let (e, c) = (channels(&eac3), channels(&copy));
        let dims = probe_video_dimensions_and_fps(&copy);
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(e, "eac3,6", "E-AC-3は5.1を保持するはず");
        assert_eq!(c, "ac3,6", "無変換コピーは音声コーデックとチャンネル数をそのまま保持するはず");
        assert_eq!((dims.0, dims.1), (320, 240), "無変換コピーでは解像度指定は適用されず元のまま");
    }

    fn mean_volume_db(path: &Path) -> f64 {
        let o = Command::new("ffmpeg").args(["-hide_banner", "-i", path.to_str().unwrap(), "-af", "volumedetect", "-f", "null", "-"]).output().unwrap();
        let text = String::from_utf8_lossy(&o.stderr).to_string();
        let line = text.lines().find(|l| l.contains("mean_volume")).expect("mean_volume line");
        line.split("mean_volume:").nth(1).unwrap().trim().trim_end_matches(" dB").trim().parse().unwrap()
    }

    #[test]
    fn real_ffmpeg_ai_denoise_reduces_noise_with_the_rnnoise_model() {
        if !ffmpeg_available() {
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_test_denoise_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        // ホワイトノイズのみの音声(4秒)。ノイズ除去なら大きく下がるはず。
        let noisy = tmp.join("noise.wav");
        let st = Command::new("ffmpeg").args(["-y", "-f", "lavfi", "-i", "anoisesrc=d=4:c=white:a=0.3:r=48000", noisy.to_str().unwrap()]).output().unwrap();
        assert!(st.status.success());
        let out = tmp.join("clean.wav");
        let job = ConvertJob {
            input_path: noisy.to_string_lossy().to_string(),
            output_path: out.to_string_lossy().to_string(),
            codec_args: vec!["-c:a".into(), "pcm_s16le".into()],
            bitrate: None,
            trim: None,
            cut_ranges: None,
            frame_accurate: false,
            resolution: None,
            fps: None,
            ai_denoise: Some(AiDenoise { mix: 1.0 }),
        };
        run_convert(&job).expect("AI denoise conversion should succeed");
        let (before, after) = (mean_volume_db(&noisy), mean_volume_db(&out));
        let _ = fs::remove_dir_all(&tmp);
        assert!(after < before - 6.0, "RNNoiseでノイズが6dB以上下がるはず(前: {before} dB, 後: {after} dB)");
    }

    #[test]
    fn equal_interval_segments_splits_into_n_equal_parts() {
        let segments = equal_interval_segments(100.0, 4);
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[0], TrimRange { start_secs: Some(0.0), duration_secs: Some(25.0) });
        assert_eq!(segments[1], TrimRange { start_secs: Some(25.0), duration_secs: Some(25.0) });
        assert_eq!(segments[3], TrimRange { start_secs: Some(75.0), duration_secs: Some(25.0) });
    }

    #[test]
    fn equal_interval_segments_returns_empty_for_zero_count_or_duration() {
        assert!(equal_interval_segments(100.0, 0).is_empty());
        assert!(equal_interval_segments(0.0, 4).is_empty());
    }

    #[test]
    fn fixed_length_segments_splits_with_a_shorter_remainder_at_the_end() {
        let segments = fixed_length_segments(250.0, 100.0);
        assert_eq!(segments, vec![
            TrimRange { start_secs: Some(0.0), duration_secs: Some(100.0) },
            TrimRange { start_secs: Some(100.0), duration_secs: Some(100.0) },
            TrimRange { start_secs: Some(200.0), duration_secs: Some(50.0) },
        ]);
    }

    #[test]
    fn fixed_length_segments_exact_division_has_no_remainder() {
        let segments = fixed_length_segments(200.0, 100.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[1], TrimRange { start_secs: Some(100.0), duration_secs: Some(100.0) });
    }

    #[test]
    fn concat_media_requires_at_least_two_files() {
        let result = concat_media(&["only-one.mp4".to_string()], "out.mp4", true);
        assert!(result.is_err());
    }

    /// `concat_media`は音声トラックも結合対象にするため、`make_test_video`
    /// (映像のみ)ではなく、無音の音声トラックも持つテスト動画を作る。
    fn make_test_video_with_audio(dir: &Path, name: &str, duration_secs: u32) -> std::path::PathBuf {
        let path = dir.join(name);
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-f", "lavfi", "-i", &format!("testsrc=duration={duration_secs}:size=320x240:rate=10"),
                "-f", "lavfi", "-i", &format!("anullsrc=r=44100:cl=stereo:d={duration_secs}"),
                "-c:v", "libx264", "-g", "10", "-pix_fmt", "yuv420p",
                "-c:a", "aac",
                path.to_str().unwrap(),
            ])
            .output()
            .expect("ffmpeg should run");
        assert!(status.status.success(), "test fixture generation failed: {}", String::from_utf8_lossy(&status.stderr));
        path
    }

    #[test]
    fn real_ffmpeg_concat_produces_the_expected_total_duration() {
        if !ffmpeg_available() {
            eprintln!("ffmpegが見つからないためスキップ / skipping: ffmpeg not found on PATH");
            return;
        }

        let tmp = std::env::temp_dir().join(format!("make_disk_test_concat_{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        let a = make_test_video_with_audio(&tmp, "a.mp4", 3);
        let b = make_test_video_with_audio(&tmp, "b.mp4", 4);
        let output = tmp.join("concat_output.mp4");

        concat_media(&[a.to_string_lossy().to_string(), b.to_string_lossy().to_string()], output.to_str().unwrap(), true).expect("concat_media should succeed");

        let result_duration = probe_duration_secs(&output);
        assert!((result_duration - 7.0).abs() < 0.5, "結合後の尺は3秒+4秒=7秒に近いはず、実際: {result_duration}s");

        let _ = fs::remove_dir_all(&tmp);
    }
}
