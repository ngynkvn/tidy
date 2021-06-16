use chrono::prelude::*;
use crossterm::event::{read, Event, KeyCode, KeyEvent};
use crossterm::QueueableCommand;
use crossterm::{cursor, style, terminal, ExecutableCommand};
use rusqlite::{params, Connection, Result};
use std::alloc::System;
use std::fmt::Display;
use std::fs;
use std::fs::DirEntry;
use std::fs::Metadata;
use std::fs::ReadDir;
use std::io;
use std::io::Error;
use std::io::{stdout, Write};
use std::path::PathBuf;
use std::time::SystemTime;
use structopt::StructOpt;
use tui::widgets::Wrap;
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Tabs,
    },
    Terminal,
};

struct MainContext {
    file_list_state: ListState,
    selection: Vec<PathBuf>,
}
struct TaggingContext {
    tag_input: Vec<String>,
}

enum Context {
    Main(MainContext),
    Tagging(TaggingContext),
}

struct State {
    path: String,
    file_list_state: ListState,
    context: Context,
}

#[derive(PartialEq)]
enum Command {
    Quit,
    None,
    CursorUp,
    CursorDown,
    Tag,
}

impl State {
    fn handle_key(&mut self, event: KeyEvent) -> Command {
        match event {
            KeyEvent {
                code: KeyCode::Char('q'),
                ..
            } => Command::Quit,

            KeyEvent {
                code: KeyCode::Up, ..
            }
            | KeyEvent {
                code: KeyCode::Char('k'),
                ..
            } => Command::CursorUp,

            KeyEvent {
                code: KeyCode::Char('j'),
                ..
            }
            | KeyEvent {
                code: KeyCode::Down,
                ..
            } => Command::CursorDown,

            KeyEvent {
                code: KeyCode::Char('t'),
                ..
            } => Command::Tag,

            _ => Command::None,
        }
    }

    fn files(&self) -> Vec<PathBuf> {
        let files: Vec<PathBuf> = fs::read_dir(self.path.clone())
            .map(|dir: ReadDir| {
                dir.map(|res: Result<DirEntry, Error>| {
                    res.map(|entry: DirEntry| entry.path().canonicalize().unwrap())
                })
            })
            .unwrap()
            .flatten()
            .collect();
        files
    }

    fn new(opts: Opts) -> Result<Self> {
        let directory = opts
            .directory
            .or(std::env::current_dir().ok())
            .unwrap()
            .canonicalize()
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap();

        let mut file_list_state = ListState::default();
        file_list_state.select(Some(0));

        Ok(State {
            path: directory,
            file_list_state,
            context: Context::Main(MainContext {
                file_list_state: ListState::default(),
                selection: vec![],
            }),
        })
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "tidy", about = "An example of StructOpt usage.")]
struct Opts {
    #[structopt(parse(from_os_str))]
    directory: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let opts = Opts::from_args();

    let mut state = State::new(opts)?;
    let conn = Connection::open("tidy.db")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS dirs (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE
            )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL,
                path_id INTEGER NOT NULL REFERENCES dirs(id)
            )",
        [],
    )?;
    conn.execute(
        "INSERT OR IGNORE INTO dirs (path) VALUES (?)",
        [state.path.clone()],
    )
    .expect("SQL Failed");

    let mut select = conn.prepare("SELECT id FROM dirs WHERE path = ?")?;

    if let Some(Ok(name)) = select
        .query_map::<u32, _, _>([state.path.clone()], |row| row.get(0))?
        .next()
    {
        let mut stmt = conn.prepare("INSERT OR IGNORE INTO files (path, path_id) VALUES (?, ?)")?;
        for path in state.files() {
            stmt.insert(params![
                path.into_os_string()
                    .into_string()
                    .expect("Could not convert to string"),
                name
            ])?;
        }
    }

    loop {
        let now = SystemTime::now();

        let files = state.files();

        // UI Loop
        terminal.draw(|rect| {
            let size = rect.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(3),
                        Constraint::Min(5),
                        Constraint::Length(4),
                    ]
                    .as_ref(),
                )
                .split(size);
            let command_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title("Command")
                .border_type(BorderType::Plain);
            rect.render_widget(command_block, chunks[0]);

            let file_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title(state.path.clone())
                .border_type(BorderType::Plain);

            let items: Vec<_> = files
                .iter()
                .map(|file| {
                    let meta = fs::metadata(file).unwrap();
                    let icon = match meta.is_dir() {
                        true => "📁",
                        false => "📄",
                    };
                    ListItem::new(Span::styled(
                        format!("{}{}", icon, file.display()),
                        Style::default(),
                    ))
                })
                .collect();
            let list = List::new(items).block(file_block).highlight_style(
                Style::default()
                    .bg(Color::Yellow)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            );

            rect.render_stateful_widget(list, chunks[1], &mut state.file_list_state);

            let mut info_str = String::new();
            if let Some(selected) = state.file_list_state.selected() {
                let file = &state.files()[selected];
                let metadata = fs::metadata(file).expect("Unable to open metadata for file.");
                info_str = metadata_str(metadata);
            }
            let time = now.elapsed().unwrap().as_millis();
            info_str += &format!("\n Render Time: {}", time);

            let info = Paragraph::new(info_str)
                .style(Style::default().fg(Color::LightCyan))
                .wrap(Wrap { trim: true })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .style(Style::default().fg(Color::White))
                        .title("Info")
                        .border_type(BorderType::Plain),
                );

            rect.render_widget(info, chunks[2]);
        })?;
        // Event Loop, Blocking
        let command = match read().unwrap() {
            Event::Key(event) => state.handle_key(event),
            Event::Mouse(_event) => Command::None,
            Event::Resize(_width, _height) => Command::None,
        };

        // Event parsing
        match command {
            Command::Quit => {
                terminal.clear()?;
                break Ok(());
            }
            Command::CursorUp => {
                if let Some(selected) = state.file_list_state.selected() {
                    let len = state.files().len();
                    if selected > 0 {
                        state.file_list_state.select(Some(selected - 1));
                    } else {
                        state.file_list_state.select(Some(len - 1));
                    }
                }
            }
            Command::CursorDown => {
                if let Some(selected) = state.file_list_state.selected() {
                    let len = state.files().len();
                    if selected >= len - 1 {
                        state.file_list_state.select(Some(0));
                    } else {
                        state.file_list_state.select(Some(selected + 1));
                    }
                }
            }
            Command::None => {}
            Command::Tag => {}
        }
    }
}

fn metadata_str(metadata: Metadata) -> String {
    let formatter = |date: SystemTime| {
        DateTime::<Utc>::from(date)
            .format("%a %b %e %T %Y")
            .to_string()
    };
    let created = metadata.created().map(formatter).unwrap();
    let accessed = metadata.accessed().map(formatter).unwrap();
    let modified = metadata.modified().map(formatter).unwrap();
    format!(
        "Created: {}, Accessed: {}, Modified: {}",
        created, accessed, modified
    )
}
