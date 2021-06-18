use std::{
    any::TypeId,
    fs::{self, Metadata},
    io,
    path::PathBuf,
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use crossterm::{
    event::{KeyCode, KeyEvent},
    terminal,
};
use tui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, ListState, Paragraph, Row, Table, Tabs,
        Wrap,
    },
    Terminal,
};

use crate::{Command, DirInfo, Msg, Signal};

pub trait Ctx {
    fn render(&mut self, rect: &mut tui::Frame<CrosstermBackend<io::Stdout>>, di: DirInfo);
    fn handle_key(&mut self, key: KeyEvent, di: DirInfo) -> Option<Signal>;
    fn send(&mut self, msg: Msg);
}

pub struct MainContext {
    pub file_list_state: ListState,
    pub selection: Vec<PathBuf>,
}
impl Ctx for MainContext {
    fn render(&mut self, rect: &mut tui::Frame<CrosstermBackend<io::Stdout>>, state: DirInfo) {
        let size = rect.size();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Min(5),
                    Constraint::Length(3),
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
        let items: Vec<_> = state
            .files
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
        rect.render_stateful_widget(list, chunks[0], &mut self.file_list_state);
        let mut info_str = String::new();
        if let Some(selected) = self.file_list_state.selected() {
            let file = &state.files[selected];
            let metadata = fs::metadata(file).expect("Unable to open metadata for file.");
            info_str = metadata_str(metadata);
        }

        let command_block = Paragraph::new("(t)ag").block(
            Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(Color::White))
                .title("Commands")
                .border_type(BorderType::Plain),
        );
        rect.render_widget(command_block, chunks[1]);
        // let time = now.elapsed().unwrap().as_millis();
        // info_str += &format!("\n Render Time: {}", time);
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
    }

    fn handle_key(&mut self, event: KeyEvent, state: DirInfo) -> Option<Signal> {
        let command = match event {
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
        };

        match command {
            Command::CursorUp => {
                if let Some(selected) = self.file_list_state.selected() {
                    let len = state.files.len();
                    if selected > 0 {
                        self.file_list_state.select(Some(selected - 1));
                    } else {
                        self.file_list_state.select(Some(len - 1));
                    }
                }
            }
            Command::CursorDown => {
                if let Some(selected) = self.file_list_state.selected() {
                    let len = state.files.len();
                    if selected >= len - 1 {
                        self.file_list_state.select(Some(0));
                    } else {
                        self.file_list_state.select(Some(selected + 1));
                    }
                }
            }
            Command::None => {}
            Command::Tag => {
                let new_ctx = TypeId::of::<TaggingContext>();
                return Some(Signal::Change(new_ctx).and(Signal::Message(
                    new_ctx,
                    Msg::File(state.files[self.file_list_state.selected().unwrap()].clone()),
                )));
            }
            Command::Quit => return Some(Signal::Quit),
        };
        None
    }

    fn send(&mut self, msg: Msg) {}
}

pub struct TaggingContext {
    pub tag_input: Vec<String>,
    pub file_path: Option<PathBuf>,
}
impl Ctx for TaggingContext {
    fn render(&mut self, rect: &mut tui::Frame<CrosstermBackend<io::Stdout>>, di: DirInfo) {
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
            .title("Tag Screen")
            .border_type(BorderType::Plain);

        let paragraph = Paragraph::new(format!("{:?}", self.file_path));
        rect.render_widget(paragraph, chunks[0]);
        rect.render_widget(command_block, chunks[1]);
    }

    fn handle_key(&mut self, key: KeyEvent, di: DirInfo) -> Option<Signal> {
        match key {
            KeyEvent {
                code: KeyCode::Char('q'),
                ..
            } => Some(Signal::Change(TypeId::of::<MainContext>())),
            _ => None,
        }
    }
    fn send(&mut self, msg: Msg) {
        match msg {
            Msg::File(path) => self.file_path = Some(path),
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
