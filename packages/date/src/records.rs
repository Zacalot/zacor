use jiff::Zoned;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DateRecord {
    pub datetime: String,
    pub date: String,
    pub time: String,
    pub year: i16,
    pub month: i8,
    pub day: i8,
    pub hour: i8,
    pub minute: i8,
    pub second: i8,
    pub nanosecond: i32,
    pub weekday: String,
    pub weekday_num: i8,
    pub week: i8,
    pub day_of_year: i16,
    pub quarter: i8,
    pub timezone: String,
    pub offset: String,
    pub unix: i64,
    pub unix_ms: i64,
    pub iso8601: String,
    pub rfc2822: String,
    pub rfc9557: String,
    pub is_dst: bool,
    pub is_leap_year: bool,
    pub days_in_month: i8,
    pub days_in_year: i16,
}

impl DateRecord {
    pub fn from_zoned(zoned: &Zoned) -> Self {
        use jiff::civil::Weekday;

        let dt = zoned.datetime();
        let d = dt.date();
        let t = dt.time();

        let weekday_name = match d.weekday() {
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
            Weekday::Sunday => "Sunday",
        };

        let weekday_num = match d.weekday() {
            Weekday::Monday => 1,
            Weekday::Tuesday => 2,
            Weekday::Wednesday => 3,
            Weekday::Thursday => 4,
            Weekday::Friday => 5,
            Weekday::Saturday => 6,
            Weekday::Sunday => 7,
        };

        let month = d.month();
        let quarter = ((month - 1) / 3) + 1;

        let tz = zoned.time_zone();
        let tz_name = tz.iana_name().unwrap_or("UTC").to_string();

        let offset = zoned.offset();
        let offset_str = {
            let raw = format!("{offset}");
            // Normalize to always include minutes: "+00" -> "+00:00"
            if raw.len() == 3 {
                format!("{raw}:00")
            } else {
                raw
            }
        };

        let unix_ts = zoned.timestamp();
        let unix_secs = unix_ts.as_second();
        let unix_ms = unix_ts.as_millisecond();

        // RFC 9557 format (with timezone annotation)
        let rfc9557 = format!("{zoned}");

        // ISO 8601 / RFC 3339 (without annotation)
        let iso8601 = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
            d.year(),
            d.month(),
            d.day(),
            t.hour(),
            t.minute(),
            t.second(),
            offset_str,
        );

        // RFC 2822
        let rfc2822 = jiff::fmt::rfc2822::to_string(zoned).unwrap_or_else(|_| iso8601.clone());

        // DST detection: compare with standard offset
        let is_dst = {
            // Create a date in January (likely standard time) to compare offsets
            let jan = jiff::civil::date(d.year(), 1, 1)
                .to_zoned(tz.clone())
                .ok();
            match jan {
                Some(jan_zoned) => zoned.offset() != jan_zoned.offset(),
                None => false,
            }
        };

        let is_leap = d.in_leap_year();
        let days_in_month = d.days_in_month();
        let days_in_year = if is_leap { 366 } else { 365 };

        // ISO week number
        let iso_week = d.iso_week_date();
        let week_num = iso_week.week();

        DateRecord {
            datetime: rfc9557.clone(),
            date: format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day()),
            time: format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second()),
            year: d.year(),
            month: d.month(),
            day: d.day(),
            hour: t.hour(),
            minute: t.minute(),
            second: t.second(),
            nanosecond: t.subsec_nanosecond(),
            weekday: weekday_name.to_string(),
            weekday_num,
            week: week_num,
            day_of_year: d.day_of_year(),
            quarter,
            timezone: tz_name,
            offset: offset_str,
            unix: unix_secs,
            unix_ms,
            iso8601,
            rfc2822,
            rfc9557,
            is_dst,
            is_leap_year: is_leap,
            days_in_month,
            days_in_year,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DiffRecord {
    pub from: String,
    pub to: String,
    pub years: i32,
    pub months: i32,
    pub weeks: i32,
    pub days: i32,
    pub hours: i32,
    pub minutes: i32,
    pub seconds: i32,
    pub total_days: f64,
    pub total_hours: f64,
    pub total_seconds: i64,
    pub humanized: String,
    pub iso8601: String,
}

impl DiffRecord {
    pub fn from_span(from: &Zoned, to: &Zoned, span: jiff::Span) -> Self {
        let from_str = format!("{from}");
        let to_str = format!("{to}");

        let years = span.get_years();
        let months = span.get_months();
        let weeks = span.get_weeks();
        let days = span.get_days();
        let hours = span.get_hours();
        let minutes = span.get_minutes();
        let seconds = span.get_seconds();

        // Compute totals from timestamp difference
        let total_secs = to.timestamp().as_second() - from.timestamp().as_second();
        let total_nanos = to.timestamp().as_nanosecond() - from.timestamp().as_nanosecond();
        let total_days_f = total_nanos as f64 / (86_400.0 * 1_000_000_000.0);
        let total_hours_f = total_nanos as f64 / (3_600.0 * 1_000_000_000.0);

        // Humanized string
        let mut parts = Vec::new();
        if years != 0 {
            let abs = years.abs();
            parts.push(if abs == 1 { format!("{abs} year") } else { format!("{abs} years") });
        }
        if months != 0 {
            let abs = months.abs();
            parts.push(if abs == 1 { format!("{abs} month") } else { format!("{abs} months") });
        }
        if weeks != 0 {
            let abs = weeks.abs();
            parts.push(if abs == 1 { format!("{abs} week") } else { format!("{abs} weeks") });
        }
        if days != 0 {
            let abs = days.abs();
            parts.push(if abs == 1 { format!("{abs} day") } else { format!("{abs} days") });
        }
        if hours != 0 {
            let abs = hours.abs();
            parts.push(if abs == 1 { format!("{abs} hour") } else { format!("{abs} hours") });
        }
        if minutes != 0 {
            let abs = minutes.abs();
            parts.push(if abs == 1 { format!("{abs} minute") } else { format!("{abs} minutes") });
        }
        if seconds != 0 || parts.is_empty() {
            let abs = seconds.abs();
            parts.push(if abs == 1 { format!("{abs} second") } else { format!("{abs} seconds") });
        }
        let is_negative = total_secs < 0;
        let humanized = if is_negative {
            format!("-{}", parts.join(", "))
        } else {
            parts.join(", ")
        };

        // ISO 8601 duration
        let iso = format_iso8601_duration(years, months, weeks, days, hours, minutes, seconds);

        DiffRecord {
            from: from_str,
            to: to_str,
            years: years as i32,
            months: months as i32,
            weeks: weeks as i32,
            days: days as i32,
            hours: hours as i32,
            minutes: minutes as i32,
            seconds: seconds as i32,
            total_days: total_days_f,
            total_hours: total_hours_f,
            total_seconds: total_secs,
            humanized,
            iso8601: iso,
        }
    }
}

fn format_iso8601_duration(
    years: i16,
    months: i32,
    weeks: i32,
    days: i32,
    hours: i32,
    minutes: i64,
    seconds: i64,
) -> String {
    let is_negative = years < 0 || months < 0 || weeks < 0 || days < 0 || hours < 0 || minutes < 0 || seconds < 0;
    let prefix = if is_negative { "-P" } else { "P" };
    let mut date_part = String::new();
    let mut time_part = String::new();

    let y = years.unsigned_abs();
    let mo = months.unsigned_abs();
    let w = weeks.unsigned_abs();
    let d = days.unsigned_abs();
    let h = hours.unsigned_abs();
    let mi = minutes.unsigned_abs();
    let s = seconds.unsigned_abs();

    if y > 0 { date_part.push_str(&format!("{y}Y")); }
    if mo > 0 { date_part.push_str(&format!("{mo}M")); }
    if w > 0 { date_part.push_str(&format!("{w}W")); }
    if d > 0 { date_part.push_str(&format!("{d}D")); }

    if h > 0 { time_part.push_str(&format!("{h}H")); }
    if mi > 0 { time_part.push_str(&format!("{mi}M")); }
    if s > 0 { time_part.push_str(&format!("{s}S")); }

    if date_part.is_empty() && time_part.is_empty() {
        return "P0D".to_string();
    }

    if time_part.is_empty() {
        format!("{prefix}{date_part}")
    } else {
        format!("{prefix}{date_part}T{time_part}")
    }
}

#[derive(Debug, Serialize)]
pub struct ZoneRecord {
    pub name: String,
    pub offset: String,
    pub abbreviation: String,
    pub is_dst: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_record_from_known_datetime() {
        let zoned: Zoned = "2026-06-15T10:30:45[UTC]".parse().unwrap();
        let rec = DateRecord::from_zoned(&zoned);
        assert_eq!(rec.year, 2026);
        assert_eq!(rec.month, 6);
        assert_eq!(rec.day, 15);
        assert_eq!(rec.hour, 10);
        assert_eq!(rec.minute, 30);
        assert_eq!(rec.second, 45);
        assert_eq!(rec.weekday, "Monday");
        assert_eq!(rec.weekday_num, 1);
        assert_eq!(rec.quarter, 2);
        assert_eq!(rec.timezone, "UTC");
        assert_eq!(rec.date, "2026-06-15");
        assert_eq!(rec.time, "10:30:45");
        assert!(!rec.is_leap_year);
        assert_eq!(rec.days_in_month, 30);
        assert_eq!(rec.days_in_year, 365);
    }

    #[test]
    fn date_record_leap_year() {
        let zoned: Zoned = "2024-02-15T00:00:00[UTC]".parse().unwrap();
        let rec = DateRecord::from_zoned(&zoned);
        assert!(rec.is_leap_year);
        assert_eq!(rec.days_in_month, 29);
        assert_eq!(rec.days_in_year, 366);
    }

    #[test]
    fn date_record_quarter_boundaries() {
        for (month, expected_q) in [(1, 1), (3, 1), (4, 2), (6, 2), (7, 3), (9, 3), (10, 4), (12, 4)] {
            let date_str = format!("2026-{month:02}-01T00:00:00[UTC]");
            let zoned: Zoned = date_str.parse().unwrap();
            let rec = DateRecord::from_zoned(&zoned);
            assert_eq!(rec.quarter, expected_q, "month {month} should be Q{expected_q}");
        }
    }

    #[test]
    fn date_record_unix_timestamps() {
        let zoned: Zoned = "2026-01-01T00:00:00[UTC]".parse().unwrap();
        let rec = DateRecord::from_zoned(&zoned);
        assert!(rec.unix > 0);
        assert_eq!(rec.unix_ms, rec.unix * 1000);
    }

    #[test]
    fn date_record_format_consistency() {
        let zoned: Zoned = "2026-03-25T14:30:00[UTC]".parse().unwrap();
        let rec = DateRecord::from_zoned(&zoned);
        assert!(rec.iso8601.contains("2026-03-25"));
        assert!(rec.iso8601.contains("14:30:00"));
        assert!(rec.rfc9557.contains("2026-03-25"));
        assert!(rec.datetime.contains("2026-03-25"));
    }
}
