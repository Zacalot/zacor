pub mod github;
pub mod gitrepo;
pub mod local;

use crate::config;
use crate::error::*;
use crate::package_definition::{self, PackageDefinition};
use crate::receipt::SourceRecord;
use crate::registry;
use std::path::{Path, PathBuf};

/// Parsed source for package installation.
#[derive(Debug, Clone)]
pub enum Source {
    LocalArchive {
        path: PathBuf,
    },
    LocalDefinition {
        path: PathBuf,
    },
    LocalProject {
        project_root: PathBuf,
        definition: PathBuf,
    },
    /// A wasm package — a single `.wasm` file with the manifest embedded
    /// as a `zacor_manifest` custom section.
    LocalWasm {
        path: PathBuf,
    },
    GitHub {
        owner: String,
        repo: String,
        version: Option<String>,
    },
    GitRepo {
        url: String,
        tag: Option<String>,
        path: Option<String>,
    },
    Registry {
        name: String,
        version: Option<String>,
        registry: Option<String>,
    },
}

/// Result of acquiring a package from a source.
#[derive(Debug)]
pub struct AcquireResult {
    /// Content directory containing package.yaml and other package files.
    /// Owned TempDir — cleaned up when AcquireResult is dropped.
    pub content_dir: tempfile::TempDir,
    /// Parsed package definition.
    pub definition: PackageDefinition,
    /// Package name (may differ from definition.name via --name override).
    pub name: String,
    /// Source provenance for the receipt.
    pub source_info: SourceRecord,
}

/// Known archive extensions.
const ARCHIVE_EXTENSIONS: &[&str] = &[".tar.gz", ".tgz", ".zip"];

/// Known definition extensions.
const DEFINITION_EXTENSIONS: &[&str] = &[".yaml", ".yml"];

/// Known wasm extensions.
const WASM_EXTENSIONS: &[&str] = &[".wasm"];

type SourceRecognizer = fn(&str) -> Result<Option<Source>>;

/// Parse a source string into a typed Source variant.
pub fn parse(input: &str) -> Result<Source> {
    let recognizers: [SourceRecognizer; 7] = [
        try_github,
        try_directory,
        try_wasm,
        try_archive,
        try_definition,
        try_git_url,
        try_bare_name,
    ];

    for recognizer in &recognizers {
        if let Some(source) = recognizer(input)? {
            return Ok(source);
        }
    }

    bail!(
        "unrecognized source: '{}'\nhint: supported formats are github.com/owner/repo, .wasm files, .tar.gz/.tgz/.zip archives, .yaml/.yml definitions, git URLs (.git), directory with package.yaml, or a bare package name",
        input
    );
}

fn try_wasm(input: &str) -> Result<Option<Source>> {
    let lower = input.to_lowercase();
    for ext in WASM_EXTENSIONS {
        if lower.ends_with(ext) {
            return Ok(Some(Source::LocalWasm {
                path: Path::new(input).to_path_buf(),
            }));
        }
    }
    Ok(None)
}

fn try_github(input: &str) -> Result<Option<Source>> {
    let rest = input
        .strip_prefix("github.com/")
        .or_else(|| input.strip_prefix("https://github.com/"))
        .or_else(|| input.strip_prefix("http://github.com/"));

    let rest = match rest {
        Some(r) => r.trim_end_matches('/'),
        None => return Ok(None),
    };

    // Don't match if this is a .git URL (handled by try_git_url)
    if rest.ends_with(".git") {
        return Ok(None);
    }

    let (path_part, version) = if let Some(at_pos) = rest.rfind('@') {
        let (p, v) = rest.split_at(at_pos);
        (p, Some(v[1..].to_string()))
    } else {
        (rest, None)
    };

    let parts: Vec<&str> = path_part.splitn(2, '/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        bail!(
            "invalid GitHub source: expected github.com/owner/repo, got '{}'",
            input
        );
    }

    Ok(Some(Source::GitHub {
        owner: parts[0].to_string(),
        repo: parts[1].to_string(),
        version,
    }))
}

fn try_directory(input: &str) -> Result<Option<Source>> {
    let input_path = Path::new(input);
    if !input_path.is_dir() {
        return Ok(None);
    }

    let def_path = input_path.join("package.yaml");
    if def_path.exists() {
        return Ok(Some(Source::LocalProject {
            project_root: input_path.to_path_buf(),
            definition: def_path,
        }));
    }

    bail!(
        "directory '{}' does not contain a package.yaml\nhint: add a package.yaml to install from a project directory",
        input
    );
}

fn try_archive(input: &str) -> Result<Option<Source>> {
    let lower = input.to_lowercase();
    for ext in ARCHIVE_EXTENSIONS {
        if lower.ends_with(ext) {
            return Ok(Some(Source::LocalArchive {
                path: Path::new(input).to_path_buf(),
            }));
        }
    }
    Ok(None)
}

fn try_definition(input: &str) -> Result<Option<Source>> {
    let lower = input.to_lowercase();
    let input_path = Path::new(input);

    for ext in DEFINITION_EXTENSIONS {
        if lower.ends_with(ext) {
            // Check if the yaml has a build section — if so, treat as project build
            if let Ok(def) = package_definition::parse_file(input_path)
                && def.build.is_some()
            {
                let project_root = resolve_project_root(input_path);
                return Ok(Some(Source::LocalProject {
                    project_root,
                    definition: input_path.to_path_buf(),
                }));
            }
            return Ok(Some(Source::LocalDefinition {
                path: input_path.to_path_buf(),
            }));
        }
    }
    Ok(None)
}

/// Recognize git repository URLs: `.git` suffix or `git://` protocol.
fn try_git_url(input: &str) -> Result<Option<Source>> {
    let is_git = input.ends_with(".git") || input.starts_with("git://");
    if !is_git {
        return Ok(None);
    }

    Ok(Some(Source::GitRepo {
        url: input.to_string(),
        tag: None,
        path: None,
    }))
}

/// Recognize bare package names with optional @version for registry lookup.
/// A bare name has no path separators, no file extensions, and is not a URL.
fn try_bare_name(input: &str) -> Result<Option<Source>> {
    // Must not contain path separators
    if input.contains('/') || input.contains('\\') {
        return Ok(None);
    }
    // Must not look like a file (no common extensions)
    let lower = input.to_lowercase();
    for ext in ARCHIVE_EXTENSIONS
        .iter()
        .chain(DEFINITION_EXTENSIONS.iter())
        .chain(WASM_EXTENSIONS.iter())
    {
        if lower.ends_with(ext) {
            return Ok(None);
        }
    }
    // Must not look like a URL
    if input.contains("://") || input.starts_with("git://") {
        return Ok(None);
    }
    // Must not start with . (relative path)
    if input.starts_with('.') {
        return Ok(None);
    }

    // Split on @ for version
    let (name, version) = if let Some(at_pos) = input.find('@') {
        let name = &input[..at_pos];
        let ver = &input[at_pos + 1..];
        if name.is_empty() || ver.is_empty() {
            return Ok(None);
        }
        (name.to_string(), Some(ver.to_string()))
    } else {
        (input.to_string(), None)
    };

    Ok(Some(Source::Registry {
        name,
        version,
        registry: None,
    }))
}

/// Resolve the project root from a package.yaml path.
/// If the yaml is inside a `package-yaml/` directory, the project root is its grandparent.
/// Otherwise, the project root is the yaml's parent directory.
fn resolve_project_root(yaml_path: &Path) -> PathBuf {
    let parent = yaml_path.parent().unwrap_or(Path::new("."));
    if parent.file_name().and_then(|n| n.to_str()) == Some("package-yaml") {
        parent.parent().unwrap_or(Path::new(".")).to_path_buf()
    } else {
        parent.to_path_buf()
    }
}

/// Acquire a package from the parsed source.
pub fn acquire(
    source: &Source,
    name_override: Option<&str>,
    home: &Path,
    from_source: bool,
) -> Result<AcquireResult> {
    let result = match source {
        Source::LocalDefinition { path } => local::install_definition(path)?,
        Source::LocalArchive { path } => local::install_archive(path, home)?,
        Source::LocalWasm { path } => local::install_wasm(path)?,
        Source::LocalProject {
            project_root,
            definition,
        } => local::install_project(project_root, definition)?,
        Source::GitHub {
            owner,
            repo,
            version,
        } => github::install(owner, repo, version.as_deref(), home)?,
        Source::GitRepo { url, tag, path } => {
            gitrepo::install(url, tag.as_deref(), path.as_deref(), home)?
        }
        Source::Registry {
            name,
            version,
            registry: registry_name,
        } => acquire_from_registry(
            home,
            name,
            version.as_deref(),
            registry_name.as_deref(),
            from_source,
        )?,
    };

    // Use name_override if provided, otherwise validate the name from package.yaml
    let final_name = if let Some(override_name) = name_override {
        crate::platform::validate_package_name(override_name).with_context(|| {
            format!(
                "invalid package name '{}' — --name value must be a valid package name",
                override_name
            )
        })?;
        override_name.to_string()
    } else {
        result.name.clone()
    };

    Ok(AcquireResult {
        name: final_name,
        ..result
    })
}

/// Resolve a package from the registry and acquire it via the appropriate source.
fn acquire_from_registry(
    home: &Path,
    name: &str,
    version: Option<&str>,
    registry_name: Option<&str>,
    from_source: bool,
) -> Result<AcquireResult> {
    let mut global_config = config::read_global(home)?;

    // Seed default registry if none configured
    if global_config.registries.is_empty() {
        registry::seed_default_if_empty(home, &mut global_config)?;
    }

    let entry = registry::resolve(home, name, version, registry_name, &global_config)?;
    eprintln!(
        "resolved {} v{} from registry '{}'",
        entry.name, entry.version, entry.registry_name
    );

    let tag = entry.tag.clone();

    // Determine acquisition method: release (GitHub) or repo (git clone)
    let use_repo = from_source || entry.release.is_none();

    let mut result = if !use_repo {
        if let Some(ref release) = entry.release {
            // Parse owner/repo from release field
            let parts: Vec<&str> = release.splitn(2, '/').collect();
            if parts.len() != 2 {
                bail!("invalid release field in registry: '{}'", release);
            }
            let package_hint = entry.path.as_deref();
            // For releases, default to v{version} tag if none specified
            let release_tag = tag.clone().unwrap_or_else(|| format!("v{}", entry.version));
            github::install_with_hint(parts[0], parts[1], Some(&release_tag), home, package_hint)?
        } else {
            bail!(
                "registry entry for '{}' has no release or repo source",
                name
            );
        }
    } else if let Some(ref repo) = entry.repo {
        gitrepo::install(repo, tag.as_deref(), entry.path.as_deref(), home)?
    } else {
        bail!(
            "registry entry for '{}' has no repo source for --from-source",
            name
        );
    };

    // Override source info to record registry provenance
    result.source_info = SourceRecord::Registry {
        registry: entry.registry_name.clone(),
        package: entry.name.clone(),
        version: entry.version.clone(),
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_archive_tar_gz() {
        match parse("./tool.tar.gz").unwrap() {
            Source::LocalArchive { path } => assert_eq!(path, Path::new("./tool.tar.gz")),
            _ => panic!("expected LocalArchive"),
        }
    }

    #[test]
    fn test_parse_local_archive_zip() {
        match parse("./tool.zip").unwrap() {
            Source::LocalArchive { path } => assert_eq!(path, Path::new("./tool.zip")),
            _ => panic!("expected LocalArchive"),
        }
    }

    #[test]
    fn test_parse_local_definition() {
        match parse("./wrapper.yaml").unwrap() {
            Source::LocalDefinition { path } => assert_eq!(path, Path::new("./wrapper.yaml")),
            _ => panic!("expected LocalDefinition"),
        }
        match parse("./wrapper.yml").unwrap() {
            Source::LocalDefinition { path } => assert_eq!(path, Path::new("./wrapper.yml")),
            _ => panic!("expected LocalDefinition"),
        }
    }

    #[test]
    fn test_parse_unrecognized_extension() {
        let result = parse("./tool");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("unrecognized"), "got: {}", err);
    }

    #[test]
    fn test_parse_github() {
        match parse("github.com/user/repo").unwrap() {
            Source::GitHub {
                owner,
                repo,
                version,
            } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert_eq!(version, None);
            }
            _ => panic!("expected GitHub"),
        }
    }

    #[test]
    fn test_parse_github_with_version() {
        match parse("github.com/user/repo@v1.2.3").unwrap() {
            Source::GitHub {
                owner,
                repo,
                version,
            } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert_eq!(version, Some("v1.2.3".to_string()));
            }
            _ => panic!("expected GitHub"),
        }
    }

    #[test]
    fn test_parse_directory_with_package_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("package.yaml"),
            "name: test\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        )
        .unwrap();
        let input = tmp.path().to_string_lossy().to_string();
        match parse(&input).unwrap() {
            Source::LocalProject {
                project_root,
                definition,
            } => {
                assert_eq!(project_root, tmp.path());
                assert_eq!(definition, tmp.path().join("package.yaml"));
            }
            other => panic!("expected LocalProject, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_directory_without_package_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().to_string_lossy().to_string();
        let result = parse(&input);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("package.yaml"), "got: {}", err);
    }

    #[test]
    fn test_try_github_returns_none_for_non_github() {
        let result = try_github("./tool.tar.gz").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_try_github_parses_owner_repo() {
        match try_github("github.com/owner/repo").unwrap() {
            Some(Source::GitHub {
                owner,
                repo,
                version,
            }) => {
                assert_eq!(owner, "owner");
                assert_eq!(repo, "repo");
                assert_eq!(version, None);
            }
            other => panic!("expected Some(GitHub), got {:?}", other),
        }
    }

    #[test]
    fn test_try_archive_returns_none_for_non_archive() {
        let result = try_archive("./tool").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_try_archive_recognizes_tar_gz() {
        match try_archive("./tool.tar.gz").unwrap() {
            Some(Source::LocalArchive { path }) => assert_eq!(path, Path::new("./tool.tar.gz")),
            other => panic!("expected Some(LocalArchive), got {:?}", other),
        }
    }

    #[test]
    fn test_try_definition_returns_none_for_non_yaml() {
        let result = try_definition("./tool").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_yaml_with_build_section() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml_path = tmp.path().join("package.yaml");
        std::fs::write(
            &yaml_path,
            "name: test\nversion: \"1.0.0\"\nbinary: test\nbuild:\n  command: \"make\"\n  output: build\ncommands:\n  default:\n    description: test\n",
        ).unwrap();
        let input = yaml_path.to_string_lossy().to_string();
        match parse(&input).unwrap() {
            Source::LocalProject {
                project_root,
                definition,
            } => {
                assert_eq!(project_root, tmp.path());
                assert_eq!(definition, yaml_path);
            }
            other => panic!("expected LocalProject, got {:?}", other),
        }
    }

    // --- Git URL recognizer tests ---

    #[test]
    fn test_try_git_url_with_dot_git_suffix() {
        match try_git_url("https://github.com/someone/tool.git").unwrap() {
            Some(Source::GitRepo { url, tag, path }) => {
                assert_eq!(url, "https://github.com/someone/tool.git");
                assert_eq!(tag, None);
                assert_eq!(path, None);
            }
            other => panic!("expected Some(GitRepo), got {:?}", other),
        }
    }

    #[test]
    fn test_try_git_url_with_git_protocol() {
        match try_git_url("git://github.com/someone/tool").unwrap() {
            Some(Source::GitRepo { url, .. }) => {
                assert_eq!(url, "git://github.com/someone/tool");
            }
            other => panic!("expected Some(GitRepo), got {:?}", other),
        }
    }

    #[test]
    fn test_try_git_url_returns_none_for_non_git() {
        assert!(try_git_url("./tool.tar.gz").unwrap().is_none());
        assert!(try_git_url("echo").unwrap().is_none());
        assert!(try_git_url("github.com/owner/repo").unwrap().is_none());
    }

    #[test]
    fn test_parse_git_url() {
        match parse("https://github.com/someone/tool.git").unwrap() {
            Source::GitRepo { url, .. } => {
                assert_eq!(url, "https://github.com/someone/tool.git");
            }
            other => panic!("expected GitRepo, got {:?}", other),
        }
    }

    // --- Bare name recognizer tests ---

    #[test]
    fn test_try_bare_name_plain() {
        match try_bare_name("echo").unwrap() {
            Some(Source::Registry {
                name,
                version,
                registry,
            }) => {
                assert_eq!(name, "echo");
                assert_eq!(version, None);
                assert_eq!(registry, None);
            }
            other => panic!("expected Some(Registry), got {:?}", other),
        }
    }

    #[test]
    fn test_try_bare_name_with_version() {
        match try_bare_name("echo@1.0.5").unwrap() {
            Some(Source::Registry {
                name,
                version,
                registry,
            }) => {
                assert_eq!(name, "echo");
                assert_eq!(version, Some("1.0.5".to_string()));
                assert_eq!(registry, None);
            }
            other => panic!("expected Some(Registry), got {:?}", other),
        }
    }

    #[test]
    fn test_try_bare_name_returns_none_for_paths() {
        assert!(try_bare_name("./tool").unwrap().is_none());
        assert!(try_bare_name("path/to/pkg").unwrap().is_none());
        assert!(try_bare_name(".hidden").unwrap().is_none());
    }

    #[test]
    fn test_try_bare_name_returns_none_for_archives() {
        assert!(try_bare_name("tool.tar.gz").unwrap().is_none());
        assert!(try_bare_name("tool.zip").unwrap().is_none());
    }

    #[test]
    fn test_try_bare_name_returns_none_for_urls() {
        assert!(try_bare_name("git://host/repo").unwrap().is_none());
    }

    #[test]
    fn test_try_bare_name_invalid_at() {
        // Empty name or version around @
        assert!(try_bare_name("@1.0.0").unwrap().is_none());
        assert!(try_bare_name("echo@").unwrap().is_none());
    }

    #[test]
    fn test_parse_bare_name() {
        match parse("echo").unwrap() {
            Source::Registry { name, version, .. } => {
                assert_eq!(name, "echo");
                assert_eq!(version, None);
            }
            other => panic!("expected Registry, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_bare_name_with_version() {
        match parse("echo@1.0.5").unwrap() {
            Source::Registry { name, version, .. } => {
                assert_eq!(name, "echo");
                assert_eq!(version, Some("1.0.5".to_string()));
            }
            other => panic!("expected Registry, got {:?}", other),
        }
    }
}
