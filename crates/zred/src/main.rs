#![warn(clippy::all)]

mod app;
mod events;
mod lua;
mod ui;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("error: {:#}", error);
        std::process::exit(1);
    }
}
