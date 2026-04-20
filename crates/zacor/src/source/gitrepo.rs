use crate::error::*;
use crate::paths;
use crate::receipt::SourceRecord;
use crate::source::AcquireResult;
use crate::source::local;
use std::path::Path;
use std::process::Command;

/// Install a package from a git repository.
/// Clones (or fetches if cached) the repo, optionally checks out a tag,
/// then delegates to `install_project` for the actual package preparation.
pub fn install(
    url: &str,
    tag: Option<&str>,
    path: Option<&str>,
    home: &Path,
) -> Result<AcquireResult> {
    ensure_git_available()?;

    let cache_path = paths::repo_cache_path(home, url);

    let clone_url = normalize_clone_url(url);

    if cache_path.exists() {
        // Fetch the specific tag/ref if requested, otherwise just fetch
        if let Some(tag) = tag {
            eprintln!("fetching {} from cached repo...", tag);
            let output = Command::new("git")
                .args([
                    "fetch",
                    "--depth",
                    "1",
                    "origin",
                    &format!("refs/tags/{}", tag),
                ])
                .current_dir(&cache_path)
                .output()
                .context("failed to run git fetch")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("warning: git fetch tag failed: {}", stderr.trim());
            }
        } else {
            eprintln!("updating cached repo...");
            let output = Command::new("git")
                .args(["fetch", "--depth", "1"])
                .current_dir(&cache_path)
                .output()
                .context("failed to run git fetch")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                eprintln!("warning: git fetch failed: {}", stderr.trim());
            }
        }
    } else {
        // Clone fresh — use --branch if a tag is specified to get it in a shallow clone
        eprintln!("cloning {}...", url);
        let mut args = vec!["clone", "--depth", "1"];
        if let Some(tag) = tag {
            args.push("--branch");
            args.push(tag);
        }
        args.push(&clone_url);
        let output = Command::new("git")
            .args(&args)
            .arg(&cache_path)
            .output()
            .context("failed to run git clone")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git clone failed: {}", stderr.trim());
        }
    }

    // Checkout tag if specified (needed for cached repos after fetch)
    if tag.is_some() && cache_path.exists() {
        let tag = tag.unwrap();
        let output = Command::new("git")
            .args(["checkout", tag])
            .current_dir(&cache_path)
            .output()
            .context("failed to run git checkout")?;
        if !output.status.success() {
            // Try FETCH_HEAD as fallback (from the fetch above)
            let output2 = Command::new("git")
                .args(["checkout", "FETCH_HEAD"])
                .current_dir(&cache_path)
                .output()
                .context("failed to run git checkout FETCH_HEAD")?;
            if !output2.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                bail!("git checkout {} failed: {}", tag, stderr.trim());
            }
        }
    }

    // Determine the project directory
    let project_dir = match path {
        Some(p) => cache_path.join(p),
        None => cache_path.clone(),
    };

    let def_path = project_dir.join("package.yaml");
    if !def_path.exists() {
        let location = match path {
            Some(p) => format!("{}/{}/package.yaml", url, p),
            None => format!("{}/package.yaml", url),
        };
        bail!(
            "package.yaml not found at {}\nhint: check the repository path or use --name to specify a subdirectory",
            location
        );
    }

    // Delegate to install_project (handles both build and no-build cases)
    let mut result = local::install_project(&project_dir, &def_path)?;

    // For direct git URL installs, record as Local source with the git URL
    result.source_info = SourceRecord::Local {
        path: url.to_string(),
    };

    Ok(result)
}

/// Check that git is available on PATH.
fn ensure_git_available() -> Result<()> {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        _ => bail!(
            "git is required but not found on PATH\nhint: install git from https://git-scm.com/"
        ),
    }
}

/// Normalize a URL for git clone — add https:// if no protocol.
fn normalize_clone_url(url: &str) -> String {
    if url.contains("://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    }
}

/// Generate a URL slug for cache path (public for testing).
#[cfg(test)]
pub fn url_to_slug(url: &str) -> String {
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("git://"))
        .unwrap_or(url)
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .replace('/', "--")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_slug_generation() {
        assert_eq!(
            url_to_slug("github.com/zacor-packages/p-zr-core"),
            "github.com--zacor-packages--p-zr-core"
        );
        assert_eq!(
            url_to_slug("https://github.com/someone/tool.git"),
            "github.com--someone--tool"
        );
        assert_eq!(
            url_to_slug("git://github.com/someone/tool"),
            "github.com--someone--tool"
        );
    }

    #[test]
    fn test_normalize_clone_url() {
        assert_eq!(
            normalize_clone_url("github.com/owner/repo"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            normalize_clone_url("https://github.com/owner/repo"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            normalize_clone_url("git://github.com/owner/repo"),
            "git://github.com/owner/repo"
        );
    }

    #[test]
    fn test_git_available() {
        // This test verifies the check doesn't panic; git may or may not be installed
        let _ = ensure_git_available();
    }
}
