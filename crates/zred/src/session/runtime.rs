use super::{Session, SessionResult, SharedSession};
use crate::kernel::{
    CommandEffect, CommandResult, JobStatus, MessageLevel, PackageInvocationRequest,
};
use serde_json::Value;

pub trait SessionLuaRuntime {
    fn eval(&mut self, script: &str) -> SessionResult<()>;
}

pub trait SessionPackageRuntime {
    fn invoke_package(
        &mut self,
        request: &PackageInvocationRequest,
        on_event: &mut dyn FnMut(PackageRunEvent),
    ) -> SessionResult<PackageRunResult>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageRunResult {
    pub exit_code: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageRunEvent {
    Record(Value),
    Message { level: MessageLevel, text: String },
}

impl<F> SessionLuaRuntime for F
where
    F: FnMut(&str) -> SessionResult<()>,
{
    fn eval(&mut self, script: &str) -> SessionResult<()> {
        self(script)
    }
}

impl<F> SessionPackageRuntime for F
where
    F: FnMut(
        &PackageInvocationRequest,
        &mut dyn FnMut(PackageRunEvent),
    ) -> SessionResult<PackageRunResult>,
{
    fn invoke_package(
        &mut self,
        request: &PackageInvocationRequest,
        on_event: &mut dyn FnMut(PackageRunEvent),
    ) -> SessionResult<PackageRunResult> {
        self(request, on_event)
    }
}

impl SessionPackageRuntime for Box<dyn SessionPackageRuntime> {
    fn invoke_package(
        &mut self,
        request: &PackageInvocationRequest,
        on_event: &mut dyn FnMut(PackageRunEvent),
    ) -> SessionResult<PackageRunResult> {
        self.as_mut().invoke_package(request, on_event)
    }
}

impl Session {
    pub fn apply_command_result(
        shared: &SharedSession,
        result: CommandResult,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) -> SessionResult<()> {
        let (effects, _data, error) = result.into_parts();
        for effect in effects {
            Self::apply_command_effect(shared, effect, lua_runtime, package_runtime)?;
        }
        if let Some(error) = error {
            return Err(error);
        }
        Ok(())
    }

    pub fn apply_command_result_shared(
        shared: &SharedSession,
        result: CommandResult,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) {
        if let Err(error) = Self::apply_command_result(shared, result, lua_runtime, package_runtime)
        {
            shared.borrow_mut().set_status(error);
        }
    }

    fn apply_command_effect(
        shared: &SharedSession,
        effect: CommandEffect,
        lua_runtime: &mut impl SessionLuaRuntime,
        package_runtime: &mut dyn SessionPackageRuntime,
    ) -> SessionResult<()> {
        match effect {
            CommandEffect::SetStatus(status) => {
                shared.borrow_mut().set_status(status);
                Ok(())
            }
            CommandEffect::Quit => {
                shared.borrow_mut().request_quit();
                Ok(())
            }
            CommandEffect::EvalLua(script) => lua_runtime.eval(&script),
            CommandEffect::InvokePackage(request) => {
                {
                    let updated = shared
                        .borrow_mut()
                        .workspace_mut()
                        .set_job_status(request.job_id, JobStatus::Running);
                    if !updated {
                        return Err(format!("unknown job id: {}", request.job_id));
                    }
                }

                let mut record_count = 0usize;
                let mut event_error = None;
                let mut on_event = |event| {
                    if event_error.is_some() {
                        return;
                    }
                    if matches!(event, PackageRunEvent::Record(_)) {
                        record_count += 1;
                    }
                    if let Err(error) = Self::apply_package_event(shared, &request, event) {
                        event_error = Some(error);
                    }
                };

                match package_runtime.invoke_package(&request, &mut on_event) {
                    Ok(result) => {
                        if let Some(error) = event_error {
                            return Err(error);
                        }
                        let mut session = shared.borrow_mut();
                        if result.exit_code == 0 {
                            let status = format!(
                                "Finished {} {} ({} records)",
                                request.package, request.command, record_count
                            );
                            session
                                .workspace_mut()
                                .set_job_status(request.job_id, JobStatus::Succeeded);
                            session.workspace_mut().messages_mut().info(status.clone());
                            session.set_status(status);
                            Ok(())
                        } else {
                            let status = format!(
                                "Package {} {} failed with status {}",
                                request.package, request.command, result.exit_code
                            );
                            session
                                .workspace_mut()
                                .set_job_status(request.job_id, JobStatus::Failed(status.clone()));
                            session.workspace_mut().messages_mut().warn(status.clone());
                            Err(status)
                        }
                    }
                    Err(error) => {
                        if let Some(error) = event_error {
                            return Err(error);
                        }
                        let status = format!(
                            "Package {} {} failed: {}",
                            request.package, request.command, error
                        );
                        let mut session = shared.borrow_mut();
                        session
                            .workspace_mut()
                            .set_job_status(request.job_id, JobStatus::Failed(status.clone()));
                        session.workspace_mut().messages_mut().error(status.clone());
                        Err(status)
                    }
                }
            }
        }
    }

    fn apply_package_event(
        shared: &SharedSession,
        request: &PackageInvocationRequest,
        event: PackageRunEvent,
    ) -> SessionResult<()> {
        let mut session = shared.borrow_mut();
        match event {
            PackageRunEvent::Record(record) => {
                if session
                    .workspace_mut()
                    .push_record_to_buffer(request.buffer_id, record)
                {
                    Ok(())
                } else {
                    Err(format!("unknown buffer id: {}", request.buffer_id))
                }
            }
            PackageRunEvent::Message { level, text } => {
                match level {
                    MessageLevel::Info => session.workspace_mut().messages_mut().info(text.clone()),
                    MessageLevel::Warning => {
                        session.workspace_mut().messages_mut().warn(text.clone())
                    }
                    MessageLevel::Error => {
                        session.workspace_mut().messages_mut().error(text.clone())
                    }
                }
                session.set_status(text);
                Ok(())
            }
        }
    }
}
