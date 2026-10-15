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
            ..Default::default()
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
        git(
            &directory,
            &["tag", "--", &identity::release_tag(id, "0.1.0")],
        )?;
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
    let tag = identity::release_tag(&m.id, version);
    ensure!(
        git(&repo, &["tag", "--list", "--", &tag])?.is_empty(),
        VersionConflict,
        "tag already exists: {tag}"
    );
    if m.version == version {
        // Retry after a failed tag: the release commit exists, only the tag is missing.
        let head: Manifest = serde_json::from_str(&git(&repo, &["show", "HEAD:plugin.json"])?)
            .map_err(|e| Error::InvalidManifest(format!("invalid plugin.json at HEAD: {e}")))?;
        ensure!(
            head.version == version
                && git(&repo, &["status", "--porcelain"])?.is_empty()
                && git(&repo, &["log", "-1", "--format=%s"])?
                    == format!("Release {} {version}", m.id),
            VersionConflict,
            "choose a new version"
        );
        git(&repo, &["tag", "--", &tag])?;
        return Ok(tag);
    }
    git(&repo, &["var", "GIT_AUTHOR_IDENT"])?;
    git(&repo, &["var", "GIT_COMMITTER_IDENT"])?;
    // A failed commit (hook, signing) must leave the manifest and the index as they were.
    let original = fs::read(repo.join("plugin.json"))?;
    let index = PathBuf::from(git(
        &repo,
        &["rev-parse", "--path-format=absolute", "--git-path", "index"],
    )?);
    let saved_index = fs::read(&index).ok();
    m.version = version.to_owned();
    fs::write(repo.join("plugin.json"), model::manifest_json(&m)?)?;
    let committed = git(&repo, &["add", "--all", "--", "."]).and_then(|_| {
        git(
            &repo,
            &["commit", "-m", &format!("Release {} {version}", m.id)],
        )
    });
    if let Err(error) = committed {
        fs::write(repo.join("plugin.json"), original)?;
        match saved_index {
            Some(bytes) => fs::write(&index, bytes)?,
            None => {
                let _ = fs::remove_file(&index);
            }
        }
        return Err(Error::Git(format!(
            "commit failed; plugin.json and the index were restored, your edits remain: {error}"
        )));
    }
    git(&repo, &["tag", "--", &tag]).context(
        Error::Git,
        "commit succeeded but tag creation failed; run the same commit again to create the tag",
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
        git(&repo, &["remote", "add", "--", "origin", url])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TempDir, git as fixture_git, tree_hash};

    #[test]
    fn shell_quote_survives_single_quotes() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), r#"'it'"'"'s'"#);
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("$(rm -rf /)"), "'$(rm -rf /)'");
    }

    #[test]
    fn create_at_makes_a_tagged_legacy_plugin() {
        let dir = TempDir::new("scaffold");
        let repo = create_at(&dir.join("demo1")).unwrap();
        assert_eq!(repo, dir.join("demo1"));
        let m = model::manifest(&repo).unwrap();
        assert_eq!((m.id.as_str(), m.version.as_str()), ("demo1", "0.1.0"));
        assert_eq!(fixture_git(&repo, &["tag", "--list"]), "v0.1.0");
        assert_eq!(
            fixture_git(&repo, &["status", "--porcelain", "--ignored"]),
            ""
        );
        assert_eq!(fixture_git(&repo, &["branch", "--show-current"]), "main");
        assert_eq!(
            fixture_git(&repo, &["remote"]),
            "",
            "no remote is configured"
        );
        let files = fixture_git(&repo, &["ls-files"]);
        assert_eq!(files, ".gitignore\nREADME.md\ndist/index.js\nplugin.json");
        let readme = fs::read_to_string(repo.join("README.md")).unwrap();
        assert!(readme.contains("--ref v0.1.0"), "{readme}");
        assert!(readme.contains(&shell_quote(repo.to_str().unwrap())));
        assert_eq!(crate::releases::latest(&repo, None).unwrap().name, "v0.1.0");
    }

    #[test]
    fn create_namespaced_uses_the_scoped_id_and_tag() {
        let dir = TempDir::new("scaffold");
        let repo = create_namespaced(&dir.join("nested/demo1"), "dan").unwrap();
        assert_eq!(model::manifest(&repo).unwrap().id, "dan/demo1");
        assert_eq!(fixture_git(&repo, &["tag", "--list"]), "dan/demo1@0.1.0");
        assert!(
            fs::read_to_string(repo.join("README.md"))
                .unwrap()
                .contains("--ref dan/demo1@0.1.1")
        );
    }

    #[test]
    fn create_never_overwrites_and_validates_names() {
        let dir = TempDir::new("scaffold");
        let existing = dir.join("demo1");
        fs::create_dir(&existing).unwrap();
        fs::write(existing.join("keep.txt"), "mine").unwrap();
        let before = tree_hash(dir.path());
        let error = create(dir.path(), "demo1").unwrap_err();
        assert_eq!(error.kind(), "workspace");
        assert_eq!(tree_hash(dir.path()), before);

        for bad in ["..", "a b", "", "x/y"] {
            assert_eq!(
                create(dir.path(), bad).unwrap_err().kind(),
                "invalid_id",
                "{bad:?}"
            );
        }
        assert_eq!(validate_name("ok-name_1").map_err(|e| e.kind()), Ok(()));
        assert_eq!(
            create_namespaced(&dir.join("x"), "bad ns")
                .unwrap_err()
                .kind(),
            "invalid_id"
        );
        assert_eq!(
            create_at(Path::new("/")).unwrap_err().kind(),
            "invalid_argument"
        );
        assert_eq!(
            create_at(&dir.join("..")).unwrap_err().kind(),
            "invalid_argument"
        );
        assert_eq!(
            tree_hash(dir.path()),
            before,
            "failed creates leave nothing behind"
        );
    }

    #[test]
    fn commit_bumps_version_commits_everything_and_tags() {
        let dir = TempDir::new("scaffold");
        let repo = create_namespaced(&dir.join("demo1"), "dan").unwrap();
        fs::write(repo.join("dist/index.js"), "v2").unwrap();
        fs::write(repo.join("dist/new.js"), "new").unwrap();
        let tag = commit(&repo, "0.2.0").unwrap();
        assert_eq!(tag, "dan/demo1@0.2.0");
        assert_eq!(model::manifest(&repo).unwrap().version, "0.2.0");
        assert_eq!(fixture_git(&repo, &["status", "--porcelain"]), "");
        assert_eq!(
            fixture_git(&repo, &["rev-parse", "HEAD"]),
            fixture_git(&repo, &["rev-parse", &tag])
        );
        assert_eq!(
            fixture_git(&repo, &["log", "-1", "--format=%s"]),
            "Release dan/demo1 0.2.0"
        );
        let latest = crate::releases::latest(&repo, None).unwrap();
        assert_eq!(latest.manifest.version, "0.2.0");
        // The manifest stays in canonical form.
        assert_eq!(
            fs::read_to_string(repo.join("plugin.json")).unwrap(),
            "{\n  \"id\": \"dan/demo1\",\n  \"version\": \"0.2.0\",\n  \"dependencies\": {}\n}\n"
        );
    }

    #[test]
    fn commit_refusals_leave_the_repository_untouched() {
        let dir = TempDir::new("scaffold");
        let repo = create_at(&dir.join("demo1")).unwrap();
        fs::write(repo.join("dist/index.js"), "pending edit").unwrap();
        // A tag for the next version already exists (for example created by hand).
        fixture_git(&repo, &["tag", "v0.3.0"]);
        let before = tree_hash(&repo);
        let cases = [
            ("0.1.0", "version_conflict"),
            ("0.3.0", "version_conflict"),
            ("0.2", "invalid_version"),
            ("1.0.0-beta", "invalid_version"),
        ];
        for (version, kind) in cases {
            assert_eq!(
                commit(&repo, version).unwrap_err().kind(),
                kind,
                "{version}"
            );
        }
        assert_eq!(
            commit(&repo.join("dist"), "0.2.0").unwrap_err().kind(),
            "invalid_argument"
        );
        assert_eq!(
            commit(&dir.join("missing"), "0.2.0").unwrap_err().kind(),
            "io"
        );
        assert_eq!(tree_hash(&repo), before);
        assert_eq!(
            fs::read_to_string(repo.join("dist/index.js")).unwrap(),
            "pending edit"
        );
    }

    #[test]
    fn failed_commit_restores_manifest_and_index() {
        let dir = TempDir::new("scaffold");
        let repo = create_at(&dir.join("demo1")).unwrap();
        fs::write(repo.join("dist/index.js"), "edit").unwrap();
        fs::write(repo.join("dist/staged.js"), "staged").unwrap();
        fixture_git(&repo, &["add", "dist/staged.js"]);
        let status = fixture_git(&repo, &["status", "--porcelain"]);
        let hook = repo.join(".git/hooks/pre-commit");
        fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
        fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let error = commit(&repo, "0.2.0").unwrap_err();
        assert_eq!(error.kind(), "git");
        assert!(error.to_string().contains("restored"), "{error}");
        assert_eq!(model::manifest(&repo).unwrap().version, "0.1.0");
        assert_eq!(fixture_git(&repo, &["status", "--porcelain"]), status);
        assert_eq!(fixture_git(&repo, &["tag", "--list"]), "v0.1.0");
    }

    #[test]
    fn commit_can_be_retried_after_a_failed_tag() {
        let dir = TempDir::new("scaffold");
        let repo = create_namespaced(&dir.join("demo1"), "dan").unwrap();
        fs::write(repo.join("dist/index.js"), "v2").unwrap();
        // A stale ref lock makes tag creation fail after the commit.
        let lock = repo.join(".git/refs/tags/dan/demo1@0.2.0.lock");
        fs::create_dir_all(lock.parent().unwrap()).unwrap();
        fs::write(&lock, "").unwrap();
        let error = commit(&repo, "0.2.0").unwrap_err();
        assert!(
            error.to_string().contains("run the same commit again"),
            "{error}"
        );
        let head = fixture_git(&repo, &["rev-parse", "HEAD"]);
        fs::remove_file(&lock).unwrap();
        assert_eq!(commit(&repo, "0.2.0").unwrap(), "dan/demo1@0.2.0");
        assert_eq!(fixture_git(&repo, &["rev-parse", "dan/demo1@0.2.0"]), head);
        // Once tagged, the same version is a conflict again.
        assert_eq!(
            commit(&repo, "0.2.0").unwrap_err().kind(),
            "version_conflict"
        );
        // An edited tree after the release commit is not a retry.
        fs::write(repo.join("dist/index.js"), "v3").unwrap();
        fixture_git(&repo, &["tag", "-d", "dan/demo1@0.2.0"]);
        assert_eq!(
            commit(&repo, "0.2.0").unwrap_err().kind(),
            "version_conflict"
        );
    }

    #[test]
    fn set_remote_adds_or_replaces_origin_without_network() {
        let dir = TempDir::new("scaffold");
        let repo = create_namespaced(&dir.join("demo1"), "dan").unwrap();
        set_remote(&repo, "git@example.com:dan/demo1.git").unwrap();
        assert_eq!(
            fixture_git(&repo, &["remote", "get-url", "origin"]),
            "git@example.com:dan/demo1.git"
        );
        fixture_git(
            &repo,
            &[
                "config",
                "--add",
                "remote.origin.url",
                "https://old.example/second.git",
            ],
        );
        fixture_git(
            &repo,
            &[
                "config",
                "remote.origin.pushurl",
                "https://old.example/push.git",
            ],
        );
        set_remote(&repo, "https://example.com/dan/demo1.git").unwrap();
        assert_eq!(
            fixture_git(&repo, &["config", "--get-all", "remote.origin.url"]),
            "https://example.com/dan/demo1.git"
        );
        assert_eq!(
            fixture_git(&repo, &["config", "--get-all", "remote.origin.pushurl"]),
            "https://example.com/dan/demo1.git"
        );

        let before = tree_hash(&repo);
        for bad in ["", "  ", "-uevil", "https://x\n.git"] {
            assert_eq!(
                set_remote(&repo, bad).unwrap_err().kind(),
                "invalid_argument",
                "{bad:?}"
            );
        }
        assert_eq!(
            set_remote(&repo.join("dist"), "https://x")
                .unwrap_err()
                .kind(),
            "invalid_argument"
        );
        assert_eq!(tree_hash(&repo), before);
        fs::remove_file(repo.join("plugin.json")).unwrap();
        assert_eq!(
            set_remote(&repo, "https://x").unwrap_err().kind(),
            "invalid_manifest"
        );
    }
}
