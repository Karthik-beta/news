//! Board, the desktop app (ADR 0007 decision 3): egui inside `news.exe`, started by `news board`. It
//! talks only to the Bureau service, over the Hotline named pipe; it opens no listener and reads no
//! database. Karthik's clicks are the only source of H-tier commands, which the service accepts only
//! from the installed `news.exe` started as `news board`.
//!
//! - `link`: the Hotline connection (requests and the event stream), generic over its transport.
//! - `app`: the state and what each control does; `logic`: forms into command arguments.
//! - `ui`: the window; `theme`: egui styles generated from `design/tokens/tokens.json`.
//! - `toast`: Windows toasts from events; `secrets`: Credential Manager, never the pipe.
//!
//! The window needs Windows; everything else builds and is tested everywhere.

pub mod app;
pub mod args;
pub mod link;
pub mod logic;
pub mod secrets;
#[cfg(windows)]
mod shell;
pub mod theme;
pub mod toast;
pub mod ui;

use args::{Action, Autostart};

/// `news board ...` (argv after `board`); the process exit code.
pub fn run(argv: &[String]) -> i32 {
    let args = match args::parse(argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("news board: {e}\n\n{}", args::HELP);
            return 2;
        }
    };
    match args.action {
        Action::Help => {
            println!("{}", args::HELP);
            0
        }
        Action::Autostart(a) => autostart(a),
        Action::Run => run_window(args),
    }
}

fn autostart(a: Autostart) -> i32 {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("cannot find news.exe: {e}");
            return 1;
        }
    };
    let result = match a {
        Autostart::On => {
            news_win::set_logon_entry(Some(&format!("\"{}\" board --minimized", exe.display())))
                .map(|()| "Board will open minimised when you log on.".to_owned())
        }
        Autostart::Off => {
            news_win::set_logon_entry(None).map(|()| "Board's log-on entry is removed.".to_owned())
        }
        Autostart::Status => Ok(if news_win::logon_entry_exists() {
            "Board opens minimised at log-on.".to_owned()
        } else {
            "Board has no log-on entry.".to_owned()
        }),
    };
    match result {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(e) => {
            eprintln!("cannot change the log-on entry: {e}");
            1
        }
    }
}

#[cfg(windows)]
fn run_window(args: args::Args) -> i32 {
    shell::run(args)
}

#[cfg(not(windows))]
fn run_window(_args: args::Args) -> i32 {
    eprintln!("Board is a Windows app: run `news board` on the laptop.");
    1
}
