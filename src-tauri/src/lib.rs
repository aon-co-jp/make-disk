mod engine;
mod progress;

use engine::burn::{self, WriteSpeed};
use engine::capacity::{self, DiscType, MediaKind, QualityWarning};
use engine::convert::{self, ConvertJob, TrimRange as ConvertTrimRange};
use engine::cpu::{self, CpuEncodeEstimate};
use engine::iso;
use engine::pdf::{self, BindingDirection};
use engine::probe;
use tauri::Emitter;

/// 欲しい部分をAIが探して切り出し範囲を提案する(時間がかかり得るため別スレッドで実行し、画面を固めない)。
#[tauri::command]
async fn ai_suggest_range(path: String, request: String, length_secs: f64, llm_url: String) -> Result<engine::ai_range::Suggestion, String> {
    tauri::async_runtime::spawn_blocking(move || engine::ai_range::suggest_range(&path, &request, length_secs, &llm_url))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn probe_media(path: String) -> Result<probe::MediaInfo, String> {
    probe::probe(&path)
}

/// 変換を実行する。数時間かかり得る(AI超解像など)ので、別スレッドで動かして画面を固めない。
#[tauri::command]
async fn convert_media(job: ConvertJob) -> Result<(), String> {
    if job.ai_upscale.is_some() {
        progress::clear_cancel();
    }
    tauri::async_runtime::spawn_blocking(move || convert::run_convert(&job)).await.map_err(|e| e.to_string())?
}

/// 実行中の長い処理(AI超解像など)を中止する。途中経過は残り、同じ設定でもう一度実行すると続きから再開する。
#[tauri::command]
fn cancel_conversion() {
    progress::request_cancel();
}

/// このPCのCPU・GPUの速さを測って、AI処理に使う装置を自動で選ぶ(初回は1〜数分かかる)。`force`で再測定。
#[tauri::command]
async fn ai_hw_benchmark(force: bool) -> Result<engine::hw_bench::HwBench, String> {
    tauri::async_runtime::spawn_blocking(move || {
        engine::hw_bench::benchmark(force, &|m| progress::emit("ai-progress", serde_json::json!({ "stage": "info", "message": m })))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 保存済みの測定結果(無ければnull)。
#[tauri::command]
fn ai_hw_status() -> Option<engine::hw_bench::HwBench> {
    engine::hw_bench::cached()
}

/// 「選んだディスクに、この解像度・fpsで収まるか」の予測(フルHD/4K × 元のfps/60/120)。静止・単色コマの割合も抜き取りで推定する。
#[tauri::command]
async fn ai_fit_predict(path: String, start_secs: Option<f64>, duration_secs: Option<f64>, disc: DiscType, audio_kbps: f64, src_fps: f64) -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let probe = engine::probe::probe(&path)?;
        let dur = duration_secs.unwrap_or((probe.duration_secs - start_secs.unwrap_or(0.0)).max(0.0));
        let static_fraction = engine::fit_predict::estimate_static_fraction(&path, start_secs, duration_secs);
        let mut fps_list = vec![src_fps];
        for f in [60.0, 120.0] {
            if f >= src_fps * 1.5 {
                fps_list.push(f);
            }
        }
        let mut rows = Vec::new();
        for (w, h) in [(1920u32, 1080u32), (3840, 2160)] {
            for &fps in &fps_list {
                let r = engine::fit_predict::predict(disc, dur, audio_kbps, static_fraction.unwrap_or(0.0), &engine::fit_predict::FitOption { width: w, height: h, fps });
                rows.push(r);
            }
        }
        Ok(serde_json::json!({ "duration_secs": dur, "static_fraction": static_fraction, "rows": rows }))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// AI超解像の下調べ: 映像の種類(インターレース・黒帯)、コマ数、このPCでの所要時間、作業用の空き容量。
#[tauri::command]
async fn ai_estimate(path: String, start_secs: Option<f64>, duration_secs: Option<f64>, options: engine::ai_upscale::AiUpscale, target_w: Option<u32>, target_h: Option<u32>, work_dir: String) -> Result<engine::ai_video::Estimate, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = target_w.zip(target_h);
        engine::ai_video::estimate(&path, start_secs, duration_secs, &options, target, std::path::Path::new(&work_dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 「AI判断で自動カット」モード(2026-09-16新設)向け: 無音区間を検出する。
/// **正直な開示**: 実際にはffmpegの音量ベースの無音検出であり、LLM/
/// 画像認識等の意味的なAI判断ではない(`convert::detect_silence_ranges`
/// のdocコメント参照)。
#[tauri::command]
fn detect_silence_ranges(path: String, silence_threshold_db: f64, min_silence_secs: f64) -> Result<Vec<convert::SilenceRange>, String> {
    convert::detect_silence_ranges(&path, silence_threshold_db, min_silence_secs)
}

/// 「サイズ指定」モード(2026-09-16新設)向け: 目標ファイルサイズと
/// 総尺から平均ビットレート(kbps)を算出する。
#[tauri::command]
fn calc_bitrate_for_target_size_kbps(target_bytes: u64, total_duration_secs: f64) -> u64 {
    convert::bitrate_for_target_size_kbps(target_bytes, total_duration_secs)
}

#[tauri::command]
fn calc_auto_bitrate_kbps(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> u64 {
    capacity::max_bitrate_for_capacity(disc, total_duration_secs, reserved_bytes) / 1000
}

#[tauri::command]
fn check_bitrate_quality(bitrate_kbps: u64, kind: MediaKind) -> Option<QualityWarning> {
    capacity::quality_warning(bitrate_kbps * 1000, kind)
}

/// 「最高音質」モード(2026-09-16新設)向け:CD品質ロスレスWAVで
/// 指定ディスクに収まるかどうか、収まらない場合は代わりに何秒までなら
/// 収まるかを算出する。
#[tauri::command]
fn estimate_lossless_audio_fit(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> capacity::LosslessFitEstimate {
    capacity::estimate_lossless_audio_fit(disc, total_duration_secs, reserved_bytes)
}

/// PDF見開き対応(2026-09-16新設): PDFを見開き画像群に変換して
/// `output_dir`へ出力する。`pdf::render_pdf_as_spreads`のdocコメントに
/// 記載の通り、現時点ではpoppler-utils(`pdftoppm`/`pdfinfo`)が
/// 実行環境のPATHに存在する必要がある(まだsidecar同梱は未対応)。
#[tauri::command]
fn convert_pdf_to_spreads(pdf_path: String, output_dir: String, binding: BindingDirection, max_dimension: u32) -> Result<Vec<String>, String> {
    // ユーザー指示「最大4Kの見開きPDF対応」通り、4Kを超える指定は常に4Kへ丸める。
    let clamped = max_dimension.min(pdf::MAX_4K_DIMENSION);
    pdf::render_pdf_as_spreads(&pdf_path, &output_dir, binding, clamped)
}

/// DSD出力のおおよそのファイルサイズ(バイト)。UIで容量超過を事前に警告するために使う。
#[tauri::command]
fn estimate_dsd_size(multiplier: u32, channels: u32, duration_secs: f64) -> Result<u64, String> {
    engine::dsd::estimate_dsd_size_bytes(multiplier, channels, duration_secs)
}

/// AI超解像のCPU版が使う計算カーネル(`AVX2+FMA`または`scalar`、open-cpuの検出結果)。UIの実行環境表示用。
#[tauri::command]
fn ai_upscale_cpu_kernel() -> String {
    engine::cpu_sr::kernel_name().to_string()
}

/// 画像1枚をAI超解像する(GPUがあればGPU、無ければCPU版)。初回のみプラグイン(約45MB)をダウンロードする。
#[tauri::command]
fn ai_upscale_image(input: String, output: String, model: String, scale: u32, backend: Option<String>) -> Result<(), String> {
    engine::ai_upscale::upscale_image(&input, &output, &engine::ai_upscale::AiUpscale { model, scale, backend: backend.unwrap_or_else(|| "auto".to_string()), ..Default::default() })
}

/// フォルダ内の全ファイルの合計サイズ(バイト)。ISO化・書き込みの前にディスク容量へ収まるか確かめるために使う。
#[tauri::command]
fn folder_size_bytes(path: String) -> Result<u64, String> {
    fn walk(dir: &std::path::Path) -> std::io::Result<u64> {
        let mut total = 0;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            total += if meta.is_dir() { walk(&entry.path())? } else { meta.len() };
        }
        Ok(total)
    }
    walk(std::path::Path::new(&path)).map_err(|e| format!("フォルダのサイズを調べられません: {e}"))
}

/// ディスク種別の実用容量(バイト、公称の約98%)。
#[tauri::command]
fn disc_usable_bytes(disc: DiscType) -> u64 {
    disc.usable_bytes()
}

/// 複数の音声/動画ファイルを結合(合成)する(2026-09-16新設)。
#[tauri::command]
fn concat_media_files(input_paths: Vec<String>, output_path: String, has_video: bool) -> Result<(), String> {
    convert::concat_media(&input_paths, &output_path, has_video)
}

/// 等間隔分割(2026-09-16新設): 総尺を指定個数の等しい区間に分割する。
#[tauri::command]
fn calc_equal_interval_segments(total_secs: f64, segment_count: u32) -> Vec<ConvertTrimRange> {
    convert::equal_interval_segments(total_secs, segment_count)
}

/// サイズ指定分割(2026-09-16新設): 指定した長さごとに区切り、
/// 割り切れない最後の「あまり」は短い区間としてそのまま返す
/// (呼び出し側〈フロントエンド〉で、あまりの区間だけディスク容量
/// いっぱいにビットレートを自動調整する想定)。
#[tauri::command]
fn calc_fixed_length_segments(total_secs: f64, segment_secs: f64) -> Vec<ConvertTrimRange> {
    convert::fixed_length_segments(total_secs, segment_secs)
}

/// PDFの綴じ方向一括変換(2026-09-16新設): 複数のPDFのページ順序を
/// まとめて反転し、`output_dir`へ`<元のファイル名>-rebind.pdf`として保存する。
/// 失敗したファイルはエラーメッセージ付きで結果に含め、他のファイルの
/// 処理は継続する(1件の失敗で全体を止めない)。
#[tauri::command]
fn rebind_pdfs(pdf_paths: Vec<String>, output_dir: String) -> Vec<Result<String, String>> {
    pdf_paths
        .into_iter()
        .map(|input_path| {
            let stem = std::path::Path::new(&input_path).file_stem().and_then(|s| s.to_str()).unwrap_or("output").to_string();
            let output_path = format!("{output_dir}/{stem}-rebind.pdf");
            pdf::reverse_pdf_page_order(&input_path, &output_path).map(|_| output_path)
        })
        .collect()
}

#[tauri::command]
fn create_iso(source_dir: String, output_iso: String, volume_label: String) -> Result<(), String> {
    iso::create_iso(&source_dir, &output_iso, &volume_label)
}

#[tauri::command]
fn burn_image(image_path: String, device: String, disc: DiscType, speed: WriteSpeed) -> Result<(), String> {
    burn::burn_image(&image_path, &device, disc, speed)
}

#[tauri::command]
fn list_burn_devices() -> Result<Vec<String>, String> {
    burn::list_devices()
}

/// rs-ffmpeg/rs-xorrisoプラグインの状態(版・同期結果)を返す。同じ版なら再コピーしない。
#[tauri::command]
fn list_plugins() -> Vec<engine::plugins::PluginStatus> {
    engine::plugins::sync_bundled_plugins()
}

/// 音楽CD(CD-DA)のトラック一覧(Windowsのみ)。
#[tauri::command]
fn list_cd_tracks(drive: String) -> Result<Vec<engine::cdda::TrackInfo>, String> {
    engine::cdda::list_tracks(&drive)
}

/// 音楽CDのトラックをWAV(16bit/44.1kHz)として取り込む。`secure`なら各区間を2回読んで一致確認する。
#[tauri::command]
fn rip_cd_tracks(drive: String, tracks: Vec<u8>, output_dir: String, secure: bool) -> Result<Vec<String>, String> {
    engine::cdda::rip_tracks(&drive, &tracks, std::path::Path::new(&output_dir), secure)
}

#[tauri::command]
fn estimate_cpu_encode_speed() -> CpuEncodeEstimate {
    cpu::estimate_cpu_encode_speed()
}

#[cfg(target_os = "android")]
#[tauri::command]
async fn pick_output_tree(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_android_folder::AndroidFolderExt;
    app.android_folder().pick_output_tree().map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 同梱のrs-*プラグインをプラグインフォルダへ同期する(同じ版ならスキップ)。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let _ = engine::plugins::sync_bundled_plugins();

    let builder = tauri::Builder::default()
        .setup(|app| {
            // 長い処理の進捗を画面へ届ける出口(エンジンはAppHandleを知らなくてよい)。
            let handle = app.handle().clone();
            progress::set_sink(Box::new(move |kind, payload| {
                let _ = handle.emit("make-disk-progress", serde_json::json!({ "kind": kind, "payload": payload }));
            }));
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init());

    // 自動アップデート確認(2026-09-16新設、デスクトップのみ——
    // `tauri-plugin-updater`はモバイル未対応、Android/iOSはストア/APK
    // サイドロードでの更新が前提のため元々対象外)。
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build()).plugin(tauri_plugin_process::init());

    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(tauri_plugin_android_folder::init())
        .invoke_handler(tauri::generate_handler![
            probe_media,
            convert_media,
            cancel_conversion,
            ai_hw_benchmark,
            ai_hw_status,
            ai_estimate,
            ai_fit_predict,
            calc_auto_bitrate_kbps,
            check_bitrate_quality,
            create_iso,
            burn_image,
            list_burn_devices,
            estimate_cpu_encode_speed,
            estimate_lossless_audio_fit,
            detect_silence_ranges,
            calc_bitrate_for_target_size_kbps,
            convert_pdf_to_spreads,
            rebind_pdfs,
            estimate_dsd_size,
            list_plugins,
            ai_upscale_image,
            ai_upscale_cpu_kernel,
            folder_size_bytes,
            disc_usable_bytes,
            concat_media_files,
            calc_equal_interval_segments,
            calc_fixed_length_segments,
            list_cd_tracks,
            rip_cd_tracks,
            ai_suggest_range,
            pick_output_tree,
        ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        probe_media,
        convert_media,
        cancel_conversion,
        ai_hw_benchmark,
        ai_hw_status,
        ai_estimate,
        ai_fit_predict,
        calc_auto_bitrate_kbps,
        check_bitrate_quality,
        create_iso,
        burn_image,
        list_burn_devices,
        estimate_cpu_encode_speed,
        estimate_lossless_audio_fit,
        detect_silence_ranges,
        calc_bitrate_for_target_size_kbps,
        convert_pdf_to_spreads,
        rebind_pdfs,
        estimate_dsd_size,
        list_plugins,
        ai_upscale_image,
        ai_upscale_cpu_kernel,
        folder_size_bytes,
        disc_usable_bytes,
        concat_media_files,
        calc_equal_interval_segments,
        calc_fixed_length_segments,
        list_cd_tracks,
        rip_cd_tracks,
        ai_suggest_range,
    ]);

    builder.run(tauri::generate_context!()).expect("error while running tauri application");
}
