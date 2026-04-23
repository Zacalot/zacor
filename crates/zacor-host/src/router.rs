use crate::package_definition::PackageDefinition;
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvocationOutcome {
    pub exit_code: i32,
    pub error: Option<String>,
}

impl InvocationOutcome {
    pub fn success(exit_code: i32) -> Self {
        Self {
            exit_code,
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            exit_code: 1,
            error: Some(error.into()),
        }
    }
}

pub trait PackageRouter: Send + Sync {
    fn invoke(
        &self,
        caller: Option<&PackageDefinition>,
        package: &str,
        command: &str,
        args: &BTreeMap<String, String>,
        depth: usize,
        max_depth: usize,
        on_output: &mut dyn FnMut(Value) -> Result<(), String>,
    ) -> InvocationOutcome;
}
