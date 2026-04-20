use std::path::Path;

fn lab_dir() -> &'static Path {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().unwrap() // p-zr-lab/
}

fn validate_yaml(pkg: &str) {
    let path = lab_dir().join(pkg).join("package.yaml");
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    assert!(content.contains("name:"), "{pkg}: missing 'name'");
    assert!(content.contains("version:"), "{pkg}: missing 'version'");
    assert!(
        content.contains("description:"),
        "{pkg}: missing 'description'"
    );
}

#[test]
fn all_package_yaml_files_exist_and_parse() {
    for pkg in &["watch", "kv", "hash", "json"] {
        validate_yaml(pkg);
    }
}

#[test]
fn watch_yaml_stream_true() {
    let c = std::fs::read_to_string(lab_dir().join("watch/package.yaml")).unwrap();
    assert!(c.contains("stream: true"));
}

#[test]
fn kv_yaml_service_section() {
    let c = std::fs::read_to_string(lab_dir().join("kv/package.yaml")).unwrap();
    assert!(c.contains("service:"));
    assert!(c.contains("execution:"));
}

#[test]
fn hash_yaml_algorithm_arg() {
    let c = std::fs::read_to_string(lab_dir().join("hash/package.yaml")).unwrap();
    assert!(c.contains("algorithm:"));
    assert!(c.contains("default: \"sha256\""));
}

#[test]
fn json_yaml_input_text() {
    let c = std::fs::read_to_string(lab_dir().join("json/package.yaml")).unwrap();
    assert!(c.contains("input: text"));
}
