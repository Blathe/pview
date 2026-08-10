use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use sysinfo::{Pid, ProcessStatus};

use crate::config::HISTORY_LEN;
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

    pub cpu_current: f32,
    pub cpu_peak: f32,
    pub mem_current_mb: u64,
    pub mem_peak_mb: u64,

    pub disk_read_rate_mb_s: f32,
    pub disk_write_rate_mb_s: f32,
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

            cpu_current: initial.cpu_usage,
            cpu_peak: initial.cpu_usage,
            mem_current_mb: initial.memory_bytes / 1_000_000,
            mem_peak_mb: initial.memory_bytes / 1_000_000,

            disk_read_rate_mb_s: 0.0,
            disk_write_rate_mb_s: 0.0,
            disk_read_bytes_session: 0,
            disk_write_bytes_session: 0,
            baseline_read_bytes: initial.disk_total_read_bytes,
            baseline_write_bytes: initial.disk_total_written_bytes,
            last_read_bytes: initial.disk_total_read_bytes,
            last_write_bytes: initial.disk_total_written_bytes,
            last_sample_at: Instant::now(),

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
            self.disk_read_rate_mb_s = read_delta as f32 / elapsed / 1_000_000.0;
            self.disk_write_rate_mb_s = write_delta as f32 / elapsed / 1_000_000.0;
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
        self.mem_current_mb = sample.memory_bytes / 1_000_000;
        self.mem_peak_mb = self.mem_peak_mb.max(self.mem_current_mb);

        if self.cpu_history.len() == HISTORY_LEN {
            self.cpu_history.pop_front();
        }
        self.cpu_history.push_back(self.cpu_current);

        if self.mem_history.len() == HISTORY_LEN {
            self.mem_history.pop_front();
        }
        self.mem_history.push_back(self.mem_current_mb);
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
