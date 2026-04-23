use super::invalid_input;
use serde_json::json;
use std::io::{IsTerminal, Write};
use zacor_host::capability::CapabilityProvider;
use zacor_host::protocol::CapabilityError;

pub struct PromptProvider;

impl CapabilityProvider for PromptProvider {
    fn domain(&self) -> &str {
        "prompt"
    }

    fn handle(
        &self,
        op: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, CapabilityError> {
        if !std::io::stdin().is_terminal() {
            return Err(CapabilityError::from_io(&std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "prompt not available in piped mode",
            )));
        }

        let message = params.get("message").and_then(|value| value.as_str()).unwrap_or("");
        match op {
            "confirm" => {
                eprint!("{} [y/N] ", message);
                std::io::stderr().flush().map_err(|error| CapabilityError::from_io(&error))?;
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|error| CapabilityError::from_io(&error))?;
                Ok(json!({"answer": matches!(line.trim().to_lowercase().as_str(), "y" | "yes")}))
            }
            "choose" => {
                let options = params
                    .get("options")
                    .and_then(|value| value.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                eprintln!("{}", message);
                for (index, option) in options.iter().enumerate() {
                    eprintln!("  {}) {}", index + 1, option);
                }
                eprint!("Choice: ");
                std::io::stderr().flush().map_err(|error| CapabilityError::from_io(&error))?;

                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|error| CapabilityError::from_io(&error))?;
                let choice = line.trim();
                if let Ok(index) = choice.parse::<usize>()
                    && index >= 1
                    && index <= options.len()
                {
                    return Ok(json!({"answer": options[index - 1]}));
                }
                if options.iter().any(|option| option == choice) {
                    return Ok(json!({"answer": choice}));
                }

                Err(invalid_input(format!("invalid choice: {choice}")))
            }
            "text" => {
                eprint!("{}: ", message);
                std::io::stderr().flush().map_err(|error| CapabilityError::from_io(&error))?;
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .map_err(|error| CapabilityError::from_io(&error))?;
                Ok(json!({"answer": line.trim_end()}))
            }
            _ => Err(invalid_input(format!("unknown prompt operation: {op}"))),
        }
    }
}
