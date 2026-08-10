use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Clear, Dataset, GraphType, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::config::HISTORY_LEN;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
            Constraint::Percentage(30),
            Constraint::Length(1),
        ])
        .split(area);

    draw_top_bar(frame, rows[0], app);
    draw_cpu_mem_row(frame, rows[1], app);
    draw_disk_panel(frame, rows[2], app);
    draw_process_panel(frame, rows[3], app);
    draw_footer(frame, rows[4], app);

    if app.help_visible {
        draw_help(frame, area);
    }
}

fn draw_top_bar(frame: &mut Frame, area: Rect, app: &App) {
    let status = if app.paused { " PAUSED" } else { "" };
    let line = Line::from(vec![
        Span::raw(format!("{}  ", app.process_name)),
        Span::raw(format!("PID {}  ", app.pid)),
        Span::styled(
            format!("{:?}", app.status),
            Style::default().fg(Color::Green),
        ),
        Span::styled(status, Style::default().fg(Color::Yellow)),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("PVIEW")
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_cpu_mem_row(frame: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_stat_panel(
        frame,
        cols[0],
        "CPU",
        format!("{:.1}%", app.cpu_current),
        app.cpu_history.iter().map(|v| *v as f64).collect(),
        100.0,
        "0%",
        "100%",
    );

    let mem_max = app.mem_history.iter().copied().max().unwrap_or(1).max(1) as f64;
    draw_stat_panel(
        frame,
        cols[1],
        "MEMORY",
        format!("{} MB", app.mem_current_mb),
        app.mem_history.iter().map(|v| *v as f64).collect(),
        mem_max,
        "0 MB",
        &format!("{mem_max:.0} MB"),
    );
}

fn draw_stat_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    value: String,
    history: Vec<f64>,
    y_max: f64,
    axis_min: &str,
    axis_max: &str,
) {
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            value,
            Style::default().add_modifier(Modifier::BOLD),
        ))),
        sections[0],
    );

    let points: Vec<(f64, f64)> = history
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();

    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(Color::Cyan))
        .data(&points);

    let chart = Chart::new(vec![dataset])
        .x_axis(Axis::default().bounds([0.0, (HISTORY_LEN.saturating_sub(1)) as f64]))
        .y_axis(Axis::default().bounds([0.0, y_max.max(1.0)]));
    frame.render_widget(chart, sections[1]);

    let axis = Line::from(vec![
        Span::raw(axis_min.to_string()),
        Span::raw(" ".repeat(inner.width.saturating_sub((axis_min.len() + axis_max.len()) as u16) as usize)),
        Span::raw(axis_max.to_string()),
    ]);
    frame.render_widget(Paragraph::new(axis), sections[2]);
}

fn draw_disk_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("DISK I/O");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(inner);

    render_kv(frame, cols[0], "Read", format!("{:.2} MB/s", app.disk_read_rate_mb_s));
    render_kv(frame, cols[1], "Write", format!("{:.2} MB/s", app.disk_write_rate_mb_s));
    render_kv(
        frame,
        cols[2],
        "Total R",
        format!("{:.2} MB", app.disk_read_bytes_session as f64 / 1_000_000.0),
    );
    render_kv(
        frame,
        cols[3],
        "Total W",
        format!("{:.2} MB", app.disk_write_bytes_session as f64 / 1_000_000.0),
    );
}

fn draw_process_panel(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title("PROCESS");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(inner);

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 3])
        .split(cols[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1); 3])
        .split(cols[1]);

    render_kv(frame, left[0], "Uptime", format_duration(app.run_time_secs));
    render_kv(frame, left[1], "Status", format!("{:?}", app.status));
    render_kv(frame, left[2], "Executable", app.exe_path.clone());

    render_kv(frame, right[0], "Started", format_unix_secs(app.started_at_unix_secs));
    render_kv(frame, right[1], "Memory Peak", format!("{} MB", app.mem_peak_mb));
    render_kv(frame, right[2], "Refresh", format!("{} ms", app.tick_interval.as_millis()));
}

fn render_kv(frame: &mut Frame, area: Rect, key: &str, value: String) {
    let line = Line::from(vec![
        Span::styled(format!("{key}: "), Style::default().fg(Color::DarkGray)),
        Span::raw(value),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, _app: &App) {
    let line = Line::from(Span::raw(
        "  q Quit    p Pause    r Reset graphs    ? Help",
    ));
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(50, 40, area);
    frame.render_widget(Clear, popup);
    let block = Block::default().borders(Borders::ALL).title("Help");
    let text = vec![
        Line::from("pview - single-process monitoring TUI"),
        Line::from(""),
        Line::from("q  Quit"),
        Line::from("p  Pause / resume refresh"),
        Line::from("r  Reset history graphs"),
        Line::from("?  Toggle this help"),
        Line::from(""),
        Line::from("Press any key to close"),
    ];
    frame.render_widget(Paragraph::new(text).block(block), popup);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
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
