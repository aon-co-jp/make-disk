//! ディスク書き込み(xorriso -as cdrecord/growisofs 経由で統一)。
//! xorrisoはcdrtools(cdrecord)・cdrdao・libburn/growisofs相当の書き込み経路を
//! 内包しているため、CD/DVD/Blu-rayを単一コマンド体系で扱える。

use crate::engine::capacity::DiscType;
use crate::engine::sidecar::resolve_tool;

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

    let output = resolve_tool("xorriso")
        .args(&args)
        .output()
        .map_err(|e| format!("xorrisoの起動に失敗しました(未インストールの可能性): {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

/// 利用可能な光学ドライブの一覧(device文字列)。
pub fn list_devices() -> Result<Vec<String>, String> {
    let output = resolve_tool("xorriso")
        .args(["-devices"])
        .output()
        .map_err(|e| format!("xorrisoの起動に失敗しました: {e}"))?;

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
