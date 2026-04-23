mod config_cmd;
mod disable;
mod enable;
mod init;
mod install;
mod list;
mod registry_cmd;
mod remove;
mod update;
mod use_cmd;
mod zacor;

pub fn run_zacor() -> i32 {
    zacor::run()
}
