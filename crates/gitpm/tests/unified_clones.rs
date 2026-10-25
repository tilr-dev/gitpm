//! A root package and later nested siblings share one source-keyed workspace clone.
mod common;
use gitpm::{InstallTarget, Manager, scaffold};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct TempDir(PathBuf);
impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "gitpm-unified-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn root_to_nested_lifecycle_keeps_one_clone_and_all_owners() {
    let dir = TempDir::new();
    let source = common::author(
        &dir.path().join("source"),
        "365ade82-57cd-53a7-a61f-c790d46cba87",
        "root",
    );
    let root = dir.path().join("workspace");
    let manager = Manager::open(&root).unwrap();
    assert!(!root.join("packages").exists());
    manager.install(source.to_str().unwrap()).unwrap();
    let initial = manager.state().unwrap().packages["365ade82-57cd-53a7-a61f-c790d46cba87"].clone();
    let key = initial
        .clone
        .clone()
        .expect("root package records its clone key");
    assert!(key.starts_with("source-"));
    assert!(initial.path.is_none());
    let clone = root.join("repos").join(&key);
    assert!(clone.join("objects").is_dir());
    assert!(!clone.join(".git").exists());
    assert_eq!(git(&clone, &["rev-parse", "--is-bare-repository"]), "true");
    assert!(
        root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87/.git")
            .is_file()
    );

    fs::create_dir_all(source.join("apps/root")).unwrap();
    fs::rename(
        source.join("plugin.json"),
        source.join("apps/root/plugin.json"),
    )
    .unwrap();
    fs::rename(source.join("dist"), source.join("apps/root/dist")).unwrap();
    fs::create_dir_all(source.join("apps/sibling/dist")).unwrap();
    fs::write(
        source.join("apps/sibling/plugin.json"),
        r#"{"id":"33832cb1-5888-5a42-83e7-843d3b4b7224","name": "fixture", "version":"0.2.0"}"#,
    )
    .unwrap();
    fs::write(source.join("apps/sibling/dist/index.js"), "sibling").unwrap();
    fs::write(source.join("gitpm.json"), r#"{"packages":["apps/*"]}"#).unwrap();
    scaffold::commit_package(
        &source,
        "0.2.0",
        Some("365ade82-57cd-53a7-a61f-c790d46cba87"),
    )
    .unwrap();

    // Refused mutations leave both state and deployed output unchanged.
    let before = fs::read(root.join("state.json")).unwrap();
    fs::write(
        root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87/local.txt"),
        "local work",
    )
    .unwrap();
    assert_eq!(
        manager
            .update("365ade82-57cd-53a7-a61f-c790d46cba87")
            .unwrap_err()
            .kind(),
        "local_changes"
    );
    assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
    assert!(
        root.join("dist/365ade82-57cd-53a7-a61f-c790d46cba87/index.js")
            .is_file()
    );
    fs::remove_file(root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87/local.txt")).unwrap();

    manager
        .update("365ade82-57cd-53a7-a61f-c790d46cba87")
        .unwrap();
    manager
        .install_package(
            source.to_str().unwrap(),
            Some("33832cb1-5888-5a42-83e7-843d3b4b7224"),
            InstallTarget::Release,
        )
        .unwrap();
    let state = manager.state().unwrap();
    assert_eq!(
        state.packages["365ade82-57cd-53a7-a61f-c790d46cba87"]
            .clone
            .as_ref(),
        Some(&key)
    );
    assert_eq!(
        state.packages["365ade82-57cd-53a7-a61f-c790d46cba87"]
            .path
            .as_deref(),
        Some("apps/root")
    );
    assert_eq!(
        state.packages["33832cb1-5888-5a42-83e7-843d3b4b7224"]
            .clone
            .as_ref(),
        Some(&key)
    );
    assert_eq!(fs::read_dir(root.join("repos")).unwrap().count(), 1);
    let sibling_head = git(
        &root.join("worktrees/33832cb1-5888-5a42-83e7-843d3b4b7224"),
        &["rev-parse", "HEAD"],
    );
    let sibling_manifest = fs::read(
        root.join("worktrees/33832cb1-5888-5a42-83e7-843d3b4b7224/apps/sibling/plugin.json"),
    )
    .unwrap();
    manager
        .rollback("365ade82-57cd-53a7-a61f-c790d46cba87")
        .unwrap();
    assert_eq!(
        git(
            &root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87"),
            &["rev-parse", "HEAD"]
        ),
        initial.current.commit
    );
    assert_eq!(
        git(
            &root.join("worktrees/33832cb1-5888-5a42-83e7-843d3b4b7224"),
            &["rev-parse", "HEAD"]
        ),
        sibling_head
    );
    assert_eq!(
        fs::read(
            root.join("worktrees/33832cb1-5888-5a42-83e7-843d3b4b7224/apps/sibling/plugin.json")
        )
        .unwrap(),
        sibling_manifest
    );
    assert!(
        manager.state().unwrap().packages["365ade82-57cd-53a7-a61f-c790d46cba87"]
            .path
            .is_none()
    );
    assert_eq!(
        fs::read_to_string(root.join("dist/33832cb1-5888-5a42-83e7-843d3b4b7224/index.js"))
            .unwrap(),
        "sibling"
    );
    manager
        .update("365ade82-57cd-53a7-a61f-c790d46cba87")
        .unwrap();
    manager
        .forget("365ade82-57cd-53a7-a61f-c790d46cba87")
        .unwrap();
    manager
        .uninstall("33832cb1-5888-5a42-83e7-843d3b4b7224", &[], false)
        .unwrap();
    assert!(
        clone.join("objects").is_dir(),
        "forgotten owner retains the clone"
    );
    manager
        .restore("365ade82-57cd-53a7-a61f-c790d46cba87")
        .unwrap();
    assert!(
        root.join("dist/365ade82-57cd-53a7-a61f-c790d46cba87/index.js")
            .is_file()
    );
    manager
        .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
        .unwrap();
    assert!(!clone.exists());
    assert!(!root.join("repos").exists());
    assert!(!root.join("packages").exists());
}

#[test]
fn legacy_clones_are_left_untouched_and_never_used() {
    let dir = TempDir::new();
    let root = dir.path().join("workspace");
    drop(Manager::open(&root).unwrap());
    let legacy = root.join("packages/365ade82-57cd-53a7-a61f-c790d46cba87");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("local.txt"), "keep").unwrap();
    let source = common::author(
        &dir.path().join("source"),
        "365ade82-57cd-53a7-a61f-c790d46cba87",
        "root",
    );
    let manager = Manager::open(&root).unwrap();
    manager.install(source.to_str().unwrap()).unwrap();
    manager
        .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
        .unwrap();
    assert_eq!(
        fs::read_to_string(legacy.join("local.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn root_and_nested_worktrees_guard_dirty_files_and_every_moved_head() {
    for nested in [false, true] {
        let dir = TempDir::new();
        let source = common::author(
            &dir.path().join("source"),
            "365ade82-57cd-53a7-a61f-c790d46cba87",
            "root",
        );
        if nested {
            fs::create_dir_all(source.join("apps/root")).unwrap();
            fs::rename(
                source.join("plugin.json"),
                source.join("apps/root/plugin.json"),
            )
            .unwrap();
            fs::rename(source.join("dist"), source.join("apps/root/dist")).unwrap();
            fs::write(source.join("gitpm.json"), r#"{"packages":["apps/*"]}"#).unwrap();
            scaffold::commit_package(
                &source,
                "0.2.0",
                Some("365ade82-57cd-53a7-a61f-c790d46cba87"),
            )
            .unwrap();
        }
        let root = dir.path().join("workspace");
        let manager = Manager::open(&root).unwrap();
        manager.install(source.to_str().unwrap()).unwrap();
        scaffold::commit_package(
            &source,
            "0.3.0",
            Some("365ade82-57cd-53a7-a61f-c790d46cba87"),
        )
        .unwrap();
        manager
            .update("365ade82-57cd-53a7-a61f-c790d46cba87")
            .unwrap();
        let record =
            manager.state().unwrap().packages["365ade82-57cd-53a7-a61f-c790d46cba87"].clone();
        let checkout = root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87");
        let payload = checkout.join(if nested {
            "apps/root/dist/index.js"
        } else {
            "dist/index.js"
        });
        let original = fs::read(&payload).unwrap();
        let state = fs::read(root.join("state.json")).unwrap();
        let deployed =
            fs::read(root.join("dist/365ade82-57cd-53a7-a61f-c790d46cba87/index.js")).unwrap();
        for staged in [false, true] {
            fs::write(&payload, "local edits").unwrap();
            if staged {
                git(&checkout, &["add", "--all"]);
            }
            for result in [
                manager
                    .update("365ade82-57cd-53a7-a61f-c790d46cba87")
                    .map(|_| ()),
                manager
                    .rollback("365ade82-57cd-53a7-a61f-c790d46cba87")
                    .map(|_| ()),
                manager
                    .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
                    .map(|_| ()),
            ] {
                assert_eq!(result.unwrap_err().kind(), "local_changes");
            }
            assert_eq!(fs::read(&payload).unwrap(), b"local edits");
            assert_eq!(fs::read(root.join("state.json")).unwrap(), state);
            assert_eq!(
                fs::read(root.join("dist/365ade82-57cd-53a7-a61f-c790d46cba87/index.js")).unwrap(),
                deployed
            );
            git(&checkout, &["reset", "--hard", &record.current.commit]);
        }
        fs::write(checkout.join("untracked.txt"), "keep").unwrap();
        for result in [
            manager
                .update("365ade82-57cd-53a7-a61f-c790d46cba87")
                .map(|_| ()),
            manager
                .rollback("365ade82-57cd-53a7-a61f-c790d46cba87")
                .map(|_| ()),
            manager
                .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
                .map(|_| ()),
        ] {
            assert_eq!(result.unwrap_err().kind(), "local_changes");
        }
        fs::remove_file(checkout.join("untracked.txt")).unwrap();
        assert_eq!(fs::read(&payload).unwrap(), original);
        // A published previous revision still differs from this package's current HEAD.
        git(
            &checkout,
            &["checkout", "--detach", &record.previous.unwrap().commit],
        );
        for result in [
            manager
                .update("365ade82-57cd-53a7-a61f-c790d46cba87")
                .map(|_| ()),
            manager
                .rollback("365ade82-57cd-53a7-a61f-c790d46cba87")
                .map(|_| ()),
            manager
                .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
                .map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), "local_changes");
            assert!(
                matches!(error, gitpm::Error::LocalChanges { items, .. } if items.iter().any(|item| matches!(item, gitpm::LostWork::HeadMoved { .. })))
            );
        }
        manager
            .forget("365ade82-57cd-53a7-a61f-c790d46cba87")
            .unwrap();
        assert_eq!(
            manager
                .restore("365ade82-57cd-53a7-a61f-c790d46cba87")
                .unwrap_err()
                .kind(),
            "local_changes"
        );
        git(&checkout, &["checkout", "--detach", &record.current.commit]);
        manager
            .restore("365ade82-57cd-53a7-a61f-c790d46cba87")
            .unwrap();
        manager
            .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
            .unwrap();
        assert!(!checkout.exists());
    }
}

#[test]
fn missing_bare_preserves_orphan_worktree_on_removal() {
    for forget in [false, true] {
        let dir = TempDir::new();
        let source = common::author(
            &dir.path().join("source"),
            "365ade82-57cd-53a7-a61f-c790d46cba87",
            "root",
        );
        let root = dir.path().join("workspace");
        let manager = Manager::open(&root).unwrap();
        manager.install(source.to_str().unwrap()).unwrap();
        let record =
            manager.state().unwrap().packages["365ade82-57cd-53a7-a61f-c790d46cba87"].clone();
        let checkout = root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87");
        fs::write(checkout.join("scratch.txt"), "keep my work").unwrap();
        fs::remove_dir_all(root.join("repos").join(record.clone.unwrap())).unwrap();
        if forget {
            manager
                .forget("365ade82-57cd-53a7-a61f-c790d46cba87")
                .unwrap();
        } else {
            manager
                .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
                .unwrap();
        }
        assert_eq!(
            fs::read(checkout.join("scratch.txt")).unwrap(),
            b"keep my work"
        );
        assert!(
            !manager
                .state()
                .unwrap()
                .packages
                .contains_key("365ade82-57cd-53a7-a61f-c790d46cba87")
        );
    }
}

#[test]
fn reinstall_preserves_orphan_worktree_after_missing_bare() {
    let dir = TempDir::new();
    let source = common::author(
        &dir.path().join("source"),
        "365ade82-57cd-53a7-a61f-c790d46cba87",
        "root",
    );
    let root = dir.path().join("workspace");
    let manager = Manager::open(&root).unwrap();
    manager.install(source.to_str().unwrap()).unwrap();
    let record = manager.state().unwrap().packages["365ade82-57cd-53a7-a61f-c790d46cba87"].clone();
    let checkout = root.join("worktrees/365ade82-57cd-53a7-a61f-c790d46cba87");
    fs::write(checkout.join("scratch.txt"), "keep my work").unwrap();
    fs::remove_dir_all(root.join("repos").join(record.clone.unwrap())).unwrap();
    manager
        .uninstall("365ade82-57cd-53a7-a61f-c790d46cba87", &[], false)
        .unwrap();
    let state = fs::read(root.join("state.json")).unwrap();
    let error = manager.install(source.to_str().unwrap()).unwrap_err();
    assert_eq!(error.kind(), "workspace");
    assert!(
        error
            .to_string()
            .contains("exists but is not a package worktree")
    );
    assert_eq!(
        fs::read(checkout.join("scratch.txt")).unwrap(),
        b"keep my work"
    );
    assert_eq!(fs::read(root.join("state.json")).unwrap(), state);
}
