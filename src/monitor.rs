use std::path::Path;

use sysinfo::{Disks, Pid, ProcessStatus, ProcessesToUpdate, System};

pub struct Sample {
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub disk_total_read_bytes: u64,
    pub disk_total_written_bytes: u64,
    pub status: ProcessStatus,
    pub run_time_secs: u64,
    pub storage_total_bytes: u64,
    pub storage_available_bytes: u64,
    pub storage_mount_point: String,
}

pub enum MonitorError {
    ProcessExited,
}

pub struct Monitor {
    sys: System,
    pid: Pid,
    disks: Disks,
    storage_disk_index: Option<usize>,
}

impl Monitor {
    pub fn new(sys: System, pid: Pid, exe_path: &Path) -> Self {
        let disks = Disks::new_with_refreshed_list();
        let storage_disk_index = best_matching_disk(&disks, exe_path);
        Self {
            sys,
            pid,
            disks,
            storage_disk_index,
        }
    }

    pub fn sample(&mut self) -> Result<Sample, MonitorError> {
        self.sys
            .refresh_processes(ProcessesToUpdate::Some(&[self.pid]), true);

        let process = self
            .sys
            .process(self.pid)
            .ok_or(MonitorError::ProcessExited)?;
        let disk = process.disk_usage();

        let (storage_total_bytes, storage_available_bytes, storage_mount_point) = match self
            .storage_disk_index
            .and_then(|i| self.disks.list_mut().get_mut(i))
        {
            Some(storage_disk) => {
                storage_disk.refresh();
                (
                    storage_disk.total_space(),
                    storage_disk.available_space(),
                    storage_disk.mount_point().display().to_string(),
                )
            }
            None => (0, 0, "?".to_string()),
        };

        Ok(Sample {
            cpu_usage: process.cpu_usage(),
            memory_bytes: process.memory(),
            disk_total_read_bytes: disk.total_read_bytes,
            disk_total_written_bytes: disk.total_written_bytes,
            status: process.status(),
            run_time_secs: process.run_time(),
            storage_total_bytes,
            storage_available_bytes,
            storage_mount_point,
        })
    }
}

/// Finds the disk whose mount point is the longest prefix of `exe_path` —
/// i.e. the most specific mount containing the executable — since Windows
/// can report multiple disks (e.g. `C:\` and a deeper mounted volume) that
/// are all technically ancestors of the path.
fn best_matching_disk(disks: &Disks, exe_path: &Path) -> Option<usize> {
    disks
        .list()
        .iter()
        .enumerate()
        .filter(|(_, disk)| exe_path.starts_with(disk.mount_point()))
        .max_by_key(|(_, disk)| disk.mount_point().as_os_str().len())
        .map(|(i, _)| i)
}
