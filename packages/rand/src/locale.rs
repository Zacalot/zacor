const SUPPORTED_LOCALES: &[&str] = &["en-US"];
const DEFAULT_LOCALE: &str = "en-US";

pub fn resolve_locale(arg: Option<&str>) -> &'static str {
    if let Some(specified) = arg {
        return match_locale(specified);
    }
    DEFAULT_LOCALE
}

fn match_locale(candidate: &str) -> &'static str {
    for &locale in SUPPORTED_LOCALES {
        if locale == candidate {
            return locale;
        }
    }
    DEFAULT_LOCALE
}
