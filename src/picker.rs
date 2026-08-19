use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Padding, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::{Frame, Terminal};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::config::BYTES_PER_MB;
use crate::ui::{TITLE_COLOR, draw_slim_header, overlap_top};

const SELECTED_BG: Color = Color::Rgb(30, 41, 59);
const ACCENT_COLOR: Color = TITLE_COLOR;

const ACCENT_W: u16 = 2;
const PID_W: u16 = 7;
const GAP_W: u16 = 2;
const CPU_W: u16 = 7;
const MEM_W: u16 = 10;

struct PickerEntry {
    pid: Pid,
    name: String,
    parent: Option<Pid>,
    is_group_root: bool,
    cpu_usage: f32,
    memory_bytes: u64,
}

struct PickerState {
    all: Vec<PickerEntry>,
    query: String,
    filtered: Vec<usize>,
    selected: usize,
    last_refresh: Instant,
}

impl PickerState {
    fn new(mut all: Vec<PickerEntry>, initial_query: String) -> Self {
        sort_by_name(&mut all);
        let mut state = Self {
            all,
            query: initial_query,
            filtered: Vec::new(),
            selected: 0,
            last_refresh: Instant::now(),
        };
        state.refilter();
        state
    }

    /// Filters `all` down to entries matching `query`. Since `all` is kept
    /// sorted alphabetically, a plain filter (no re-sort) is enough to keep
    /// the filtered view in that same stable order — sorting by a live
    /// metric like CPU% instead made rows jump around between refreshes,
    /// which made it hard to pick something without typing a search.
    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.all.len()).collect();
            return;
        }

        let matcher = SkimMatcherV2::default();
        self.filtered = self
            .all
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| matcher.fuzzy_match(&entry.name, &self.query).map(|_| i))
            .collect();
    }

    fn refresh(&mut self, sys: &System, own_pid: Pid) {
        let remembered_pid = self.filtered.get(self.selected).map(|&i| self.all[i].pid);

        self.all = snapshot_processes(sys, own_pid);
        sort_by_name(&mut self.all);
        self.refilter();

        self.selected = remembered_pid
            .and_then(|pid| self.filtered.iter().position(|&i| self.all[i].pid == pid))
            .unwrap_or(0)
            .min(self.filtered.len().saturating_sub(1));
    }
}

fn sort_by_name(entries: &mut [PickerEntry]) {
    entries.sort_by_key(|entry| entry.name.to_lowercase());
}

fn snapshot_processes(sys: &System, own_pid: Pid) -> Vec<PickerEntry> {
    let mut entries: Vec<PickerEntry> = sys
        .processes()
        .values()
        .filter(|p| p.pid() != own_pid)
        .map(|p| PickerEntry {
            pid: p.pid(),
            name: p.name().to_string_lossy().into_owned(),
            parent: p.parent(),
            is_group_root: false,
            cpu_usage: p.cpu_usage(),
            memory_bytes: p.memory(),
        })
        .collect();

    mark_group_roots(&mut entries);
    entries
}

/// Tags each entry whose name has 2+ concurrent instances with whether it's
/// the root of that name-group, i.e. none of the *other* same-named
/// processes is its OS parent. This is what distinguishes a top-level
/// chrome.exe (parented by e.g. explorer.exe) from its own renderer/GPU
/// child chrome.exe processes (parented by another chrome.exe).
fn mark_group_roots(entries: &mut [PickerEntry]) {
    use std::collections::{HashMap, HashSet};

    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, entry) in entries.iter().enumerate() {
        groups.entry(entry.name.to_lowercase()).or_default().push(i);
    }

    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }

        let pids_in_group: HashSet<Pid> = indices.iter().map(|&i| entries[i].pid).collect();

        for &i in indices {
            let parent_is_sibling = entries[i]
                .parent
                .map(|p| pids_in_group.contains(&p))
                .unwrap_or(false);
            entries[i].is_group_root = !parent_is_sibling;
        }
    }
}

enum PickerAction {
    Continue,
    Select(Pid),
    Cancel,
}

pub fn run_picker(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    sys: &mut System,
    tick_rate: Duration,
    initial_query: String,
) -> io::Result<Option<Pid>> {
    let own_pid = sysinfo::get_current_pid().unwrap_or_else(|_| Pid::from_u32(std::process::id()));

    let mut state = PickerState::new(snapshot_processes(sys, own_pid), initial_query);

    loop {
        terminal.draw(|frame| draw(frame, &state))?;

        if event::poll(tick_rate)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            match handle_key(&mut state, key.code) {
                PickerAction::Continue => {}
                PickerAction::Select(pid) => return Ok(Some(pid)),
                PickerAction::Cancel => return Ok(None),
            }
        }

        if state.last_refresh.elapsed() >= tick_rate {
            sys.refresh_processes(ProcessesToUpdate::All, true);
            state.refresh(sys, own_pid);
            state.last_refresh = Instant::now();
        }
    }
}

// Deliberately not bound: plain 'q'/'j'/'k'. Process names (e.g. "qemu",
// "jest", "kernel_task") can contain any of these letters, so the search
// box must treat all printable characters as filter input. Only Esc cancels
// and only Up/Down navigate.
fn handle_key(state: &mut PickerState, key: KeyCode) -> PickerAction {
    match key {
        KeyCode::Char(c) => {
            state.query.push(c);
            state.refilter();
            state.selected = 0;
        }
        KeyCode::Backspace => {
            state.query.pop();
            state.refilter();
            state.selected = 0;
        }
        KeyCode::Up => {
            state.selected = state.selected.saturating_sub(1);
        }
        KeyCode::Down => {
            if state.selected + 1 < state.filtered.len() {
                state.selected += 1;
            }
        }
        KeyCode::Enter => {
            if let Some(&i) = state.filtered.get(state.selected) {
                return PickerAction::Select(state.all[i].pid);
            }
        }
        KeyCode::Esc => return PickerAction::Cancel,
        _ => {}
    }

    PickerAction::Continue
}

fn draw(frame: &mut Frame, state: &PickerState) {
    let area = frame.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(area);

    draw_slim_header(frame, rows[0]);
    draw_search_box(frame, rows[1], state);
    draw_results(frame, overlap_top(rows[2], 1), state);
    draw_footer(frame, rows[3]);
}

fn draw_search_box(frame: &mut Frame, area: Rect, state: &PickerState) {
    let summary: String = if !state.query.is_empty() {
        format!(
            "{} matches  ·  {} processes total",
            state.filtered.len(),
            state.all.len()
        )
    } else {
        format!("{} processes total", state.all.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .title_top(Line::from("SEARCH").left_aligned())
        .title_style(
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .title_top(
            Line::from(Span::styled(summary, Style::default().fg(Color::DarkGray))).right_aligned(),
        );

    let line = if state.query.is_empty() {
        Line::from(Span::styled(
            "type to filter…",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                state.query.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled("▏", Style::default().fg(TITLE_COLOR)),
        ])
    };
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_results(frame: &mut Frame, area: Rect, state: &PickerState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .title_top(Line::from("PROCESSES").left_aligned())
        .title_style(
            Style::default()
                .fg(TITLE_COLOR)
                .add_modifier(Modifier::BOLD),
        )
        .title_top(
            Line::from(Span::styled(
                "sorted by name  ▲",
                Style::default().fg(Color::DarkGray),
            ))
            .right_aligned(),
        )
        .padding(Padding::new(0, 1, 0, 0));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height < 2 {
        return;
    }

    let body_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    draw_column_headers(frame, body_rows[0]);

    if state.filtered.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No matching processes",
                Style::default().fg(Color::DarkGray),
            ))),
            body_rows[1],
        );
        return;
    }

    let capacity = body_rows[1].height as usize;
    let total = state.filtered.len();
    let offset = draw_process_rows(frame, body_rows[1], state);

    if total > capacity {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .thumb_style(Style::default().fg(TITLE_COLOR))
            .track_style(Style::default().fg(Color::DarkGray));
        let content_capacity = capacity.saturating_sub(1).max(1);
        let mut scrollbar_state = ScrollbarState::new(total)
            .viewport_content_length(content_capacity)
            .position(offset);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin::new(0, 1)),
            &mut scrollbar_state,
        );
    }
}

fn columns(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(ACCENT_W),
            Constraint::Length(PID_W),
            Constraint::Length(GAP_W),
            Constraint::Min(10),
            Constraint::Length(GAP_W),
            Constraint::Length(CPU_W),
            Constraint::Length(GAP_W),
            Constraint::Length(MEM_W),
        ])
        .split(area)
}

fn draw_column_headers(frame: &mut Frame, area: Rect) {
    let cols = columns(area);
    let style = Style::default().fg(Color::DarkGray);

    frame.render_widget(
        Paragraph::new("PID")
            .style(style)
            .alignment(Alignment::Right),
        cols[1],
    );
    frame.render_widget(Paragraph::new("PROCESS").style(style), cols[3]);
    frame.render_widget(
        Paragraph::new("CPU%")
            .style(style)
            .alignment(Alignment::Right),
        cols[5],
    );
    frame.render_widget(
        Paragraph::new("MEM")
            .style(style)
            .alignment(Alignment::Right),
        cols[7],
    );
}

/// Renders as many filtered rows as fit `area`, keeping the selected row in
/// view. When the filtered list overflows the available height, the last
/// row is replaced with a "N more below" hint rather than silently cutting
/// a row off mid-list.
fn draw_process_rows(frame: &mut Frame, area: Rect, state: &PickerState) -> usize {
    let capacity = area.height as usize;
    let total = state.filtered.len();

    let (visible, offset, more_below) = if total <= capacity {
        (total, 0, 0)
    } else {
        let content_capacity = capacity.saturating_sub(1).max(1);
        let offset = state
            .selected
            .saturating_sub(content_capacity.saturating_sub(1))
            .min(total.saturating_sub(content_capacity));
        let more_below = total - (offset + content_capacity);
        (content_capacity, offset, more_below)
    };

    let row_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![
            Constraint::Length(1);
            visible + usize::from(more_below > 0)
        ])
        .split(area);

    for (row_idx, &filtered_idx) in state.filtered[offset..offset + visible].iter().enumerate() {
        let entry = &state.all[filtered_idx];
        draw_process_row(
            frame,
            row_rects[row_idx],
            entry,
            offset + row_idx == state.selected,
        );
    }

    if more_below > 0 {
        let hint = format!(
            "▼ {more_below} more match{} below — keep scrolling",
            if more_below == 1 { "" } else { "es" }
        );
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                hint,
                Style::default().fg(Color::DarkGray),
            ))),
            row_rects[visible],
        );
    }

    offset
}

fn draw_process_row(frame: &mut Frame, area: Rect, entry: &PickerEntry, selected: bool) {
    if selected {
        frame.render_widget(
            Block::default().style(Style::default().bg(SELECTED_BG)),
            area,
        );
    }

    let cols = columns(area);

    if selected {
        frame.render_widget(
            Paragraph::new("▎").style(Style::default().fg(ACCENT_COLOR)),
            cols[0],
        );
    }

    frame.render_widget(
        Paragraph::new(entry.pid.to_string())
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Right),
        cols[1],
    );

    frame.render_widget(Paragraph::new(name_line(entry, selected)), cols[3]);

    let cpu_style = if selected {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(cpu_color(entry.cpu_usage))
    };
    frame.render_widget(
        Paragraph::new(format!("{:.1}", entry.cpu_usage))
            .style(cpu_style)
            .alignment(Alignment::Right),
        cols[5],
    );

    frame.render_widget(
        Paragraph::new(format_mem(entry.memory_bytes))
            .style(Style::default().fg(if selected { Color::White } else { Color::Gray }))
            .alignment(Alignment::Right),
        cols[7],
    );
}

/// Splits the process name at its extension so the base name reads as the
/// primary, accented part of the row (matching the app's title-color accent
/// elsewhere), while the extension stays a plain, less prominent color. On
/// the selected row everything switches to a flat bold white instead, since
/// the row's own background highlight already carries the emphasis.
fn name_line(entry: &PickerEntry, selected: bool) -> Line<'static> {
    let mut spans = if selected {
        vec![Span::styled(
            entry.name.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]
    } else {
        match entry.name.rfind('.') {
            Some(dot) => vec![
                Span::styled(
                    entry.name[..dot].to_string(),
                    Style::default()
                        .fg(TITLE_COLOR)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    entry.name[dot..].to_string(),
                    Style::default().fg(Color::White),
                ),
            ],
            None => vec![Span::styled(
                entry.name.clone(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )],
        }
    };

    if entry.is_group_root {
        spans.push(Span::styled(
            " (parent)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    Line::from(spans)
}

/// Thresholds tuned for single-process CPU% (as opposed to the dashboard's
/// system-wide health_badge thresholds), since most processes in a picker
/// list sit well under 100% even when notably busy.
fn cpu_color(pct: f32) -> Color {
    if pct >= 20.0 {
        Color::Red
    } else if pct >= 10.0 {
        Color::Rgb(249, 115, 22)
    } else if pct >= 2.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn format_mem(bytes: u64) -> String {
    let mb = bytes / BYTES_PER_MB;
    if mb >= 1024 {
        format!("{:.1} GB", mb as f64 / 1024.0)
    } else {
        format!("{mb} MB")
    }
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let key_style = Style::default()
        .bg(TITLE_COLOR)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    let line = Line::from(vec![
        Span::raw(" "),
        Span::raw("↑↓"),
        Span::raw(" Navigate    "),
        Span::styled(" Enter ", key_style),
        Span::raw(" Select    "),
        Span::styled(" Esc ", key_style),
        Span::raw(" Cancel"),
    ]);
    frame.render_widget(Paragraph::new(line), inner);
}
