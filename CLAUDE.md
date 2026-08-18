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
cargo check && cargo fmt --check && cargo clippy && cargo build   # full health check before committing
```

There are no automated tests in this repo currently.

## Architecture

Data flows in one direction each tick: `Monitor` samples `sysinfo` -> produces a `Sample` -> `App::update` folds it into UI-ready state -> `ui::draw` renders it. Modules:

- **`main.rs`** — owns the terminal/event loop. Resolves the CLI target to a PID (via the picker if none/ambiguous), then runs a fixed-tick loop: poll input, sample on tick boundary (unless paused), redraw every iteration. `TerminalGuard` (RAII) + `install_panic_hook` guarantee raw mode / alternate screen are torn down even on panic — preserve this if touching terminal setup.
- **`cli.rs`** — `clap`-derived `Args`: optional `target` (name or PID) and `--interval` (ms).
- **`process.rs`** — `resolve_target`: bare integers are always a PID; otherwise matches process names case-insensitively, reporting `Found` / `NotFound` / `Ambiguous`.
- **`picker.rs`** — interactive fuzzy-search process picker (`fuzzy_matcher::skim`), used when no target is given or a name is ambiguous. Only Esc/Up/Down/Enter/Backspace are special-cased — every other char (including `q`/`j`/`k`) is filter input, since those letters legitimately appear in process names.
- **`monitor.rs`** — `Monitor` wraps a `sysinfo::System` scoped to one PID, producing a `Sample` per call to `sample()`; returns `MonitorError::ProcessExited` once the process disappears.
- **`app.rs`** — UI state machine: rolling CPU/memory history (`VecDeque`, capped at `HISTORY_LEN`), all-time peaks, disk I/O rate from byte deltas (session baseline so totals start at zero), pause/quit/exited flags, `handle_key` for dashboard keybindings (`q`/`p`/`r`).
- **`ui.rs`** — pure rendering from `&App` into `ratatui` widgets. No state mutation.
- **`config.rs`** — shared constants (`DEFAULT_INTERVAL_MS`, `HISTORY_LEN`).

### Key conventions

- Memory/disk values are raw bytes (`u64`) from `sysinfo`. Byte-to-MB/GB conversion uses binary math (1024-based, via `config::BYTES_PER_MB`) everywhere, matching Windows tools despite the "MB"/"GB" labels — keep new conversions on this convention, not decimal.
- Terminal setup in `main.rs` is deferred until the target resolves to `Found`/`Ambiguous`, so a `NotFound` error prints to a plain terminal instead of the alternate screen.
