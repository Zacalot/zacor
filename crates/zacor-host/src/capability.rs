use crate::protocol::{CapabilityError, CapabilityReq, CapabilityRes, CapabilityResult};
use std::collections::HashMap;
use std::sync::Arc;

pub trait CapabilityProvider: Send + Sync {
    fn domain(&self) -> &str;

    fn handle(&self, op: &str, params: &serde_json::Value)
        -> Result<serde_json::Value, CapabilityError>;
}

pub struct CapabilityRegistry {
    providers: HashMap<String, Arc<dyn CapabilityProvider>>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn CapabilityProvider>) -> Result<(), CapabilityError> {
        let domain = provider.domain().to_string();
        if self.providers.contains_key(&domain) {
            return Err(CapabilityError {
                kind: "invalid_input".into(),
                message: format!("capability domain already registered: {domain}"),
            });
        }
        self.providers.insert(domain, provider);
        Ok(())
    }

    pub fn unregister(&mut self, domain: &str) -> bool {
        self.providers.remove(domain).is_some()
    }

    pub fn dispatch(&self, req: &CapabilityReq) -> CapabilityRes {
        let result = match self.providers.get(&req.domain) {
            Some(provider) => provider.handle(&req.op, &req.params),
            None => Err(CapabilityError {
                kind: "invalid_input".into(),
                message: format!("unknown domain: {}", req.domain),
            }),
        };

        CapabilityRes {
            id: req.id,
            result: match result {
                Ok(data) => CapabilityResult::Ok { data },
                Err(error) => CapabilityResult::Error { error },
            },
        }
    }

    pub fn has_domain(&self, domain: &str) -> bool {
        self.providers.contains_key(domain)
    }

    pub fn domains(&self) -> Vec<&str> {
        let mut domains = self.providers.keys().map(|domain| domain.as_str()).collect::<Vec<_>>();
        domains.sort_unstable();
        domains
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct TestProvider {
        domain: &'static str,
    }

    impl CapabilityProvider for TestProvider {
        fn domain(&self) -> &str {
            self.domain
        }

        fn handle(
            &self,
            op: &str,
            params: &serde_json::Value,
        ) -> Result<serde_json::Value, CapabilityError> {
            match op {
                "ok" => Ok(json!({"params": params})),
                _ => Err(CapabilityError {
                    kind: "invalid_input".into(),
                    message: format!("unknown operation: {op}"),
                }),
            }
        }
    }

    fn req(domain: &str, op: &str) -> CapabilityReq {
        CapabilityReq {
            id: 7,
            domain: domain.into(),
            op: op.into(),
            params: json!({"hello": "world"}),
        }
    }

    #[test]
    fn dispatch_unknown_domain_returns_structured_error() {
        let registry = CapabilityRegistry::new();
        let res = registry.dispatch(&req("missing", "ok"));

        match res.result {
            CapabilityResult::Error { error } => {
                assert_eq!(error.kind, "invalid_input");
                assert_eq!(error.message, "unknown domain: missing");
            }
            CapabilityResult::Ok { .. } => panic!("expected error"),
        }
    }

    #[test]
    fn register_unregister_and_domains_work() {
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(TestProvider { domain: "fs" }))
            .unwrap();
        registry
            .register(Arc::new(TestProvider { domain: "prompt" }))
            .unwrap();

        assert!(registry.has_domain("fs"));
        assert_eq!(registry.domains(), vec!["fs", "prompt"]);
        assert!(registry.unregister("fs"));
        assert!(!registry.has_domain("fs"));
        assert!(!registry.unregister("fs"));
    }

    #[test]
    fn duplicate_registration_is_an_error() {
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(TestProvider { domain: "fs" }))
            .unwrap();

        let error = registry
            .register(Arc::new(TestProvider { domain: "fs" }))
            .unwrap_err();
        assert_eq!(error.kind, "invalid_input");
        assert_eq!(error.message, "capability domain already registered: fs");
    }

    #[test]
    fn dispatch_calls_registered_provider() {
        let mut registry = CapabilityRegistry::new();
        registry
            .register(Arc::new(TestProvider { domain: "fs" }))
            .unwrap();

        let res = registry.dispatch(&req("fs", "ok"));
        match res.result {
            CapabilityResult::Ok { data } => {
                assert_eq!(data["params"]["hello"], "world");
            }
            CapabilityResult::Error { .. } => panic!("expected ok"),
        }
    }
}
