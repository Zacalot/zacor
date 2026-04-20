use jiff::{Zoned, Unit};
use jiff::tz::TimeZone;
use serde_json::Value;

use crate::args::{DefaultArgs, AddArgs, DiffArgs, SeqArgs, RoundArgs, ZonesArgs};
use crate::parse::{parse_date, parse_duration, resolve_timezone};
use crate::records::{DateRecord, DiffRecord, ZoneRecord};

/// Default command: parse --date (or now), apply --timezone/--utc, return DateRecord
pub fn cmd_default(args: &DefaultArgs) -> Result<Vec<Value>, String> {
    let tz = resolve_timezone(args.timezone.as_deref(), args.utc)?;

    let zoned = match args.date.as_deref() {
        Some(date_str) => {
            let parsed = parse_date(date_str, &tz)?;
            if args.timezone.is_some() || args.utc {
                parsed.with_time_zone(tz)
            } else {
                parsed
            }
        }
        None => Zoned::now().with_time_zone(tz),
    };

    let record = DateRecord::from_zoned(&zoned);
    Ok(vec![serde_json::to_value(&record).unwrap()])
}

/// Add command: parse --date (or now), parse --duration, add span, return DateRecord
pub fn cmd_add(args: &AddArgs) -> Result<Vec<Value>, String> {
    let tz = resolve_timezone(args.timezone.as_deref(), args.utc)?;

    let zoned = match args.date.as_deref() {
        Some(date_str) => parse_date(date_str, &tz)?,
        None => Zoned::now().with_time_zone(tz),
    };

    let span = parse_duration(&args.duration)?;
    let result = zoned
        .checked_add(span)
        .map_err(|e| format!("date add failed: {e}"))?;

    let record = DateRecord::from_zoned(&result);
    Ok(vec![serde_json::to_value(&record).unwrap()])
}

/// Diff command: parse --from and --to (default now), compute span, return DiffRecord
pub fn cmd_diff(args: &DiffArgs) -> Result<Vec<Value>, String> {
    let tz = resolve_timezone(None, false)?;
    let from = parse_date(&args.from, &tz)?;

    let to = match args.to.as_deref() {
        Some(to_str) => parse_date(to_str, &tz)?,
        None => Zoned::now(),
    };

    let span = from
        .until(
            jiff::ZonedDifference::new(&to)
                .smallest(Unit::Second)
                .largest(Unit::Year),
        )
        .map_err(|e| format!("date diff failed: {e}"))?;

    let record = DiffRecord::from_span(&from, &to, span);
    Ok(vec![serde_json::to_value(&record).unwrap()])
}

/// Seq command: generate sequence of DateRecords
pub fn cmd_seq(args: &SeqArgs) -> Result<Vec<Value>, String> {
    let tz = resolve_timezone(args.timezone.as_deref(), args.utc)?;

    let from = match args.from.as_deref() {
        Some(s) => parse_date(s, &tz)?,
        None => Zoned::now().with_time_zone(tz),
    };

    let to = match args.to.as_deref() {
        Some(s) => Some(parse_date(s, &from.time_zone().clone())?),
        None => None,
    };

    let count = args.count.map(|n| n as u64);

    if to.is_none() && count.is_none() {
        return Err("date seq: either --to or --count is required".to_string());
    }

    let step = match args.step.as_deref() {
        Some(s) => parse_duration(s)?,
        None => jiff::Span::new().days(1),
    };

    let mut results = Vec::new();
    let mut current = from;

    match (to, count) {
        (Some(end), _) => {
            while current <= end {
                results.push(serde_json::to_value(&DateRecord::from_zoned(&current)).unwrap());
                current = current
                    .checked_add(step)
                    .map_err(|e| format!("date seq: step overflow: {e}"))?;
            }
        }
        (None, Some(n)) => {
            for _ in 0..n {
                results.push(serde_json::to_value(&DateRecord::from_zoned(&current)).unwrap());
                current = current
                    .checked_add(step)
                    .map_err(|e| format!("date seq: step overflow: {e}"))?;
            }
        }
        _ => unreachable!(),
    }

    Ok(results)
}

/// Round command: round datetime to nearest unit
pub fn cmd_round(args: &RoundArgs) -> Result<Vec<Value>, String> {
    let tz = resolve_timezone(args.timezone.as_deref(), args.utc)?;

    let zoned = match args.date.as_deref() {
        Some(date_str) => parse_date(date_str, &tz)?,
        None => Zoned::now().with_time_zone(tz),
    };

    let unit_lower = args.to.to_lowercase();

    let rounded = match unit_lower.as_str() {
        "year" => {
            let d = zoned.datetime().date();
            let tz = zoned.time_zone().clone();
            let mid_year = jiff::civil::date(d.year(), 7, 1).to_zoned(tz.clone())
                .map_err(|e| format!("date round failed: {e}"))?;
            let target_year = if zoned >= mid_year { d.year() + 1 } else { d.year() };
            jiff::civil::date(target_year, 1, 1).to_zoned(tz)
                .map_err(|e| format!("date round failed: {e}"))?
        }
        "month" => {
            let d = zoned.datetime().date();
            let tz = zoned.time_zone().clone();
            let days_in_month = d.days_in_month();
            let mid_day = (days_in_month / 2) + 1;
            let (target_year, target_month) = if d.day() >= mid_day || (d.day() == mid_day - 1 && zoned.datetime().time().hour() >= 12) {
                if d.month() == 12 {
                    (d.year() + 1, 1)
                } else {
                    (d.year(), d.month() + 1)
                }
            } else {
                (d.year(), d.month())
            };
            jiff::civil::date(target_year, target_month, 1).to_zoned(tz)
                .map_err(|e| format!("date round failed: {e}"))?
        }
        "week" | "day" | "hour" | "minute" | "second" => {
            let unit = match unit_lower.as_str() {
                "week" => Unit::Week,
                "day" => Unit::Day,
                "hour" => Unit::Hour,
                "minute" => Unit::Minute,
                "second" => Unit::Second,
                _ => unreachable!(),
            };
            zoned
                .round(jiff::ZonedRound::new().smallest(unit).mode(jiff::RoundMode::HalfExpand))
                .map_err(|e| format!("date round failed: {e}"))?
        }
        other => return Err(format!("date round: unknown unit '{other}'. Valid: year, month, week, day, hour, minute, second")),
    };

    let record = DateRecord::from_zoned(&rounded);
    Ok(vec![serde_json::to_value(&record).unwrap()])
}

/// Zones command: list all IANA timezones
pub fn cmd_zones(_args: &ZonesArgs) -> Result<Vec<Value>, String> {
    let now = jiff::Timestamp::now();
    let mut results = Vec::new();

    for name in jiff_tzdb::available() {
        let tz = match TimeZone::get(name) {
            Ok(tz) => tz,
            Err(_) => continue,
        };

        let zoned = now.to_zoned(tz);
        let offset = zoned.offset();
        let offset_str = {
            let raw = format!("{offset}");
            if raw.len() == 3 { format!("{raw}:00") } else { raw }
        };

        let abbr = {
            let fmt = format!("{zoned}");
            let _ = fmt;
            String::new()
        };

        let jan = jiff::civil::date(zoned.datetime().date().year(), 1, 1)
            .to_zoned(zoned.time_zone().clone())
            .ok();
        let is_dst = match jan {
            Some(jan_zoned) => zoned.offset() != jan_zoned.offset(),
            None => false,
        };

        let abbreviation = if abbr.is_empty() {
            offset_str.clone()
        } else {
            abbr
        };

        results.push(
            serde_json::to_value(&ZoneRecord {
                name: name.to_string(),
                offset: offset_str,
                abbreviation,
                is_dst,
            })
            .unwrap(),
        );
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zacor_package::FromArgs;
    use std::collections::BTreeMap;
    use serde_json::json;

    fn make_args<T: FromArgs>(pairs: &[(&str, Value)]) -> T {
        let map: BTreeMap<String, Value> = pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
        T::from_args(&map).unwrap()
    }

    #[test]
    fn default_no_args_returns_now() {
        let args: DefaultArgs = make_args(&[]);
        let result = cmd_default(&args).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].get("year").unwrap().as_i64().unwrap() >= 2026);
    }

    #[test]
    fn default_with_date() {
        let args: DefaultArgs = make_args(&[("date", json!("2026-03-25"))]);
        let result = cmd_default(&args).unwrap();
        assert_eq!(result[0]["year"], 2026);
        assert_eq!(result[0]["month"], 3);
        assert_eq!(result[0]["day"], 25);
    }

    #[test]
    fn default_with_timezone() {
        let args: DefaultArgs = make_args(&[("timezone", json!("Asia/Tokyo"))]);
        let result = cmd_default(&args).unwrap();
        assert_eq!(result[0]["timezone"], "Asia/Tokyo");
    }

    #[test]
    fn default_with_utc() {
        let args: DefaultArgs = make_args(&[("utc", json!(true))]);
        let result = cmd_default(&args).unwrap();
        assert_eq!(result[0]["timezone"], "UTC");
        assert_eq!(result[0]["offset"], "+00:00");
    }

    #[test]
    fn add_positive_duration() {
        let args: AddArgs = make_args(&[
            ("date", json!("2026-01-01T00:00:00[UTC]")),
            ("duration", json!("1year")),
        ]);
        let result = cmd_add(&args).unwrap();
        assert_eq!(result[0]["year"], 2027);
    }

    #[test]
    fn add_negative_duration() {
        let args: AddArgs = make_args(&[
            ("date", json!("2026-06-15T00:00:00[UTC]")),
            ("duration", json!("-2weeks")),
        ]);
        let result = cmd_add(&args).unwrap();
        assert_eq!(result[0]["day"], 1);
        assert_eq!(result[0]["month"], 6);
    }

    #[test]
    fn add_dst_safe() {
        let args: AddArgs = make_args(&[
            ("date", json!("2026-03-08T01:00:00[America/New_York]")),
            ("duration", json!("1day")),
        ]);
        let result = cmd_add(&args).unwrap();
        assert_eq!(result[0]["day"], 9);
        assert_eq!(result[0]["hour"], 1);
    }

    #[test]
    fn add_missing_duration() {
        let map: BTreeMap<String, Value> = [("date".to_string(), json!("2026-01-01"))].into();
        assert!(AddArgs::from_args(&map).is_err());
    }

    #[test]
    fn diff_forward() {
        let args: DiffArgs = make_args(&[
            ("from", json!("2026-01-01T00:00:00[UTC]")),
            ("to", json!("2026-12-31T00:00:00[UTC]")),
        ]);
        let result = cmd_diff(&args).unwrap();
        assert!(result[0]["total_days"].as_f64().unwrap() > 300.0);
    }

    #[test]
    fn diff_time_only() {
        let args: DiffArgs = make_args(&[
            ("from", json!("2026-01-01T00:00:00[UTC]")),
            ("to", json!("2026-01-01T05:30:00[UTC]")),
        ]);
        let result = cmd_diff(&args).unwrap();
        assert_eq!(result[0]["hours"], 5);
        assert_eq!(result[0]["minutes"], 30);
    }

    #[test]
    fn diff_negative() {
        let args: DiffArgs = make_args(&[
            ("from", json!("2026-12-31T00:00:00[UTC]")),
            ("to", json!("2026-01-01T00:00:00[UTC]")),
        ]);
        let result = cmd_diff(&args).unwrap();
        assert!(result[0]["total_seconds"].as_i64().unwrap() < 0);
    }

    #[test]
    fn seq_daily() {
        let args: SeqArgs = make_args(&[
            ("from", json!("2026-01-01T00:00:00[UTC]")),
            ("to", json!("2026-01-07T00:00:00[UTC]")),
        ]);
        let result = cmd_seq(&args).unwrap();
        assert_eq!(result.len(), 7);
    }

    #[test]
    fn seq_monthly() {
        let args: SeqArgs = make_args(&[
            ("from", json!("2026-01-01T00:00:00[UTC]")),
            ("to", json!("2026-12-01T00:00:00[UTC]")),
            ("step", json!("1month")),
        ]);
        let result = cmd_seq(&args).unwrap();
        assert_eq!(result.len(), 12);
    }

    #[test]
    fn seq_with_count() {
        let args: SeqArgs = make_args(&[
            ("from", json!("2026-01-01T00:00:00[UTC]")),
            ("count", json!(5)),
            ("step", json!("1week")),
        ]);
        let result = cmd_seq(&args).unwrap();
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn seq_default_step_is_1day() {
        let args: SeqArgs = make_args(&[
            ("from", json!("2026-01-01T00:00:00[UTC]")),
            ("to", json!("2026-01-03T00:00:00[UTC]")),
        ]);
        let result = cmd_seq(&args).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn seq_missing_to_and_count() {
        let args: SeqArgs = make_args(&[("from", json!("2026-01-01"))]);
        assert!(cmd_seq(&args).is_err());
    }

    #[test]
    fn round_to_hour() {
        let args: RoundArgs = make_args(&[
            ("date", json!("2026-03-25T14:45:00[UTC]")),
            ("to", json!("hour")),
        ]);
        let result = cmd_round(&args).unwrap();
        assert_eq!(result[0]["hour"], 15);
        assert_eq!(result[0]["minute"], 0);
    }

    #[test]
    fn round_to_day() {
        let args: RoundArgs = make_args(&[
            ("date", json!("2026-03-25T14:30:00[UTC]")),
            ("to", json!("day")),
        ]);
        let result = cmd_round(&args).unwrap();
        assert_eq!(result[0]["day"], 26);
    }

    #[test]
    fn round_to_month() {
        let args: RoundArgs = make_args(&[
            ("date", json!("2026-03-25T00:00:00[UTC]")),
            ("to", json!("month")),
        ]);
        let result = cmd_round(&args).unwrap();
        assert_eq!(result[0]["month"], 4);
        assert_eq!(result[0]["day"], 1);
    }

    #[test]
    fn round_to_year() {
        let args: RoundArgs = make_args(&[
            ("date", json!("2026-08-01T00:00:00[UTC]")),
            ("to", json!("year")),
        ]);
        let result = cmd_round(&args).unwrap();
        assert_eq!(result[0]["year"], 2027);
    }

    #[test]
    fn round_missing_to() {
        let map: BTreeMap<String, Value> = [("date".to_string(), json!("2026-01-01"))].into();
        assert!(RoundArgs::from_args(&map).is_err());
    }

    #[test]
    fn zones_contains_known() {
        let args: ZonesArgs = make_args(&[]);
        let result = cmd_zones(&args).unwrap();
        let names: Vec<&str> = result.iter()
            .filter_map(|v| v["name"].as_str())
            .collect();
        assert!(names.contains(&"America/New_York"));
        assert!(names.contains(&"Europe/London"));
        assert!(names.contains(&"Asia/Tokyo"));
        assert!(names.contains(&"UTC"));
    }

    #[test]
    fn zones_utc_offset() {
        let args: ZonesArgs = make_args(&[]);
        let result = cmd_zones(&args).unwrap();
        let utc = result.iter().find(|v| v["name"] == "UTC").unwrap();
        assert_eq!(utc["offset"], "+00:00");
    }
}
