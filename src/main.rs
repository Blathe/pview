mod app;
mod cli;
mod config;
mod monitor;
mod picker;
mod process;
mod ui;

use std::io::{self, Stdout};
use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use sysinfo::{Pid, ProcessesToUpdate, System};

use app::App;
use cli::Args;
use monitor::{Monitor, MonitorError};
use process::LookupResult;

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut sys = System::new_all();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let tick_interval = Duration::from_millis(args.interval);

    // Terminal setup is deliberately lazy: it only happens once we know
    // we're either showing the picker or going straight to the dashboard,
    // so a NotFound error can still print to a plain, untouched terminal.
    let (guard, pid): (TerminalGuard, Pid) = match &args.target {
        None => {
            let mut guard = match init_terminal_guard() {
                Ok(guard) => guard,
                Err(code) => return code,
            };
            match picker::run_picker(&mut guard.terminal, &mut sys, tick_interval, String::new()) {
                Ok(Some(pid)) => (guard, pid),
                Ok(None) => {
                    drop(guard);
                    return ExitCode::SUCCESS;
                }
                Err(err) => {
                    drop(guard);
                    eprintln!("error: {err}");
                    return ExitCode::from(4);
                }
            }
        }
        Some(target) => match process::resolve_target(&sys, target) {
            LookupResult::NotFound => {
                eprintln!("error: no running process matching '{target}'");
                return ExitCode::from(1);
            }
            LookupResult::Ambiguous => {
                let mut guard = match init_terminal_guard() {
                    Ok(guard) => guard,
                    Err(code) => return code,
                };
                match picker::run_picker(
                    &mut guard.terminal,
                    &mut sys,
                    tick_interval,
                    target.clone(),
                ) {
                    Ok(Some(pid)) => (guard, pid),
                    Ok(None) => {
                        drop(guard);
                        return ExitCode::SUCCESS;
                    }
                    Err(err) => {
                        drop(guard);
                        eprintln!("error: {err}");
                        return ExitCode::from(4);
                    }
                }
            }
            LookupResult::Found(pid) => {
                let guard = match init_terminal_guard() {
                    Ok(guard) => guard,
                    Err(code) => return code,
                };
                (guard, pid)
            }
        },
    };

    run_dashboard(guard, sys, pid, tick_interval)
}

fn init_terminal_guard() -> Result<TerminalGuard, ExitCode> {
    match init_terminal() {
        Ok(terminal) => {
            install_panic_hook();
            Ok(TerminalGuard { terminal })
        }
        Err(err) => {
            eprintln!("error: failed to initialize terminal: {err}");
            Err(ExitCode::from(4))
        }
    }
}

fn run_dashboard(
    mut guard: TerminalGuard,
    sys: System,
    pid: Pid,
    tick_interval: Duration,
) -> ExitCode {
    let process_name;
    let exe_path;
    let exe_path_buf;
    let started_at_unix_secs;
    {
        let process = sys.process(pid).expect("resolved pid must exist");
        process_name = process.name().to_string_lossy().into_owned();
        exe_path_buf = process.exe().map(|p| p.to_path_buf());
        exe_path = exe_path_buf
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        started_at_unix_secs = process.start_time();
    }
    let mem_total_mb = sys.total_memory() / config::BYTES_PER_MB;
    let core_count = sys.cpus().len();

    let mut monitor = Monitor::new(
        sys,
        pid,
        exe_path_buf.as_deref().unwrap_or_else(|| Path::new("")),
    );
    let initial_sample = match monitor.sample() {
        Ok(sample) => sample,
        Err(MonitorError::ProcessExited) => {
            drop(guard);
            eprintln!("error: process exited before monitoring could start");
            return ExitCode::from(3);
        }
    };

    let mut app = App::new(
        pid,
        process_name,
        exe_path,
        started_at_unix_secs,
        mem_total_mb,
        core_count,
        tick_interval,
        initial_sample,
    );

    if let Err(err) = run(&mut guard.terminal, &mut app, &mut monitor) {
        drop(guard);
        eprintln!("error: {err}");
        return ExitCode::from(4);
    }
    drop(guard);

    if app.exited {
        println!(
            "Process '{}' (pid {}) is no longer running.",
            app.process_name, app.pid
        );
        return ExitCode::from(3);
    }

    ExitCode::SUCCESS
}

fn run(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    monitor: &mut Monitor,
) -> io::Result<()> {
    let mut last_tick = Instant::now();

    loop {
        let timeout = app.tick_interval.saturating_sub(last_tick.elapsed());

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.handle_key(key.code);
        }

        if last_tick.elapsed() >= app.tick_interval {
            if !app.paused {
                match monitor.sample() {
                    Ok(sample) => app.update(sample),
                    Err(MonitorError::ProcessExited) => {
                        app.exited = true;
                    }
                }
            }
            last_tick = Instant::now();
        }

        terminal.draw(|frame| ui::draw(frame, app))?;

        if app.should_quit || app.exited {
            break;
        }
    }

    Ok(())
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn install_panic_hook() {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(panic_info);
    }));
}
