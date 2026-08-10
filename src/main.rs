mod app;
mod cli;
mod config;
mod monitor;
mod process;
mod ui;

use std::io::{self, Stdout};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use clap::Parser;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use sysinfo::{ProcessesToUpdate, System};

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

    let pid = match process::resolve_target(&sys, &args.target) {
        LookupResult::Found(pid) => pid,
        LookupResult::NotFound => {
            eprintln!("error: no running process matching '{}'", args.target);
            return ExitCode::from(1);
        }
        LookupResult::Ambiguous(matches) => {
            eprintln!("error: multiple processes match '{}':", args.target);
            eprintln!("{:<10} {:<24} STARTED (unix secs)", "PID", "NAME");
            for m in matches {
                eprintln!("{:<10} {:<24} {}", m.pid, m.name, m.start_time);
            }
            eprintln!("re-run with a specific PID");
            return ExitCode::from(2);
        }
    };

    let process_name;
    let exe_path;
    let started_at_unix_secs;
    {
        let process = sys.process(pid).expect("resolved pid must exist");
        process_name = process.name().to_string_lossy().into_owned();
        exe_path = process
            .exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        started_at_unix_secs = process.start_time();
    }
    let mem_total_mb = sys.total_memory() / 1_000_000;

    let tick_interval = Duration::from_millis(args.interval);
    let mut monitor = Monitor::new(sys, pid);
    let initial_sample = match monitor.sample() {
        Ok(sample) => sample,
        Err(MonitorError::ProcessExited) => {
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
        tick_interval,
        initial_sample,
    );

    let mut guard = match init_terminal() {
        Ok(terminal) => TerminalGuard { terminal },
        Err(err) => {
            eprintln!("error: failed to initialize terminal: {err}");
            return ExitCode::from(4);
        }
    };

    install_panic_hook();

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
        let timeout = app
            .tick_interval
            .saturating_sub(last_tick.elapsed());

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.handle_key(key.code);
                }
            }
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
