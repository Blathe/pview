use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use sysinfo::{Pid, ProcessStatus};

use crate::config::{
    BYTES_PER_MB, HISTORY_LEN, MEM_TREND_MAX_SAMPLES, MEM_TREND_SAMPLE_INTERVAL_SECS,
};
use crate::monitor::Sample;

pub struct App {
    pub pid: Pid,
    pub process_name: String,
    pub exe_path: String,
    pub started_at_unix_secs: u64,
    pub mem_total_mb: u64,
    pub tick_interval: Duration,

    pub status: ProcessStatus,
    pub run_time_secs: u64,

    pub cpu_history: VecDeque<f32>,
    pub mem_history: VecDeque<u64>, // MB

    // Long-lived, coarsely-sampled memory history (independent of
    // `mem_history`'s short display window) used to compute the
    // "memory over 1h" trend badge.
    mem_trend_samples: VecDeque<(Instant, u64)>,
    last_mem_trend_sample_at: Instant,

    pub cpu_current: f32,
    pub cpu_peak: f32,
    pub mem_current_mb: u64,
    pub mem_peak_mb: u64,

    pub storage_total_mb: u64,
    pub storage_used_mb: u64,
    pub storage_mount_point: String,

    pub disk_read_rate_mb_s: f32,
    pub disk_write_rate_mb_s: f32,
    pub disk_read_rate_peak_mb_s: f32,
    pub disk_write_rate_peak_mb_s: f32,
    pub disk_read_bytes_session: u64,
    pub disk_write_bytes_session: u64,
    baseline_read_bytes: u64,
    baseline_write_bytes: u64,
    last_read_bytes: u64,
    last_write_bytes: u64,
    last_sample_at: Instant,

    pub paused: bool,
    pub should_quit: bool,
    pub exited: bool,
}

impl App {
    pub fn new(
        pid: Pid,
        process_name: String,
        exe_path: String,
        started_at_unix_secs: u64,
        mem_total_mb: u64,
        tick_interval: Duration,
        initial: Sample,
    ) -> Self {
        let now = Instant::now();
        Self {
            pid,
            process_name,
            exe_path,
            started_at_unix_secs,
            mem_total_mb,
            tick_interval,

            status: initial.status,
            run_time_secs: initial.run_time_secs,

            cpu_history: VecDeque::with_capacity(HISTORY_LEN),
            mem_history: VecDeque::with_capacity(HISTORY_LEN),

            mem_trend_samples: VecDeque::from([(now, initial.memory_bytes / BYTES_PER_MB)]),
            last_mem_trend_sample_at: now,

            cpu_current: initial.cpu_usage,
            cpu_peak: initial.cpu_usage,
            mem_current_mb: initial.memory_bytes / BYTES_PER_MB,
            mem_peak_mb: initial.memory_bytes / BYTES_PER_MB,

            storage_total_mb: initial.storage_total_bytes / BYTES_PER_MB,
            storage_used_mb: initial
                .storage_total_bytes
                .saturating_sub(initial.storage_available_bytes)
                / BYTES_PER_MB,
            storage_mount_point: initial.storage_mount_point,

            disk_read_rate_mb_s: 0.0,
            disk_write_rate_mb_s: 0.0,
            disk_read_rate_peak_mb_s: 0.0,
            disk_write_rate_peak_mb_s: 0.0,
            disk_read_bytes_session: 0,
            disk_write_bytes_session: 0,
            baseline_read_bytes: initial.disk_total_read_bytes,
            baseline_write_bytes: initial.disk_total_written_bytes,
            last_read_bytes: initial.disk_total_read_bytes,
            last_write_bytes: initial.disk_total_written_bytes,
            last_sample_at: now,

            paused: false,
            should_quit: false,
            exited: false,
        }
    }

    pub fn update(&mut self, sample: Sample) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample_at).as_secs_f32();

        if elapsed > 0.0 {
            let read_delta = sample
                .disk_total_read_bytes
                .saturating_sub(self.last_read_bytes);
            let write_delta = sample
                .disk_total_written_bytes
                .saturating_sub(self.last_write_bytes);
            self.disk_read_rate_mb_s = read_delta as f32 / elapsed / BYTES_PER_MB as f32;
            self.disk_write_rate_mb_s = write_delta as f32 / elapsed / BYTES_PER_MB as f32;
            self.disk_read_rate_peak_mb_s =
                self.disk_read_rate_peak_mb_s.max(self.disk_read_rate_mb_s);
            self.disk_write_rate_peak_mb_s = self
                .disk_write_rate_peak_mb_s
                .max(self.disk_write_rate_mb_s);
        }

        self.last_read_bytes = sample.disk_total_read_bytes;
        self.last_write_bytes = sample.disk_total_written_bytes;
        self.last_sample_at = now;

        self.disk_read_bytes_session = sample
            .disk_total_read_bytes
            .saturating_sub(self.baseline_read_bytes);
        self.disk_write_bytes_session = sample
            .disk_total_written_bytes
            .saturating_sub(self.baseline_write_bytes);

        self.status = sample.status;
        self.run_time_secs = sample.run_time_secs;

        self.cpu_current = sample.cpu_usage;
        self.cpu_peak = self.cpu_peak.max(self.cpu_current);
        self.mem_current_mb = sample.memory_bytes / BYTES_PER_MB;
        self.mem_peak_mb = self.mem_peak_mb.max(self.mem_current_mb);

        self.storage_total_mb = sample.storage_total_bytes / BYTES_PER_MB;
        self.storage_used_mb = sample
            .storage_total_bytes
            .saturating_sub(sample.storage_available_bytes)
            / BYTES_PER_MB;
        self.storage_mount_point = sample.storage_mount_point;

        if self.cpu_history.len() == HISTORY_LEN {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(self.cpu_current);

        if self.mem_history.len() == HISTORY_LEN {
            self.mem_history.pop_front();
        }
        self.mem_history.push_back(self.mem_current_mb);

        if now.duration_since(self.last_mem_trend_sample_at)
            >= Duration::from_secs(MEM_TREND_SAMPLE_INTERVAL_SECS)
        {
            if self.mem_trend_samples.len() == MEM_TREND_MAX_SAMPLES {
                self.mem_trend_samples.pop_front();
            }
            self.mem_trend_samples.push_back((now, self.mem_current_mb));
            self.last_mem_trend_sample_at = now;
        }
    }

    /// Memory trend rate in MB/hr computed from the oldest and newest
    /// entries in `mem_trend_samples`, plus the elapsed span that rate was
    /// computed over. `None` until at least one sampling interval
    /// (`MEM_TREND_SAMPLE_INTERVAL_SECS`) has elapsed, since a single
    /// sample can't yet describe a trend.
    pub fn mem_trend_mb_per_hr(&self) -> Option<(f32, Duration)> {
        let oldest = self.mem_trend_samples.front()?;
        let newest = self.mem_trend_samples.back()?;
        let elapsed = newest.0.duration_since(oldest.0);
        if elapsed.as_secs() < MEM_TREND_SAMPLE_INTERVAL_SECS {
            return None;
        }
        let delta_mb = newest.1 as f32 - oldest.1 as f32;
        let rate = delta_mb / elapsed.as_secs_f32() * 3600.0;
        Some((rate, elapsed))
    }

    pub fn handle_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('p') => self.paused = !self.paused,
            KeyCode::Char('r') => {
                self.cpu_history.clear();
                self.mem_history.clear();
            }
            _ => {}
        }
    }
}
