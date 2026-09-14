//! xorriso(libisofs/libburnベース)によるISOイメージ生成。
//! xorrisoはWindows/macOS/Linuxいずれにも移植されており、
//! 本アプリではOS判定を持ち込まず単一コマンド経路に統一する。

use crate::engine::sidecar::resolve_tool;

pub fn create_iso(source_dir: &str, output_iso: &str, volume_label: &str) -> Result<(), String> {
    let output = resolve_tool("xorriso")
        .args([
            "-as", "mkisofs",
            "-iso-level", "3",
            "-J", "-R",
            "-V", volume_label,
            "-o", output_iso,
            source_dir,
        ])
        .output()
        .map_err(|e| format!("xorrisoの起動に失敗しました(未インストールの可能性): {e}"))?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}
