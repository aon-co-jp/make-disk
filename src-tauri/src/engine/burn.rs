//! ディスク書き込み(xorriso -as cdrecord/growisofs 経由で統一)。
//! xorrisoはcdrtools(cdrecord)・cdrdao・libburn/growisofs相当の書き込み経路を
//! 内包しているため、CD/DVD/Blu-rayを単一コマンド体系で扱える。
//!
//! ## rs-xorrisoについて(2026-09-17)
//!
//! `engine::iso`のISO生成とは異なり、実際のディスクへの書き込み
//! (`-as cdrecord`)・ドライブ列挙(`-devices`)はrs-xorriso側が
//! 明示的に「未実装、本家xorrisoを使ってください」という分かりやすい
//! エラーを返す設計になっている(ISO生成のみ対応、正直な開示)。
//! そのため本家xorrisoが無い環境でここへフォールバックしても実際の
//! 書き込みは行えないが、OSレベルの生の「program not found」より
//! ずっと分かりやすいエラーメッセージになる。

use crate::engine::capacity::DiscType;
use crate::engine::sidecar::resolve_tool;

fn run_xorriso(args: &[String]) -> Result<std::process::Output, String> {
    match resolve_tool("xorriso").args(args).output() {
        Ok(output) => Ok(output),
        Err(_) => resolve_tool("rs-xorriso")
            .args(args)
            .output()
            .map_err(|e| format!("xorriso・rs-xorrisoともに起動に失敗しました(いずれも未インストール/未同梱の可能性): {e}")),
    }
}

/// 書き込み速度。`Auto`は指定を省略し、ドライブ・メディアの自動判定に委ねる。
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteSpeed {
    Auto,
    Max,
    Fixed(u32),
}

pub fn burn_image(image_path: &str, device: &str, disc: DiscType, speed: WriteSpeed) -> Result<(), String> {
    let mut args: Vec<String> = vec!["-as".into(), "cdrecord".into()];

    match speed {
        WriteSpeed::Auto => {}
        WriteSpeed::Max => args.push("speed=0".into()),
        WriteSpeed::Fixed(v) => args.push(format!("speed={v}")),
    }

    args.push(format!("dev={device}"));

    match disc {
        DiscType::Cd700 => {
            args.push("-v".into());
            args.push("-data".into());
        }
        DiscType::Dvd47 | DiscType::DvdDl85 | DiscType::Bd25 | DiscType::Bd50 | DiscType::Bd128 => {
            args.push("-dao".into());
        }
    }

    args.push(image_path.to_string());

    let output = run_xorriso(&args)?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

/// 利用可能な光学ドライブの一覧(device文字列)。
pub fn list_devices() -> Result<Vec<String>, String> {
    let output = run_xorriso(&["-devices".to_string()])?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut devices = Vec::new();
    for line in text.lines() {
        if let Some(idx) = line.find("dev='") {
            if let Some(end) = line[idx + 5..].find('\'') {
                devices.push(line[idx + 5..idx + 5 + end].to_string());
            }
        }
    }
    Ok(devices)
}
