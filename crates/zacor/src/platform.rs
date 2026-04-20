use crate::error::*;

/// Validate a package name against the pattern `[a-z][a-z0-9]*(-[a-z0-9]+)*`.
pub fn validate_package_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("package name must not be empty");
    }
    if name.len() > 100 {
        bail!("package name must not exceed 100 characters");
    }
    if name.starts_with('.') {
        bail!("package name must not start with '.'");
    }
    validate_hyphenated_identifier(name, "package name")?;
    check_reserved_package_name(name)?;
    check_windows_reserved_name(name)?;
    Ok(())
}

/// Validate a config key against the pattern `[a-z][a-z0-9]*(-[a-z0-9]+)*`.
pub fn validate_config_key(key: &str) -> Result<()> {
    if key.is_empty() {
        bail!("config key must not be empty");
    }
    validate_hyphenated_identifier(key, "config key")
}

/// Normalize a version string: strip leading v/V, lowercase.
pub fn normalize_version(version: &str) -> String {
    let stripped = version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version);
    stripped.to_lowercase()
}

/// Validate that a string matches `[a-z][a-z0-9]*(-[a-z0-9]+)*`.
fn validate_hyphenated_identifier(s: &str, label: &str) -> Result<()> {
    let first = s.chars().next().unwrap();
    if !first.is_ascii_lowercase() {
        bail!("{} must start with a lowercase letter: '{}'", label, s);
    }
    let mut prev_hyphen = false;
    for ch in s.chars().skip(1) {
        if ch == '-' {
            if prev_hyphen {
                bail!("{} must not contain consecutive hyphens: '{}'", label, s);
            }
            prev_hyphen = true;
        } else if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            prev_hyphen = false;
        } else {
            bail!("{} contains invalid characters: '{}'", label, s);
        }
    }
    if s.ends_with('-') {
        bail!("{} must not end with a hyphen: '{}'", label, s);
    }
    Ok(())
}

fn check_reserved_package_name(name: &str) -> Result<()> {
    const RESERVED: &[&str] = &[
        "install",
        "remove",
        "list",
        "enable",
        "disable",
        "update",
        "help",
        "use",
        "config",
        "set-mode",
        "set-transport",
        "serve",
        "daemon",
        "zr",
        "zacor",
    ];
    if RESERVED.contains(&name) {
        bail!("package name '{}' is reserved", name);
    }
    Ok(())
}

fn check_windows_reserved_name(name: &str) -> Result<()> {
    const RESERVED: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
        "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    if RESERVED.contains(&name) {
        bail!("package name is a Windows reserved device name: '{}'", name);
    }
    Ok(())
}

pub fn current_os() -> &'static str {
    std::env::consts::OS
}

pub fn current_arch() -> &'static str {
    std::env::consts::ARCH
}

pub fn exe_suffix() -> &'static str {
    std::env::consts::EXE_SUFFIX
}

struct PlatformEntry {
    name: &'static str,
    keywords: &'static [&'static str],
}

struct FormatPreference {
    os: &'static str,
    suffix: &'static str,
    score: i32,
}

struct ArchFallback {
    os: &'static str,
    from_arch: &'static str,
    to_arch: &'static str,
}

const OS_TABLE: &[PlatformEntry] = &[
    PlatformEntry {
        name: "linux",
        keywords: &["linux"],
    },
    PlatformEntry {
        name: "macos",
        keywords: &["darwin", "macos", "osx", "apple"],
    },
    PlatformEntry {
        name: "windows",
        keywords: &["windows", "win64", "win32"],
    },
    PlatformEntry {
        name: "freebsd",
        keywords: &["freebsd"],
    },
    PlatformEntry {
        name: "netbsd",
        keywords: &["netbsd"],
    },
    PlatformEntry {
        name: "openbsd",
        keywords: &["openbsd"],
    },
    PlatformEntry {
        name: "android",
        keywords: &["android"],
    },
];

const ARCH_TABLE: &[PlatformEntry] = &[
    PlatformEntry {
        name: "x86_64",
        keywords: &["x86_64", "x86-64", "amd64", "x64"],
    },
    PlatformEntry {
        name: "aarch64",
        keywords: &["aarch64", "arm64"],
    },
    PlatformEntry {
        name: "x86",
        keywords: &["i686", "i386"],
    },
    PlatformEntry {
        name: "arm",
        keywords: &["armv7", "armhf"],
    },
    PlatformEntry {
        name: "riscv64gc",
        keywords: &["riscv64"],
    },
    PlatformEntry {
        name: "powerpc64",
        keywords: &["ppc64", "ppc64le"],
    },
    PlatformEntry {
        name: "s390x",
        keywords: &["s390x"],
    },
];

const EXCLUDED_EXTENSIONS: &[&str] = &[
    ".deb",
    ".rpm",
    ".msi",
    ".dmg",
    ".pkg",
    ".apk",
    ".sha256",
    ".sha256sum",
    ".sha512",
    ".md5",
    ".sig",
    ".asc",
    ".sbom",
    ".json",
    ".txt",
    ".yaml",
    ".yml",
];

const FORMAT_PREFERENCES: &[FormatPreference] = &[
    FormatPreference {
        os: "windows",
        suffix: ".zip",
        score: 10,
    },
    FormatPreference {
        os: "windows",
        suffix: ".tar.gz",
        score: 5,
    },
    FormatPreference {
        os: "windows",
        suffix: ".tgz",
        score: 5,
    },
    FormatPreference {
        os: "*",
        suffix: ".tar.gz",
        score: 10,
    },
    FormatPreference {
        os: "*",
        suffix: ".tgz",
        score: 10,
    },
    FormatPreference {
        os: "*",
        suffix: ".zip",
        score: 5,
    },
];

const ARCH_FALLBACKS: &[ArchFallback] = &[ArchFallback {
    os: "macos",
    from_arch: "aarch64",
    to_arch: "x86_64",
}];

/// Check if `keyword` appears in `name` at a word boundary.
/// A word boundary means the character before/after the match is non-alphanumeric or a string edge.
fn keyword_present(name: &str, keyword: &str) -> bool {
    let name_bytes = name.as_bytes();
    let kw_len = keyword.len();
    let mut start = 0;
    while let Some(pos) = name[start..].find(keyword) {
        let abs_pos = start + pos;
        let before_ok = abs_pos == 0 || !name_bytes[abs_pos - 1].is_ascii_alphanumeric();
        let after_pos = abs_pos + kw_len;
        let after_ok =
            after_pos >= name_bytes.len() || !name_bytes[after_pos].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs_pos + 1;
    }
    false
}

fn matches_other(name_lower: &str, ours: &str, table: &[PlatformEntry]) -> bool {
    for entry in table {
        if entry.name == ours {
            continue;
        }
        for kw in entry.keywords {
            if keyword_present(name_lower, kw) {
                return true;
            }
        }
    }
    false
}

fn matches_current(name_lower: &str, ours: &str, table: &[PlatformEntry]) -> bool {
    for entry in table {
        if entry.name == ours {
            for kw in entry.keywords {
                if keyword_present(name_lower, kw) {
                    return true;
                }
            }
            return false;
        }
    }
    false
}

fn is_excluded(name_lower: &str) -> bool {
    EXCLUDED_EXTENSIONS
        .iter()
        .any(|ext| name_lower.ends_with(ext))
}

fn score_format(name_lower: &str, os: &str) -> i32 {
    for pref in FORMAT_PREFERENCES {
        if (pref.os == os || pref.os == "*") && name_lower.ends_with(pref.suffix) {
            return pref.score;
        }
    }
    0
}

pub fn select_asset(asset_names: &[String], package_hint: Option<&str>) -> Option<String> {
    select_asset_for(asset_names, current_os(), current_arch(), package_hint)
}

pub fn select_asset_for(
    asset_names: &[String],
    os: &str,
    arch: &str,
    package_hint: Option<&str>,
) -> Option<String> {
    let mut candidates: Vec<(String, i32)> = Vec::new();

    for name in asset_names {
        let lower = name.to_lowercase();

        // Filter by package hint if provided
        if let Some(hint) = package_hint {
            if !lower.contains(&hint.to_lowercase()) {
                continue;
            }
        }

        if matches_other(&lower, os, OS_TABLE) {
            continue;
        }

        if matches_other(&lower, arch, ARCH_TABLE) {
            continue;
        }

        if is_excluded(&lower) {
            continue;
        }

        let mut score = score_format(&lower, os);
        if matches_current(&lower, os, OS_TABLE) {
            score += 20;
        }
        if matches_current(&lower, arch, ARCH_TABLE) {
            score += 10;
        }
        candidates.push((name.clone(), score));
    }

    if candidates.is_empty() {
        for fb in ARCH_FALLBACKS {
            if fb.os == os && fb.from_arch == arch {
                return select_asset_for(asset_names, os, fb.to_arch, package_hint);
            }
        }
        return None;
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(candidates[0].0.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_package_name_valid() {
        assert!(validate_package_name("my-tool").is_ok());
        assert!(validate_package_name("abc123").is_ok());
        assert!(validate_package_name("a").is_ok());
        assert!(validate_package_name("tool2").is_ok());
        assert!(validate_package_name("my-great-tool").is_ok());
    }

    #[test]
    fn test_validate_package_name_underscore() {
        let err = validate_package_name("my_tool").unwrap_err().to_string();
        assert!(err.contains("invalid characters"), "got: {}", err);
    }

    #[test]
    fn test_validate_package_name_uppercase() {
        let err = validate_package_name("MyTool").unwrap_err().to_string();
        assert!(
            err.contains("must start with a lowercase letter")
                || err.contains("invalid characters"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_validate_package_name_leading_digit() {
        let err = validate_package_name("123tool").unwrap_err().to_string();
        assert!(
            err.contains("must start with a lowercase letter"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_validate_package_name_consecutive_hyphens() {
        let err = validate_package_name("my--tool").unwrap_err().to_string();
        assert!(err.contains("consecutive hyphens"), "got: {}", err);
    }

    #[test]
    fn test_validate_package_name_trailing_hyphen() {
        let err = validate_package_name("my-tool-").unwrap_err().to_string();
        assert!(err.contains("end with a hyphen"), "got: {}", err);
    }

    #[test]
    fn test_validate_package_name_reserved() {
        assert!(validate_package_name("install").is_err());
        assert!(validate_package_name("remove").is_err());
        assert!(validate_package_name("use").is_err());
        assert!(validate_package_name("config").is_err());
        assert!(validate_package_name("set-mode").is_err());
        assert!(validate_package_name("set-transport").is_err());
        assert!(validate_package_name("serve").is_err());
        assert!(validate_package_name("daemon").is_err());
        assert!(validate_package_name("zr").is_err());
        assert!(validate_package_name("zacor").is_err());
        // Non-reserved similar names should pass
        assert!(validate_package_name("installer").is_ok());
        assert!(validate_package_name("my-config").is_ok());
    }

    #[test]
    fn test_validate_package_name_windows_reserved() {
        assert!(validate_package_name("con").is_err());
        assert!(validate_package_name("prn").is_err());
        assert!(validate_package_name("aux").is_err());
        assert!(validate_package_name("nul").is_err());
        assert!(validate_package_name("com1").is_err());
        assert!(validate_package_name("lpt9").is_err());
    }

    #[test]
    fn test_validate_config_key_valid() {
        assert!(validate_config_key("output-format").is_ok());
        assert!(validate_config_key("model").is_ok());
        assert!(validate_config_key("timeout").is_ok());
    }

    #[test]
    fn test_validate_config_key_underscore() {
        let err = validate_config_key("output_format")
            .unwrap_err()
            .to_string();
        assert!(err.contains("invalid characters"), "got: {}", err);
    }

    #[test]
    fn test_validate_config_key_uppercase() {
        assert!(validate_config_key("Model").is_err());
    }

    #[test]
    fn test_normalize_version() {
        assert_eq!(normalize_version("v14.1.0"), "14.1.0");
        assert_eq!(normalize_version("V2.0"), "2.0");
        assert_eq!(normalize_version("14.1.0"), "14.1.0");
        assert_eq!(normalize_version("v1.0.0-RC1"), "1.0.0-rc1");
    }

    #[test]
    fn test_current_os() {
        assert!(!current_os().is_empty());
    }

    #[test]
    fn test_current_arch() {
        assert!(!current_arch().is_empty());
    }

    #[test]
    fn test_exe_suffix() {
        let suffix = exe_suffix();
        if cfg!(windows) {
            assert_eq!(suffix, ".exe");
        } else {
            assert_eq!(suffix, "");
        }
    }

    #[test]
    fn test_asset_matching_linux_x86_64() {
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-darwin-amd64.tar.gz".to_string(),
            "tool-windows-amd64.zip".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, Some("tool-linux-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_excludes_wrong_os() {
        let assets = vec![
            "tool-darwin-amd64.tar.gz".to_string(),
            "tool-windows-amd64.zip".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, None);
    }

    #[test]
    fn test_asset_matching_prefers_tar_gz_on_linux() {
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-linux-amd64.deb".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, Some("tool-linux-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_prefers_zip_on_windows() {
        let assets = vec![
            "tool-windows-amd64.tar.gz".to_string(),
            "tool-windows-amd64.zip".to_string(),
        ];
        let result = select_asset_for(&assets, "windows", "x86_64", None);
        assert_eq!(result, Some("tool-windows-amd64.zip".to_string()));
    }

    #[test]
    fn test_asset_matching_arm64() {
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-linux-arm64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "aarch64", None);
        assert_eq!(result, Some("tool-linux-arm64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_macos_arm_fallback() {
        let assets = vec![
            "tool-darwin-amd64.tar.gz".to_string(),
            "tool-linux-amd64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "macos", "aarch64", None);
        assert_eq!(result, Some("tool-darwin-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_deterministic() {
        let assets = vec![
            "tool-b-linux-amd64.tar.gz".to_string(),
            "tool-a-linux-amd64.tar.gz".to_string(),
        ];
        let r1 = select_asset_for(&assets, "linux", "x86_64", None);
        let r2 = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_asset_matching_skips_checksums() {
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-linux-amd64.tar.gz.sha256".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, Some("tool-linux-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_freebsd() {
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-freebsd-amd64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "freebsd", "x86_64", None);
        assert_eq!(result, Some("tool-freebsd-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_unknown_os() {
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-amd64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "haiku", "x86_64", None);
        assert_eq!(result, Some("tool-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_i686_excluded_on_x86_64() {
        let assets = vec![
            "tool-linux-i686.tar.gz".to_string(),
            "tool-linux-amd64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, Some("tool-linux-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_asset_matching_riscv64() {
        let assets = vec![
            "tool-linux-riscv64.tar.gz".to_string(),
            "tool-linux-amd64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "riscv64gc", None);
        assert_eq!(result, Some("tool-linux-riscv64.tar.gz".to_string()));
    }

    #[test]
    fn test_platform_specific_zip_beats_agnostic_tar_gz() {
        // tool.tar.gz: format=10, os=0, arch=0 → 10
        // tool-linux-amd64.zip: format=5, os=+20, arch=+10 → 35
        let assets = vec![
            "tool.tar.gz".to_string(),
            "tool-linux-amd64.zip".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, Some("tool-linux-amd64.zip".to_string()));
    }

    #[test]
    fn test_both_platform_specific_format_decides() {
        // Both have linux+amd64 bonuses (+30), tar.gz format (10) beats zip (5)
        let assets = vec![
            "tool-linux-amd64.tar.gz".to_string(),
            "tool-linux-amd64.zip".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        assert_eq!(result, Some("tool-linux-amd64.tar.gz".to_string()));
    }

    #[test]
    fn test_box64_not_excluded_on_aarch64() {
        // "box64" contains "x64" as a substring, but "x64" is not at a word boundary
        let assets = vec!["box64-linux-aarch64.tar.gz".to_string()];
        let result = select_asset_for(&assets, "linux", "aarch64", None);
        assert!(result.is_some(), "box64 should not be excluded on aarch64");
        assert_eq!(result, Some("box64-linux-aarch64.tar.gz".to_string()));
    }

    #[test]
    fn test_x64_at_word_boundary_matches() {
        // "x64" after a delimiter should match for x86_64
        let assets = vec![
            "tool-x64-windows.zip".to_string(),
            "tool-arm64-windows.zip".to_string(),
        ];
        let result = select_asset_for(&assets, "windows", "x86_64", None);
        assert_eq!(result, Some("tool-x64-windows.zip".to_string()));
    }

    #[test]
    fn test_package_hint_filters_monorepo_assets() {
        let assets = vec![
            "echo-linux-x86_64.tar.gz".to_string(),
            "head-linux-x86_64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", Some("echo"));
        assert_eq!(result, Some("echo-linux-x86_64.tar.gz".to_string()));
        let result = select_asset_for(&assets, "linux", "x86_64", Some("head"));
        assert_eq!(result, Some("head-linux-x86_64.tar.gz".to_string()));
    }

    #[test]
    fn test_package_hint_none_preserves_behavior() {
        let assets = vec![
            "echo-linux-x86_64.tar.gz".to_string(),
            "head-linux-x86_64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", None);
        // Without hint, both are candidates; first alphabetically wins at same score
        assert!(result.is_some());
    }

    #[test]
    fn test_package_hint_no_match_returns_none() {
        let assets = vec![
            "echo-linux-x86_64.tar.gz".to_string(),
            "head-linux-x86_64.tar.gz".to_string(),
        ];
        let result = select_asset_for(&assets, "linux", "x86_64", Some("nonexistent"));
        assert_eq!(result, None);
    }
}
