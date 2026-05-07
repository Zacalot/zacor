use anyhow::Result;

mod app;
mod compositor;
mod input;
mod scene;
mod surface;
mod window;

pub fn run() -> Result<()> {
    app::run()
}
