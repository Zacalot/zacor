//! The service plane: `zri`'s single owner of daemon-side concerns (Zed
//! `Project` precedent). The app shell holds exactly one `ZrPlane`; daemon
//! events reach the UI through the ingress queue; facade methods own boundary
//! validation and return typed errors so no UI code ever touches transport
//! state directly.

use std::cell::{Cell, RefCell};
use std::fmt;

use zacor_protocol::DaemonRefusal;
use zacor_protocol::daemon_catalog::{InstalledPackageSummary, PackageDescriptor};
use zacor_protocol::daemon_invoke::{CommandInvocationRequest, InvocationContext};

use crate::ingress::{IngressEvent, IngressSender, InvocationId};
use crate::zr::{InvocationStream, ZrClient, ZrClientError};

/// Whether an invocation should be cancelled when its consumer goes away
/// (`Attached`, the default contract) or run to completion with errors
/// sinking to the daemon log (`Detached`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvokeMode {
    Attached,
    Detached,
}

#[derive(Debug)]
pub enum ZrPlaneError {
    /// The daemon is not reachable. The UI should suggest starting it.
    DaemonUnavailable { message: String },
    /// Runtime version mismatch — distinct so the UI can say "restart the
    /// daemon" rather than showing a generic refusal.
    VersionMismatch { daemon: String, client: String },
    /// The daemon refused the request (typed refusal, pre-stream).
    Refused(DaemonRefusal),
    /// Any other transport/decode failure.
    Transport(String),
}

impl fmt::Display for ZrPlaneError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DaemonUnavailable { message } => {
                write!(formatter, "daemon unavailable: {message}")
            }
            Self::VersionMismatch { daemon, client } => write!(
                formatter,
                "daemon version mismatch (daemon {daemon}, client {client}); restart the daemon"
            ),
            Self::Refused(refusal) => write!(formatter, "daemon refused: {refusal:?}"),
            Self::Transport(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for ZrPlaneError {}

/// Completion provider seam for the prompt phase: implemented by the plane
/// over its catalog cache, substitutable by a fake in tests (Zed
/// `CompletionProvider` precedent) — prompt/completion never needs a live
/// daemon to be testable.
pub trait CompletionSource {
    fn complete_packages(&self, prefix: &str) -> Vec<InstalledPackageSummary>;
}

pub struct ZrPlane {
    client: ZrClient,
    catalog: RefCell<Option<Vec<InstalledPackageSummary>>>,
    next_invocation: Cell<u64>,
    ingress: IngressSender,
}

impl ZrPlane {
    pub fn new(client: ZrClient, ingress: IngressSender) -> Self {
        Self {
            client,
            catalog: RefCell::new(None),
            next_invocation: Cell::new(1),
            ingress,
        }
    }

    /// Fetch and cache the installed-package catalog. Explicit refresh: the
    /// cache never goes stale silently mid-interaction.
    pub fn refresh_packages(&self) -> Result<Vec<InstalledPackageSummary>, ZrPlaneError> {
        let packages = self.client.list_packages().map_err(map_client_error)?;
        *self.catalog.borrow_mut() = Some(packages.clone());
        Ok(packages)
    }

    /// The cached catalog (empty until the first successful refresh).
    pub fn packages(&self) -> Vec<InstalledPackageSummary> {
        self.catalog.borrow().clone().unwrap_or_default()
    }

    pub fn describe_package(&self, name: &str) -> Result<PackageDescriptor, ZrPlaneError> {
        self.client.describe_package(name).map_err(map_client_error)
    }

    /// Start a daemon command invocation. The blocking ack happens on the
    /// caller thread, so a refusal surfaces immediately as a typed error
    /// (the refusal-before-stream invariant). On success a named reader
    /// thread forwards every stream event into the ingress queue, ending with
    /// `InvocationClosed`.
    pub fn invoke(
        &self,
        package: impl Into<String>,
        command: impl Into<String>,
        args: std::collections::BTreeMap<String, String>,
        cwd: impl Into<String>,
        mode: InvokeMode,
    ) -> Result<InvocationId, ZrPlaneError> {
        let request = CommandInvocationRequest {
            package: package.into(),
            command: command.into(),
            args,
            context: InvocationContext { cwd: cwd.into() },
            detach: mode == InvokeMode::Detached,
        };

        let stream = self
            .client
            .invoke_command(request)
            .map_err(map_client_error)?;

        let id = InvocationId(self.next_invocation.get());
        self.next_invocation.set(id.0 + 1);

        let ingress = self.ingress.clone();
        std::thread::Builder::new()
            .name(format!("zri-invocation-{}", id.0))
            .spawn(move || forward_invocation_events(stream, id, ingress))
            .map_err(|error| ZrPlaneError::Transport(format!("spawn reader thread: {error}")))?;

        Ok(id)
    }
}

impl CompletionSource for ZrPlane {
    fn complete_packages(&self, prefix: &str) -> Vec<InstalledPackageSummary> {
        self.packages()
            .into_iter()
            .filter(|package| package.name.starts_with(prefix))
            .collect()
    }
}

fn forward_invocation_events(
    mut stream: InvocationStream,
    id: InvocationId,
    ingress: IngressSender,
) {
    loop {
        match stream.next_event() {
            Ok(Some(event)) => {
                if !ingress.send(IngressEvent::Invocation { id, event }) {
                    // Foreground is gone; dropping the stream closes the
                    // connection, which cancels daemon-side execution
                    // (disconnect-as-cancel contract).
                    return;
                }
            }
            Ok(None) => break,
            Err(error) => {
                let _ = ingress.send(IngressEvent::PlaneError {
                    message: format!("invocation {} stream failed: {error}", id.0),
                });
                break;
            }
        }
    }
    let _ = ingress.send(IngressEvent::InvocationClosed { id });
}

fn map_client_error(error: ZrClientError) -> ZrPlaneError {
    match error {
        ZrClientError::Connect { addr, source } => ZrPlaneError::DaemonUnavailable {
            message: format!("{addr}: {source}"),
        },
        ZrClientError::VersionMismatch { daemon, client } => {
            ZrPlaneError::VersionMismatch { daemon, client }
        }
        ZrClientError::DaemonRefusal(refusal) => ZrPlaneError::Refused(refusal),
        other => ZrPlaneError::Transport(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    use zacor_protocol::daemon_invoke::InvocationEvent;

    use crate::ingress::ZrIngress;

    use super::*;

    fn plane_with_server(
        ack: serde_json::Value,
        events: Vec<serde_json::Value>,
    ) -> (ZrPlane, ZrIngress) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(stream, "{}", ack).unwrap();
            for event in events {
                writeln!(stream, "{}", event).unwrap();
            }
            stream.flush().unwrap();
        });
        let ingress = ZrIngress::new(Arc::new(|| {}));
        let plane = ZrPlane::new(ZrClient::new(addr), ingress.sender());
        (plane, ingress)
    }

    fn drain_until_closed(ingress: &mut ZrIngress) -> Vec<IngressEvent> {
        let mut seen = Vec::new();
        for _ in 0..1_000 {
            ingress.drain(64, |event| seen.push(event));
            if seen
                .iter()
                .any(|event| matches!(event, IngressEvent::InvocationClosed { .. }))
            {
                return seen;
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("invocation stream never closed; saw: {seen:?}");
    }

    #[test]
    fn invoke_forwards_stream_events_through_ingress() {
        let (plane, mut ingress) = plane_with_server(
            serde_json::json!({"ok": true}),
            vec![
                serde_json::json!({"type": "output", "record": {"value": "hello"}}),
                serde_json::json!({"type": "done", "exit_code": 0}),
            ],
        );

        let id = plane
            .invoke(
                "echo",
                "default",
                std::collections::BTreeMap::new(),
                ".",
                InvokeMode::Attached,
            )
            .unwrap();

        let seen = drain_until_closed(&mut ingress);
        assert!(seen.iter().any(|event| matches!(
            event,
            IngressEvent::Invocation {
                id: seen_id,
                event: InvocationEvent::Output { .. }
            } if *seen_id == id
        )));
        assert!(
            seen.iter()
                .any(|event| matches!(event, IngressEvent::InvocationClosed { id: seen_id } if *seen_id == id))
        );
    }

    #[test]
    fn refusal_surfaces_as_typed_error_before_any_stream() {
        let (plane, mut ingress) = plane_with_server(
            serde_json::json!({
                "ok": false,
                "refusal": {"kind": "package_not_found", "name": "ghost"},
                "error": "package not found: ghost"
            }),
            Vec::new(),
        );

        let error = plane
            .invoke(
                "ghost",
                "default",
                std::collections::BTreeMap::new(),
                ".",
                InvokeMode::Attached,
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ZrPlaneError::Refused(DaemonRefusal::PackageNotFound { name }) if name == "ghost"
        ));
        // No reader thread was spawned: nothing arrives on the ingress.
        let outcome = ingress.drain(64, |_| {});
        assert_eq!(outcome.applied, 0);
    }

    #[test]
    fn version_mismatch_is_distinct_plane_error() {
        let (plane, _ingress) = plane_with_server(
            serde_json::json!({
                "ok": false,
                "refusal": {"kind": "version_mismatch", "daemon": "9.9.9", "client": "0.1.0"},
                "error": "daemon version mismatch"
            }),
            Vec::new(),
        );

        let error = plane.refresh_packages().unwrap_err();

        assert!(matches!(error, ZrPlaneError::VersionMismatch { .. }));
    }

    #[test]
    fn daemon_unavailable_is_typed() {
        let ingress = ZrIngress::new(Arc::new(|| {}));
        // Nothing is listening on this address.
        let plane = ZrPlane::new(ZrClient::new("127.0.0.1:1"), ingress.sender());

        let error = plane.refresh_packages().unwrap_err();

        assert!(matches!(error, ZrPlaneError::DaemonUnavailable { .. }));
    }

    #[test]
    fn completion_source_filters_cached_catalog() {
        let (plane, _ingress) = plane_with_server(
            serde_json::json!({
                "ok": true,
                "result": [
                    {"name": "echo", "version": "0.1.0", "active": true},
                    {"name": "everything", "version": "0.1.0", "active": true},
                    {"name": "watch", "version": "0.1.0", "active": true}
                ]
            }),
            Vec::new(),
        );
        plane.refresh_packages().unwrap();

        let matches = plane.complete_packages("e");

        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|package| package.name.starts_with('e')));
    }

    #[test]
    fn detached_mode_sets_request_flag() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(stream, "{}", serde_json::json!({"ok": true})).unwrap();
            stream.flush().unwrap();
            line
        });
        let ingress = ZrIngress::new(Arc::new(|| {}));
        let plane = ZrPlane::new(ZrClient::new(addr), ingress.sender());

        let _ = plane
            .invoke(
                "watch",
                "default",
                std::collections::BTreeMap::new(),
                ".",
                InvokeMode::Detached,
            )
            .unwrap();

        let request_line = handle.join().unwrap();
        let request: serde_json::Value = serde_json::from_str(request_line.trim()).unwrap();
        assert_eq!(request["invoke"]["detach"], true);
        assert_eq!(request["zacor_version"], env!("CARGO_PKG_VERSION"));
    }
}
