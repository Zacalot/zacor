mod http;

use std::sync::Arc;

use zacor_host::capability::CapabilityRegistry;
use zacor_host::protocol::CapabilityRes;

use http::HttpProvider;

pub(super) struct CapabilityRouter {
    registry: CapabilityRegistry,
}

impl CapabilityRouter {
    pub(super) fn new() -> Self {
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(HttpProvider::new()))
            .expect("unique daemon http provider");
        Self { registry }
    }

    pub(super) fn dispatch(
        &self,
        id: u64,
        domain: &str,
        op: &str,
        params: serde_json::Value,
    ) -> CapabilityRes {
        self.registry.dispatch(&zacor_host::protocol::CapabilityReq {
            id,
            domain: domain.into(),
            op: op.into(),
            params,
        })
    }
}
