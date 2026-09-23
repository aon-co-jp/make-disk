//! AI超解像(Real-ESRGAN、GPU=NCNN-Vulkan)(2026-09-19新設)。
//!
//! ユーザー指示「DVD→4K等の拡大は、補間ではなく失われたディテールを補う本格的なAI超解像を」への対応。
//! 公式の[Real-ESRGAN-ncnn-vulkan](https://github.com/xinntao/Real-ESRGAN)(MITライセンス)を
//! **オンデマンドでダウンロードするプラグイン**として使う(約45MB、インストーラーを肥大化させない。
//! バージョン`v0.2.5.0`ごとに`<プラグインフォルダ>/realesrgan/<版>/`へ展開し、あれば再取得しない)。
//!
//! ## 実機で確認した事実・制限(正直な開示)
//! - **GPU版(NCNN-Vulkan)はVulkan対応GPUが必須**(NVIDIA/AMD/Intel、内蔵GPUも可)。公式ビルドにCPUモードは無い
//!   (`-g -1`は「invalid gpu device」で失敗することを実機確認)。そこで**GPUが使えない環境向けに自前のCPU版**
//!   (`engine::cpu_sr`、`realesr-animevideov3`のみ、AVX2+FMA対応)を用意し、`backend="auto"`ではGPUが動かなければ自動でCPU版に切り替える。
//!   `realesrgan-x4plus`(高品質)はGPU必須。
//! - **速度は非常に遅い**: このPCのGT 730(2GB)で720×480の1フレームあたり、軽量モデル
//!   `realesr-animevideov3`が約4.6秒、高品質モデル`realesrgan-x4plus`が約110秒。映画1本(約13万フレーム)は
//!   現実的ではなく、短いクリップ向け。`MAX_FRAMES`を超える素材はトリミングを促すエラーにする。
//! - フレームをPNGへ書き出して処理するため一時ディスク容量を使う(100フレームずつ処理して都度削除)。

use crate::engine::plugins;
use crate::engine::sidecar::resolve_tool;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// 配布物の版(GitHub: xinntao/Real-ESRGAN のリリースタグ)。
pub const REALESRGAN_VERSION: &str = "v0.2.5.0";
/// 一度に処理できる最大フレーム数(超えたらトリミングを促す)。24fpsで約100秒。
pub const MAX_FRAMES: u64 = 2400;
const CHUNK_FRAMES: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiUpscale {
    /// `"realesr-animevideov3"`(軽量・高速、アニメ/実写どちらにも使える動画向け)または
    /// `"realesrgan-x4plus"`(高品質だが非常に遅い、実写向け)。
    pub model: String,
    /// 拡大倍率(2/3/4)。`realesrgan-x4plus`は4のみ。
    pub scale: u32,
    /// 実行環境: `"auto"`(既定。Vulkan対応GPUが使えればGPU、無ければCPU)/`"gpu"`/`"cpu"`。
    /// CPU版は`realesr-animevideov3`のみ対応(`engine::cpu_sr`)。
    #[serde(default = "default_backend")]
    pub backend: String,
}

fn default_backend() -> String {
    "auto".to_string()
}

/// 実際に使う実行環境。
enum Backend {
    Gpu(PathBuf),
    Cpu(crate::engine::cpu_sr::SrModel),
}

/// Vulkan対応GPUで実際に動くか(小さな画像で試す。結果はプロセス内でキャッシュ)。
pub(crate) fn gpu_usable(exe: &Path) -> bool {
    static CACHE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("make-disk-gpucheck-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let (input, output) = (dir.join("in.png"), dir.join("out.png"));
        let ok = image::RgbImage::new(16, 16).save(&input).is_ok()
            && crate::engine::sidecar::background_command(exe)
                .args([
                    "-i",
                    &input.to_string_lossy(),
                    "-o",
                    &output.to_string_lossy(),
                    "-m",
                    &exe.parent()
                        .map(|p| p.join("models"))
                        .unwrap_or_default()
                        .to_string_lossy(),
                    "-n",
                    "realesr-animevideov3",
                    "-s",
                    "2",
                    "-f",
                    "png",
                ])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            && output.is_file();
        let _ = std::fs::remove_dir_all(&dir);
        ok
    })
}

/// 設定と実機の状況から実行環境を決める。CPUのみのPC(GPU非搭載/Vulkan非対応)ではCPU版へ自動で切り替える。
fn choose_backend(exe: &Path, up: &AiUpscale) -> Result<Backend, String> {
    let models = exe
        .parent()
        .ok_or("プラグインの場所が不正です")?
        .join("models");
    let cpu_capable = up.model == "realesr-animevideov3";
    match up.backend.as_str() {
        "gpu" => Ok(Backend::Gpu(exe.to_path_buf())),
        "cpu" if cpu_capable => Ok(Backend::Cpu(crate::engine::cpu_sr::load_model(&models, up.scale)?)),
        "cpu" => Err("CPU版はrealesr-animevideov3のみ対応です(realesrgan-x4plusはGPUが必要) / the CPU build only supports realesr-animevideov3".to_string()),
        _ => {
            if gpu_usable(exe) {
                Ok(Backend::Gpu(exe.to_path_buf()))
            } else if cpu_capable {
                Ok(Backend::Cpu(crate::engine::cpu_sr::load_model(&models, up.scale)?))
            } else {
                Err("Vulkan対応GPUが見つかりません。realesrgan-x4plusはGPUが必要です(CPUでは高速モデルrealesr-animevideov3を選んでください) / no Vulkan GPU found; realesrgan-x4plus needs one".to_string())
            }
        }
    }
}

/// 画像ファイルまたはフォルダ`input`を`output`へ超解像する(選ばれた実行環境で)。
fn run_backend(
    backend: &Backend,
    input: &Path,
    output: &Path,
    up: &AiUpscale,
) -> Result<(), String> {
    match backend {
        Backend::Gpu(exe) => run_realesrgan(exe, input, output, up),
        Backend::Cpu(model) => {
            if input.is_dir() {
                std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
                let mut files: Vec<_> = std::fs::read_dir(input)
                    .map_err(|e| e.to_string())?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .collect();
                files.sort();
                for f in files {
                    let name = f.file_name().ok_or("ファイル名が不正です")?;
                    crate::engine::cpu_sr::upscale_image_file(model, &f, &output.join(name))?;
                }
                Ok(())
            } else {
                crate::engine::cpu_sr::upscale_image_file(model, input, output)
            }
        }
    }
}

fn asset_name() -> Result<&'static str, String> {
    if cfg!(target_os = "windows") {
        Ok("realesrgan-ncnn-vulkan-20220424-windows.zip")
    } else if cfg!(target_os = "macos") {
        Ok("realesrgan-ncnn-vulkan-20220424-macos.zip")
    } else if cfg!(target_os = "linux") {
        Ok("realesrgan-ncnn-vulkan-20220424-ubuntu.zip")
    } else {
        Err("AI超解像はデスクトップ(Windows/macOS/Linux)のみ対応です / AI upscaling is desktop-only".to_string())
    }
}

fn exe_file_name() -> String {
    format!("realesrgan-ncnn-vulkan{}", std::env::consts::EXE_SUFFIX)
}

/// プラグインの展開先(版ごと)。
fn plugin_root() -> Result<PathBuf, String> {
    Ok(plugins::plugin_dir()
        .ok_or("プラグインフォルダを特定できません")?
        .join("realesrgan")
        .join(REALESRGAN_VERSION))
}

/// 実行ファイルがあれば(=導入済みなら)そのパスを返す。ネットワークは使わない。
pub fn installed_exe() -> Option<PathBuf> {
    let exe = plugin_root().ok()?.join(exe_file_name());
    exe.is_file().then_some(exe)
}

/// プラグインが無ければダウンロード・展開して、実行ファイルのパスを返す(導入済みならスキップ)。
pub fn ensure_plugin() -> Result<PathBuf, String> {
    if let Some(exe) = installed_exe() {
        return Ok(exe);
    }
    let asset = asset_name()?;
    let url = format!(
        "https://github.com/xinntao/Real-ESRGAN/releases/download/{REALESRGAN_VERSION}/{asset}"
    );
    let root = plugin_root()?;
    std::fs::create_dir_all(&root)
        .map_err(|e| format!("プラグインフォルダを作成できません: {e}"))?;

    let mut bytes: Vec<u8> = Vec::new();
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("Real-ESRGANのダウンロードに失敗しました({url}): {e}"))?;
    std::io::copy(&mut resp.into_reader(), &mut bytes)
        .map_err(|e| format!("ダウンロードの読み取りに失敗しました: {e}"))?;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("ZIPを開けません: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| format!("ZIPの読み取りに失敗しました: {e}"))?;
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let dest = root.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&dest)
                .map_err(|e| format!("{}を作成できません: {e}", dest.display()))?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let exe = root.join(exe_file_name());
        if let Ok(meta) = std::fs::metadata(&exe) {
            let mut perm = meta.permissions();
            perm.set_mode(0o755);
            let _ = std::fs::set_permissions(&exe, perm);
        }
    }
    installed_exe().ok_or_else(|| "展開後にrealesrgan-ncnn-vulkanが見つかりません".to_string())
}

fn validate(up: &AiUpscale) -> Result<(), String> {
    match up.model.as_str() {
        "realesr-animevideov3" if (2..=4).contains(&up.scale) => Ok(()),
        "realesrgan-x4plus" if up.scale == 4 => Ok(()),
        "realesr-animevideov3" => Err("realesr-animevideov3の倍率は2/3/4のみです".to_string()),
        "realesrgan-x4plus" => Err("realesrgan-x4plusの倍率は4のみです".to_string()),
        other => Err(format!("未対応のAIモデルです: {other}")),
    }
}

/// `input`(画像ファイルまたはフォルダ)を`output`へAI超解像する。
fn run_realesrgan(exe: &Path, input: &Path, output: &Path, up: &AiUpscale) -> Result<(), String> {
    let models = exe
        .parent()
        .ok_or("プラグインの場所が不正です")?
        .join("models");
    let out = crate::engine::sidecar::background_command(exe)
        .args([
            "-i",
            &input.to_string_lossy(),
            "-o",
            &output.to_string_lossy(),
            "-m",
            &models.to_string_lossy(),
            "-n",
            &up.model,
            "-s",
            &up.scale.to_string(),
            "-f",
            "png",
        ])
        .output()
        .map_err(|e| format!("realesrgan-ncnn-vulkanの起動に失敗しました: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "AI超解像に失敗しました(Vulkan対応GPUが必要です): {}",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .filter(|l| !l.contains('%'))
                .collect::<Vec<_>>()
                .join(" / ")
        ));
    }
    Ok(())
}

/// 1枚の画像をAI超解像する。
pub fn upscale_image(input: &str, output: &str, up: &AiUpscale) -> Result<(), String> {
    validate(up)?;
    let exe = ensure_plugin()?;
    let backend = choose_backend(&exe, up)?;
    run_backend(&backend, Path::new(input), Path::new(output), up)
}

/// 動画を、AI超解像済みの映像に元の音声を付けた中間ファイル(`mezzanine`)へ変換する。
/// `trim`は(開始秒, 長さ秒)。フレームを100枚ずつ処理して都度一時ファイルを削除する。
pub fn make_upscaled_mezzanine(
    input: &str,
    trim: Option<(Option<f64>, Option<f64>)>,
    up: &AiUpscale,
    mezzanine: &Path,
) -> Result<(), String> {
    validate(up)?;
    let info = crate::engine::probe::probe(input)?;
    let fps = info
        .fps
        .filter(|f| *f > 0.0)
        .ok_or("入力に映像が見つかりません(フレームレートを取得できません)")?;
    let (start, dur) = trim.unwrap_or((None, None));
    let seconds = dur.unwrap_or((info.duration_secs - start.unwrap_or(0.0)).max(0.0));
    let est_frames = (seconds * fps).ceil() as u64;
    if est_frames > MAX_FRAMES {
        return Err(format!(
            "AI超解像は短いクリップ向けです(推定{est_frames}フレーム > 上限{MAX_FRAMES})。1フレームあたり数秒〜数分かかるため、トリミングで短くしてください。 / AI upscaling is for short clips ({est_frames} frames > {MAX_FRAMES}); please trim."
        ));
    }
    let exe = ensure_plugin()?;
    let backend = choose_backend(&exe, up)?;

    let work = mezzanine
        .parent()
        .unwrap_or(Path::new("."))
        .join(format!(".make-disk-ai-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let frames_in = work.join("in");
    std::fs::create_dir_all(&frames_in)
        .map_err(|e| format!("作業フォルダを作成できません: {e}"))?;
    let cleanup = |w: &Path| {
        let _ = std::fs::remove_dir_all(w);
    };

    // 1) 全フレームをPNGへ展開(トリミング適用、フレーム落ちが無いようpassthrough)。
    let mut cmd = resolve_tool("ffmpeg");
    cmd.args(["-v", "error"]);
    if let Some(s) = start {
        cmd.args(["-ss", &s.to_string()]);
    }
    cmd.args(["-i", input]);
    if let Some(d) = dur {
        cmd.args(["-t", &d.to_string()]);
    }
    cmd.args([
        "-an",
        "-fps_mode",
        "passthrough",
        &frames_in.join("%08d.png").to_string_lossy(),
    ]);
    let out = cmd
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
    if !out.status.success() {
        cleanup(&work);
        return Err(format!(
            "フレームの展開に失敗しました: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let mut names: Vec<_> = std::fs::read_dir(&frames_in)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    names.sort();
    if names.is_empty() {
        cleanup(&work);
        return Err("展開できるフレームがありませんでした".to_string());
    }

    // 2) 100枚ずつ: 超解像 → 中間動画(高品質x264) → 元のフレームを削除。
    let mut chunk_videos: Vec<PathBuf> = Vec::new();
    for (ci, chunk) in names.chunks(CHUNK_FRAMES).enumerate() {
        let cin = work.join(format!("c{ci}_in"));
        let cout = work.join(format!("c{ci}_out"));
        std::fs::create_dir_all(&cin)
            .and_then(|_| std::fs::create_dir_all(&cout))
            .map_err(|e| e.to_string())?;
        for (i, n) in chunk.iter().enumerate() {
            // 連番を0始まりにそろえて、ffmpegの入力パターンを単純にする。
            std::fs::rename(frames_in.join(n), cin.join(format!("{i:08}.png")))
                .map_err(|e| e.to_string())?;
        }
        if let Err(e) = run_backend(&backend, &cin, &cout, up) {
            cleanup(&work);
            return Err(e);
        }
        let video = work.join(format!("chunk_{ci}.mkv"));
        let enc = resolve_tool("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-framerate",
                &fps.to_string(),
                "-i",
                &cout.join("%08d.png").to_string_lossy(),
            ])
            .args([
                "-c:v", "libx264", "-crf", "12", "-preset", "veryfast", "-pix_fmt", "yuv420p",
            ])
            .arg(&video)
            .output()
            .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"))?;
        if !enc.status.success() {
            cleanup(&work);
            return Err(format!(
                "中間動画の作成に失敗しました: {}",
                String::from_utf8_lossy(&enc.stderr)
            ));
        }
        let _ = std::fs::remove_dir_all(&cin);
        let _ = std::fs::remove_dir_all(&cout);
        chunk_videos.push(video);
    }

    // 3) 中間動画を結合し、元の音声(同じトリミング)を付ける。
    let list: String = chunk_videos
        .iter()
        .map(|p| {
            format!(
                "file '{}'\n",
                p.to_string_lossy()
                    .replace('\\', "/")
                    .replace('\'', "'\\''")
            )
        })
        .collect();
    let list_path = work.join("list.txt");
    std::fs::write(&list_path, list).map_err(|e| e.to_string())?;
    let mut mux = resolve_tool("ffmpeg");
    mux.args([
        "-v",
        "error",
        "-y",
        "-f",
        "concat",
        "-safe",
        "0",
        "-i",
        &list_path.to_string_lossy(),
    ]);
    if let Some(s) = start {
        mux.args(["-ss", &s.to_string()]);
    }
    if let Some(d) = dur {
        mux.args(["-t", &d.to_string()]);
    }
    mux.args([
        "-i",
        input,
        "-map",
        "0:v",
        "-map",
        "1:a?",
        "-c",
        "copy",
        "-shortest",
    ])
    .arg(mezzanine);
    let res = mux
        .output()
        .map_err(|e| format!("ffmpegの起動に失敗しました: {e}"));
    cleanup(&work);
    let res = res?;
    if !res.status.success() {
        return Err(format!(
            "中間ファイルの作成に失敗しました: {}",
            String::from_utf8_lossy(&res.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_models_and_scales() {
        let ok = |m: &str, s: u32| {
            validate(&AiUpscale {
                model: m.into(),
                scale: s,
                backend: "auto".into(),
            })
            .is_ok()
        };
        assert!(
            ok("realesr-animevideov3", 2)
                && ok("realesr-animevideov3", 4)
                && ok("realesrgan-x4plus", 4)
        );
        assert!(!ok("realesr-animevideov3", 5) && !ok("realesrgan-x4plus", 2) && !ok("unknown", 4));
    }

    #[test]
    fn asset_name_matches_this_os() {
        let name = asset_name().unwrap();
        assert!(name.starts_with("realesrgan-ncnn-vulkan-20220424-") && name.ends_with(".zip"));
    }

    /// GPUを使わずCPU版だけで、実クリップを本当にAI超解像して4倍になり音声も保持されることを検証する
    /// (Vulkan非対応のPC相当)。ネットワーク(初回のみ)とffmpegが無い環境ではスキップする。
    #[test]
    fn real_ai_upscale_quadruples_a_short_clip_on_the_cpu_backend() {
        if !Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }
        if let Err(e) = ensure_plugin() {
            eprintln!("Real-ESRGANのモデルを用意できないためスキップ: {e}");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_ai_cpu_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("src.mp4");
        let st = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=160x120:rate=3",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
                src.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(st.status.success());
        let mezz = tmp.join("mezz.mkv");
        let up = AiUpscale {
            model: "realesr-animevideov3".into(),
            scale: 4,
            backend: "cpu".into(),
        };
        make_upscaled_mezzanine(src.to_str().unwrap(), None, &up, &mezz)
            .expect("CPU AI upscaling should succeed");
        let o = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,width,height",
                "-of",
                "csv=p=0",
                mezz.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            text.contains("640,480"),
            "160x120が4倍の640x480になるはず(実際: {text})"
        );
        assert!(
            text.contains("audio"),
            "元の音声が保持されるはず(実際: {text})"
        );
    }

    #[test]
    fn x4plus_requires_the_gpu_backend() {
        let exe = std::path::PathBuf::from("dummy");
        let up = AiUpscale {
            model: "realesrgan-x4plus".into(),
            scale: 4,
            backend: "cpu".into(),
        };
        assert!(choose_backend(&exe, &up).is_err(), "x4plusはCPU版の対象外");
    }

    /// 実GPU・実プラグイン・実ffmpegで、短いクリップを本当にAI超解像して解像度が4倍になることを検証する。
    /// Vulkan対応GPU・ネットワーク(初回のみ約45MBのダウンロード)・ffmpegが無い環境ではスキップする。
    #[test]
    fn real_ai_upscale_quadruples_a_short_clip_on_the_gpu() {
        if !Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            eprintln!("ffmpegが無いためスキップ");
            return;
        }
        if let Err(e) = ensure_plugin() {
            eprintln!("Real-ESRGANプラグインを用意できないためスキップ: {e}");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("make_disk_ai_up_{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let src = tmp.join("src.mp4");
        let st = Command::new("ffmpeg")
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=160x120:rate=3",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                "-shortest",
                src.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(st.status.success());
        let mezz = tmp.join("mezz.mkv");
        let up = AiUpscale {
            model: "realesr-animevideov3".into(),
            scale: 4,
            backend: "gpu".into(),
        };
        let result = make_upscaled_mezzanine(src.to_str().unwrap(), None, &up, &mezz);
        if let Err(e) = &result {
            if e.contains("Vulkan") || e.contains("gpu") {
                eprintln!("Vulkan対応GPUが無いためスキップ: {e}");
                let _ = std::fs::remove_dir_all(&tmp);
                return;
            }
        }
        result.expect("AI upscaling should succeed on a Vulkan GPU");
        let o = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,width,height",
                "-of",
                "csv=p=0",
                mezz.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&o.stdout).to_string();
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            text.contains("640,480"),
            "160x120が4倍の640x480になるはず(実際: {text})"
        );
        assert!(
            text.contains("audio"),
            "元の音声が保持されるはず(実際: {text})"
        );
    }
}
