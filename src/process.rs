use sysinfo::{Pid, System};

pub struct ProcessMatch {
    pub pid: Pid,
    pub name: String,
    pub start_time: u64,
}

pub enum LookupResult {
    Found(Pid),
    NotFound,
    Ambiguous(Vec<ProcessMatch>),
}

/// Resolve a user-supplied target (a PID or an exact, case-insensitive process
/// name) to a single process. A bare integer is always treated as a PID.
pub fn resolve_target(sys: &System, target: &str) -> LookupResult {
    if let Ok(pid_num) = target.parse::<u32>() {
        let pid = Pid::from_u32(pid_num);
        return match sys.process(pid) {
            Some(_) => LookupResult::Found(pid),
            None => LookupResult::NotFound,
        };
    }

    let matches: Vec<ProcessMatch> = sys
        .processes()
        .values()
        .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(target))
        .map(|p| ProcessMatch {
            pid: p.pid(),
            name: p.name().to_string_lossy().into_owned(),
            start_time: p.start_time(),
        })
        .collect();

    match matches.len() {
        0 => LookupResult::NotFound,
        1 => LookupResult::Found(matches[0].pid),
        _ => LookupResult::Ambiguous(matches),
    }
}
