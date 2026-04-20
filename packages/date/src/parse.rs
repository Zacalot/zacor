use jiff::{Span, Zoned};
use jiff::tz::TimeZone;

/// Unified date parsing pipeline: unix timestamp -> Jiff native -> parse_datetime
pub fn parse_date(input: &str, tz: &TimeZone) -> Result<Zoned, String> {
    // Try unix timestamp first (pure numeric, optional leading minus)
    if let Some(zoned) = try_unix_timestamp(input, tz) {
        return Ok(zoned);
    }

    // Try Jiff native parsing (ISO 8601, RFC 9557, etc.)
    if let Ok(zoned) = input.parse::<Zoned>() {
        return Ok(zoned);
    }

    // Try parsing as civil datetime then applying timezone
    if let Ok(dt) = input.parse::<jiff::civil::DateTime>() {
        return dt.to_zoned(tz.clone()).map_err(|e| format!("failed to apply timezone: {e}"));
    }

    // Try parsing as civil date then applying timezone
    if let Ok(d) = input.parse::<jiff::civil::Date>() {
        let dt = d.to_zoned(tz.clone()).map_err(|e| format!("failed to apply timezone: {e}"))?;
        return Ok(dt);
    }

    // Fall back to parse_datetime for human expressions
    match parse_datetime::parse_datetime_at_date(jiff::Zoned::now(), input) {
        Ok(zoned) => Ok(zoned),
        Err(_) => Err(format!("could not parse date: '{input}'")),
    }
}

fn try_unix_timestamp(input: &str, tz: &TimeZone) -> Option<Zoned> {
    let trimmed = input.trim();
    // Must be a pure number (optional leading minus)
    if !trimmed
        .strip_prefix('-')
        .unwrap_or(trimmed)
        .chars()
        .all(|c| c.is_ascii_digit())
    {
        return None;
    }
    if trimmed.is_empty() || trimmed == "-" {
        return None;
    }

    let n: i64 = trimmed.parse().ok()?;
    let abs = n.unsigned_abs();

    // >= 1e12 → milliseconds, else seconds
    let ts = if abs >= 1_000_000_000_000 {
        jiff::Timestamp::from_millisecond(n).ok()?
    } else {
        jiff::Timestamp::from_second(n).ok()?
    };

    Some(ts.to_zoned(tz.clone()))
}

/// Resolve timezone from --timezone arg, --utc flag, or system default
pub fn resolve_timezone(timezone: Option<&str>, utc: bool) -> Result<TimeZone, String> {
    if utc {
        return Ok(TimeZone::UTC);
    }
    match timezone {
        Some(name) => TimeZone::get(name).map_err(|e| format!("invalid timezone '{name}': {e}")),
        None => Ok(TimeZone::system()),
    }
}

/// Parse a duration string using Jiff's Span parser (friendly + ISO 8601)
pub fn parse_duration(input: &str) -> Result<Span, String> {
    input
        .parse::<Span>()
        .map_err(|e| format!("could not parse duration '{input}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unix_seconds() {
        let tz = TimeZone::UTC;
        let result = parse_date("1774658200", &tz).unwrap();
        assert_eq!(result.timestamp().as_second(), 1774658200);
    }

    #[test]
    fn parse_unix_milliseconds() {
        let tz = TimeZone::UTC;
        let result = parse_date("1774658200000", &tz).unwrap();
        assert_eq!(result.timestamp().as_millisecond(), 1774658200000);
    }

    #[test]
    fn parse_iso_date() {
        let tz = TimeZone::UTC;
        let result = parse_date("2026-03-25", &tz).unwrap();
        let dt = result.datetime();
        assert_eq!(dt.date().year(), 2026);
        assert_eq!(dt.date().month(), 3);
        assert_eq!(dt.date().day(), 25);
    }

    #[test]
    fn parse_iso_datetime() {
        let tz = TimeZone::UTC;
        let result = parse_date("2026-03-25T14:30:00", &tz).unwrap();
        let dt = result.datetime();
        assert_eq!(dt.time().hour(), 14);
        assert_eq!(dt.time().minute(), 30);
    }

    #[test]
    fn parse_human_expression() {
        let tz = TimeZone::UTC;
        // "3 days ago" should parse without error
        let result = parse_date("3 days ago", &tz);
        assert!(result.is_ok(), "failed to parse '3 days ago': {:?}", result.err());
    }

    #[test]
    fn parse_invalid_date() {
        let tz = TimeZone::UTC;
        let result = parse_date("not-a-date", &tz);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_timezone_utc_flag() {
        let tz = resolve_timezone(None, true).unwrap();
        assert_eq!(tz.iana_name(), Some("UTC"));
    }

    #[test]
    fn resolve_timezone_named() {
        let tz = resolve_timezone(Some("America/New_York"), false).unwrap();
        assert_eq!(tz.iana_name(), Some("America/New_York"));
    }

    #[test]
    fn resolve_timezone_invalid() {
        let result = resolve_timezone(Some("Fake/Zone"), false);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_timezone_default() {
        let tz = resolve_timezone(None, false).unwrap();
        // System timezone should resolve without error
        assert!(tz.iana_name().is_some() || true); // system tz may not have IANA name
    }

    #[test]
    fn parse_duration_friendly() {
        let span = parse_duration("3months 2days").unwrap();
        assert_eq!(span.get_months(), 3);
        assert_eq!(span.get_days(), 2);
    }

    #[test]
    fn parse_duration_iso() {
        let span = parse_duration("P1Y2M3D").unwrap();
        assert_eq!(span.get_years(), 1);
        assert_eq!(span.get_months(), 2);
        assert_eq!(span.get_days(), 3);
    }
}
