use sysinfo::{Pid, System};

pub enum LookupResult {
    Found(Pid),
    NotFound,
    Ambiguous,
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

    let matches: Vec<Pid> = sys
        .processes()
        .values()
        .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(target))
        .map(|p| p.pid())
        .collect();

    match matches.len() {
        0 => LookupResult::NotFound,
        1 => LookupResult::Found(matches[0]),
        _ => LookupResult::Ambiguous,
    }
}
