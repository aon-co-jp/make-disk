//! AIによるアップミックス(モノラル/ステレオ→5.1ch/7.1ch、2026-09-28新設)。
//!
//! `convert::channel_layout_args`の単純な複製・按分によるpanフィルタ疑似サラウンドとは異なり、
//! 実在する音源分離モデル**HT-Demucs**(Meta、MITライセンス、[StemSplitio/htdemucs-onnx]
//! (https://huggingface.co/StemSplitio/htdemucs-onnx)配布のONNX版)で、音声を実際に
//! ボーカル/ドラム/ベース/その他(残響・伴奏)の4パートへ分離し、それぞれを異なるスピーカーへ
//! 配置する(センター=ボーカル、フロント=ドラム+ベース、リア=その他)。他の音声AI機能
//! (RNNoise/LavaSR)と同じく推論は純Rustの`tract`で行い、Pythonは使わない。
//!
//! ## 正直な開示(重要)
//! - モデル自体は実在するボーカル/ドラム/ベース/その他の**4パート分離**であり、5.1ch/7.1ch
//!   そのものを直接出力するわけではない。分離結果をどのスピーカーへ配置するかは
//!   本モジュールの設計(下記)であり、実際のディスクリート5.1ch/7.1ch制作物とは異なる。
//! - tractの標準のONNX `Pad`/`Range`演算子実装がこのモデルの一部の使い方(省略可能な
//!   `Pad`の第3入力、動的形状に依存する`Range`)に対応していなかったため、
//!   `tools/demucs-onnx-patch/patch_htdemucs_onnx.py`で事前にグラフを固定長
//!   (343,980サンプル=44.1kHzで約7.8秒)化・定数化したパッチ済みモデルを使う
//!   (onnxruntimeでは無印のモデルのまま問題なく動く。これはtract側の制約)。
//! - パッチ済みモデルは約2.2GB(.onnx本体+外部データ)と大きく、make-disk本体には同梱せず、
//!   利用者が別途用意したモデルフォルダを指す必要がある(自動ダウンロードは未実装、
//!   配布インフラ〈2GB超のためGitHub Releasesの標準上限を超える〉は次回課題)。
//!   モデルが無い/読み込めない環境では[`is_available`]が`false`を返し、
//!   呼び出し側(`convert.rs`)は既存のpanフィルタ疑似サラウンドへフォールバックする。
//! - 固定長(343,980サンプル)単位のチャンクに分けて処理し、最後の端数はゼロ埋めする
//!   (Demucs本来のオーバーラップ加算によるクロスフェードは行わないため、チャンクの
//!   境目でごく僅かな不連続が生じ得る近似)。

use std::path::{Path, PathBuf};
use tract_onnx::prelude::*;

use crate::engine::convert::ChannelLayoutTarget;

/// パッチ済みモデルが1回に処理する固定入力長(サンプル数、44.1kHz換算で約7.8秒)。
/// `tools/demucs-onnx-patch/patch_htdemucs_onnx.py`の`FIXED_INPUT_SAMPLES`と一致させること。
const CHUNK_SAMPLES: usize = 343_980;
const MODEL_SR: u32 = 44_100;
/// モデル出力の4ステムの順序(demucs-onnxの`list-models`が報告する順序どおり)。
const STEM_ORDER: [&str; 4] = ["drums", "bass", "other", "vocals"];

fn model_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("MAKE_DISK_DEMUCS_MODEL_DIR") {
        return Some(PathBuf::from(dir));
    }
    crate::engine::plugins::plugin_dir().map(|d| d.join("ai-upmix").join("htdemucs"))
}

fn model_path() -> Option<PathBuf> {
    let dir = model_dir()?;
    let path = dir.join("htdemucs_patched.onnx");
    path.is_file().then_some(path)
}

/// このマシンでAIアップミックスが実際に使えるか(モデルファイルが用意されているか)の軽い確認。
/// 実際にモデルを読み込むわけではないので高速。
pub fn is_available() -> bool {
    model_path().is_some()
}

type Plan = TypedRunnableModel<TypedModel>;

fn load_model() -> Result<Plan, String> {
    let path = model_path().ok_or("AIアップミックスのモデルが見つかりません(MAKE_DISK_DEMUCS_MODEL_DIRを確認してください)")?;
    tract_onnx::onnx()
        .model_for_path(&path)
        .and_then(|m| m.into_optimized())
        .and_then(|m| m.into_runnable())
        .map_err(|e| format!("AIアップミックスのモデルを読み込めません: {e}"))
}

/// 1チャンク分(ちょうど[`CHUNK_SAMPLES`]サンプル、ステレオ)を推論し、
/// `[drums, bass, other, vocals]`の4ステム(各ステレオ)を返す。
fn separate_chunk(plan: &Plan, left: &[f32], right: &[f32]) -> Result<[[Vec<f32>; 2]; 4], String> {
    debug_assert_eq!(left.len(), CHUNK_SAMPLES);
    debug_assert_eq!(right.len(), CHUNK_SAMPLES);
    let mut data = vec![0f32; 2 * CHUNK_SAMPLES];
    data[..CHUNK_SAMPLES].copy_from_slice(left);
    data[CHUNK_SAMPLES..].copy_from_slice(right);
    let input = tract_ndarray::Array3::from_shape_vec((1, 2, CHUNK_SAMPLES), data).map_err(|e| e.to_string())?.into_tensor();
    let result = plan.run(tvec!(input.into())).map_err(|e| format!("推論に失敗しました: {e}"))?;
    let out = result[0].to_array_view::<f32>().map_err(|e| e.to_string())?;
    // 出力shape: [1, 4, 2, CHUNK_SAMPLES]
    let mut stems: [[Vec<f32>; 2]; 4] = std::array::from_fn(|_| [Vec::new(), Vec::new()]);
    for (s, stem) in stems.iter_mut().enumerate() {
        for (c, ch) in stem.iter_mut().enumerate() {
            ch.reserve(CHUNK_SAMPLES);
            for t in 0..CHUNK_SAMPLES {
                ch.push(out[[0, s, c, t]]);
            }
        }
    }
    Ok(stems)
}

/// 入力(任意長・任意チャンネル数)を、[`CHUNK_SAMPLES`]単位のステレオチャンクに分けて
/// 全区間分離し、`[drums, bass, other, vocals]`(各ステレオ、元の長さぶん)を返す。
fn separate_all(sr: u32, channels: &[Vec<f32>]) -> Result<[[Vec<f32>; 2]; 4], String> {
    if sr != MODEL_SR {
        return Err(format!("AIアップミックスは{MODEL_SR}Hz固定です(入力: {sr}Hz)。呼び出し側でリサンプルしてください。"));
    }
    let total = channels.first().map_or(0, |c| c.len());
    let (left, right) = match channels.len() {
        1 => (channels[0].clone(), channels[0].clone()),
        _ => (channels[0].clone(), channels[1].clone()),
    };
    let plan = load_model()?;
    let mut stems: [Vec<Vec<f32>>; 4] = std::array::from_fn(|_| vec![Vec::with_capacity(total), Vec::with_capacity(total)]);
    let mut pos = 0usize;
    while pos < total.max(1) {
        let end = (pos + CHUNK_SAMPLES).min(total);
        let mut chunk_l = vec![0f32; CHUNK_SAMPLES];
        let mut chunk_r = vec![0f32; CHUNK_SAMPLES];
        chunk_l[..end - pos].copy_from_slice(&left[pos..end]);
        chunk_r[..end - pos].copy_from_slice(&right[pos..end]);
        let separated = separate_chunk(&plan, &chunk_l, &chunk_r)?;
        let keep = end - pos;
        for (s, stem) in separated.into_iter().enumerate() {
            stems[s][0].extend_from_slice(&stem[0][..keep]);
            stems[s][1].extend_from_slice(&stem[1][..keep]);
        }
        pos += CHUNK_SAMPLES;
        if total == 0 {
            break;
        }
    }
    Ok(stems.map(|s| [s[0].clone(), s[1].clone()]))
}

fn stem_index(name: &str) -> usize {
    STEM_ORDER.iter().position(|s| *s == name).expect("known stem name")
}

/// 分離した4ステムから、5.1ch/7.1chのチャンネル配列(FL,FR,FC,LFE,BL,BR[,SL,SR])を組み立てる。
/// センター=ボーカル(左右の単純和)、フロント=ドラム+ベース(元のステレオ配置)、
/// リア(サイド)=その他(残響・伴奏成分)。LFEは常に無音。
fn mix_to_surround(stems: &[[Vec<f32>; 2]; 4], target: ChannelLayoutTarget) -> Result<Vec<Vec<f32>>, String> {
    let drums = &stems[stem_index("drums")];
    let bass = &stems[stem_index("bass")];
    let other = &stems[stem_index("other")];
    let vocals = &stems[stem_index("vocals")];
    let n = drums[0].len();

    let mix = |a: &[f32], b: &[f32]| -> Vec<f32> { a.iter().zip(b).map(|(x, y)| x + y).collect() };
    let fl = mix(&drums[0], &bass[0]);
    let fr = mix(&drums[1], &bass[1]);
    let fc: Vec<f32> = (0..n).map(|i| 0.5 * (vocals[0][i] + vocals[1][i])).collect();
    let lfe = vec![0f32; n];
    let rear_gain = 0.9f32; // 残響/伴奏成分をそのままリアへ(前方と重複しないため減衰は最小限)
    let bl: Vec<f32> = other[0].iter().map(|v| v * rear_gain).collect();
    let br: Vec<f32> = other[1].iter().map(|v| v * rear_gain).collect();

    let mut channels = vec![fl, fr, fc, lfe, bl, br];
    if matches!(target, ChannelLayoutTarget::Surround71) {
        let side_gain = 0.6f32;
        channels.push(other[0].iter().map(|v| v * side_gain).collect());
        channels.push(other[1].iter().map(|v| v * side_gain).collect());
    }
    Ok(channels)
}

/// 44.1kHzのWAV(`input`、モノラルまたはステレオ)を読み、AIで4パートに分離してから
/// 5.1ch/7.1chへミックスし、`output`へ書く。`target`は`Surround51`/`Surround71`のみ有効。
pub fn upmix_wav_file(input: &Path, output: &Path, target: ChannelLayoutTarget) -> Result<(), String> {
    if !matches!(target, ChannelLayoutTarget::Surround51 | ChannelLayoutTarget::Surround71) {
        return Err("AIアップミックスは5.1ch/7.1ch指定のときだけ使えます".to_string());
    }
    let (sr, channels) = crate::engine::audio_sr::read_f32_wav(input)?;
    let stems = separate_all(sr, &channels)?;
    let surround = mix_to_surround(&stems, target)?;
    crate::engine::audio_sr::write_f32_wav(output, sr, &surround)
}

/// 任意の音声/動画ソースファイルから、AIアップミックス済み(5.1ch/7.1ch)のWAVを作る。
/// 内部でffmpegを使い、いったん44.1kHzステレオWAVへデコードしてから[`upmix_wav_file`]を呼ぶ
/// (`convert.rs`の`AudioBwe`〈帯域拡張〉が行っているのと同じ「一旦WAVへ落としてRustのAI処理へ渡す」構成)。
pub fn upmix_source_file(input_path: &str, output: &Path, target: ChannelLayoutTarget) -> Result<(), String> {
    let tag = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let decoded = std::env::temp_dir().join(format!("make-disk-upmix-in-{tag}.wav"));
    let status = crate::engine::sidecar::resolve_tool("ffmpeg")
        .args(["-y", "-v", "error", "-i", input_path, "-vn", "-ac", "2", "-ar", &MODEL_SR.to_string(), "-c:a", "pcm_f32le", decoded.to_str().unwrap_or_default()])
        .status()
        .map_err(|e| format!("ffmpegを実行できません: {e}"))?;
    if !status.success() {
        return Err(format!("ffmpegによるデコードに失敗しました(終了コード{:?})", status.code()));
    }
    let result = upmix_wav_file(&decoded, output, target);
    let _ = std::fs::remove_file(&decoded);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent_stems(n: usize) -> [[Vec<f32>; 2]; 4] {
        std::array::from_fn(|_| [vec![0f32; n], vec![0f32; n]])
    }

    #[test]
    fn stem_index_matches_known_order() {
        assert_eq!(stem_index("drums"), 0);
        assert_eq!(stem_index("bass"), 1);
        assert_eq!(stem_index("other"), 2);
        assert_eq!(stem_index("vocals"), 3);
    }

    #[test]
    fn mix_to_surround51_has_six_channels_with_silent_lfe() {
        let mut stems = silent_stems(2);
        stems[stem_index("vocals")] = [vec![1.0, 1.0], vec![1.0, 1.0]];
        let out = mix_to_surround(&stems, ChannelLayoutTarget::Surround51).unwrap();
        assert_eq!(out.len(), 6);
        assert_eq!(out[3], vec![0.0, 0.0], "LFEは常に無音");
        assert_eq!(out[2], vec![1.0, 1.0], "センターはボーカルの単純和(0.5*(1+1)=1)");
    }

    #[test]
    fn mix_to_surround71_adds_two_side_channels_from_other_stem() {
        let mut stems = silent_stems(3);
        stems[stem_index("other")] = [vec![1.0, 1.0, 1.0], vec![1.0, 1.0, 1.0]];
        let out = mix_to_surround(&stems, ChannelLayoutTarget::Surround71).unwrap();
        assert_eq!(out.len(), 8);
        assert!((out[6][0] - 0.6).abs() < 1e-6, "サイドLはotherの0.6倍");
        assert!((out[4][0] - 0.9).abs() < 1e-6, "リアLもotherの0.9倍");
    }

    #[test]
    fn front_channels_sum_drums_and_bass_keeping_stereo_placement() {
        let mut stems = silent_stems(2);
        stems[stem_index("drums")] = [vec![0.3, 0.3], vec![0.1, 0.1]];
        stems[stem_index("bass")] = [vec![0.2, 0.2], vec![0.05, 0.05]];
        let out = mix_to_surround(&stems, ChannelLayoutTarget::Surround51).unwrap();
        assert!((out[0][0] - 0.5).abs() < 1e-6, "FL = drums.L + bass.L");
        assert!((out[1][0] - 0.15).abs() < 1e-6, "FR = drums.R + bass.R");
    }

    #[test]
    fn upmix_requires_a_surround_target() {
        let dir = std::env::temp_dir().join(format!("make_disk_ai_upmix_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.wav");
        crate::engine::audio_sr::write_f32_wav(&input, 44_100, &[vec![0.0; 100], vec![0.0; 100]]).unwrap();
        let output = dir.join("out.wav");
        let err = upmix_wav_file(&input, &output, ChannelLayoutTarget::Stereo).unwrap_err();
        assert!(err.contains("5.1ch/7.1ch"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_available_is_false_without_a_configured_model_dir() {
        // このテスト環境にMAKE_DISK_DEMUCS_MODEL_DIRを設定していない前提(CI/開発機の既定)。
        if std::env::var("MAKE_DISK_DEMUCS_MODEL_DIR").is_ok() {
            return;
        }
        assert!(!is_available());
    }
}
