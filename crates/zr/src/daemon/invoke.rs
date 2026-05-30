use crate::error::*;
use std::io::{BufWriter, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use zacor_host::protocol::DaemonRefusal;
use zacor_protocol::daemon_invoke::{
    InvocationEvent as DaemonInvocationEvent,
    InvocationMessageLevel as DaemonInvocationMessageLevel,
};

use super::dispatch;
use super::{DaemonControl, DaemonRequest};

pub(super) fn handle_command_invoke(
    stream: TcpStream,
    req: DaemonRequest,
    control: &Arc<DaemonControl>,
    home: &Path,
) -> Result<()> {
    let mut writer = BufWriter::new(stream);

    if let Some(refusal) = control.dispatch_refusal(req.zacor_version.as_deref()) {
        return dispatch::send_ack_err(writer.get_mut(), refusal);
    }

    let daemon_version = env!("CARGO_PKG_VERSION");
    if let Some(ref client_version) = req.zacor_version
        && client_version != daemon_version
    {
        let refusal = DaemonRefusal::VersionMismatch {
            daemon: daemon_version.into(),
            client: client_version.clone(),
        };
        control.begin_dispatch_drain(refusal.clone());
        return dispatch::send_ack_err(writer.get_mut(), refusal);
    }

    let invocation = match req.invoke {
        Some(invocation) => invocation,
        None => {
            return dispatch::send_ack_err(
                writer.get_mut(),
                DaemonRefusal::InvalidRequest {
                    reason: "invoke-command requires invocation payload".into(),
                },
            );
        }
    };

    if invocation.package.trim().is_empty() {
        return dispatch::send_ack_err(
            writer.get_mut(),
            DaemonRefusal::InvalidRequest {
                reason: "invoke-command requires package".into(),
            },
        );
    }
    if invocation.command.trim().is_empty() {
        return dispatch::send_ack_err(
            writer.get_mut(),
            DaemonRefusal::InvalidRequest {
                reason: "invoke-command requires command".into(),
            },
        );
    }
    if invocation.context.cwd.trim().is_empty() {
        return dispatch::send_ack_err(
            writer.get_mut(),
            DaemonRefusal::InvalidRequest {
                reason: "invoke-command requires cwd".into(),
            },
        );
    }

    writeln!(writer, "{{\"ok\":true}}")?;
    writer.flush()?;

    let cwd = Path::new(&invocation.context.cwd);
    let result = zr_dispatch::invoke_in_process_with_events_at_cwd(
        home,
        &invocation.package,
        &invocation.command,
        &invocation.args,
        cwd,
        &mut |event| {
            write_event(&mut writer, map_event(event)).map_err(|error| format!("{:#}", error))
        },
    );

    match result {
        Ok(exit_code) => write_event(
            &mut writer,
            DaemonInvocationEvent::Done {
                exit_code,
                error: None,
            },
        )?,
        Err(error) => write_event(
            &mut writer,
            DaemonInvocationEvent::Done {
                exit_code: 1,
                error: Some(format!("{:#}", error)),
            },
        )?,
    }

    writer.flush()?;
    Ok(())
}

fn write_event(writer: &mut impl Write, event: DaemonInvocationEvent) -> Result<()> {
    let json = serde_json::to_string(&event)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}

fn map_event(event: zr_dispatch::InvocationEvent) -> DaemonInvocationEvent {
    match event {
        zr_dispatch::InvocationEvent::Record(record) => DaemonInvocationEvent::Output { record },
        zr_dispatch::InvocationEvent::Progress(fraction) => {
            DaemonInvocationEvent::Progress { fraction }
        }
        zr_dispatch::InvocationEvent::Message { level, text } => DaemonInvocationEvent::Message {
            level: match level {
                zr_dispatch::InvocationMessageLevel::Info => DaemonInvocationMessageLevel::Info,
                zr_dispatch::InvocationMessageLevel::Warning => {
                    DaemonInvocationMessageLevel::Warning
                }
                zr_dispatch::InvocationMessageLevel::Error => DaemonInvocationMessageLevel::Error,
            },
            text,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_protocol::daemon_invoke::InvocationMessageLevel;

    #[test]
    fn maps_dispatch_events_to_daemon_events() {
        assert_eq!(
            map_event(zr_dispatch::InvocationEvent::Record(
                serde_json::json!({"value": "hello"})
            )),
            DaemonInvocationEvent::Output {
                record: serde_json::json!({"value": "hello"})
            }
        );
        assert_eq!(
            map_event(zr_dispatch::InvocationEvent::Progress(0.25)),
            DaemonInvocationEvent::Progress { fraction: 0.25 }
        );
        assert_eq!(
            map_event(zr_dispatch::InvocationEvent::Message {
                level: zr_dispatch::InvocationMessageLevel::Warning,
                text: "careful".into(),
            }),
            DaemonInvocationEvent::Message {
                level: InvocationMessageLevel::Warning,
                text: "careful".into(),
            }
        );
    }
}
