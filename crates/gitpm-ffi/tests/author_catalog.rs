//! Native hosts can run author and catalog commands without opening a workspace.
use gitpm_ffi::*;
use std::{fs, path::Path, process::Command};

fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn author_commands_preserve_options_results_and_refusals() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("author");
    let created = init(
        path(&repo),
        "first".into(),
        InitOptions {
            multi: true,
            release: false,
        },
    )
    .unwrap();
    assert!(created.new_repository);
    assert_eq!(created.repo, path(&repo.canonicalize().unwrap()));
    assert_eq!(created.version, "0.1.0");
    assert!(created.folder.is_some());
    assert!(created.tag.is_none());
    assert!(git(&repo, &["tag", "--list"]).is_empty());
    let second = init(path(&repo), "second".into(), InitOptions::default()).unwrap();
    assert!(!second.new_repository);
    assert_ne!(created.id, second.id);
    assert!(second.tag.is_none());
    let done = commit(path(&repo), "0.2.0".into(), Some(second.id.clone())).unwrap();
    assert_eq!(done.package, second.id);
    assert_eq!(done.version, "0.2.0");
    assert_eq!(done.tag, "v0.2.0");
    let packages = repository_packages(path(&repo), None).unwrap();
    assert_eq!(packages.len(), 2);
    assert!(packages.iter().all(|p| p.version == "0.2.0"));
    assert!(matches!(
        commit(path(&repo), "bad".into(), None),
        Err(PmError::InvalidVersion { .. })
    ));
    assert!(commit(path(&repo), "0.2.0".into(), Some(second.id)).is_err());
    for url in [
        "https://example.invalid/first.git",
        "https://example.invalid/second.git",
    ] {
        set_remote(path(&repo), url.into()).unwrap();
        assert_eq!(git(&repo, &["remote", "get-url", "origin"]), url);
        assert_eq!(git(&repo, &["remote", "get-url", "--push", "origin"]), url);
    }
    assert!(!repo.join("state.json").exists());
}

#[test]
fn catalog_writers_return_publication_and_discovery_data() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("author");
    let created = init(path(&repo), "theme".into(), InitOptions::default()).unwrap();
    assert_eq!(created.tag.as_deref(), Some("v0.1.0"));
    assert!(created.folder.is_none());
    // Keep arbitrary manifest data in the publication, beyond the typed summary.
    let manifest_path = repo.join("plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["custom"] = serde_json::json!({"nested": [1, true]});
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let done = commit(path(&repo), "0.2.0".into(), None).unwrap();
    let catalog = path(&temp.path().join("catalog"));
    let build = CatalogBuild {
        url: "https://example.invalid/build.tar.gz".into(),
        sha256: "a".repeat(64),
    };
    let registered = catalog_register(
        catalog.clone(),
        path(&repo),
        done.tag,
        Some(created.id.clone()),
        Some(build.clone()),
        false,
    )
    .unwrap();
    assert!(registered.warnings.is_empty());
    assert_eq!(registered.release.id, created.id);
    assert_eq!(registered.release.version, "0.2.0");
    assert_eq!(registered.release.build_url, Some(build.url));
    assert_eq!(registered.release.build_sha256, Some(build.sha256));
    let raw: serde_json::Value = serde_json::from_str(&registered.release_json).unwrap();
    assert_eq!(raw["manifest"]["custom"], manifest["custom"]);
    let files = raw["files"].as_object().unwrap();
    assert!(!files.is_empty());
    for digest in files.values() {
        assert_eq!(digest["sha256"].as_str().unwrap().len(), 64);
        assert!(digest["size"].as_u64().is_some());
    }
    let sources = temp.path().join("sources.txt");
    fs::write(&sources, "# local repositories\nauthor\n").unwrap();
    // A release-looking tag with a mismatched manifest is reported, not installed.
    git(&repo, &["tag", "v0.1.1"]);
    // Move a valid release tag to a changed commit; the published record must win.
    fs::write(repo.join("dist/changed.txt"), "changed output").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "Change published output"]);
    git(&repo, &["tag", "-f", "v0.2.0"]);
    let updated = catalog_update(catalog.clone(), path(&sources), false).unwrap();
    assert_eq!(updated.releases, 2);
    assert_eq!(updated.sources, [path(&repo.canonicalize().unwrap())]);
    assert_eq!(updated.skipped.len(), 1);
    assert_eq!(updated.skipped[0].source, updated.sources[0]);
    assert_eq!(updated.skipped[0].tag.name, "v0.1.1");
    assert!(!updated.skipped[0].tag.reason.is_empty());
    // The moved tag reports a warning while retaining the published record.
    assert!(!updated.warnings.is_empty());
    assert_eq!(
        catalog_release(catalog.clone(), created.id, None)
            .unwrap()
            .version,
        "0.2.0"
    );
    assert!(matches!(
        catalog_register(
            catalog.clone(),
            path(&repo),
            "v0.2.0".into(),
            None,
            Some(CatalogBuild {
                url: "http://example.invalid/build".into(),
                sha256: "a".repeat(64)
            }),
            false
        ),
        Err(PmError::Catalog { .. })
    ));
    fs::write(
        temp.path().join("catalog/catalog.json"),
        r#"{"schema":1,"verified":true,"directories":{}}"#,
    )
    .unwrap();
    assert!(matches!(
        catalog_update(catalog, path(&sources), false),
        Err(PmError::InvalidArgument { .. })
    ));
    assert!(!repo.join("state.json").exists());
}
