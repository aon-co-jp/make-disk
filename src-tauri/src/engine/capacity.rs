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
///
/// 下限は設けていない。収録時間がディスク容量に対して極端に長い場合
/// (例: 10時間分を1枚のCD/DVDに収める等)でも、常に「収まる値」まで
/// ビットレートを下げて返す。品質面の目安は[`quality_warning`]で別途
/// 判定する。
pub fn max_bitrate_for_capacity(disc: DiscType, total_duration_secs: f64, reserved_bytes: u64) -> u64 {
    if total_duration_secs <= 0.0 {
        return 0;
    }
    let usable = disc.usable_bytes().saturating_sub(reserved_bytes);
    ((usable as f64 * 8.0) / total_duration_secs) as u64
}

/// 品質の目安として基準とするビットレート(bit/s)。
/// 音声はMP3の高品質帯(256kbps)、動画はSD〜HD相当の実用帯(6Mbps)を基準にする。
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Audio,
    Video,
}

impl MediaKind {
    fn reference_bps(self) -> f64 {
        match self {
            MediaKind::Audio => 256_000.0,
            MediaKind::Video => 6_000_000.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct QualityWarning {
    pub level: u8,
    pub message_ja: String,
    pub message_en: String,
}

use serde::Serialize;

/// 自動算出ビットレートが基準よりどれだけ低いかに応じて、
/// 4段階(下がります → 少し下がります → かなり下がります → 画質/音質が
/// 落ちます)で警告メッセージを返す。基準の85%以上ならNoneで問題なし。
pub fn quality_warning(bitrate_bps: u64, kind: MediaKind) -> Option<QualityWarning> {
    let reference = kind.reference_bps();
    let ratio = bitrate_bps as f64 / reference;

    let (level, ja, en): (u8, &str, &str) = if ratio >= 0.85 {
        return None;
    } else if ratio >= 0.7 {
        (1, "ビットレートが下がります。", "The bitrate will be reduced.")
    } else if ratio >= 0.4 {
        (2, "ビットレートが少し下がります。", "The bitrate will be reduced a bit further.")
    } else if ratio >= 0.15 {
        (3, "ビットレートがかなり下がります。", "The bitrate will be reduced considerably.")
    } else {
        match kind {
            MediaKind::Audio => (4, "音質が落ちます。", "Audio quality will noticeably drop."),
            MediaKind::Video => (4, "画質が落ちます。", "Picture quality will noticeably drop."),
        }
    };

    Some(QualityWarning {
        level,
        message_ja: ja.to_string(),
        message_en: en.to_string(),
    })
}
