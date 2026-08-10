use clap::Parser;

use crate::config::DEFAULT_INTERVAL_MS;

/// Single-process monitoring TUI.
#[derive(Parser, Debug)]
#[command(name = "pview", version, about = "Real-time TUI stats for a single process")]
pub struct Args {
    /// Process name (exact, case-insensitive) or PID to monitor.
    pub target: String,

    /// Refresh interval in milliseconds.
    #[arg(short, long, default_value_t = DEFAULT_INTERVAL_MS)]
    pub interval: u64,
}
