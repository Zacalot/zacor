use std::io::BufRead;

use rand::Rng;
use rand::seq::SliceRandom;
use serde_json::{Value, json};

use chrono::Datelike;

use crate::args::*;
use crate::data;
use crate::make_rng;

pub fn cmd_int(args: &IntArgs) -> Result<Vec<Value>, String> {
    let min = args.min.unwrap_or(0);
    let max = args.max.unwrap_or(100);
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    if min > max {
        return Err(format!("rand int: min ({min}) must be <= max ({max})"));
    }

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let n: i64 = rng.gen_range(min..=max);
        results.push(json!({"value": n}));
    }
    Ok(results)
}

pub fn cmd_float(args: &FloatArgs) -> Result<Vec<Value>, String> {
    let min = args.min.unwrap_or(0.0);
    let max = args.max.unwrap_or(1.0);
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    if min > max {
        return Err(format!("rand float: min ({min}) must be <= max ({max})"));
    }

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let f: f64 = if min == max {
            min
        } else {
            rng.gen_range(min..max)
        };
        results.push(json!({"value": f}));
    }
    Ok(results)
}

pub fn cmd_bool(args: &BoolArgs) -> Result<Vec<Value>, String> {
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let b: bool = rng.gen_bool(0.5);
        results.push(json!({"value": b}));
    }
    Ok(results)
}

pub fn cmd_word(args: &WordArgs) -> Result<Vec<Value>, String> {
    let pool = match args.pool.as_deref() {
        Some(s) => data::Pool::from_str(s)?,
        None => data::Pool::Standard,
    };
    let _locale = crate::locale::resolve_locale(args.locale.as_deref());
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);
    let words = data::words(pool);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let word = words.choose(&mut rng).unwrap_or(&"");
        results.push(json!({"value": *word}));
    }
    Ok(results)
}

pub fn cmd_syllable(
    args: &SyllableArgs,
    input: Option<Box<dyn BufRead>>,
) -> Result<Vec<Value>, String> {
    let set_name = args.set.as_deref().unwrap_or("english");
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    if set_name == "input" {
        let reader =
            input.ok_or_else(|| "rand syllable: set=input requires piped input".to_string())?;
        let syllables: Vec<String> = reader
            .lines()
            .filter_map(|l| l.ok())
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if syllables.is_empty() {
            return Err("rand syllable: no syllables provided in input".to_string());
        }
        let min_syl = args.min_syllables.unwrap_or(2) as usize;
        let max_syl = args.max_syllables.unwrap_or(3) as usize;

        let mut results = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let n = rng.gen_range(min_syl..=max_syl);
            let word: String = (0..n)
                .map(|_| syllables.choose(&mut rng).unwrap().as_str())
                .collect();
            results.push(json!({"value": word}));
        }
        return Ok(results);
    }

    let set = data::syllable_set(set_name)?;
    let min_syl = args
        .min_syllables
        .map(|n| n as usize)
        .unwrap_or(set.min_syllables as usize);
    let max_syl = args
        .max_syllables
        .map(|n| n as usize)
        .unwrap_or(set.max_syllables as usize);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let n = rng.gen_range(min_syl..=max_syl);
        let mut word = String::new();
        for _ in 0..n {
            let onset = set.onsets.choose(&mut rng).unwrap_or(&"");
            let nucleus = set.nuclei.choose(&mut rng).unwrap_or(&"a");
            let coda = set.codas.choose(&mut rng).unwrap_or(&"");
            word.push_str(onset);
            word.push_str(nucleus);
            word.push_str(coda);
        }
        results.push(json!({"value": word}));
    }
    Ok(results)
}

pub fn cmd_name(args: &NameArgs) -> Result<Vec<Value>, String> {
    let pool = match args.pool.as_deref() {
        Some(s) => data::Pool::from_str(s)?,
        None => data::Pool::Standard,
    };
    let _locale = crate::locale::resolve_locale(args.locale.as_deref());
    let kind = args.kind.as_deref().unwrap_or("first");
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        match kind {
            "first" => {
                let name = data::firstnames(pool).choose(&mut rng).unwrap_or(&"");
                results.push(json!({"value": *name}));
            }
            "last" => {
                let name = data::lastnames(pool).choose(&mut rng).unwrap_or(&"");
                results.push(json!({"value": *name}));
            }
            "full" => {
                let first = *data::firstnames(pool).choose(&mut rng).unwrap_or(&"");
                let last = *data::lastnames(pool).choose(&mut rng).unwrap_or(&"");
                let full = format!("{first} {last}");
                results.push(json!({"value": full, "first": first, "last": last}));
            }
            other => {
                return Err(format!(
                    "rand name: unknown kind '{other}'. Valid: first, last, full"
                ));
            }
        }
    }
    Ok(results)
}

pub fn cmd_char(args: &CharArgs) -> Result<Vec<Value>, String> {
    let len = args.len.unwrap_or(8) as usize;
    let charset = args.charset.as_deref().unwrap_or("alnum");
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let chars: &[u8] = match charset {
        "alpha" => b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "alnum" => b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
        "hex" => b"0123456789abcdef",
        "digit" => b"0123456789",
        other => {
            return Err(format!(
                "rand char: unknown charset '{other}'. Valid: alpha, alnum, hex, digit"
            ));
        }
    };

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let s: String = (0..len)
            .map(|_| *chars.choose(&mut rng).unwrap() as char)
            .collect();
        results.push(json!({"value": s}));
    }
    Ok(results)
}

pub fn cmd_uuid(args: &UuidArgs) -> Result<Vec<Value>, String> {
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let mut bytes = [0u8; 16];
        rng.fill(&mut bytes);
        let uuid = uuid::Builder::from_random_bytes(bytes).into_uuid();
        results.push(json!({"value": uuid.to_string()}));
    }
    Ok(results)
}

pub fn cmd_phrase(args: &PhraseArgs) -> Result<Vec<Value>, String> {
    let pool = match args.pool.as_deref() {
        Some(s) => data::Pool::from_str(s)?,
        None => data::Pool::Standard,
    };
    let _locale = crate::locale::resolve_locale(args.locale.as_deref());
    let word_count = args.words.unwrap_or(4) as usize;
    let sep = args.sep.as_deref().unwrap_or("-");
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);
    let words = data::words(pool);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let phrase: Vec<&str> = (0..word_count)
            .map(|_| *words.choose(&mut rng).unwrap_or(&"word"))
            .collect();
        results.push(json!({"value": phrase.join(sep)}));
    }
    Ok(results)
}

pub fn cmd_pass(args: &PassArgs) -> Result<Vec<Value>, String> {
    let len = args.len.unwrap_or(16) as usize;
    // pass flags default to true when not explicitly set
    // Since bools default to false in the schema, we need to treat
    // "all false" as "all true" (the default behavior)
    let all_false = !args.upper && !args.lower && !args.digit && !args.symbol;
    let use_upper = args.upper || all_false;
    let use_lower = args.lower || all_false;
    let use_digit = args.digit || all_false;
    let use_symbol = args.symbol || all_false;
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let mut charset = Vec::new();
    if use_upper {
        charset.extend_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }
    if use_lower {
        charset.extend_from_slice(b"abcdefghijklmnopqrstuvwxyz");
    }
    if use_digit {
        charset.extend_from_slice(b"0123456789");
    }
    if use_symbol {
        charset.extend_from_slice(b"!@#$%^&*()-_=+[]{}|;:,.<>?");
    }

    if charset.is_empty() {
        return Err("rand pass: at least one character class must be enabled".to_string());
    }

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let s: String = (0..len)
            .map(|_| *charset.choose(&mut rng).unwrap() as char)
            .collect();
        results.push(json!({"value": s}));
    }
    Ok(results)
}

pub fn cmd_pattern(args: &PatternArgs) -> Result<Vec<Value>, String> {
    let fmt = &args.fmt;
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let upper: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let lower: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    let digits: &[u8] = b"0123456789";
    let alnum: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let mut s = String::new();
        let mut chars = fmt.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    s.push(next);
                }
            } else {
                match ch {
                    'L' => s.push(*upper.choose(&mut rng).unwrap() as char),
                    'l' => s.push(*lower.choose(&mut rng).unwrap() as char),
                    '#' => s.push(*digits.choose(&mut rng).unwrap() as char),
                    'X' => s.push(*alnum.choose(&mut rng).unwrap() as char),
                    other => s.push(other),
                }
            }
        }
        results.push(json!({"value": s}));
    }
    Ok(results)
}

pub fn cmd_color(args: &ColorArgs) -> Result<Vec<Value>, String> {
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let r: u8 = rng.gen_range(0u8..=255);
        let g: u8 = rng.gen_range(0u8..=255);
        let b: u8 = rng.gen_range(0u8..=255);
        let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
        results.push(json!({"value": hex, "r": r, "g": g, "b": b}));
    }
    Ok(results)
}

pub fn cmd_date(args: &DateArgs) -> Result<Vec<Value>, String> {
    let min_str = args.min.as_deref().unwrap_or("2000-01-01");
    let max_str = args.max.as_deref();
    let count = args.count.unwrap_or(1);
    let mut rng = make_rng(args.seed);

    let min_days = parse_date_to_days(min_str)?;
    let max_days = match max_str {
        Some(s) => parse_date_to_days(s)?,
        None => today_days(),
    };

    if min_days > max_days {
        return Err("rand date: min must be <= max".to_string());
    }

    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let days = rng.gen_range(min_days..=max_days);
        let date_str = days_to_date(days);
        results.push(json!({"value": date_str}));
    }
    Ok(results)
}

fn parse_date_to_days(s: &str) -> Result<i32, String> {
    let date = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| format!("rand date: invalid date '{s}': {e}"))?;
    Ok(date.num_days_from_ce())
}

fn today_days() -> i32 {
    chrono::Local::now().date_naive().num_days_from_ce()
}

fn days_to_date(days: i32) -> String {
    chrono::NaiveDate::from_num_days_from_ce_opt(days)
        .unwrap_or(chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap())
        .format("%Y-%m-%d")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use zacor_package::FromArgs;

    fn make_args<T: FromArgs>(pairs: &[(&str, Value)]) -> T {
        let map: BTreeMap<String, Value> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        T::from_args(&map).unwrap()
    }

    #[test]
    fn int_default_range() {
        let args: IntArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_int(&args).unwrap();
        assert_eq!(result.len(), 1);
        let v = result[0]["value"].as_i64().unwrap();
        assert!((0..=100).contains(&v));
    }

    #[test]
    fn int_custom_range() {
        let args: IntArgs = make_args(&[
            ("min", json!(50)),
            ("max", json!(60)),
            ("count", json!(10)),
            ("seed", json!(42)),
        ]);
        let result = cmd_int(&args).unwrap();
        assert_eq!(result.len(), 10);
        for r in &result {
            let v = r["value"].as_i64().unwrap();
            assert!((50..=60).contains(&v));
        }
    }

    #[test]
    fn int_seed_determinism() {
        let a: IntArgs = make_args(&[("count", json!(5)), ("seed", json!(42))]);
        let b: IntArgs = make_args(&[("count", json!(5)), ("seed", json!(42))]);
        assert_eq!(cmd_int(&a).unwrap(), cmd_int(&b).unwrap());
    }

    #[test]
    fn int_min_gt_max_error() {
        let args: IntArgs = make_args(&[("min", json!(100)), ("max", json!(0))]);
        assert!(cmd_int(&args).is_err());
    }

    #[test]
    fn float_default_range() {
        let args: FloatArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_float(&args).unwrap();
        let v = result[0]["value"].as_f64().unwrap();
        assert!((0.0..1.0).contains(&v));
    }

    #[test]
    fn float_seed_determinism() {
        let a: FloatArgs = make_args(&[("count", json!(5)), ("seed", json!(99))]);
        let b: FloatArgs = make_args(&[("count", json!(5)), ("seed", json!(99))]);
        assert_eq!(cmd_float(&a).unwrap(), cmd_float(&b).unwrap());
    }

    #[test]
    fn bool_generates() {
        let args: BoolArgs = make_args(&[("count", json!(20)), ("seed", json!(42))]);
        let result = cmd_bool(&args).unwrap();
        assert_eq!(result.len(), 20);
        let has_true = result.iter().any(|r| r["value"].as_bool() == Some(true));
        let has_false = result.iter().any(|r| r["value"].as_bool() == Some(false));
        assert!(has_true && has_false);
    }

    #[test]
    fn word_from_dictionary() {
        let args: WordArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        let result = cmd_word(&args).unwrap();
        assert_eq!(result.len(), 3);
        for r in &result {
            assert!(r["value"].as_str().unwrap().len() > 0);
        }
    }

    #[test]
    fn word_seed_determinism() {
        let a: WordArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        let b: WordArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        assert_eq!(cmd_word(&a).unwrap(), cmd_word(&b).unwrap());
    }

    #[test]
    fn name_first() {
        let args: NameArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_name(&args).unwrap();
        assert!(result[0]["value"].as_str().unwrap().len() > 0);
    }

    #[test]
    fn name_full_has_rich_fields() {
        let args: NameArgs = make_args(&[("kind", json!("full")), ("seed", json!(42))]);
        let result = cmd_name(&args).unwrap();
        let r = &result[0];
        assert!(r["value"].as_str().unwrap().contains(' '));
        assert!(r["first"].as_str().is_some());
        assert!(r["last"].as_str().is_some());
    }

    #[test]
    fn char_default_alnum() {
        let args: CharArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_char(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.len(), 8);
        assert!(s.chars().all(|c| c.is_alphanumeric()));
    }

    #[test]
    fn char_hex() {
        let args: CharArgs = make_args(&[
            ("charset", json!("hex")),
            ("len", json!(16)),
            ("seed", json!(42)),
        ]);
        let result = cmd_char(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.len(), 16);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn uuid_format() {
        let args: UuidArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_uuid(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.len(), 36);
        assert_eq!(s.chars().filter(|c| *c == '-').count(), 4);
    }

    #[test]
    fn uuid_seed_determinism() {
        let a: UuidArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        let b: UuidArgs = make_args(&[("count", json!(3)), ("seed", json!(42))]);
        assert_eq!(cmd_uuid(&a).unwrap(), cmd_uuid(&b).unwrap());
    }

    #[test]
    fn phrase_default() {
        let args: PhraseArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_phrase(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.split('-').count(), 4);
    }

    #[test]
    fn phrase_custom_sep_and_words() {
        let args: PhraseArgs = make_args(&[
            ("words", json!(3)),
            ("sep", json!(".")),
            ("seed", json!(42)),
        ]);
        let result = cmd_phrase(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.split('.').count(), 3);
    }

    #[test]
    fn pass_default_length() {
        let args: PassArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_pass(&args).unwrap();
        assert_eq!(result[0]["value"].as_str().unwrap().len(), 16);
    }

    #[test]
    fn pass_digits_only() {
        let args: PassArgs = make_args(&[
            ("len", json!(6)),
            ("digit", json!(true)),
            ("seed", json!(42)),
        ]);
        let result = cmd_pass(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.len(), 6);
        assert!(s.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn pattern_license_plate() {
        let args: PatternArgs = make_args(&[("fmt", json!("LLL-####")), ("seed", json!(42))]);
        let result = cmd_pattern(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.len(), 8);
        assert!(s[0..3].chars().all(|c| c.is_ascii_uppercase()));
        assert_eq!(&s[3..4], "-");
        assert!(s[4..8].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn pattern_escaped() {
        let args: PatternArgs = make_args(&[("fmt", json!("\\L-###")), ("seed", json!(42))]);
        let result = cmd_pattern(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert!(s.starts_with("L-"));
        assert!(s[2..5].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn color_has_hex_and_rgb() {
        let args: ColorArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_color(&args).unwrap();
        let r = &result[0];
        let hex = r["value"].as_str().unwrap();
        assert!(hex.starts_with('#'));
        assert_eq!(hex.len(), 7);
        assert!(r["r"].as_u64().unwrap() <= 255);
        assert!(r["g"].as_u64().unwrap() <= 255);
        assert!(r["b"].as_u64().unwrap() <= 255);
    }

    #[test]
    fn date_default_range() {
        let args: DateArgs = make_args(&[("seed", json!(42))]);
        let result = cmd_date(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.len(), 10);
        assert!(s >= "2000-01-01");
    }

    #[test]
    fn date_custom_range() {
        let args: DateArgs = make_args(&[
            ("min", json!("2020-06-01")),
            ("max", json!("2020-06-30")),
            ("count", json!(10)),
            ("seed", json!(42)),
        ]);
        let result = cmd_date(&args).unwrap();
        assert_eq!(result.len(), 10);
        for r in &result {
            let d = r["value"].as_str().unwrap();
            assert!(d >= "2020-06-01" && d <= "2020-06-30");
        }
    }

    #[test]
    fn syllable_english() {
        let args: SyllableArgs = make_args(&[("count", json!(5)), ("seed", json!(42))]);
        let result = cmd_syllable(&args, None).unwrap();
        assert_eq!(result.len(), 5);
        for r in &result {
            assert!(r["value"].as_str().unwrap().len() >= 2);
        }
    }

    #[test]
    fn syllable_fantasy() {
        let args: SyllableArgs = make_args(&[
            ("set", json!("fantasy")),
            ("count", json!(3)),
            ("seed", json!(42)),
        ]);
        let result = cmd_syllable(&args, None).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn syllable_simple() {
        let args: SyllableArgs = make_args(&[
            ("set", json!("simple")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        let result = cmd_syllable(&args, None).unwrap();
        for r in &result {
            assert!(r["value"].as_str().unwrap().len() <= 5);
        }
    }

    #[test]
    fn syllable_input_mode() {
        let input_data = "ka\nri\nmo\nzu\n";
        let reader: Box<dyn BufRead> = Box::new(std::io::Cursor::new(input_data));
        let args: SyllableArgs = make_args(&[
            ("set", json!("input")),
            ("count", json!(3)),
            ("seed", json!(42)),
        ]);
        let result = cmd_syllable(&args, Some(reader)).unwrap();
        assert_eq!(result.len(), 3);
        for r in &result {
            let word = r["value"].as_str().unwrap();
            assert!(word.len() >= 4);
        }
    }

    #[test]
    fn word_extended_pool() {
        let args: WordArgs = make_args(&[
            ("pool", json!("extended")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        let result = cmd_word(&args).unwrap();
        assert_eq!(result.len(), 5);
        for r in &result {
            assert!(!r["value"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn word_no_pool_uses_standard() {
        let a: WordArgs = make_args(&[("count", json!(3)), ("seed", json!(99))]);
        let b: WordArgs = make_args(&[
            ("pool", json!("standard")),
            ("count", json!(3)),
            ("seed", json!(99)),
        ]);
        assert_eq!(cmd_word(&a).unwrap(), cmd_word(&b).unwrap());
    }

    #[test]
    fn name_extended_first() {
        let args: NameArgs = make_args(&[
            ("pool", json!("extended")),
            ("kind", json!("first")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        let result = cmd_name(&args).unwrap();
        assert_eq!(result.len(), 5);
        for r in &result {
            assert!(!r["value"].as_str().unwrap().is_empty());
        }
    }

    #[test]
    fn name_extended_last_uses_standard() {
        let standard: NameArgs = make_args(&[
            ("kind", json!("last")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        let extended: NameArgs = make_args(&[
            ("pool", json!("extended")),
            ("kind", json!("last")),
            ("count", json!(5)),
            ("seed", json!(42)),
        ]);
        assert_eq!(cmd_name(&standard).unwrap(), cmd_name(&extended).unwrap());
    }

    #[test]
    fn name_extended_full_mixed_pools() {
        let args: NameArgs = make_args(&[
            ("pool", json!("extended")),
            ("kind", json!("full")),
            ("seed", json!(42)),
        ]);
        let result = cmd_name(&args).unwrap();
        let r = &result[0];
        assert!(r["value"].as_str().unwrap().contains(' '));
        assert!(r["first"].as_str().is_some());
        assert!(r["last"].as_str().is_some());
        let last = r["last"].as_str().unwrap();
        assert!(data::lastnames(data::Pool::Standard).contains(&last));
    }

    #[test]
    fn phrase_extended_pool() {
        let args: PhraseArgs = make_args(&[("pool", json!("extended")), ("seed", json!(42))]);
        let result = cmd_phrase(&args).unwrap();
        let s = result[0]["value"].as_str().unwrap();
        assert_eq!(s.split('-').count(), 4);
    }

    #[test]
    fn invalid_pool_returns_error() {
        let args: WordArgs = make_args(&[("pool", json!("giant"))]);
        let result = cmd_word(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("giant"));
    }

    #[test]
    fn resolve_locale_fallback() {
        let locale = crate::locale::resolve_locale(Some("fr"));
        assert_eq!(locale, "en-US");
        let locale = crate::locale::resolve_locale(Some("en-US"));
        assert_eq!(locale, "en-US");
        let locale = crate::locale::resolve_locale(None);
        assert_eq!(locale, "en-US");
    }
}
