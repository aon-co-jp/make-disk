mod engine;

use engine::burn::{self, WriteSpeed};
use engine::capacity::{self, DiscType, MediaKind, QualityWarning};
use engine::convert::{self, ConvertJob};
use engine::cpu::{self, CpuEncodeEstimate};
use engine::iso;
use engine::pdf::{self, BindingDirection};
use engine::probe;

#[tauri::command]
fn probe_media(path: String) -> Result<probe::MediaInfo, String> {
    probe::probe(&path)
}

#[tauri::command]
fn convert_media(job: ConvertJob) -> Result<(), String> {
    convert::run_convert(&job)
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
    let builder = tauri::Builder::default()
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
            pick_output_tree,
        ]);

    #[cfg(not(target_os = "android"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        probe_media,
        convert_media,
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
    ]);

    builder.run(tauri::generate_context!()).expect("error while running tauri application");
}
