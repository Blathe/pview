pub const DEFAULT_INTERVAL_MS: u64 = 1000;
pub const HISTORY_LEN: usize = 60;

/// Binary (1024-based) byte-to-MB divisor, used consistently everywhere a
/// byte count is converted to MB or GB so displayed units match Windows
/// tools (Task Manager, Explorer, Resource Monitor), which all compute this
/// way despite labeling it "MB"/"GB".
pub const BYTES_PER_MB: u64 = 1_048_576;
