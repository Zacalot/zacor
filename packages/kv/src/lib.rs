use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use zacor_package::io::fs;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct KvRecord {
    pub key: String,
    pub value: String,
}

pub fn kv_data_file() -> Result<std::path::PathBuf, String> {
    Ok(zacor_package::ensure_data_dir()?.join("kv.json"))
}

pub fn load_store(path: &std::path::Path) -> HashMap<String, String> {
    match fs::read_string(path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

pub fn save_store(path: &std::path::Path, store: &HashMap<String, String>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("kv: cannot create data dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|e| format!("kv: serialize: {e}"))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, json.as_bytes()).map_err(|e| format!("kv: write tmp: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("kv: rename: {e}"))?;
    Ok(())
}

pub fn cmd_set(args: &args::SetArgs) -> Result<Vec<Value>, String> {
    let path = kv_data_file()?;
    let mut store = load_store(&path);
    store.insert(args.key.clone(), args.value.clone());
    save_store(&path, &store)?;
    Ok(vec![
        serde_json::json!({"key": args.key, "value": args.value}),
    ])
}

pub fn cmd_get(args: &args::GetArgs) -> Result<Vec<Value>, String> {
    let path = kv_data_file()?;
    let store = load_store(&path);
    match store.get(&args.key) {
        Some(value) => Ok(vec![serde_json::json!({"key": args.key, "value": value})]),
        None => Err(format!("kv: key not found: {}", args.key)),
    }
}

pub fn cmd_list(args: &args::ListArgs) -> Result<Vec<Value>, String> {
    let _ = args;
    let path = kv_data_file()?;
    let store = load_store(&path);
    let mut pairs: Vec<_> = store.into_iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs
        .into_iter()
        .map(|(k, v)| serde_json::json!({"key": k, "value": v}))
        .collect())
}

pub fn cmd_delete(args: &args::DeleteArgs) -> Result<Vec<Value>, String> {
    let path = kv_data_file()?;
    let mut store = load_store(&path);
    match store.remove(&args.key) {
        Some(value) => {
            save_store(&path, &store)?;
            Ok(vec![serde_json::json!({"key": args.key, "value": value})])
        }
        None => Err(format!("kv: key not found: {}", args.key)),
    }
}

pub fn service_handler(
    store: &mut HashMap<String, String>,
    invoke: zacor_package::protocol::Invoke,
) -> Vec<Value> {
    use zacor_package::FromArgs;
    match invoke.command.as_str() {
        "set" => {
            let Ok(a) = args::SetArgs::from_args(&invoke.args) else {
                return vec![];
            };
            store.insert(a.key.clone(), a.value.clone());
            vec![serde_json::json!({"key": a.key, "value": a.value})]
        }
        "get" => {
            let Ok(a) = args::GetArgs::from_args(&invoke.args) else {
                return vec![];
            };
            match store.get(&a.key) {
                Some(value) => vec![serde_json::json!({"key": a.key, "value": value})],
                None => vec![],
            }
        }
        "list" => {
            let mut pairs: Vec<_> = store.iter().collect();
            pairs.sort_by_key(|(k, _)| (*k).clone());
            pairs
                .into_iter()
                .map(|(k, v)| serde_json::json!({"key": k, "value": v}))
                .collect()
        }
        "delete" => {
            let Ok(a) = args::DeleteArgs::from_args(&invoke.args) else {
                return vec![];
            };
            match store.remove(&a.key) {
                Some(value) => vec![serde_json::json!({"key": a.key, "value": value})],
                None => vec![],
            }
        }
        _ => vec![],
    }
}
