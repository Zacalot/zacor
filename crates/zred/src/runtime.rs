use crate::kernel::PackageInvocationRequest;
use crate::kernel::PaneId;
use crate::kernel::Viewport;
use crate::lua::LuaRuntime;
use crate::session::{
    PackageRunEvent, PackageRunResult, Session, SessionFrontendEffect, SessionLuaRuntime,
    SessionPackageRuntime, SessionResult, SharedSession,
};
use anyhow::Result;
use std::cell::Ref;

pub struct AppRuntime {
    state: SharedSession,
    lua: LuaRuntime,
    package_runner: Box<dyn SessionPackageRuntime>,
}

impl AppRuntime {
    pub fn new(state: SharedSession) -> Result<Self> {
        let lua = LuaRuntime::new(state.clone())?;
        let package_runner: Box<dyn SessionPackageRuntime> = Box::new(ZrPackageRuntime);
        Ok(Self {
            state,
            lua,
            package_runner,
        })
    }

    pub fn state(&self) -> Ref<'_, Session> {
        Session::borrow(&self.state)
    }

    pub fn drain_frontend_effects(&mut self) -> Vec<SessionFrontendEffect> {
        self.state.borrow_mut().drain_frontend_effects()
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.state.borrow_mut().set_status(status);
    }

    pub fn set_workspace_id(&mut self, workspace_id: crate::kernel::WorkspaceId) {
        self.state.borrow_mut().workspace_mut().set_id(workspace_id);
    }

    #[cfg(test)]
    pub fn state_mut(&self) -> std::cell::RefMut<'_, Session> {
        self.state.borrow_mut()
    }

    pub fn runtimes(&mut self) -> (&mut dyn SessionLuaRuntime, &mut dyn SessionPackageRuntime) {
        (&mut self.lua, self.package_runner.as_mut())
    }

    pub fn focus_pane(&mut self, pane_id: PaneId) -> bool {
        self.state.borrow_mut().workspace_mut().focus_pane(pane_id)
    }

    pub fn adjust_active_pane_viewport(&mut self, delta_x: isize, delta_y: isize) -> bool {
        let mut session = self.state.borrow_mut();
        let pane_id = session.workspace().active_pane_id();
        let Some(pane) = session.workspace_mut().pane_mut(pane_id) else {
            return false;
        };
        let viewport = pane.viewport();
        pane.set_viewport(Viewport::new(
            viewport.offset_x.saturating_add_signed(delta_x),
            viewport.offset_y.saturating_add_signed(delta_y),
        ));
        true
    }

    pub fn select_record_row_in_active_pane(&mut self, row: usize) -> bool {
        self.state
            .borrow_mut()
            .workspace_mut()
            .select_record_row(row)
    }

    pub fn select_tree_node_in_active_pane(&mut self, node_id: &str) -> bool {
        self.state
            .borrow_mut()
            .workspace_mut()
            .select_tree_node(node_id.to_string())
    }

    pub fn open_active_structured_selection(&mut self) {
        let result = self
            .state
            .borrow_mut()
            .dispatch_command("buffer.structured.open");
        Session::apply_command_result_shared(
            &self.state,
            result,
            &mut self.lua,
            self.package_runner.as_mut(),
        );
    }

    pub fn run_command(&mut self, command: &str) {
        let result = self.state.borrow_mut().dispatch_command(command);
        Session::apply_command_result_shared(
            &self.state,
            result,
            &mut self.lua,
            self.package_runner.as_mut(),
        );
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
        zr_dispatch::invoke_local_with_events(
            &home,
            &request.package,
            &request.command,
            &request.args,
            &mut |event| {
                match event {
                    zr_dispatch::InvocationEvent::Record(record) => {
                        on_event(PackageRunEvent::Record(record));
                    }
                    zr_dispatch::InvocationEvent::Progress(fraction) => {
                        on_event(PackageRunEvent::Message {
                            level: crate::kernel::MessageLevel::Info,
                            text: format!(
                                "{} {}: {}%",
                                request.package,
                                request.command,
                                (fraction.clamp(0.0, 1.0) * 100.0).round() as u32
                            ),
                        });
                    }
                    zr_dispatch::InvocationEvent::Message { level, text } => {
                        on_event(PackageRunEvent::Message {
                            level: match level {
                                zr_dispatch::InvocationMessageLevel::Info => {
                                    crate::kernel::MessageLevel::Info
                                }
                                zr_dispatch::InvocationMessageLevel::Warning => {
                                    crate::kernel::MessageLevel::Warning
                                }
                                zr_dispatch::InvocationMessageLevel::Error => {
                                    crate::kernel::MessageLevel::Error
                                }
                            },
                            text,
                        });
                    }
                }
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
