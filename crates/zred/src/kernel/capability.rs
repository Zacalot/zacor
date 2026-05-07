use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityRegistry {
    capabilities: BTreeSet<Capability>,
}

impl CapabilityRegistry {
    pub fn new() -> Self {
        Self {
            capabilities: BTreeSet::new(),
        }
    }

    pub fn grant(&mut self, capability: Capability) -> bool {
        self.capabilities.insert(capability)
    }

    pub fn contains(&self, capability: &Capability) -> bool {
        self.capabilities.contains(capability)
    }

    #[allow(dead_code)]
    pub fn entries(&self) -> impl Iterator<Item = &Capability> {
        self.capabilities.iter()
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Capability {
    domain: CapabilityDomain,
    operation: String,
}

impl Capability {
    pub fn new(domain: CapabilityDomain, operation: impl Into<String>) -> Self {
        Self {
            domain,
            operation: operation.into(),
        }
    }

    #[allow(dead_code)]
    pub fn domain(&self) -> CapabilityDomain {
        self.domain
    }

    #[allow(dead_code)]
    pub fn operation(&self) -> &str {
        &self.operation
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CapabilityDomain {
    Buffer,
    Window,
    Keymap,
    Minibuffer,
    Workspace,
    Messages,
    Fs,
    Clipboard,
    Prompt,
    Subprocess,
    Terminal,
    Browser,
    Media,
    Canvas,
    Notification,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_tracks_granted_capabilities() {
        let mut registry = CapabilityRegistry::new();
        let capability = Capability::new(CapabilityDomain::Buffer, "create");

        assert!(registry.grant(capability.clone()));
        assert!(registry.contains(&capability));
    }
}
