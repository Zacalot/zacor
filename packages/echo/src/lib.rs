use serde::Serialize;

zacor_package::include_args!();

#[derive(Serialize)]
pub struct EchoRecord {
    pub text: String,
}

pub fn echo(args: args::DefaultArgs) -> Result<EchoRecord, String> {
    Ok(EchoRecord { text: args.text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_package::FromArgs;
    use std::collections::BTreeMap;
    use serde_json::json;

    #[test]
    fn from_args_string() {
        let map: BTreeMap<String, _> = [("text".into(), json!("hello"))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.text, "hello");
    }

    #[test]
    fn from_args_string_coercion_from_number() {
        let map: BTreeMap<String, _> = [("text".into(), json!(42))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.text, "42");
    }

    #[test]
    fn from_args_string_coercion_from_bool() {
        let map: BTreeMap<String, _> = [("text".into(), json!(true))].into();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.text, "true");
    }

    #[test]
    fn from_args_missing_string_defaults_empty() {
        let map: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        let args = args::DefaultArgs::from_args(&map).unwrap();
        assert_eq!(args.text, "");
    }
}
