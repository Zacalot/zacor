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

    // Refuse-before-ack: resolve the package from daemon-owned install state
    // so "not found"/"disabled" surface as typed refusals (matching the
    // catalog requests) instead of flattened done-error events after the ack.
    match crate::receipt::read(home, &invocation.package) {
        Ok(Some(receipt)) => {
            if !receipt.active {
                return dispatch::send_ack_err(
                    writer.get_mut(),
                    DaemonRefusal::InvalidRequest {
                        reason: format!("package '{}' is disabled", invocation.package),
                    },
                );
            }
        }
        Ok(None) => {
            return dispatch::send_ack_err(
                writer.get_mut(),
                DaemonRefusal::PackageNotFound {
                    name: invocation.package.clone(),
                },
            );
        }
        Err(error) => {
            return dispatch::send_ack_err(
                writer.get_mut(),
                DaemonRefusal::LoadFailed {
                    reason: format!("read receipt for '{}': {:#}", invocation.package, error),
                },
            );
        }
    }

    writeln!(writer, "{{\"ok\":true}}")?;
    writer.flush()?;

    // Disconnect-as-cancel is an explicit contract, not a transport accident:
    // an attached client going away makes the event write fail, which aborts
    // execution at the next event boundary. Detached invocations swallow
    // write failures and run to completion, with errors sinking to the
    // daemon log (the consumer is gone; never discard silently).
    let detach = invocation.detach;
    let mut disconnected = false;
    let cwd = Path::new(&invocation.context.cwd);
    let result = zr_dispatch::invoke_in_process_with_events_at_cwd(
        home,
        &invocation.package,
        &invocation.command,
        &invocation.args,
        cwd,
        &mut |event| match write_event(&mut writer, map_event(event)) {
            Ok(()) => Ok(()),
            Err(error) if detach => {
                eprintln!(
                    "daemon: detached invocation '{}' lost its client: {:#}",
                    invocation.package, error
                );
                Ok(())
            }
            Err(error) => {
                disconnected = true;
                Err(format!("client disconnected: {:#}", error))
            }
        },
    );

    match result {
        Ok(exit_code) => {
            let done = write_event(
                &mut writer,
                DaemonInvocationEvent::Done {
                    exit_code,
                    error: None,
                },
            );
            if let Err(error) = done {
                if detach {
                    eprintln!(
                        "daemon: detached invocation '{}' finished (exit {}) with no client: {:#}",
                        invocation.package, exit_code, error
                    );
                } else {
                    return Err(error);
                }
            }
        }
        Err(_cancelled) if disconnected => {
            // The socket is gone; there is no one to write a Done event to.
            eprintln!(
                "daemon: invocation '{}' cancelled by client disconnect",
                invocation.package
            );
        }
        Err(error) if detach => {
            eprintln!(
                "daemon: detached invocation '{}' failed: {:#}",
                invocation.package, error
            );
            let _ = write_event(
                &mut writer,
                DaemonInvocationEvent::Done {
                    exit_code: 1,
                    error: Some(format!("{:#}", error)),
                },
            );
        }
        Err(error) => write_event(
            &mut writer,
            DaemonInvocationEvent::Done {
                exit_code: 1,
                error: Some(format!("{:#}", error)),
            },
        )?,
    }

    let _ = writer.flush();
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
