//! ディスク種別ごとの公称容量とビットレート自動計算。

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiscType {
    Cd700,
    Dvd47,
    DvdDl85,
    Bd25,
    Bd50,
}

impl DiscType {
    /// 公称容量(バイト)。書き込み時のファイルシステムオーバーヘッド分を
    /// 差し引いた実用値(公称値の約98%)を返す。
    pub fn usable_bytes(self) -> u64 {
        let nominal: u64 = match self {
            DiscType::Cd700 => 700 * 1024 * 1024,
            DiscType::Dvd47 => 4_700_000_000,
            DiscType::DvdDl85 => 8_500_000_000,
            DiscType::Bd25 => 25_000_000_000,
            DiscType::Bd50 => 50_000_000_000,
        };
        nominal * 98 / 100
    }
}

/// 収録したいメディアの合計尺(秒)から、指定ディスクに収まる
/// 最大平均ビットレート(bit/s)を算出する。
/// `reserved_bytes` はISO9660オーバーヘッド等の予約分。
pub fn max_bitrate_for_capacity(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> u64 {
    if total_duration_secs <= 0.0 {
        return 0;
    }
    let usable = disc.usable_bytes().saturating_sub(reserved_bytes);
    ((usable as f64 * 8.0) / total_duration_secs) as u64
}
