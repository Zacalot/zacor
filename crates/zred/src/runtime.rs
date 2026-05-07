use crate::frontends::tui::input::TuiInputController;
use crate::kernel::PackageInvocationRequest;
use crate::lua::LuaRuntime;
use crate::session::{
    PackageRunEvent, PackageRunResult, Session, SessionLuaRuntime, SessionPackageRuntime,
    SessionResult, SharedSession,
};
use anyhow::Result;
use crossterm::event::KeyEvent;
use std::cell::Ref;

pub struct AppRuntime {
    input: TuiInputController,
    lua: LuaRuntime,
    package_runner: Box<dyn SessionPackageRuntime>,
}

impl AppRuntime {
    pub fn new(state: SharedSession) -> Result<Self> {
        let lua = LuaRuntime::new(state.clone())?;
        let input = TuiInputController::new(state);
        let package_runner: Box<dyn SessionPackageRuntime> = Box::new(ZrPackageRuntime);
        Ok(Self {
            input,
            lua,
            package_runner,
        })
    }

    pub fn state(&self) -> Ref<'_, Session> {
        self.input.state()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        self.input
            .handle_key(key, &mut self.lua, &mut self.package_runner);
    }

    #[cfg(test)]
    pub fn run_command(&mut self, command: &str) {
        self.input
            .run_command(command, &mut self.lua, self.package_runner.as_mut());
    }

    #[cfg(test)]
    pub fn set_package_runner(&mut self, runner: impl SessionPackageRuntime + 'static) {
        self.package_runner = Box::new(runner);
    }
}

struct ZrPackageRuntime;

impl SessionPackageRuntime for ZrPackageRuntime {
    fn invoke_package(
        &mut self,
        request: &PackageInvocationRequest,
        on_event: &mut dyn FnMut(PackageRunEvent),
    ) -> SessionResult<PackageRunResult> {
        let home = zacor_host::paths::zr_home()
            .map_err(|error| format!("failed to resolve zr home: {error:#}"))?;
        zr_dispatch::invoke_local(
            &home,
            &request.package,
            &request.command,
            &request.args,
            &mut |record| {
                on_event(PackageRunEvent::Record(record));
                Ok(())
            },
        )
        .map(|exit_code| PackageRunResult { exit_code })
        .map_err(|error| format!("package dispatch failed: {error:#}"))
    }
}

impl SessionLuaRuntime for LuaRuntime {
    fn eval(&mut self, script: &str) -> SessionResult<()> {
        LuaRuntime::eval(self, script).map_err(|error| format!("Lua error: {error:#}"))
    }
}
