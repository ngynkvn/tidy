use crossterm::event::{read, Event, KeyCode, KeyEvent};
use crossterm::QueueableCommand;
use crossterm::{cursor, style, terminal, ExecutableCommand};
use rusqlite::{params, Connection, Result};
use std::fs;
use std::io;
use std::io::{stdout, Write};
use std::path::PathBuf;
use structopt::StructOpt;

struct State {
    path: String,
}

#[derive(PartialEq)]
enum Command {
    Quit,
    None,
}

impl State {
    fn render(&self, stdout: &mut std::io::Stdout) {
        let size = terminal::size();
        stdout.queue(cursor::MoveTo(0, 0));
        let paths = fs::read_dir(self.path.clone()).unwrap();
        for path in paths {
            println!("{}", path.unwrap().path().canonicalize().unwrap().display())
        }
    }
    fn handle_key(&mut self, event: KeyEvent) -> Command {
        match event {
            KeyEvent {
                code: KeyCode::Char('q'),
                ..
            } => Command::Quit,
            _ => Command::None,
        }
    }

    fn new(opts: Opts) -> Result<Self> {
        let directory = opts
            .directory
            .or(std::env::current_dir().ok())
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap();
        let conn = Connection::open("cats.db")?;

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

        Ok(State { path: directory })
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "tidy", about = "An example of StructOpt usage.")]
struct Opts {
    #[structopt(parse(from_os_str))]
    directory: Option<PathBuf>,
}

fn main() -> Result<(), crossterm::ErrorKind> {
    let mut stdout = std::io::stdout();
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    let opts = Opts::from_args();

    let mut state = State::new(opts).unwrap();

    loop {
        // UI Loop
        state.render(&mut stdout);
        // Event Loop, Blocking
        let command = match read().unwrap() {
            Event::Key(event) => state.handle_key(event),
            Event::Mouse(_event) => Command::None,
            Event::Resize(_width, _height) => Command::None,
        };

        // Event parsing
        if command == Command::Quit {
            break Ok(());
        }
    }
}
