use crossterm::event::{read, Event, KeyCode, KeyEvent};
use crossterm::QueueableCommand;
use crossterm::{cursor, style, terminal, ExecutableCommand};
use rusqlite::{params, Connection, Result};
use std::fs;
use std::fs::DirEntry;
use std::fs::ReadDir;
use std::io;
use std::io::Error;
use std::io::{stdout, Write};
use std::path::PathBuf;
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

#[derive(Default)]
struct State {
    path: String,
    file_list_state: ListState,
}

#[derive(PartialEq)]
enum Command {
    Quit,
    None,
    CursorUp,
    CursorDown,
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
            } => Command::CursorUp,
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => Command::CursorDown,
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

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Ok(State {
            path: directory,
            file_list_state: list_state,
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

    loop {
        // UI Loop
        terminal.draw(|rect| {
            let size = rect.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(2)
                .constraints(
                    [
                        Constraint::Length(0),
                        Constraint::Min(2),
                        Constraint::Length(4),
                    ]
                    .as_ref(),
                )
                .split(size);
            let file_block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title(state.path.clone())
                .border_type(BorderType::Plain);

            let file_list = state.files();
            let items: Vec<_> = file_list
                .iter()
                .map(|file| {
                    ListItem::new(Span::styled(
                        format!("{}", file.display()),
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
                let metadata = fs::metadata(file);
                info_str = format!("{:?}", metadata);
            }

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
        }
    }
}
