use crate::clipboard;
use crate::db::{ClipEntry, Store};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::prelude::*;
use ratatui::widgets::*;
use std::io::stdout;
use std::path::Path;

struct App {
    store: Store,
    entries: Vec<ClipEntry>,
    filtered: Vec<usize>, // indices into entries
    query: String,
    selected: usize,
    scroll_offset: usize,
    should_quit: bool,
    copied_id: Option<i64>,
}

impl App {
    fn new(store: Store) -> Self {
        let entries = store.all().unwrap_or_default();
        let filtered: Vec<usize> = (0..entries.len()).collect();
        App {
            store,
            entries,
            filtered,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
            should_quit: false,
            copied_id: None,
        }
    }

    fn refresh(&mut self) {
        self.entries = self.store.all().unwrap_or_default();
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        if self.query.is_empty() {
            self.filtered = (0..self.entries.len()).collect();
        } else {
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(usize, i64)> = self
                .entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    matcher
                        .fuzzy_match(&entry.content, &self.query)
                        .map(|score| (i, score))
                })
                .collect();
            scored.sort_by(|a, b| b.1.cmp(&a.1));
            self.filtered = scored.into_iter().map(|(i, _)| i).collect();
        }
        self.selected = 0;
        self.scroll_offset = 0;
    }

    fn selected_entry(&self) -> Option<&ClipEntry> {
        self.filtered
            .get(self.selected)
            .map(|&i| &self.entries[i])
    }

    fn copy_selected(&mut self) {
        if let Some(entry) = self.selected_entry() {
            clipboard::set_clipboard(&entry.content);
            self.copied_id = Some(entry.id);
        }
    }

    fn delete_selected(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let id = entry.id;
            self.store.delete(id).ok();
            self.refresh();
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        if self.selected + 1 < self.filtered.len() {
            self.selected += 1;
        }
    }
}

pub fn run(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let store = Store::open(db_path)?;
    let mut app = App::new(store);

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                app.copied_id = None;

                match key.code {
                    KeyCode::Esc => app.should_quit = true,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Up | KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.move_up();
                    }
                    KeyCode::Down | KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.move_down();
                    }
                    KeyCode::Enter => {
                        app.copy_selected();
                        app.should_quit = true;
                    }
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.delete_selected();
                    }
                    KeyCode::Backspace => {
                        app.query.pop();
                        app.apply_filter();
                    }
                    KeyCode::Char(c) => {
                        app.query.push(c);
                        app.apply_filter();
                    }
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // search bar
            Constraint::Min(1),   // list
            Constraint::Length(1), // status bar
        ])
        .split(area);

    // Search bar
    let search_text = if app.query.is_empty() {
        " Type to search…".to_string()
    } else {
        format!(" {}", app.query)
    };
    let search = Paragraph::new(search_text)
        .style(if app.query.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" rippy "),
        );
    f.render_widget(search, chunks[0]);

    // Clip list
    let list_height = chunks[1].height as usize;

    // Adjust scroll so selected item is visible
    if app.selected < app.scroll_offset {
        app.scroll_offset = app.selected;
    }
    if app.selected >= app.scroll_offset + list_height {
        app.scroll_offset = app.selected - list_height + 1;
    }

    let items: Vec<ListItem> = app
        .filtered
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(list_height)
        .map(|(i, &entry_idx)| {
            let entry = &app.entries[entry_idx];
            let preview = entry
                .content
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(200)
                .collect::<String>();

            let time = entry.timestamp.format("%m/%d %H:%M");

            let style = if i == app.selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if Some(entry.id) == app.copied_id {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::styled(
                    format!(" {time} "),
                    style.patch(Style::default().fg(if i == app.selected {
                        Color::Cyan
                    } else {
                        Color::DarkGray
                    })),
                ),
                Span::styled(format!("│ {preview}"), style),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::NONE));
    f.render_widget(list, chunks[1]);

    // Status bar
    let count = app.filtered.len();
    let total = app.entries.len();
    let status_text = if let Some(_) = app.copied_id {
        " Copied! ".to_string()
    } else {
        format!(
            " {count}/{total} │ ↑↓ navigate │ Enter copy │ Ctrl+D delete │ Esc quit"
        )
    };
    let status_style = if app.copied_id.is_some() {
        Style::default().bg(Color::Green).fg(Color::Black)
    } else {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    };
    let status = Paragraph::new(status_text).style(status_style);
    f.render_widget(status, chunks[2]);
}
