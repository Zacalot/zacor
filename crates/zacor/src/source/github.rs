use crate::error::*;
use crate::platform;
use crate::receipt::SourceRecord;
use crate::source::AcquireResult;
use crate::source::local;
use sha2::{Digest, Sha256};
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::Path;

/// Resolve and install a package from a GitHub release.
pub fn install(
    owner: &str,
    repo: &str,
    version: Option<&str>,
    home: &Path,
) -> Result<AcquireResult> {
    install_with_hint(owner, repo, version, home, None)
}

/// Install from GitHub with optional package name hint for monorepo asset selection.
pub fn install_with_hint(
    owner: &str,
    repo: &str,
    version: Option<&str>,
    home: &Path,
    package_hint: Option<&str>,
) -> Result<AcquireResult> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("zacor")
        .build()
        .context("failed to create HTTP client")?;

    let token = env::var("GH_TOKEN")
        .or_else(|_| env::var("GITHUB_TOKEN"))
        .ok();

    let release = resolve_release(&client, owner, repo, version, token.as_deref())?;

    let raw_tag = release["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("release missing tag_name"))?
        .to_string();

    let normalized_version = platform::normalize_version(&raw_tag);

    let assets = release["assets"]
        .as_array()
        .ok_or_else(|| anyhow!("release has no assets"))?;

    if assets.is_empty() {
        bail!("no assets found for {}/{} release {}", owner, repo, raw_tag);
    }

    let asset_names: Vec<String> = assets
        .iter()
        .filter_map(|a| a["name"].as_str().map(|s| s.to_string()))
        .collect();

    let selected_name = platform::select_asset(&asset_names, package_hint).ok_or_else(|| {
        anyhow!(
            "no matching asset for {} {}\navailable assets:\n{}",
            platform::current_os(),
            platform::current_arch(),
            asset_names
                .iter()
                .map(|n| format!("  - {}", n))
                .collect::<Vec<_>>()
                .join("\n")
        )
    })?;

    let asset = assets
        .iter()
        .find(|a| a["name"].as_str() == Some(&selected_name))
        .ok_or_else(|| anyhow!("selected asset not found"))?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| anyhow!("asset missing browser_download_url"))?;

    eprintln!("downloading {}...", selected_name);

    let cache_dir = crate::paths::cache_dir(home);
    fs::create_dir_all(&cache_dir).context("failed to create cache dir")?;
    let download_path = cache_dir.join(&selected_name);

    download_file(&client, download_url, &download_path, token.as_deref())?;

    // Check for checksum sidecar
    let checksum_names = [
        format!("{}.sha256", selected_name),
        format!("{}.sha256sum", selected_name),
    ];
    for cksum_name in &checksum_names {
        if asset_names.contains(&cksum_name.to_string()) {
            let cksum_asset = assets
                .iter()
                .find(|a| a["name"].as_str() == Some(cksum_name.as_str()));
            if let Some(cksum_asset) = cksum_asset {
                let cksum_url = cksum_asset["browser_download_url"]
                    .as_str()
                    .ok_or_else(|| anyhow!("checksum asset missing URL"))?;
                let cksum_path = cache_dir.join(cksum_name);
                download_file(&client, cksum_url, &cksum_path, token.as_deref())?;
                verify_checksum(&download_path, &cksum_path)?;
                let _ = fs::remove_file(&cksum_path);
            }
            break;
        }
    }

    // Extract archive — package.yaml is required
    let result = local::install_archive(&download_path, home)?;

    // Cleanup download
    let _ = fs::remove_file(&download_path);

    // Warn if package.yaml version diverges from tag
    if result.definition.version != normalized_version {
        eprintln!(
            "warning: package.yaml version '{}' differs from tag '{}'",
            result.definition.version, raw_tag
        );
    }

    Ok(AcquireResult {
        source_info: SourceRecord::Github {
            owner: owner.to_string(),
            repo: repo.to_string(),
            tag: raw_tag,
            asset: selected_name,
        },
        ..result
    })
}

fn resolve_release(
    client: &reqwest::blocking::Client,
    owner: &str,
    repo: &str,
    version: Option<&str>,
    token: Option<&str>,
) -> Result<serde_json::Value> {
    let url = match version {
        Some(tag) => format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            owner, repo, tag
        ),
        None => format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            owner, repo
        ),
    };

    let mut req = client
        .get(&url)
        .header("Accept", "application/vnd.github+json");

    if let Some(token) = token {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let resp = req.send().context("failed to query GitHub API")?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return match version {
            Some(tag) => Err(anyhow!("release {} not found for {}/{}", tag, owner, repo)),
            None => Err(anyhow!("no releases found for {}/{}", owner, repo)),
        };
    }

    if !resp.status().is_success() {
        bail!(
            "GitHub API error: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    resp.json::<serde_json::Value>()
        .context("failed to parse GitHub API response")
}

fn download_file(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    token: Option<&str>,
) -> Result<()> {
    let mut req = client.get(url);
    if let Some(token) = token {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let mut resp = req.send().context("download failed")?;

    if !resp.status().is_success() {
        bail!("download failed: HTTP {}", resp.status());
    }

    let mut file = File::create(dest).context("failed to create download file")?;
    if let Err(e) = std::io::copy(&mut resp, &mut file) {
        let _ = fs::remove_file(dest);
        return Err(anyhow!(e).context("failed to download file"));
    }

    Ok(())
}

fn verify_checksum(file_path: &Path, checksum_path: &Path) -> Result<()> {
    let checksum_content =
        fs::read_to_string(checksum_path).context("failed to read checksum file")?;

    let expected_hash = checksum_content
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("empty checksum file"))?
        .to_lowercase();

    let mut file = File::open(file_path).context("failed to open file for checksum")?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).context("failed to read file for checksum")?;
    let actual_hash = format!("{:x}", hasher.finalize());

    if actual_hash != expected_hash {
        bail!(
            "checksum mismatch!\n  expected: {}\n  actual:   {}",
            expected_hash,
            actual_hash
        );
    }

    Ok(())
}
