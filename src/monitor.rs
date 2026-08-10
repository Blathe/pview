use sysinfo::{Pid, ProcessStatus, ProcessesToUpdate, System};

pub struct Sample {
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub disk_total_read_bytes: u64,
    pub disk_total_written_bytes: u64,
    pub status: ProcessStatus,
    pub run_time_secs: u64,
}

pub enum MonitorError {
    ProcessExited,
}

pub struct Monitor {
    sys: System,
    pid: Pid,
}

impl Monitor {
    pub fn new(sys: System, pid: Pid) -> Self {
        Self { sys, pid }
    }

    pub fn sample(&mut self) -> Result<Sample, MonitorError> {
        self.sys
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);

        let process = self.sys.process(self.pid).ok_or(MonitorError::ProcessExited)?;
        let disk = process.disk_usage();

        Ok(Sample {
            cpu_usage: process.cpu_usage(),
            memory_bytes: process.memory(),
            disk_total_read_bytes: disk.total_read_bytes,
            disk_total_written_bytes: disk.total_written_bytes,
            status: process.status(),
            run_time_secs: process.run_time(),
        })
    }
}
