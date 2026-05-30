use crate::error::*;
use crate::package_definition::{
    ArgType, ArgumentDefinition, CommandDefinition, InputType, OutputDeclaration, PackageDefinition,
};
use crate::receipt::{self, Receipt};
use std::path::Path;
use zacor_protocol::daemon_catalog::{
    ArgumentDescriptor, ArgumentType, CommandDescriptor, InputKind, InstalledPackageSummary,
    OutputCardinality, OutputDescriptor, OutputDisplay, PackageDescriptor,
};

pub(super) fn list_packages(home: &Path) -> Result<Vec<InstalledPackageSummary>> {
    let receipts = receipt::list_all(home)?;
    let mut packages = Vec::with_capacity(receipts.len());
    for (name, receipt) in receipts {
        let description = crate::wasm_manifest::load_from_store(home, &name, &receipt.current)
            .ok()
            .and_then(|definition| definition.description);
        packages.push(InstalledPackageSummary {
            name,
            version: receipt.current,
            active: receipt.active,
            description,
        });
    }
    Ok(packages)
}

pub(super) fn describe_package(home: &Path, name: &str) -> Result<Option<PackageDescriptor>> {
    let Some(receipt) = receipt::read(home, name)? else {
        return Ok(None);
    };
    let definition = crate::wasm_manifest::load_from_store(home, name, &receipt.current)?;
    Ok(Some(project_package(name, &receipt, &definition)))
}

fn project_package(
    name: &str,
    receipt: &Receipt,
    definition: &PackageDefinition,
) -> PackageDescriptor {
    PackageDescriptor {
        name: name.to_string(),
        version: receipt.current.clone(),
        active: receipt.active,
        description: definition.description.clone(),
        commands: definition
            .commands
            .iter()
            .map(|(name, definition)| (name.clone(), project_command(definition)))
            .collect(),
    }
}

fn project_command(definition: &CommandDefinition) -> CommandDescriptor {
    CommandDescriptor {
        description: definition.description.clone(),
        args: definition
            .args
            .iter()
            .map(|(name, definition)| (name.clone(), project_argument(definition)))
            .collect(),
        commands: definition
            .commands
            .iter()
            .map(|(name, definition)| (name.clone(), project_command(definition)))
            .collect(),
        input: definition.input.map(project_input),
        output: definition.output.as_ref().map(project_output),
    }
}

fn project_argument(definition: &ArgumentDefinition) -> ArgumentDescriptor {
    ArgumentDescriptor {
        arg_type: project_argument_type(&definition.arg_type),
        required: definition.required,
        flag: definition.flag.clone(),
        values: definition.values.clone(),
        rest: definition.rest,
    }
}

fn project_argument_type(arg_type: &ArgType) -> ArgumentType {
    match arg_type {
        ArgType::String => ArgumentType::String,
        ArgType::Number => ArgumentType::Number,
        ArgType::Integer => ArgumentType::Integer,
        ArgType::Bool => ArgumentType::Bool,
        ArgType::Path => ArgumentType::Path,
        ArgType::Choice => ArgumentType::Choice,
    }
}

fn project_input(input: InputType) -> InputKind {
    match input {
        InputType::Text => InputKind::Text,
        InputType::Jsonl => InputKind::Jsonl,
        InputType::Binary => InputKind::Binary,
    }
}

fn project_output(output: &OutputDeclaration) -> OutputDescriptor {
    OutputDescriptor {
        cardinality: match output.resolved_cardinality() {
            crate::package_definition::Cardinality::One => OutputCardinality::One,
            crate::package_definition::Cardinality::Many => OutputCardinality::Many,
        },
        display: output.resolved_display().map(|display| match display {
            crate::package_definition::DisplayType::Text => OutputDisplay::Text,
            crate::package_definition::DisplayType::Table => OutputDisplay::Table,
            crate::package_definition::DisplayType::Record => OutputDisplay::Record,
        }),
        field: output.field.clone(),
        stream: output.stream,
        schema: output.schema.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use crate::receipt::{Receipt, SourceRecord};
    use std::collections::BTreeMap;

    fn local_source() -> SourceRecord {
        SourceRecord::Local {
            path: "/tmp/tool.tar.gz".to_string(),
        }
    }

    fn write_package(home: &Path, name: &str, version: &str, active: bool, yaml: &str) {
        let mut receipt = Receipt::new(version.to_string(), local_source());
        receipt.active = active;
        crate::receipt::write(home, name, &receipt).unwrap();
        std::fs::create_dir_all(paths::store_path(home, name, version)).unwrap();
        std::fs::write(paths::definition_path(home, name, version), yaml).unwrap();
    }

    #[test]
    fn list_packages_returns_sorted_summaries() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        write_package(
            home,
            "beta",
            "1.0.0",
            true,
            "name: beta\nversion: \"1.0.0\"\ndescription: Beta package\ncommands:\n  default: {}\n",
        );
        write_package(
            home,
            "alpha",
            "0.2.0",
            false,
            "name: alpha\nversion: \"0.2.0\"\ndescription: Alpha package\ncommands:\n  default: {}\n",
        );

        let packages = list_packages(home).unwrap();

        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].name, "alpha");
        assert_eq!(packages[0].version, "0.2.0");
        assert!(!packages[0].active);
        assert_eq!(packages[0].description.as_deref(), Some("Alpha package"));
        assert_eq!(packages[1].name, "beta");
    }

    #[test]
    fn describe_package_projects_nested_commands_and_output_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();

        write_package(
            home,
            "tool",
            "1.0.0",
            true,
            r#"name: tool
version: "1.0.0"
description: Test tool
commands:
  default:
    description: Root command
    args:
      path:
        type: path
        required: true
      verbose:
        type: bool
        flag: verbose
    commands:
      batch:
        description: Batch command
        input: text
        output:
          type: table
          stream: true
          field: value
          schema:
            value: string
"#,
        );

        let descriptor = describe_package(home, "tool").unwrap().unwrap();

        assert_eq!(descriptor.name, "tool");
        assert_eq!(descriptor.version, "1.0.0");
        assert!(descriptor.active);
        let root = &descriptor.commands["default"];
        assert_eq!(root.description.as_deref(), Some("Root command"));
        assert_eq!(root.args["path"].arg_type, ArgumentType::Path);
        assert!(root.args["path"].required);
        assert_eq!(root.args["verbose"].flag.as_deref(), Some("verbose"));
        let batch = &root.commands["batch"];
        assert_eq!(batch.input, Some(InputKind::Text));
        let output = batch.output.as_ref().unwrap();
        assert_eq!(output.cardinality, OutputCardinality::Many);
        assert_eq!(output.display, Some(OutputDisplay::Table));
        assert!(output.stream);
        assert_eq!(output.field.as_deref(), Some("value"));
        assert_eq!(
            output.schema.as_ref().unwrap(),
            &BTreeMap::from([("value".to_string(), "string".to_string())])
        );
    }

    #[test]
    fn describe_package_returns_none_when_receipt_is_missing() {
        let tmp = tempfile::tempdir().unwrap();

        assert!(describe_package(tmp.path(), "missing").unwrap().is_none());
    }
}
