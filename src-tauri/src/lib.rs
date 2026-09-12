mod engine;

use engine::burn::{self, WriteSpeed};
use engine::capacity::{self, DiscType, MediaKind, QualityWarning};
use engine::convert::{self, ConvertJob};
use engine::iso;
use engine::probe;

#[tauri::command]
fn probe_media(path: String) -> Result<probe::MediaInfo, String> {
    probe::probe(&path)
}

#[tauri::command]
fn convert_media(job: ConvertJob) -> Result<(), String> {
    convert::run_convert(&job)
}

#[tauri::command]
fn calc_auto_bitrate_kbps(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> u64 {
    capacity::max_bitrate_for_capacity(disc, total_duration_secs, reserved_bytes) / 1000
}

#[tauri::command]
fn check_bitrate_quality(bitrate_kbps: u64, kind: MediaKind) -> Option<QualityWarning> {
    capacity::quality_warning(bitrate_kbps * 1000, kind)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            probe_media,
            convert_media,
            calc_auto_bitrate_kbps,
            check_bitrate_quality,
            create_iso,
            burn_image,
            list_burn_devices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
