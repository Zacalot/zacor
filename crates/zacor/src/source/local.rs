use crate::error::*;
use crate::package_definition;
use crate::platform;
use crate::receipt::SourceRecord;
use crate::source::AcquireResult;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Install a definition-only package from a .yaml/.yml file.
pub fn install_definition(path: &Path) -> Result<AcquireResult> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let def = package_definition::parse_file(path)
        .with_context(|| format!("failed to parse definition at {}", path.display()))?;

    let content_dir = tempfile::tempdir().context("failed to create content dir")?;
    fs::copy(path, content_dir.path().join("package.yaml")).context("failed to copy definition")?;

    let name = def.name.clone();
    Ok(AcquireResult {
        content_dir,
        definition: def,
        name,
        source_info: SourceRecord::Local {
            path: path.to_string_lossy().into_owned(),
        },
    })
}

/// Install a wasm package from a bare `.wasm` file. The manifest is
/// embedded in the wasm's `zacor_manifest` custom section and is also
/// written to a sidecar `package.yaml` in the content directory so the
/// installed store entry stays inspectable and consistent. The wasm file
/// is copied into the content directory under its original filename,
/// which is what `store::put` and later `wasm_manifest::find_wasm_in_store`
/// key off of.
pub fn install_wasm(path: &Path) -> Result<AcquireResult> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let def = crate::wasm_manifest::read_manifest(path).with_context(|| {
        format!(
            "failed to read embedded manifest from wasm artifact {}",
            path.display()
        )
    })?;

    // Validate the declared `wasm:` field matches this file's name
    // (or — if absent — is OK; we tolerate minor author mistakes).
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("wasm file has no filename: {}", path.display()))?;
    if let Some(ref declared) = def.wasm
        && declared != file_name
    {
        eprintln!(
            "warning: wasm file name '{}' doesn't match manifest's 'wasm: {}' — using file name",
            file_name, declared
        );
    }

    let content_dir = tempfile::tempdir().context("failed to create content dir")?;
    let dest_wasm = content_dir.path().join(file_name);
    fs::copy(path, &dest_wasm).context("failed to copy wasm artifact")?;

    if let Some(bytes) = crate::wasm_manifest::read_manifest_bytes(path)? {
        fs::write(content_dir.path().join("package.yaml"), &bytes)
            .context("failed to write embedded package manifest")?;
    }

    // Best-effort AOT precompile: write `.cwasm` sibling so dispatch
    // can `Module::deserialize_file` instead of cranelift-compiling the
    // wasm on first invocation. Failures are non-fatal — dispatch will
    // JIT on first use and write the cwasm itself.
    if let Ok(host) = crate::wasm_runtime::WasmHost::shared()
        && let Err(e) = host.precompile(&dest_wasm)
    {
        eprintln!(
            "warning: failed to precompile {}: {} — dispatch will JIT on first use",
            dest_wasm.display(),
            e
        );
    }

    let name = def.name.clone();
    Ok(AcquireResult {
        content_dir,
        definition: def,
        name,
        source_info: SourceRecord::Local {
            path: path.to_string_lossy().into_owned(),
        },
    })
}

/// Locate the most recently-built artifact (wasm vs native) given the
/// sidecar's native `build.output` path. Derives the wasm-target sibling
/// by swapping `target/release` → `target/wasm32-wasip1/release`, which
/// covers the convention used by `zacor-package-build::workspace_target_rel_path`.
/// Returns `Some((path, is_wasm))` with the newer artifact, or `None` if
/// neither has been built yet.
fn detect_built_artifact(
    project_root: &Path,
    native_build_output: &str,
    binary_name: &str,
) -> Option<(PathBuf, bool)> {
    let native_dir = project_root.join(native_build_output);
    let wasm_dir_rel = native_build_output.replace("target/release", "target/wasm32-wasip1/release");
    let wasm_dir = project_root.join(&wasm_dir_rel);

    let wasm = wasm_dir.join(format!("{binary_name}.wasm"));
    let native = native_dir.join(format!("{}{}", binary_name, platform::exe_suffix()));

    let wasm_mtime = wasm.metadata().and_then(|m| m.modified()).ok();
    let native_mtime = native.metadata().and_then(|m| m.modified()).ok();

    match (wasm_mtime, native_mtime) {
        (Some(w), Some(n)) => {
            if w >= n {
                Some((wasm, true))
            } else {
                Some((native, false))
            }
        }
        (Some(_), None) => Some((wasm, true)),
        (None, Some(_)) => Some((native, false)),
        (None, None) => None,
    }
}

/// Install a wasm artifact into a content dir, using the wasm's embedded
/// manifest as the authoritative definition. Shared between `install_wasm`
/// (bare `.wasm`) and `install_project` (directory install that detected
/// a wasm build).
fn install_wasm_artifact(
    wasm_path: &Path,
    source_display: String,
) -> Result<AcquireResult> {
    let def = crate::wasm_manifest::read_manifest(wasm_path).with_context(|| {
        format!(
            "failed to read embedded manifest from wasm artifact {}",
            wasm_path.display()
        )
    })?;

    let file_name = wasm_path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("wasm file has no filename: {}", wasm_path.display()))?;

    let content_dir = tempfile::tempdir().context("failed to create content dir")?;
    let dest_wasm = content_dir.path().join(file_name);
    fs::copy(wasm_path, &dest_wasm).context("failed to copy wasm artifact")?;
    if let Some(bytes) = crate::wasm_manifest::read_manifest_bytes(wasm_path)? {
        fs::write(content_dir.path().join("package.yaml"), &bytes)
            .context("failed to write embedded package manifest")?;
    }

    if let Ok(host) = crate::wasm_runtime::WasmHost::shared()
        && let Err(e) = host.precompile(&dest_wasm)
    {
        eprintln!(
            "warning: failed to precompile {}: {} — dispatch will JIT on first use",
            dest_wasm.display(),
            e
        );
    }

    eprintln!("installed {} v{} (wasm)", def.name, def.version);

    let name = def.name.clone();
    Ok(AcquireResult {
        content_dir,
        definition: def,
        name,
        source_info: SourceRecord::Local {
            path: source_display,
        },
    })
}

/// Install from a local project directory. The sidecar `package.yaml` is
/// always the native view (stable across targets). This function detects
/// which build artifact is newest (wasm vs native) and installs that —
/// giving users a predictable "install whatever I just built" workflow.
pub fn install_project(project_root: &Path, definition_path: &Path) -> Result<AcquireResult> {
    let def = package_definition::parse_file(definition_path)
        .with_context(|| format!("failed to parse {}", definition_path.display()))?;

    let content_dir = tempfile::tempdir().context("failed to create content dir")?;

    // Authored-wasm packages (sidecar explicitly declares `wasm:`) still
    // work the legacy way — copy the declared wasm from build.output.
    if let Some(ref wasm_name) = def.wasm {
        let build_output = def
            .build
            .as_ref()
            .and_then(|b| b.output.as_ref())
            .map(|o| project_root.join(o))
            .unwrap_or_else(|| {
                project_root.join("target").join("wasm32-wasip1").join("release")
            });

        let src_wasm = build_output.join(wasm_name);
        if !src_wasm.exists() {
            bail!(
                "wasm artifact '{}' not found at {}\nhint: build with `cargo build --target wasm32-wasip1 --release` before installing",
                wasm_name,
                src_wasm.display()
            );
        }
        return install_wasm_artifact(&src_wasm, project_root.to_string_lossy().into_owned());
    }

    if let Some(ref build) = def.build {
        // Autodetect: if the user has a wasm build that's newer than the
        // native binary (or only has a wasm build), install wasm. Otherwise
        // fall through to the native build+install flow.
        let binary_name = def.binary.as_ref().ok_or_else(|| {
            anyhow!("package.yaml has a build section but no 'binary' field\nhint: add binary: <name> to specify the output binary name")
        })?;
        let native_build_output = build.output.as_deref().unwrap_or("target/release");
        if let Some((artifact, is_wasm)) =
            detect_built_artifact(project_root, native_build_output, binary_name)
            && is_wasm
        {
            return install_wasm_artifact(
                &artifact,
                project_root.to_string_lossy().into_owned(),
            );
        }
        // Has build section — run build and copy binary
        let build_command = build
            .command
            .as_ref()
            .ok_or_else(|| anyhow!("build section missing 'command' field"))?;

        let build_output = build
            .output
            .as_ref()
            .ok_or_else(|| anyhow!("build section missing 'output' field"))?;

        eprintln!("building {} v{}...", def.name, def.version);
        let status = if cfg!(windows) {
            Command::new("cmd")
                .args(["/C", build_command])
                .current_dir(project_root)
                .status()
        } else {
            Command::new("sh")
                .args(["-c", build_command])
                .current_dir(project_root)
                .status()
        }
        .with_context(|| format!("failed to execute build command: {}", build_command))?;

        if !status.success() {
            bail!(
                "build failed with exit code {}\ncommand: {}",
                status.code().unwrap_or(-1),
                build_command
            );
        }

        let bin_filename = format!("{}{}", binary_name, platform::exe_suffix());
        let bin_path = project_root.join(build_output).join(&bin_filename);

        if !bin_path.exists() {
            bail!(
                "binary '{}' not found at {}\nhint: check that build.output points to the directory containing the built binary",
                bin_filename,
                bin_path.display()
            );
        }

        fs::copy(definition_path, content_dir.path().join("package.yaml"))
            .context("failed to copy package.yaml")?;
        fs::copy(&bin_path, content_dir.path().join(&bin_filename))
            .context("failed to copy binary")?;

        eprintln!("built {} v{}", def.name, def.version);
    } else if def.run.is_some() && def.binary.is_none() {
        // run package without build — copy all project files
        copy_project_files(project_root, content_dir.path())
            .context("failed to copy project files for run package")?;
    } else {
        // No build section — definition-only (copy package.yaml + binary script if declared)
        fs::copy(definition_path, content_dir.path().join("package.yaml"))
            .context("failed to copy package.yaml")?;

        if let Some(ref binary_name) = def.binary {
            let src_file = project_root.join(binary_name);
            if src_file.exists() {
                fs::copy(&src_file, content_dir.path().join(binary_name))
                    .context("failed to copy binary file")?;
            }
        }
    }

    let name = def.name.clone();
    Ok(AcquireResult {
        content_dir,
        definition: def,
        name,
        source_info: SourceRecord::Local {
            path: project_root.to_string_lossy().into_owned(),
        },
    })
}

/// Install from a local archive file. Archives MUST contain a package.yaml.
pub fn install_archive(path: &Path, home: &Path) -> Result<AcquireResult> {
    if !path.exists() {
        bail!("file not found: {}", path.display());
    }

    let extract_dir = tempfile::tempdir_in(crate::paths::cache_dir(home))
        .context("failed to create extract dir")?;

    let lower = path.to_string_lossy().to_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        extract_tar_gz(path, extract_dir.path())?;
    } else if lower.ends_with(".zip") {
        extract_zip(path, extract_dir.path())?;
    } else {
        bail!("unsupported archive format: {}", path.display());
    }

    // Find package.yaml in extracted contents
    let definition_found = find_package_yaml(extract_dir.path())?
        .ok_or_else(|| anyhow!(
            "archive does not contain a package.yaml\nhint: all packages must include a package.yaml definition"
        ))?;

    let def = package_definition::parse_file(&definition_found)
        .context("failed to parse package.yaml from archive")?;

    // Prepare content directory with relevant files
    let content_dir = tempfile::tempdir().context("failed to create content dir")?;
    fs::copy(&definition_found, content_dir.path().join("package.yaml"))
        .context("failed to copy package.yaml to content dir")?;

    if let Some(ref binary_name) = def.binary {
        let bin_filename = format!("{}{}", binary_name, platform::exe_suffix());
        let found = find_file_recursive(extract_dir.path(), &bin_filename)?;
        if let Some(bin) = found {
            fs::copy(&bin, content_dir.path().join(&bin_filename))
                .context("failed to copy binary to content dir")?;
        } else {
            bail!(
                "package.yaml declares binary '{}' but '{}' was not found in archive",
                binary_name,
                bin_filename
            );
        }
    }

    let name = def.name.clone();
    Ok(AcquireResult {
        content_dir,
        definition: def,
        name,
        source_info: SourceRecord::Local {
            path: path.to_string_lossy().into_owned(),
        },
    })
}

/// Recursively copy all files from a project directory to the destination.
fn copy_project_files(src: &Path, dst: &Path) -> Result<()> {
    for entry in fs::read_dir(src).context("failed to read project directory")? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&dest)?;
            copy_project_files(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest).context("failed to copy project file")?;
        }
    }
    Ok(())
}

/// Search for package.yaml recursively in an extracted directory.
fn find_package_yaml(dir: &Path) -> Result<Option<PathBuf>> {
    find_file_recursive(dir, "package.yaml")
}

/// Search for a file by name recursively.
fn find_file_recursive(dir: &Path, filename: &str) -> Result<Option<PathBuf>> {
    // Check root first
    let root = dir.join(filename);
    if root.is_file() {
        return Ok(Some(root));
    }
    // Search one level of subdirectories (common archive structure: name-version/package.yaml)
    let entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read directory: {}", dir.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let candidate = path.join(filename);
            if candidate.is_file() {
                return Ok(Some(candidate));
            }
        }
    }
    Ok(None)
}

pub fn extract_tar_gz(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive_path).context("failed to open archive")?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(gz);
    archive.unpack(dest).context("failed to extract tar.gz")?;
    Ok(())
}

pub fn extract_zip(archive_path: &Path, dest: &Path) -> Result<()> {
    let file = File::open(archive_path).context("failed to open archive")?;
    let mut archive = zip::ZipArchive::new(file).context("failed to read zip")?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("failed to read zip entry")?;
        let out_path = dest.join(
            entry
                .enclosed_name()
                .ok_or_else(|| anyhow!("invalid zip entry name"))?,
        );

        if entry.is_dir() {
            fs::create_dir_all(&out_path).context("failed to create dir")?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).context("failed to create dir")?;
            }
            let mut out_file = File::create(&out_path).context("failed to create file")?;
            io::copy(&mut entry, &mut out_file).context("failed to extract file")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn echo_wasm_path() -> Option<std::path::PathBuf> {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent()?.parent()?;
        let path = workspace_root
            .join("target")
            .join("wasm32-wasip1")
            .join("release")
            .join("echo.wasm");
        path.exists().then_some(path)
    }

    #[test]
    fn test_install_definition_not_found() {
        let result = install_definition(Path::new("/nonexistent/package.yaml"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_install_definition_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml_path = tmp.path().join("package.yaml");
        fs::write(
            &yaml_path,
            "name: my-tool\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: test\n",
        )
        .unwrap();

        let result = install_definition(&yaml_path).unwrap();
        assert_eq!(result.name, "my-tool");
        assert_eq!(result.definition.version, "1.0.0");
        assert!(result.content_dir.path().join("package.yaml").exists());
    }

    #[test]
    fn test_install_project_no_build_definition_only() {
        let project = tempfile::tempdir().unwrap();
        let yaml =
            "name: wrapper\nversion: \"1.0.0\"\ncommands:\n  default:\n    description: wrap\n";
        let def_path = project.path().join("package.yaml");
        fs::write(&def_path, yaml).unwrap();

        let result = install_project(project.path(), &def_path).unwrap();
        assert_eq!(result.name, "wrapper");
        assert!(result.content_dir.path().join("package.yaml").exists());
        // No binary in content dir
        assert_eq!(
            fs::read_dir(result.content_dir.path()).unwrap().count(),
            1,
            "content dir should only have package.yaml"
        );
    }

    #[test]
    fn test_install_project_no_build_copies_script() {
        let project = tempfile::tempdir().unwrap();
        let yaml = "name: my-script\nversion: \"1.0.0\"\nbinary: run.sh\ncommands:\n  default:\n    description: run\n";
        let def_path = project.path().join("package.yaml");
        fs::write(&def_path, yaml).unwrap();
        fs::write(project.path().join("run.sh"), "#!/bin/sh\necho hi").unwrap();

        let result = install_project(project.path(), &def_path).unwrap();
        assert_eq!(result.name, "my-script");
        assert!(result.content_dir.path().join("package.yaml").exists());
        assert!(result.content_dir.path().join("run.sh").exists());
    }

    #[test]
    fn test_install_project_run_copies_all_files() {
        let project = tempfile::tempdir().unwrap();
        let yaml = "name: py-tool\nversion: \"1.0.0\"\nrun: \"python3 main.py\"\ncommands:\n  default:\n    description: run\n";
        let def_path = project.path().join("package.yaml");
        fs::write(&def_path, yaml).unwrap();
        fs::write(project.path().join("main.py"), "print('hello')").unwrap();
        let lib_dir = project.path().join("lib");
        fs::create_dir(&lib_dir).unwrap();
        fs::write(lib_dir.join("utils.py"), "# utils").unwrap();

        let result = install_project(project.path(), &def_path).unwrap();
        assert_eq!(result.name, "py-tool");
        let content = result.content_dir.path();
        assert!(content.join("package.yaml").exists());
        assert!(content.join("main.py").exists());
        assert!(content.join("lib").join("utils.py").exists());
    }

    #[test]
    fn test_install_wasm_artifact_writes_sidecar_manifest() {
        let Some(wasm_path) = echo_wasm_path() else {
            eprintln!("skipping: build zr-echo for wasm32-wasip1 first");
            return;
        };

        let result = install_wasm(&wasm_path).unwrap();
        let yaml_path = result.content_dir.path().join("package.yaml");
        assert!(yaml_path.exists());
        let yaml = fs::read_to_string(yaml_path).unwrap();
        assert!(yaml.contains("name: echo"), "got: {}", yaml);
    }
}
