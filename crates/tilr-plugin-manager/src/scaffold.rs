//! Author operations: create an independent plugin repository, commit releases, set remotes.
//! Never pushes and never touches a workspace.
use crate::catalog::version_key;
use crate::error::{Context, Error, Result, ensure};
use crate::{Manifest, exec::git, identity, model};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn validate_name(name: &str) -> Result<()> {
    identity::valid_part(name)
}

/// Never overwrites an existing path. `parent` is the author directory, not a workspace.
pub fn create(parent: &Path, name: &str) -> Result<PathBuf> {
    create_with_id(parent, name, name)
}
fn create_with_id(parent: &Path, name: &str, id: &str) -> Result<PathBuf> {
    validate_name(name)?;
    fs::create_dir_all(parent)?;
    let directory = parent.join(name);
    fs::create_dir(&directory).context(
        Error::Workspace,
        format!(
            "cannot create {}; choose a new name if the path already exists",
            directory.display()
        ),
    )?;
    let result = (|| {
        let tag = identity::release_tag(id, "0.1.0");
        let next_tag = identity::release_tag(id, "0.1.1");
        let source = shell_quote(&fs::canonicalize(&directory)?.to_string_lossy());
        let m = Manifest {
            id: id.to_owned(),
            version: "0.1.0".into(),
            dependencies: Default::default(),
            fork_of: None,
        };
        fs::write(directory.join("plugin.json"), model::manifest_json(&m)?)?;
        fs::create_dir(directory.join("dist"))?;
        fs::write(
            directory.join("dist/index.js"),
            format!(
                "// Ready-to-copy demo artifact. No build step required.\nexport const greeting = \"Hello from {name} 0.1.0\";\n"
            ),
        )?;
        fs::write(directory.join(".gitignore"), ".DS_Store\nnode_modules/\n")?;
        fs::write(
            directory.join("README.md"),
            format!(
                "# {name}\n\nLocal plugin-manager demo, version 0.1.0.\n\nEdit `dist/index.js` directly; there is no build step. `plugin.json` holds the\nplugin ID and version. This is a package fixture, not a Tilr runtime plugin.\n\nFrom the plugin-manager directory:\n\n```sh\n./pm install {source} --ref {tag}\n# After editing this repository:\n./pm commit {source} 0.1.1\n./pm update {id} --ref {next_tag}\n./pm rollback {id}\n```\n\nTo generate a static catalog: add the repository URL/path to sources.txt, then run:\n\n```sh\n./pm catalog-update\n```\n\nNo remote is configured. If Git identity was missing, the scaffold configured\nPlugin Demo <demo@example.invalid> locally in this repository only.\n"
            ),
        )?;
        git(&directory, &["init", "-b", "main"])?;
        ensure_identity(&directory)?;
        git(&directory, &["add", "--all", "--", "."])?;
        git(
            &directory,
            &["commit", "-m", &format!("Create {name} 0.1.0")],
        )?;
        git(&directory, &["tag", &identity::release_tag(id, "0.1.0")])?;
        Ok(fs::canonicalize(&directory)?)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&directory);
    }
    result
}

/// Shell-safe paths in printed next-step commands and the generated README.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn split_target(path: &Path) -> Result<(&Path, &str)> {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| Error::InvalidArgument("path must end with a plugin name".into()))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok((parent, name))
}

/// Create a legacy (unscoped) plugin at PATH.
pub fn create_at(path: &Path) -> Result<PathBuf> {
    let (parent, name) = split_target(path)?;
    create(parent, name)
}

/// Create `namespace/<last path component>` at PATH.
pub fn create_namespaced(path: &Path, namespace: &str) -> Result<PathBuf> {
    let (parent, name) = split_target(path)?;
    create_with_id(parent, name, &identity::qualified(namespace, name)?)
}

pub(crate) fn ensure_identity(directory: &Path) -> Result<()> {
    // Keep authoring usable on a fresh machine without changing global Git settings.
    if git(directory, &["config", "user.name"]).is_err() {
        git(
            directory,
            &["config", "--local", "user.name", "Plugin Demo"],
        )?;
    }
    if git(directory, &["config", "user.email"]).is_err() {
        git(
            directory,
            &["config", "--local", "user.email", "demo@example.invalid"],
        )?;
    }
    Ok(())
}

/// Commit plugin changes locally and create an immutable version tag. Never pushes.
pub fn commit(repo: &Path, version: &str) -> Result<String> {
    version_key(version)?;
    let repo = fs::canonicalize(repo)?;
    ensure!(
        Path::new(&git(&repo, &["rev-parse", "--show-toplevel"])?) == repo,
        InvalidArgument,
        "pass the repository root"
    );
    let mut m = model::manifest(&repo)?;
    ensure!(
        m.version != version,
        VersionConflict,
        "choose a new version"
    );
    let tag = identity::release_tag(&m.id, version);
    ensure!(
        git(&repo, &["tag", "--list", &tag])?.is_empty(),
        VersionConflict,
        "tag already exists: {tag}"
    );
    git(&repo, &["var", "GIT_AUTHOR_IDENT"])?;
    git(&repo, &["var", "GIT_COMMITTER_IDENT"])?;
    m.version = version.to_owned();
    fs::write(repo.join("plugin.json"), model::manifest_json(&m)?)?;
    git(&repo, &["add", "--all", "--", "."])?;
    git(
        &repo,
        &["commit", "-m", &format!("Release {} {version}", m.id)],
    )
    .context(
        Error::Git,
        "commit failed; your edits and staged files remain in the repository",
    )?;
    git(&repo, &["tag", &tag]).context(
        Error::Git,
        "commit succeeded but tag creation failed; inspect the repository before retrying",
    )?;
    Ok(tag)
}

/// Set the author's origin for fetch and push; no network operations.
pub fn set_remote(repo: &Path, url: &str) -> Result<()> {
    ensure!(
        !url.trim().is_empty() && !url.starts_with('-') && !url.chars().any(char::is_control),
        InvalidArgument,
        "invalid git remote URL"
    );
    let repo = fs::canonicalize(repo)?;
    let top = fs::canonicalize(git(&repo, &["rev-parse", "--show-toplevel"])?)?;
    ensure!(
        top == repo,
        InvalidArgument,
        "pass the plugin repository root"
    );
    ensure!(
        repo.join("plugin.json").is_file(),
        InvalidManifest,
        "plugin.json not found in repository root"
    );
    let remotes = git(&repo, &["remote"])?;
    if remotes.lines().any(|name| name == "origin") {
        // Replace all configured URLs so an old secondary destination cannot remain.
        git(
            &repo,
            &[
                "config",
                "--local",
                "--replace-all",
                "remote.origin.url",
                url,
            ],
        )?;
        if git(&repo, &["config", "--get-all", "remote.origin.pushurl"]).is_ok() {
            git(
                &repo,
                &[
                    "config",
                    "--local",
                    "--replace-all",
                    "remote.origin.pushurl",
                    url,
                ],
            )?;
        }
    } else {
        git(&repo, &["remote", "add", "origin", url])?;
    }
    Ok(())
}
