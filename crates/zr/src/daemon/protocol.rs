use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use zacor_host::protocol::DaemonRefusal;

#[derive(Debug, Deserialize)]
pub(super) struct DaemonRequest {
    pub(super) request: String,
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) pkg_name: Option<String>,
    #[serde(default)]
    pub(super) version: Option<String>,
    #[serde(default)]
    pub(super) env: HashMap<String, String>,
    #[serde(default)]
    pub(super) command: Option<String>,
    #[serde(default)]
    pub(super) args: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) zacor_version: Option<String>,
    #[serde(default)]
    pub(super) domain: Option<String>,
    #[serde(default)]
    pub(super) op: Option<String>,
    #[serde(default)]
    pub(super) params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct DaemonResponse {
    pub(super) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) refusal: Option<DaemonRefusal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) services: Option<Vec<ServiceStatusEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) result: Option<serde_json::Value>,
}

impl Default for DaemonResponse {
    fn default() -> Self {
        Self {
            ok: false,
            error: None,
            refusal: None,
            services: None,
            result: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub(super) struct ServiceStatusEntry {
    pub(super) name: String,
    pub(super) port: u16,
    pub(super) status: String,
}
