use crate::error::*;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use zacor_protocol::DaemonRefusal;

use super::capability_router::CapabilityRouter;
use super::catalog;
use super::dispatch;
use super::invoke;
use super::module_cache::LibraryPool;
use super::service_supervisor::{self, ManagedService};
use super::{DAEMON_PORT, DaemonControl, DaemonRequest, DaemonResponse};

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_connection(
    stream: TcpStream,
    services: &Arc<Mutex<HashMap<String, ManagedService>>>,
    library_pools: &Arc<Mutex<HashMap<String, LibraryPool>>>,
    capabilities: &Arc<CapabilityRouter>,
    control: &Arc<DaemonControl>,
    last_activity: &Arc<Mutex<Instant>>,
    catalog_cache: &Arc<Mutex<catalog::CatalogCache>>,
    home: &Path,
) -> Result<()> {
    *last_activity.lock().unwrap() = Instant::now();

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;

    let req: DaemonRequest = serde_json::from_str(line.trim()).context("invalid daemon request")?;

    if req.request == "dispatch" {
        return dispatch::handle_dispatch(reader, stream, req, control, home);
    }
    if req.request == "invoke-library" {
        return dispatch::handle_library_invoke(reader, stream, req, library_pools, control, home);
    }
    if req.request == "invoke-command" {
        return invoke::handle_command_invoke(stream, req, control, home);
    }

    let response = match req.request.as_str() {
        "ping" => DaemonResponse {
            ok: true,
            ..Default::default()
        },
        "status" => service_supervisor::handle_status(services),
        "list-packages" => DaemonResponse {
            ok: true,
            result: Some(serde_json::to_value(catalog::list_packages(
                home,
                &mut catalog_cache.lock().unwrap(),
            )?)?),
            ..Default::default()
        },
        "describe-package" => {
            let name = req.name.as_deref().unwrap_or("");
            if name.is_empty() {
                DaemonResponse {
                    ok: false,
                    refusal: Some(DaemonRefusal::InvalidRequest {
                        reason: "describe-package requires package name".into(),
                    }),
                    error: Some("describe-package requires package name".into()),
                    ..Default::default()
                }
            } else {
                match catalog::describe_package(home, name)? {
                    Some(descriptor) => DaemonResponse {
                        ok: true,
                        result: Some(serde_json::to_value(descriptor)?),
                        ..Default::default()
                    },
                    None => DaemonResponse {
                        ok: false,
                        refusal: Some(DaemonRefusal::PackageNotFound { name: name.into() }),
                        error: Some(format!("package not found: {}", name)),
                        ..Default::default()
                    },
                }
            }
        }
        "start-service" => {
            let name = req.name.as_deref().unwrap_or("");
            if name.is_empty() {
                DaemonResponse {
                    ok: false,
                    error: Some("missing service name".into()),
                    ..Default::default()
                }
            } else {
                service_supervisor::handle_start_service(services, home, name)
            }
        }
        "stop-service" => {
            let name = req.name.as_deref().unwrap_or("");
            service_supervisor::handle_stop_service(services, name)
        }
        "shutdown" => {
            control.shutdown_all();
            let _ = TcpStream::connect(format!("127.0.0.1:{}", DAEMON_PORT));
            DaemonResponse {
                ok: true,
                ..Default::default()
            }
        }
        "capability-forward" => {
            let domain = req.domain.as_deref().unwrap_or("");
            let op = req.op.as_deref().unwrap_or("");
            if domain.is_empty() || op.is_empty() {
                DaemonResponse {
                    ok: false,
                    error: Some(
                        serde_json::to_string(&zacor_host::protocol::CapabilityError {
                            kind: "invalid_input".into(),
                            message: "capability-forward requires domain and op".into(),
                        })
                        .unwrap(),
                    ),
                    ..Default::default()
                }
            } else {
                let result = capabilities.dispatch(
                    0,
                    domain,
                    op,
                    req.params.unwrap_or(serde_json::Value::Null),
                );
                DaemonResponse {
                    ok: true,
                    result: Some(serde_json::to_value(result)?),
                    ..Default::default()
                }
            }
        }
        _ => DaemonResponse {
            ok: false,
            error: Some(format!("unknown request: {}", req.request)),
            ..Default::default()
        },
    };

    let mut writer = stream;
    let json = serde_json::to_string(&response)?;
    writeln!(writer, "{}", json)?;
    writer.flush()?;
    Ok(())
}
