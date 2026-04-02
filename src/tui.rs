use crate::clipboard;
use crate::db::{ClipEntry, Store};
use crate::watcher::Watcher;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::stdout;
use std::path::Path;
use std::time::Instant;

// --- Actions (Elm-style message type) ---

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Insert,
}

enum Action {
    Quit,
    CopyAndQuit,
    MoveUp,
    MoveDown,
    MoveToTop,
    MoveToBottom,
    HalfPageUp,
    HalfPageDown,
    DeleteSelected,
    EnterInsert,
    ExitInsert,
    TypeChar(char),
    Backspace,
    ClearSearch,
    Noop,
}

// --- State ---

struct App {
    store: Store,
    entries: Vec<ClipEntry>,
    filtered: Vec<usize>,
    query: String,
    selected: usize,
    scroll_offset: usize,
    should_quit: bool,
    copied_id: Option<i64>,
    mode: Mode,
    pending_key: Option<char>,
    list_height: usize,
}

impl App {
    fn new(store: Store) -> Self {
        let entries = store.all().unwrap_or_default();
        let filtered = compute_filtered(&entries, "");
        App {
            store,
            entries,
            filtered,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            should_quit: false,
            copied_id: None,
            mode: Mode::Normal,
            pending_key: None,
            list_height: 0,
        }
    }

    fn refresh(&mut self) {
        let prev_id = self.selected_entry().map(|e| e.id);
        self.entries = self.store.all().unwrap_or_default();
        self.filtered = compute_filtered(&self.entries, &self.query);
        // Restore selection to the same entry by ID, falling back to clamp
        if let Some(id) = prev_id {
            if let Some(pos) = self.filtered.iter().position(|&i| self.entries[i].id == id) {
                self.selected = pos;
            } else {
                self.clamp_selection();
            }
        } else {
            self.clamp_selection();
        }
    }

    fn refilter(&mut self) {
        self.filtered = compute_filtered(&self.entries, &self.query);
        self.clamp_selection();
    }

    fn reset_selection(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    fn clamp_selection(&mut self) {
        if self.filtered.is_empty() {
            self.selected = 0;
            self.scroll_offset = 0;
        } else {
            self.selected = self.selected.min(self.filtered.len() - 1);
        }
    }

    fn selected_entry(&self) -> Option<&ClipEntry> {
        self.filtered.get(self.selected).map(|&i| &self.entries[i])
    }
}

// --- Pure functions ---

fn handle_key(key: KeyEvent, mode: Mode, pending: &mut Option<char>) -> Action {
    // Ctrl+C always quits
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Action::Quit;
    }

    match mode {
        Mode::Normal => handle_normal_key(key, pending),
        Mode::Insert => handle_insert_key(key),
    }
}

fn handle_normal_key(key: KeyEvent, pending: &mut Option<char>) -> Action {
    // Check for two-key combos first
    if let Some(first) = pending.take() {
        return match (first, key.code) {
            ('g', KeyCode::Char('g')) => Action::MoveToTop,
            ('d', KeyCode::Char('d')) => Action::DeleteSelected,
            _ => Action::Noop, // invalid combo, discard
        };
    }

    // Check Ctrl modifiers first since they overlap with bare keys
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char('u') => Action::HalfPageUp,
            KeyCode::Char('d') => Action::HalfPageDown,
            _ => Action::Noop,
        };
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('j') | KeyCode::Down => Action::MoveDown,
        KeyCode::Char('k') | KeyCode::Up => Action::MoveUp,
        KeyCode::Char('G') => Action::MoveToBottom,
        KeyCode::Char('g') => { *pending = Some('g'); Action::Noop }
        KeyCode::Char('d') => { *pending = Some('d'); Action::Noop }
        KeyCode::Enter => Action::CopyAndQuit,
        KeyCode::Char('/') | KeyCode::Char('i') => Action::EnterInsert,
        _ => Action::Noop,
    }
}

fn handle_insert_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => Action::ExitInsert,
        KeyCode::Enter => Action::CopyAndQuit,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::ClearSearch,
        KeyCode::Char(c) => Action::TypeChar(c),
        KeyCode::Up => Action::MoveUp,
        KeyCode::Down => Action::MoveDown,
        _ => Action::Noop,
    }
}

fn apply_action(app: &mut App, action: Action) {
    match action {
        Action::Noop => {}
        Action::Quit => app.should_quit = true,
        Action::CopyAndQuit => {
            if let Some(entry) = app.selected_entry() {
                clipboard::set_clipboard(&entry.content);
                app.copied_id = Some(entry.id);
            }
            app.should_quit = true;
        }
        Action::MoveUp => {
            app.selected = app.selected.saturating_sub(1);
        }
        Action::MoveDown => {
            if app.selected + 1 < app.filtered.len() {
                app.selected += 1;
            }
        }
        Action::MoveToTop => {
            app.selected = 0;
        }
        Action::MoveToBottom => {
            if !app.filtered.is_empty() {
                app.selected = app.filtered.len() - 1;
            }
        }
        Action::HalfPageUp => {
            let half = app.list_height / 2;
            app.selected = app.selected.saturating_sub(half.max(1));
        }
        Action::HalfPageDown => {
            let half = app.list_height / 2;
            if !app.filtered.is_empty() {
                app.selected = (app.selected + half.max(1)).min(app.filtered.len() - 1);
            }
        }
        Action::DeleteSelected => {
            if let Some(entry) = app.selected_entry() {
                let id = entry.id;
                app.store.delete(id).ok();
                app.refresh();
            }
        }
        Action::EnterInsert => {
            app.mode = Mode::Insert;
        }
        Action::ExitInsert => {
            app.mode = Mode::Normal;
            app.pending_key = None;
        }
        Action::TypeChar(c) => {
            app.query.push(c);
            app.refilter();
            app.reset_selection();
        }
        Action::Backspace => {
            app.query.pop();
            app.refilter();
            app.reset_selection();
        }
        Action::ClearSearch => {
            app.query.clear();
            app.refilter();
            app.reset_selection();
        }
    }
}

fn compute_filtered(entries: &[ClipEntry], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..entries.len()).collect();
    }

    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(usize, i64)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            matcher
                .fuzzy_match(&entry.content, query)
                .map(|score| (i, score))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1));
    scored.into_iter().map(|(i, _)| i).collect()
}

// --- Main loop ---

pub fn run(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let watcher = Watcher::spawn(db_path);

    let store = Store::open(db_path)?;
    let mut app = App::new(store);

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    watcher.stop();
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_refresh = Instant::now();

    loop {
        // Refresh entries from DB every second to pick up watcher inserts
        if last_refresh.elapsed() >= std::time::Duration::from_secs(1) {
            app.refresh();
            last_refresh = Instant::now();
        }

        terminal.draw(|f| render(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                app.copied_id = None;
                let action = handle_key(key, app.mode, &mut app.pending_key);
                apply_action(app, action);
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

// --- Rendering (pure view functions) ---

fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(render_search_bar(&app.query, app.mode), chunks[0]);

    let list_height = chunks[1].height as usize;
    app.list_height = list_height;
    adjust_scroll(app, list_height);
    f.render_widget(
        render_clip_list(&app.entries, &app.filtered, app.selected, app.scroll_offset, list_height, app.copied_id),
        chunks[1],
    );

    f.render_widget(
        render_status_bar(app.filtered.len(), app.entries.len(), app.copied_id, app.mode),
        chunks[2],
    );
}

fn render_search_bar(query: &str, mode: Mode) -> Paragraph<'static> {
    let border_color = match mode {
        Mode::Insert => Color::Green,
        Mode::Normal => Color::Cyan,
    };

    let (text, style) = match mode {
        Mode::Insert if query.is_empty() => {
            (" Type to search…".to_string(), Style::default().fg(Color::DarkGray))
        }
        Mode::Insert => {
            (format!(" {query}█"), Style::default().fg(Color::White))
        }
        Mode::Normal if query.is_empty() => {
            (" Press / to search".to_string(), Style::default().fg(Color::DarkGray))
        }
        Mode::Normal => {
            (format!(" {query}"), Style::default().fg(Color::White))
        }
    };

    let mode_label = match mode {
        Mode::Normal => " rippy [NORMAL] ",
        Mode::Insert => " rippy [INSERT] ",
    };

    Paragraph::new(text)
        .style(style)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .title(mode_label),
        )
}

fn render_clip_list<'a>(
    entries: &'a [ClipEntry],
    filtered: &[usize],
    selected: usize,
    scroll_offset: usize,
    list_height: usize,
    copied_id: Option<i64>,
) -> List<'a> {
    let items: Vec<ListItem> = filtered
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(list_height)
        .map(|(i, &entry_idx)| render_list_item(&entries[entry_idx], i == selected, copied_id))
        .collect();

    List::new(items).block(Block::default().borders(Borders::NONE))
}

fn render_list_item(entry: &ClipEntry, is_selected: bool, copied_id: Option<i64>) -> ListItem<'_> {
    let preview: String = entry.content.lines().next().unwrap_or("").chars().take(200).collect();
    let time = entry.timestamp.format("%m/%d %H:%M");

    let style = match (is_selected, Some(entry.id) == copied_id) {
        (true, _) => Style::default().bg(Color::DarkGray).fg(Color::White),
        (_, true) => Style::default().fg(Color::Green),
        _ => Style::default(),
    };

    let time_color = if is_selected { Color::Cyan } else { Color::DarkGray };

    ListItem::new(Line::from(vec![
        Span::styled(format!(" {time} "), style.patch(Style::default().fg(time_color))),
        Span::styled(format!("│ {preview}"), style),
    ]))
}

fn render_status_bar(count: usize, total: usize, copied_id: Option<i64>, mode: Mode) -> Paragraph<'static> {
    let (text, style) = if copied_id.is_some() {
        (" Copied! ".to_string(), Style::default().bg(Color::Green).fg(Color::Black))
    } else {
        let help = match mode {
            Mode::Normal => format!(" {count}/{total} │ j/k move │ Enter copy │ dd delete │ / search │ q quit"),
            Mode::Insert => format!(" {count}/{total} │ type to filter │ Enter copy │ Esc normal mode"),
        };
        (help, Style::default().bg(Color::DarkGray).fg(Color::White))
    };

    Paragraph::new(text).style(style)
}

fn adjust_scroll(app: &mut App, list_height: usize) {
    if app.selected < app.scroll_offset {
        app.scroll_offset = app.selected;
    }
    if app.selected >= app.scroll_offset + list_height {
        app.scroll_offset = app.selected - list_height + 1;
    }
}
