pub const DEFAULT_INTERVAL_MS: u64 = 1000;
pub const MIN_INTERVAL_MS: u64 = 250;
pub const MAX_INTERVAL_MS: u64 = 1000;
pub const HISTORY_LEN: usize = 60;

/// Spacing between samples in `App::mem_trend_samples`, the long-lived
/// buffer used for the "memory over 1h" trend badge. Sampled far less
/// often than the dashboard tick rate so a modest, fixed-size buffer can
/// cover a full hour.
pub const MEM_TREND_SAMPLE_INTERVAL_SECS: u64 = 60;

/// Time span covered by `App::mem_trend_samples`.
pub const MEM_TREND_WINDOW_SECS: u64 = 60 * 60;

/// Max samples kept in `App::mem_trend_samples`. Covering the full window
/// requires both endpoints, so a one-hour window sampled every minute needs
/// 61 samples: minute zero plus the following 60 minute boundaries.
pub const MEM_TREND_MAX_SAMPLES: usize =
    (MEM_TREND_WINDOW_SECS / MEM_TREND_SAMPLE_INTERVAL_SECS) as usize + 1;

/// Binary (1024-based) byte-to-MB divisor, used consistently everywhere a
/// byte count is converted to MB or GB so displayed units match Windows
/// tools (Task Manager, Explorer, Resource Monitor), which all compute this
/// way despite labeling it "MB"/"GB".
pub const BYTES_PER_MB: u64 = 1_048_576;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// How much headroom the CPU panel's "Relative" view gives itself above the
/// visible window's peak (e.g. 1.5 = axis tops out 50% above the peak),
/// so a current reading sitting at or near the recent peak still renders
/// with visible room above it instead of hugging the top of the chart.
pub const CPU_RELATIVE_AXIS_HEADROOM: f64 = 1.1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_trend_capacity_covers_full_window() {
        let covered_intervals = MEM_TREND_MAX_SAMPLES.saturating_sub(1) as u64;
        assert_eq!(
            covered_intervals * MEM_TREND_SAMPLE_INTERVAL_SECS,
            MEM_TREND_WINDOW_SECS
        );
    }
}
