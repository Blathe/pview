use std::io::{self, Stdout};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{Frame, Terminal};
use sysinfo::{Pid, ProcessesToUpdate, System};

struct PickerEntry {
    pid: Pid,
    name: String,
    parent: Option<Pid>,
    is_group_root: bool,
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
        all.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
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

    fn refilter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.all.len()).collect();
            return;
        }

        let matcher = SkimMatcherV2::default();
        let mut scored: Vec<(usize, i64)> = self
            .all
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| {
                matcher
                    .fuzzy_match(&entry.name, &self.query)
                    .map(|score| (i, score))
            })
            .collect();

        scored.sort_by(|(i_a, score_a), (i_b, score_b)| {
            score_b
                .cmp(score_a)
                .then_with(|| self.all[*i_a].name.cmp(&self.all[*i_b].name))
        });

        self.filtered = scored.into_iter().map(|(i, _)| i).collect();
    }

    fn refresh(&mut self, sys: &System, own_pid: Pid) {
        let remembered_pid = self.filtered.get(self.selected).map(|&i| self.all[i].pid);

        self.all = snapshot_processes(sys, own_pid);
        self.all
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.refilter();

        self.selected = remembered_pid
            .and_then(|pid| self.filtered.iter().position(|&i| self.all[i].pid == pid))
            .unwrap_or(0)
            .min(self.filtered.len().saturating_sub(1));
    }
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

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match handle_key(&mut state, key.code) {
                        PickerAction::Continue => {}
                        PickerAction::Select(pid) => return Ok(Some(pid)),
                        PickerAction::Cancel => return Ok(None),
                    }
                }
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
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_search_box(frame, rows[0], state);
    draw_results(frame, rows[1], state);
    draw_footer(frame, rows[2]);
}

fn draw_search_box(frame: &mut Frame, area: Rect, state: &PickerState) {
    let block = Block::default().borders(Borders::ALL).title("SEARCH");
    let line = if state.query.is_empty() {
        Line::from(Span::styled(
            "type to filter…",
            Style::default().fg(Color::DarkGray),
        ))
    } else {
        Line::from(format!("{}▏", state.query))
    };
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_results(frame: &mut Frame, area: Rect, state: &PickerState) {
    let block = Block::default().borders(Borders::ALL).title("PROCESSES");

    if state.filtered.is_empty() {
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "No matching processes",
                Style::default().fg(Color::DarkGray),
            ))),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = state
        .filtered
        .iter()
        .map(|&i| {
            let entry = &state.all[i];
            let suffix = if entry.is_group_root { " (parent)" } else { "" };
            ListItem::new(format!("PID: {} | {}{}", entry.pid, entry.name, suffix))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(Color::Cyan).fg(Color::Black));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let line = Line::from(Span::raw("  ↑↓ Navigate    Enter Select    Esc Cancel"));
    frame.render_widget(Paragraph::new(line), area);
}
