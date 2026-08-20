use clap::Parser;

use crate::config::{DEFAULT_INTERVAL_MS, MAX_INTERVAL_MS, MIN_INTERVAL_MS};

/// Single-process monitoring TUI.
#[derive(Parser, Debug)]
#[command(
    name = "pview",
    version,
    about = "Real-time TUI stats for a single process"
)]
pub struct Args {
    /// Process name (exact, case-insensitive) or PID to monitor. If omitted,
    /// an interactive picker is shown to search running processes.
    pub target: Option<String>,

    /// Refresh interval in milliseconds.
    #[arg(
        short,
        long,
        default_value_t = DEFAULT_INTERVAL_MS,
        value_parser = parse_interval_ms
    )]
    pub interval: u64,
}

fn parse_interval_ms(value: &str) -> Result<u64, String> {
    let interval = value.parse::<u64>().map_err(|_| {
        format!("enter a whole number between {MIN_INTERVAL_MS} and {MAX_INTERVAL_MS} milliseconds")
    })?;

    if (MIN_INTERVAL_MS..=MAX_INTERVAL_MS).contains(&interval) {
        Ok(interval)
    } else {
        Err(format!(
            "enter a number between {MIN_INTERVAL_MS} and {MAX_INTERVAL_MS} milliseconds"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_interval_boundaries() {
        for interval in [MIN_INTERVAL_MS, MAX_INTERVAL_MS] {
            let args = Args::try_parse_from(["pview", "--interval", &interval.to_string()])
                .expect("interval boundary should be accepted");
            assert_eq!(args.interval, interval);
        }
    }

    #[test]
    fn rejects_intervals_outside_supported_range() {
        for interval in [MIN_INTERVAL_MS - 1, MAX_INTERVAL_MS + 1] {
            let error = Args::try_parse_from(["pview", "--interval", &interval.to_string()])
                .expect_err("out-of-range interval should be rejected");
            assert!(
                error
                    .to_string()
                    .contains("enter a number between 250 and 1000 milliseconds")
            );
        }
    }
}
