#![warn(clippy::all)]

mod app;
mod frontends;
mod kernel;
mod lua;
mod runtime;
mod session;
mod shell;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("error: {:#}", error);
        std::process::exit(1);
    }
}
