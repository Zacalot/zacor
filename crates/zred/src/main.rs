#![warn(clippy::all)]

mod app;
mod frontends;
mod kernel;
mod lua;
mod runtime;
mod session;

fn main() {
    if let Err(error) = app::run() {
        eprintln!("error: {:#}", error);
        std::process::exit(1);
    }
}
