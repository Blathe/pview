# pview

A terminal UI for watching a single process in real time — CPU, memory, disk I/O, and process metadata, updated live. Think `htop`, but scoped to exactly the one process you care about.

Built with [ratatui](https://github.com/ratatui-org/ratatui), [crossterm](https://github.com/crossterm-rs/crossterm), [clap](https://github.com/clap-rs/clap), and [sysinfo](https://github.com/GuillaumeGomez/sysinfo).

## Screenshots

<!-- Add a screenshot or GIF of pview in action here. -->

## Features

- **Live CPU and memory graphs** — braille line charts with a side axis and a rolling time window (e.g. last 60s), scaled against 100% CPU and total system memory so the numbers are directly comparable.
- **Disk I/O** — current read/write rate plus cumulative totals for the monitoring session.
- **Process info** — uptime, executable path, start time, and all-time CPU/memory peaks.
- **Interactive process picker** — run `pview` with no arguments to fuzzy-search running processes and pick one interactively, no need to know the exact name or PID up front.
- **Pause and reset** — freeze the display to inspect a moment, or clear the graphs and start a fresh window without restarting.
- **Configurable refresh rate** — default 1s, tune it down for a more responsive view.

## Installation

Requires a recent [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/Blathe/pview.git
cd pview
cargo build --release
```

The binary is built to `target/release/pview` (`pview.exe` on Windows).

## Usage

```bash
# Interactive picker — fuzzy-search running processes and select one
pview

# Monitor by exact process name (case-insensitive)
pview explorer.exe

# Monitor by PID
pview 12345

# Custom refresh interval (milliseconds)
pview explorer.exe --interval 250
pview explorer.exe -i 250
```

If a name matches more than one running process, `pview` opens the same interactive picker, pre-filtered to what you typed, so you can pick the exact instance you meant.

## Keybindings

**Dashboard**

| Key | Action |
| --- | --- |
| `q` | Quit |
| `p` | Pause / resume the display |
| `r` | Reset the CPU/memory graph history |

**Process picker**

| Key | Action |
| --- | --- |
| Type | Filter the process list |
| `↑` / `↓` | Move the selection |
| `Enter` | Monitor the selected process |
| `Esc` | Cancel and exit |

## Platform notes

`pview` is built on `sysinfo` for cross-platform process stats, but has primarily been developed and tested on Windows. Some fields (e.g. executable path, disk I/O, or process start time) may be unavailable for processes you don't own without elevated privileges, depending on the OS.
