use std::time::Duration;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph, Sparkline};

use crate::app::App;
use crate::config::{APP_VERSION, BYTES_PER_MB, HISTORY_LEN};

const CPU_COLOR: Color = Color::Rgb(59, 130, 246); // blue
const MEM_COLOR: Color = Color::Rgb(249, 115, 22); // orange
const STORAGE_COLOR: Color = Color::Rgb(168, 85, 247); // purple
pub(crate) const TITLE_COLOR: Color = Color::LightBlue;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(5),
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Fill(1),
            Constraint::Length(3),
        ])
        .split(area);

    draw_slim_header(frame, rows[0]);
    draw_status_bar(frame, rows[1], app);
    draw_cpu_mem_row(frame, rows[2], app);
    draw_disk_panel(frame, rows[3], app);
    draw_storage_panel(frame, rows[4], app);
    draw_footer(frame, rows[6], app);
}

pub fn draw_slim_header(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            "[pview ● ",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            APP_VERSION,
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "]",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        ),
    ])
    .alignment(Alignment::Right);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("PROCESS")
        .title_style(
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::new(1, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let (status_text, status_color) = status_display(app);
    let name = format!("{}  ", app.process_name);
    let badge_text = format!(" {status_text} ");
    let right = format!(
        "PID {}    started {}",
        app.pid,
        format_unix_secs(app.started_at_unix_secs)
    );

    let left_len = name.len() as u16 + badge_text.len() as u16;
    let gap = sections[0]
        .width
        .saturating_sub(left_len + right.len() as u16 + 1);

    let line = Line::from(vec![
        Span::styled(name, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(
            badge_text,
            Style::default().bg(status_color).fg(Color::Black),
        ),
        Span::raw(" ".repeat(gap as usize + 1)),
        Span::styled(right, Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), sections[0]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(sections[1]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 2])
        .split(cols[0]);

    render_kv(frame, left[0], "Uptime", format_duration(app.run_time_secs));
    render_kv(
        frame,
        left[1],
        "Executable",
        truncate_exe_path(&app.exe_path),
    );
}

/// Returns the display text and color for the process/monitoring status,
/// showing PAUSED (in place of the OS process status) while pview itself
/// is paused, since the underlying process status stops being meaningful
/// to the user once pview has stopped refreshing it.
fn status_display(app: &App) -> (String, Color) {
    if app.paused {
        ("PAUSED".to_string(), Color::Yellow)
    } else {
        (format!("{:?}", app.status), Color::Green)
    }
}

/// Thresholds a percentage into a health label + color, used for the peak
/// badges on the CPU/Memory panels.
fn health_badge(pct: f64) -> (&'static str, Color) {
    if pct >= 90.0 {
        ("CRIT", Color::Red)
    } else if pct >= 70.0 {
        ("HIGH", Color::Yellow)
    } else {
        ("OK", Color::Green)
    }
}

fn draw_cpu_mem_row(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let window_secs = HISTORY_LEN as f64 * app.tick_interval.as_secs_f64();
    let time_labels = (format!("{window_secs:.0}s"), "0s".to_string());

    let cpu_ratio = app.cpu_current / 100.0;
    let cpu_history: Vec<u64> = app
        .cpu_history
        .iter()
        .map(|v| v.round().max(0.0) as u64)
        .collect();

    // The sparkline shares the same fixed 0-100% scale as the usage bar/
    // header, so the baseline is stable and the bar's height is a direct
    // read of actual usage instead of shifting as the window's peak changes.
    draw_stat_panel(
        frame,
        cols[0],
        "CPU",
        None,
        Some(format!("{:.1}%", app.cpu_current)),
        cpu_history,
        100.0,
        Some(("0%".to_string(), "100%".to_string())),
        time_labels.clone(),
        format!("peak {:.1}%", app.cpu_peak),
        health_badge(app.cpu_current as f64),
        cpu_ratio,
        format!("{:.1}%", app.cpu_current),
        CPU_COLOR,
    );

    let mem_total_mb = app.mem_total_mb.max(1) as f64;
    let mem_peak_pct = app.mem_peak_mb as f64 / mem_total_mb * 100.0;
    let mem_ratio = (app.mem_current_mb as f64 / mem_total_mb) as f32;

    // The sparkline shares the same fixed 0-100% scale as the usage bar/
    // header (percent of total system RAM), so the baseline is stable and
    // the bar's height is a direct, apples-to-apples read of actual usage.
    let mem_history_pct: Vec<u64> = app
        .mem_history
        .iter()
        .map(|&mb| (mb as f64 / mem_total_mb * 100.0).round() as u64)
        .collect();

    let mem_trend_label = app
        .mem_trend_mb_per_hr()
        .map(|(rate, elapsed)| format_mem_trend(rate, elapsed));

    draw_stat_panel(
        frame,
        cols[1],
        "MEMORY",
        mem_trend_label,
        Some(format!(
            "{} MB / {}",
            app.mem_current_mb,
            format_mb_as_gb(app.mem_total_mb)
        )),
        mem_history_pct,
        100.0,
        Some(("0%".to_string(), "100%".to_string())),
        time_labels,
        format!("peak {mem_peak_pct:.1}%"),
        health_badge(mem_ratio as f64 * 100.0),
        mem_ratio,
        format!("{:.1}%", mem_ratio * 100.0),
        MEM_COLOR,
    );
}

/// Formats a size in MB as GB, rounding to a whole number when close to one
/// (e.g. 32 GB) and otherwise showing one decimal place.
fn format_mb_as_gb(total_mb: u64) -> String {
    let total_gb = total_mb as f64 / 1024.0;
    if (total_gb - total_gb.round()).abs() < 0.05 {
        format!("{:.0} GB", total_gb.round())
    } else {
        format!("{total_gb:.1} GB")
    }
}

/// Formats the "memory over 1h" trend badge, e.g. "▲ +4.2 MB/hr" once a
/// full hour of samples is available, or "▲ +4.2 MB/hr (12m)" while the
/// long-lived sample buffer is still warming up. Flat/negligible drift
/// (under 0.05 MB/hr either way) shows a neutral "flat" arrow instead of
/// noise from rounding.
fn format_mem_trend(rate_mb_per_hr: f32, elapsed: Duration) -> String {
    let arrow = if rate_mb_per_hr.abs() < 0.05 {
        "►"
    } else if rate_mb_per_hr > 0.0 {
        "▲"
    } else {
        "▼"
    };
    let rate_text = format!("{arrow} {rate_mb_per_hr:+.1} MB/hr");
    if elapsed.as_secs() < 3600 {
        let minutes = (elapsed.as_secs() / 60).max(1);
        format!("{rate_text} ({minutes}m)")
    } else {
        rate_text
    }
}

fn draw_stat_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    title_extra: Option<String>,
    value: Option<String>,
    history: Vec<u64>,
    y_max: f64,
    side_axis_labels: Option<(String, String)>,
    time_labels: (String, String),
    peak_label: String,
    badge: (&'static str, Color),
    usage_ratio: f32,
    usage_label: String,
    chart_color: Color,
) {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_top(Line::from(title).left_aligned())
        .title_style(
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::uniform(1));
    if let Some(extra) = title_extra {
        block = block.title_top(
            Line::from(Span::styled(extra, Style::default().fg(Color::DarkGray))).right_aligned(),
        );
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if value.is_some() { 1 } else { 0 }),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    if let Some(value) = value {
        let badge_text = format!(" {} ", badge.0);
        let right = format!("{peak_label}  ");
        let left_len = value.len() as u16;
        let right_len = right.len() as u16 + badge_text.len() as u16;
        let gap = sections[0].width.saturating_sub(left_len + right_len);

        let line = Line::from(vec![
            Span::styled(value, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" ".repeat(gap as usize)),
            Span::styled(right, Style::default().fg(Color::DarkGray)),
            Span::styled(badge_text, Style::default().bg(badge.1).fg(Color::Black)),
        ]);
        frame.render_widget(Paragraph::new(line), sections[0]);
    }

    draw_bar(
        frame,
        sections[1],
        usage_ratio,
        chart_color,
        Some(usage_label),
    );

    let label_width = side_axis_labels
        .as_ref()
        .map(|(min_label, max_label)| min_label.len().max(max_label.len()) as u16 + 1)
        .unwrap_or(0);

    let plot_area = if let Some((min_label, max_label)) = &side_axis_labels {
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(label_width), Constraint::Min(1)])
            .split(sections[3]);

        let label_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(split[0]);
        frame.render_widget(Paragraph::new(max_label.clone()), label_rows[0]);
        frame.render_widget(Paragraph::new(min_label.clone()), label_rows[2]);

        split[1]
    } else {
        sections[3]
    };

    let resampled = resample_to_width(&history, plot_area.width as usize, HISTORY_LEN);
    let max = y_max.round() as u64;
    // ratatui's Sparkline computes each bar's height as `value * height * 8 /
    // max`, floored. On a short viewport the plot area may only be 1-2 rows
    // (8-16 eighths of resolution against the fixed 0-100 scale), so any
    // genuinely nonzero reading below roughly max/levels floors straight to
    // zero and disappears. Floor real (nonzero) samples up to whatever value
    // guarantees at least one visible eighth, so low usage still shows as a
    // sliver instead of vanishing; true zero/no-data samples stay blank.
    let levels = plot_area.height as u64 * 8;
    let min_visible = if levels == 0 {
        0
    } else {
        max.div_ceil(levels).max(1)
    };
    let resampled: Vec<u64> = resampled
        .into_iter()
        .map(|v| if v > 0 { v.max(min_visible) } else { 0 })
        .collect();
    let sparkline = Sparkline::default()
        .data(&resampled)
        .max(max)
        .style(Style::default().fg(chart_color));
    frame.render_widget(sparkline, plot_area);

    let (left_time, right_time) = time_labels;
    let time_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(label_width), Constraint::Min(1)])
        .split(sections[4]);
    let gap = time_row[1]
        .width
        .saturating_sub((left_time.len() + right_time.len()) as u16);
    let line = Line::from(vec![
        Span::raw(left_time),
        Span::raw(" ".repeat(gap as usize)),
        Span::raw(right_time),
    ]);
    frame.render_widget(Paragraph::new(line), time_row[1]);
}

/// Maps `data` onto exactly `width` columns, where each of the `capacity`
/// possible ticks (`HISTORY_LEN`) always occupies the same `width / capacity`
/// slice — so bars grow at a steady, uniform rate tick by tick instead of
/// stretching to fill the whole width while the history buffer is still
/// warming up. Existing samples are right-aligned (most recent tick nearest
/// the right edge, matching the "0s" time label) and nearest-neighbor
/// resampled within their slice; unfilled ticks render as zero-height bars.
fn resample_to_width(data: &[u64], width: usize, capacity: usize) -> Vec<u64> {
    if width == 0 || capacity == 0 {
        return Vec::new();
    }
    let filled_width = (data.len() * width / capacity).min(width);
    let empty_width = width - filled_width;

    let mut result = vec![0u64; empty_width];
    if !data.is_empty() && filled_width > 0 {
        result.extend((0..filled_width).map(|x| data[x * data.len() / filled_width]));
    }
    result
}

fn draw_disk_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("DISK I/O")
        .title_style(
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::uniform(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    draw_disk_row(
        frame,
        [rows[0], rows[1], rows[2]],
        "Read",
        app.disk_read_rate_mb_s,
        app.disk_read_rate_peak_mb_s,
        app.disk_read_bytes_session as f64 / BYTES_PER_MB as f64,
        CPU_COLOR,
    );

    draw_disk_row(
        frame,
        [rows[4], rows[5], rows[6]],
        "Write",
        app.disk_write_rate_mb_s,
        app.disk_write_rate_peak_mb_s,
        app.disk_write_bytes_session as f64 / BYTES_PER_MB as f64,
        MEM_COLOR,
    );
}

fn draw_disk_row(
    frame: &mut Frame,
    rows: [Rect; 3],
    label: &str,
    rate: f32,
    peak_rate: f32,
    total_mb: f64,
    color: Color,
) {
    let rate_text = format!("{rate:.2} MB/s");
    let gap = rows[0]
        .width
        .saturating_sub(label.len() as u16 + rate_text.len() as u16);
    let line = Line::from(vec![
        Span::raw(label.to_string()),
        Span::raw(" ".repeat(gap as usize)),
        Span::raw(rate_text),
    ]);
    frame.render_widget(Paragraph::new(line), rows[0]);

    let ratio = if peak_rate > 0.0 {
        (rate / peak_rate).clamp(0.0, 1.0)
    } else {
        0.0
    };
    draw_bar(frame, rows[1], ratio, color, None);

    let summary = format!("Max {peak_rate:.2} MB/s    Total {total_mb:.2} MB");
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            summary,
            Style::default().fg(Color::DarkGray),
        ))),
        rows[2],
    );
}

fn draw_storage_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title("STORAGE")
        .title_style(
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .padding(Padding::uniform(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let total_mb = app.storage_total_mb.max(1);
    let ratio = (app.storage_used_mb as f32 / total_mb as f32).clamp(0.0, 1.0);
    let free_mb = total_mb.saturating_sub(app.storage_used_mb);

    let left = app.storage_mount_point.clone();
    let right = format!(
        "{} / {}",
        format_mb_as_gb(app.storage_used_mb),
        format_mb_as_gb(app.storage_total_mb)
    );
    let gap = rows[0]
        .width
        .saturating_sub(left.len() as u16 + right.len() as u16);
    let line = Line::from(vec![
        Span::styled(left, Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" ".repeat(gap as usize)),
        Span::raw(right),
    ]);
    frame.render_widget(Paragraph::new(line), rows[0]);

    draw_bar(
        frame,
        rows[1],
        ratio,
        STORAGE_COLOR,
        Some(format!("{:.1}%", ratio * 100.0)),
    );

    let summary = format!("Free {}", format_mb_as_gb(free_mb));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            summary,
            Style::default().fg(Color::DarkGray),
        ))),
        rows[2],
    );
}

/// Renders a filled horizontal bar: `ratio` (0.0-1.0) worth of solid blocks
/// in `color`, followed by dim track characters for the remainder, so the
/// bar is visibly present (not blank) even at 0%. An optional `label` is
/// centered on top, overwriting whichever bar/track characters sit beneath
/// it, styled independently of the fill so it stays readable either way.
fn draw_bar(frame: &mut Frame, area: Rect, ratio: f32, color: Color, label: Option<String>) {
    let width = area.width as usize;
    let filled = ((width as f32) * ratio.clamp(0.0, 1.0)).round() as usize;
    let filled = filled.min(width);

    let mut spans = Vec::new();

    if let Some(label) = label {
        let label = format!(" {label} ");
        let label_len = label.chars().count().min(width);
        let label_start = (width.saturating_sub(label_len)) / 2;
        let label_end = label_start + label_len;

        spans.extend(bar_segment_spans(0, label_start, filled, color));
        spans.push(Span::styled(
            label,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        spans.extend(bar_segment_spans(label_end, width, filled, color));
    } else {
        spans.extend(bar_segment_spans(0, width, filled, color));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Splits a `[start, end)` slice of a bar into filled/track spans around the
/// `filled` boundary, so a label overlay can sit on either side without
/// losing the fill/track distinction underneath it.
fn bar_segment_spans(start: usize, end: usize, filled: usize, color: Color) -> Vec<Span<'static>> {
    if start >= end {
        return Vec::new();
    }

    let split = filled.clamp(start, end);
    let mut spans = Vec::new();
    if split > start {
        spans.push(Span::styled(
            "█".repeat(split - start),
            Style::default().fg(color),
        ));
    }
    if end > split {
        spans.push(Span::styled(
            "░".repeat(end - split),
            Style::default().fg(Color::DarkGray),
        ));
    }
    spans
}

fn render_kv(frame: &mut Frame, area: Rect, key: &str, value: String) {
    render_kv_styled(frame, area, key, value, Color::Reset);
}

fn render_kv_styled(frame: &mut Frame, area: Rect, key: &str, value: String, value_color: Color) {
    let line = Line::from(vec![
        Span::styled(format!("{key}: "), Style::default().fg(Color::DarkGray)),
        Span::styled(value, Style::default().fg(value_color)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, _app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let key_style = Style::default()
        .bg(TITLE_COLOR)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(" q ", key_style),
        Span::raw(" Quit    "),
        Span::styled(" p ", key_style),
        Span::raw(" Pause    "),
        Span::styled(" r ", key_style),
        Span::raw(" Reset graphs"),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}

/// Shortens an executable path to at most its last 2 directories plus the
/// filename (e.g. "...\Ascension\bin\worldserver.exe"), since full install
/// paths are often too long to be useful in a fixed-width panel.
fn truncate_exe_path(path: &str) -> String {
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|s| !s.is_empty()).collect();

    if parts.len() <= 3 {
        return path.to_string();
    }

    format!("...\\{}", parts[parts.len() - 3..].join("\\"))
}

fn format_duration(total_secs: u64) -> String {
    let h = total_secs / 3600;
    let m = (total_secs % 3600) / 60;
    let s = total_secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Formats a Unix timestamp as a UTC "HH:MM:SS" time-of-day string, avoiding
/// a dependency on a full date/time crate for this single display field.
fn format_unix_secs(unix_secs: u64) -> String {
    let secs_of_day = unix_secs % 86400;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    format!("{h:02}:{m:02}:{s:02} UTC")
}
