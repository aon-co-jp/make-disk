//! `open-cpu`(エコシステム共通のCPU命令セット検出ライブラリ)を使い、
//! GPUハードウェアエンコーダが見つからない場合のCPUフォールバック
//! (libx264)がどの程度実用的な速度になりそうかをUIへ伝える。
//!
//! 実際のffmpeg/libx264の内部SIMDディスパッチには関与しない
//! (libx264自身が実行時に最適な命令セットを自動選択するため、
//! その挙動を変えることはできないし変える必要も無い)。ここでの役割は
//! あくまで「フレーム精度カットでGPUが無い場合、この見積もりで進めて
//! よいか」をユーザーに判断してもらうための参考情報の提示。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CpuEncodeEstimate {
    pub avx2: bool,
    pub avx512: bool,
    /// "fast" | "moderate" | "slow"(参考値、実測ではない)。
    pub speed_hint: &'static str,
    pub message_ja: String,
    pub message_en: String,
}

pub fn estimate_cpu_encode_speed() -> CpuEncodeEstimate {
    let caps = open_cpu::detect();
    let avx512 = caps.has_all(&[caps.avx512f]);
    let avx2 = caps.has_all(&[caps.avx2]);

    let (speed_hint, ja, en) = if avx512 {
        (
            "fast",
            "AVX-512対応CPUです。libx264が自動的にAVX-512を使うため、GPU無しでも比較的高速です。",
            "AVX-512-capable CPU detected. libx264 will automatically use it, so CPU-only encoding should be reasonably fast.",
        )
    } else if avx2 {
        (
            "moderate",
            "AVX2対応CPUです。libx264が自動的にAVX2を使いますが、GPUエンコーダに比べると時間はかかります。",
            "AVX2-capable CPU detected. libx264 will automatically use it, but it will still be slower than a GPU encoder.",
        )
    } else {
        (
            "slow",
            "AVX2非対応の古いCPUです。フレーム精度カットのCPUエンコードは数時間規模の動画では非常に時間がかかる可能性があります。",
            "This CPU lacks AVX2. CPU-only frame-accurate encoding may take a very long time for multi-hour footage.",
        )
    };

    CpuEncodeEstimate { avx2, avx512, speed_hint, message_ja: ja.to_string(), message_en: en.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_a_valid_speed_hint_on_this_machine() {
        let estimate = estimate_cpu_encode_speed();
        assert!(["fast", "moderate", "slow"].contains(&estimate.speed_hint));
        // avx512はavx2の上位互換なので、avx512ならavx2も真であるはず。
        if estimate.avx512 {
            assert!(estimate.avx2);
        }
    }
}
