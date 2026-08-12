# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`pview` is a Windows-first (cross-platform via `sysinfo`) terminal UI, built with `ratatui` + `crossterm`, that monitors a single OS process in real time: CPU, memory, disk I/O, and process metadata. Think `htop` scoped to one process.

## Commands

```bash
cargo build --release       # binary at target/release/pview(.exe)
cargo run -- explorer.exe   # run against a process by name
cargo run -- 12345          # run against a process by PID
cargo run                   # no target -> interactive fuzzy picker
cargo check                 # fast type-check
cargo clippy                # lint
cargo fmt                   # format
```

There are no automated tests in this repo currently.

## Architecture

Data flows in one direction each tick: `Monitor` samples `sysinfo` -> produces a `Sample` -> `App::update` folds it into UI-ready state -> `ui::draw` renders it. Modules:

- **`main.rs`** — entry point and the only place that owns the terminal/event loop. Resolves the CLI target to a PID (going through the picker if none/ambiguous), then runs a fixed-tick loop: poll input for the remaining tick budget, sample on tick boundary (unless paused), redraw every iteration. `TerminalGuard` (RAII) and `install_panic_hook` guarantee raw mode / alternate screen are torn down even on panic or early error paths — preserve this if touching terminal setup.
- **`cli.rs`** — `clap`-derived `Args`: an optional `target` (name or PID) and `--interval` (ms).
- **`process.rs`** — `resolve_target`: bare integers are always treated as a PID; otherwise matches process names case-insensitively and reports `Found` / `NotFound` / `Ambiguous`.
- **`picker.rs`** — the interactive fuzzy-search process picker (`fuzzy_matcher::skim`), used when no target is given or a name is ambiguous. Self-contained event loop, mirrors the same poll/refresh-on-tick pattern as `main.rs`'s dashboard loop. By design, only Esc/Up/Down/Enter/Backspace are special-cased — every other printable char (including `q`/`j`/`k`) is filter input, since those letters legitimately appear in process names.
- **`monitor.rs`** — `Monitor` wraps a `sysinfo::System` scoped to one PID and produces a `Sample` per call to `sample()`; returns `MonitorError::ProcessExited` once the process disappears.
- **`app.rs`** — `App` is the UI state machine: rolling CPU/memory history (`VecDeque`, capped at `HISTORY_LEN`), all-time peaks, disk I/O rate computed from byte deltas between samples (with a session baseline so totals start at zero), pause/quit/exited flags, and `handle_key` for the dashboard keybindings (`q`/`p`/`r`).
- **`ui.rs`** — pure rendering from `&App` into `ratatui` widgets (braille line charts for CPU/mem, disk panel, process info panel, footer). No state mutation happens here.
- **`config.rs`** — shared constants (`DEFAULT_INTERVAL_MS`, `HISTORY_LEN`).

### Key conventions

- The dashboard and the picker each run their own event loop with the same shape: `event::poll(remaining_timeout)` for input, sample/refresh only once a full tick has elapsed, redraw unconditionally every iteration.
- Memory/disk values move through the codebase as raw bytes (`u64`) from `sysinfo` and are only converted to MB/GB at the display edge (`ui.rs`), using decimal (1_000_000) not binary (1024*1024) byte-to-MB conversion — GB display in `format_mem_total` does use 1024-based math, so be deliberate about which convention applies where.
- Terminal setup is deliberately lazy in `main.rs`: it's deferred until after the target is resolved to `Found`/`Ambiguous`, so a `NotFound` error can print to a plain, untouched terminal instead of the alternate screen.
