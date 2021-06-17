mod ctx;
use ctx::{Ctx, MainContext, TaggingContext};

use crossterm::event::{read, Event, KeyEvent};
use rusqlite::{params, Connection, Result};
use std::any::TypeId;
use std::collections::HashMap;
use std::fs;
use std::fs::DirEntry;
use std::fs::ReadDir;
use std::io::Error;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::mpsc;
use structopt::StructOpt;
use tui::Frame;
use tui::{backend::CrosstermBackend, widgets::ListState, Terminal};

struct Channel {
    sender: mpsc::Sender<Signal>,
    receiver: mpsc::Receiver<Signal>,
}

impl Channel {
    fn new() -> Channel {
        let (tx, rx) = mpsc::channel();
        Channel {
            sender: tx,
            receiver: rx,
        }
    }

    fn send(&mut self, msg: Signal) {
        match msg {
            Signal::And(s1, s2) => {
                self.sender.send(*s1);
                self.sender.send(*s2);
            }
            msg => {
                self.sender.send(msg);
            }
        }
    }
}
struct State {
    info: DirInfo,
    context: TypeId,
    channel: Channel,
    ctx_map: HashMap<TypeId, Box<dyn Ctx>>,
}

#[derive(PartialEq)]
enum Command {
    Quit,
    None,
    CursorUp,
    CursorDown,
    Tag,
}

pub enum Msg {
    File(PathBuf),
}

pub enum Signal {
    Quit,
    Change(TypeId),
    Message(TypeId, Msg),
    And(Box<Signal>, Box<Signal>),
}

impl Signal {
    fn and(self, other: Signal) -> Signal {
        Signal::And(Box::new(self), Box::new(other))
    }
}

#[derive(Clone)]
pub struct DirInfo {
    files: Vec<PathBuf>,
    path: String,
}

impl State {
    fn handle_key(&mut self, event: KeyEvent) {
        let signal = self
            .ctx_map
            .get_mut(&self.context)
            .expect("Context not found.")
            .handle_key(event, self.info.clone());

        if let Some(signal) = signal {
            self.channel.send(signal);
        }
    }

    fn render(&mut self, rect: &mut Frame<CrosstermBackend<Stdout>>) {
        self.ctx_map
            .get_mut(&self.context)
            .expect("Context not found.")
            .render(rect, self.info.clone());
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

        let main_ctx = MainContext {
            file_list_state,
            selection: vec![],
        };

        let tag_ctx = TaggingContext {
            tag_input: vec![],
            file_path: None,
        };

        let mut ctx_map: HashMap<TypeId, Box<dyn Ctx>> = HashMap::new();
        ctx_map.insert(TypeId::of::<MainContext>(), Box::new(main_ctx));
        ctx_map.insert(TypeId::of::<TaggingContext>(), Box::new(tag_ctx));

        let files: Vec<PathBuf> = fs::read_dir(directory.clone())
            .map(|dir: ReadDir| {
                dir.map(|res: Result<DirEntry, Error>| {
                    res.map(|entry: DirEntry| entry.path().canonicalize().unwrap())
                })
            })
            .unwrap()
            .flatten()
            .collect();

        Ok(State {
            info: DirInfo {
                path: directory,
                files,
            },
            channel: Channel::new(),
            ctx_map,
            context: TypeId::of::<MainContext>(),
        })
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "tidy", about = "A tui file explorer in rust")]
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
        [state.info.path.clone()],
    )?;

    let mut select = conn.prepare("SELECT id FROM dirs WHERE path = ?")?;

    if let Some(Ok(name)) = select
        .query_map::<u32, _, _>([state.info.path.clone()], |row| row.get(0))?
        .next()
    {
        let mut stmt = conn.prepare("INSERT OR IGNORE INTO files (path, path_id) VALUES (?, ?)")?;
        for path in &state.info.files {
            stmt.insert(params![
                path.clone()
                    .into_os_string()
                    .into_string()
                    .expect("Could not convert to string"),
                name
            ])?;
        }
    }

    loop {
        // UI Loop
        terminal.draw(|rect| {
            state.render(rect);
        })?;
        // Event Loop, Blocking
        match read().unwrap() {
            Event::Key(event) => state.handle_key(event),
            Event::Mouse(_event) => {}
            Event::Resize(_width, _height) => {}
        };

        for signal in state.channel.receiver.try_iter() {
            match signal {
                Signal::Quit => {
                    terminal.clear()?;
                    return Ok(());
                }
                Signal::Change(context) => {
                    state.context = context;
                }
                Signal::Message(context, msg) => state
                    .ctx_map
                    .get_mut(&context)
                    .expect("Context not found.")
                    .send(msg),
                _ => {}
            }
        }
    }
}
