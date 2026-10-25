//! A root package and later nested siblings share one source-keyed workspace clone.
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

fn installed_workspace(dir: &TempDir) -> (PathBuf, PathBuf, Manager) {
    let source = scaffold::init(&dir.path().join("source"), "acme", "root")
        .unwrap()
        .repo;
    let root = dir.path().join("workspace");
    let manager = Manager::open(&root).unwrap();
    manager.install(source.to_str().unwrap()).unwrap();
    (source, root, manager)
}

#[test]
fn copied_workspace_repairs_links_and_keeps_source_unchanged() {
    let dir = TempDir::new();
    let (source, root, original) = installed_workspace(&dir);
    let record = original.state().unwrap().packages["acme/root"].clone();
    let checkout = root.join("worktrees/acme/root");
    let original_gitfile = fs::read(checkout.join(".git")).unwrap();
    let original_state = fs::read(root.join("state.json")).unwrap();
    let original_head = git(&checkout, &["rev-parse", "HEAD"]);
    let copied = dir.path().join("copied");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(&root)
            .arg(&copied)
            .status()
            .unwrap()
            .success()
    );
    let manager = Manager::open(&copied).unwrap();
    assert_eq!(manager.list().unwrap().len(), 1);
    let copied_checkout = copied.join("worktrees/acme/root");
    assert_eq!(
        fs::canonicalize(git(&copied_checkout, &["rev-parse", "--git-common-dir"])).unwrap(),
        copied.join("repos").join(record.clone.as_ref().unwrap())
    );
    assert_eq!(
        git(&copied_checkout, &["rev-parse", "--show-toplevel"]),
        copied_checkout.to_str().unwrap()
    );
    assert_eq!(git(&copied_checkout, &["status", "--porcelain"]), "");
    assert_eq!(
        git(&copied_checkout, &["rev-parse", "HEAD"]),
        record.current.commit
    );
    scaffold::commit_package(&source, "0.2.0", None).unwrap();
    manager.update("acme/root").unwrap();
    assert_eq!(git(&copied_checkout, &["status", "--porcelain"]), "");
    manager.rollback("acme/root").unwrap();
    assert_eq!(git(&copied_checkout, &["status", "--porcelain"]), "");
    assert_eq!(git(&copied_checkout, &["rev-parse", "HEAD"]), original_head);
    manager.uninstall("acme/root", &[], false).unwrap();
    assert!(manager.list().unwrap().is_empty());
    assert!(!copied_checkout.exists());
    assert_eq!(fs::read(root.join("state.json")).unwrap(), original_state);
    assert_eq!(fs::read(checkout.join(".git")).unwrap(), original_gitfile);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(git(&checkout, &["status", "--porcelain"]), "");
}

#[test]
fn moved_workspace_repairs_forgotten_worktrees_without_losing_local_work() {
    let dir = TempDir::new();
    let (_source, root, manager) = installed_workspace(&dir);
    manager.forget("acme/root").unwrap();
    fs::write(root.join("worktrees/acme/root/local.txt"), "keep me").unwrap();
    drop(manager);
    let moved = dir.path().join("moved");
    fs::rename(&root, &moved).unwrap();
    let before_state = fs::read(moved.join("state.json")).unwrap();
    let manager = Manager::open(&moved).unwrap();
    let checkout = moved.join("worktrees/acme/root");
    assert_eq!(git(&checkout, &["status", "--porcelain"]), "?? local.txt");
    assert_eq!(fs::read(checkout.join("local.txt")).unwrap(), b"keep me");
    assert_eq!(fs::read(moved.join("state.json")).unwrap(), before_state);
    fs::remove_file(checkout.join("local.txt")).unwrap();
    manager.restore("acme/root").unwrap();
    assert_eq!(git(&checkout, &["status", "--porcelain"]), "");
    let gitfile = checkout.join(".git");
    let modified = fs::metadata(&gitfile).unwrap().modified().unwrap();
    drop(manager);
    let _manager = Manager::open(&moved).unwrap();
    assert_eq!(
        fs::metadata(&gitfile).unwrap().modified().unwrap(),
        modified
    );
}

#[cfg(unix)]
#[test]
fn opening_workspace_through_alias_preserves_correct_links() {
    use std::{
        os::unix::fs::symlink,
        time::{Duration, SystemTime},
    };
    let dir = TempDir::new();
    let source = scaffold::init(&dir.path().join("source"), "acme", "root")
        .unwrap()
        .repo;
    let real = dir.path().join("real");
    fs::create_dir(&real).unwrap();
    let alias = dir.path().join("alias");
    symlink(&real, &alias).unwrap();
    let root = alias.join("workspace");
    let manager = Manager::open(&root).unwrap();
    manager.install(source.to_str().unwrap()).unwrap();
    let gitfile = root.join("worktrees/acme/root/.git");
    let content = fs::read(&gitfile).unwrap();
    // A fixed old timestamp makes rewrites observable without clock resolution races.
    let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
    fs::File::options()
        .write(true)
        .open(&gitfile)
        .unwrap()
        .set_modified(stamp)
        .unwrap();
    let modified = fs::metadata(&gitfile).unwrap().modified().unwrap();
    drop(manager);
    let _manager = Manager::open(&root).unwrap();
    assert_eq!(fs::read(&gitfile).unwrap(), content);
    assert_eq!(
        fs::metadata(&gitfile).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(
        git(
            &root.join("worktrees/acme/root"),
            &["status", "--porcelain"]
        ),
        ""
    );
}

#[test]
fn opening_workspace_restores_missing_link_files() {
    for target in [".git", "gitdir", "commondir"] {
        let dir = TempDir::new();
        let (_source, root, manager) = installed_workspace(&dir);
        let checkout = root.join("worktrees/acme/root");
        let gitfile = checkout.join(".git");
        let content = fs::read_to_string(&gitfile).unwrap();
        let admin = Path::new(content.trim().strip_prefix("gitdir: ").unwrap());
        let paths = [
            gitfile.clone(),
            admin.join("gitdir"),
            admin.join("commondir"),
        ];
        let expected = paths.each_ref().map(|p| fs::read(p).unwrap());
        let missing = if target == ".git" {
            gitfile
        } else {
            admin.join(target)
        };
        fs::remove_file(missing).unwrap();
        drop(manager);
        let _manager = Manager::open(&root).unwrap();
        for (path, bytes) in paths.iter().zip(expected) {
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
        assert_eq!(git(&checkout, &["status", "--porcelain"]), "");
    }
}

#[cfg(unix)]
#[test]
fn opening_workspace_refuses_symlinked_worktree_metadata_and_parents() {
    use std::os::unix::fs::symlink;
    for target in [
        "worktrees/acme/root/.git",
        "worktrees/acme",
        "repos",
        "gitdir",
        "commondir",
    ] {
        let dir = TempDir::new();
        let (_source, root, manager) = installed_workspace(&dir);
        drop(manager);
        let path = if matches!(target, "gitdir" | "commondir") {
            let gitfile = fs::read_to_string(root.join("worktrees/acme/root/.git")).unwrap();
            Path::new(gitfile.trim().strip_prefix("gitdir: ").unwrap()).join(target)
        } else {
            root.join(target)
        };
        let outside = dir.path().join("outside");
        fs::rename(&path, &outside).unwrap();
        symlink(&outside, &path).unwrap();
        let before = fs::read(root.join("state.json")).unwrap();
        let error = Manager::open(&root).err().expect("unsafe path refused");
        assert_eq!(error.kind(), "workspace");
        assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
        assert!(
            fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}

#[test]
fn root_to_nested_lifecycle_keeps_one_clone_and_all_owners() {
    let dir = TempDir::new();
    let source = scaffold::init(&dir.path().join("source"), "acme", "root")
        .unwrap()
        .repo;
    let root = dir.path().join("workspace");
    let manager = Manager::open(&root).unwrap();
    assert!(!root.join("packages").exists());
    manager.install(source.to_str().unwrap()).unwrap();
    let initial = manager.state().unwrap().packages["acme/root"].clone();
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
    assert!(root.join("worktrees/acme/root/.git").is_file());

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
        r#"{"id":"acme/sibling","version":"0.2.0"}"#,
    )
    .unwrap();
    fs::write(source.join("apps/sibling/dist/index.js"), "sibling").unwrap();
    fs::write(source.join("gitpm.json"), r#"{"packages":["apps/*"]}"#).unwrap();
    scaffold::commit_package(&source, "0.2.0", Some("acme/root")).unwrap();

    // Refused mutations leave both state and deployed output unchanged.
    let before = fs::read(root.join("state.json")).unwrap();
    fs::write(root.join("worktrees/acme/root/local.txt"), "local work").unwrap();
    assert_eq!(
        manager.update("acme/root").unwrap_err().kind(),
        "local_changes"
    );
    assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
    assert!(root.join("dist/acme/root/index.js").is_file());
    fs::remove_file(root.join("worktrees/acme/root/local.txt")).unwrap();

    manager.update("acme/root").unwrap();
    manager
        .install_package(
            source.to_str().unwrap(),
            Some("acme/sibling"),
            InstallTarget::Release,
        )
        .unwrap();
    let state = manager.state().unwrap();
    assert_eq!(state.packages["acme/root"].clone.as_ref(), Some(&key));
    assert_eq!(
        state.packages["acme/root"].path.as_deref(),
        Some("apps/root")
    );
    assert_eq!(state.packages["acme/sibling"].clone.as_ref(), Some(&key));
    assert_eq!(fs::read_dir(root.join("repos")).unwrap().count(), 1);
    let sibling_head = git(&root.join("worktrees/acme/sibling"), &["rev-parse", "HEAD"]);
    let sibling_manifest =
        fs::read(root.join("worktrees/acme/sibling/apps/sibling/plugin.json")).unwrap();
    manager.rollback("acme/root").unwrap();
    assert_eq!(
        git(&root.join("worktrees/acme/root"), &["rev-parse", "HEAD"]),
        initial.current.commit
    );
    assert_eq!(
        git(&root.join("worktrees/acme/sibling"), &["rev-parse", "HEAD"]),
        sibling_head
    );
    assert_eq!(
        fs::read(root.join("worktrees/acme/sibling/apps/sibling/plugin.json")).unwrap(),
        sibling_manifest
    );
    assert!(
        manager.state().unwrap().packages["acme/root"]
            .path
            .is_none()
    );
    assert_eq!(
        fs::read_to_string(root.join("dist/acme/sibling/index.js")).unwrap(),
        "sibling"
    );
    manager.update("acme/root").unwrap();
    manager.forget("acme/root").unwrap();
    manager.uninstall("acme/sibling", &[], false).unwrap();
    assert!(
        clone.join("objects").is_dir(),
        "forgotten owner retains the clone"
    );
    manager.restore("acme/root").unwrap();
    assert!(root.join("dist/acme/root/index.js").is_file());
    manager.uninstall("acme/root", &[], false).unwrap();
    assert!(!clone.exists());
    assert!(!root.join("repos").exists());
    assert!(!root.join("packages").exists());
}

#[test]
fn legacy_clones_are_left_untouched_and_never_used() {
    let dir = TempDir::new();
    let root = dir.path().join("workspace");
    drop(Manager::open(&root).unwrap());
    let legacy = root.join("packages/acme/root");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("local.txt"), "keep").unwrap();
    let source = scaffold::init(&dir.path().join("source"), "acme", "root")
        .unwrap()
        .repo;
    let manager = Manager::open(&root).unwrap();
    manager.install(source.to_str().unwrap()).unwrap();
    manager.uninstall("acme/root", &[], false).unwrap();
    assert_eq!(
        fs::read_to_string(legacy.join("local.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn root_and_nested_worktrees_guard_dirty_files_and_every_moved_head() {
    for nested in [false, true] {
        let dir = TempDir::new();
        let source = scaffold::init(&dir.path().join("source"), "acme", "root")
            .unwrap()
            .repo;
        if nested {
            fs::create_dir_all(source.join("apps/root")).unwrap();
            fs::rename(
                source.join("plugin.json"),
                source.join("apps/root/plugin.json"),
            )
            .unwrap();
            fs::rename(source.join("dist"), source.join("apps/root/dist")).unwrap();
            fs::write(source.join("gitpm.json"), r#"{"packages":["apps/*"]}"#).unwrap();
            scaffold::commit_package(&source, "0.2.0", Some("acme/root")).unwrap();
        }
        let root = dir.path().join("workspace");
        let manager = Manager::open(&root).unwrap();
        manager.install(source.to_str().unwrap()).unwrap();
        scaffold::commit_package(&source, "0.3.0", Some("acme/root")).unwrap();
        manager.update("acme/root").unwrap();
        let record = manager.state().unwrap().packages["acme/root"].clone();
        let checkout = root.join("worktrees/acme/root");
        let payload = checkout.join(if nested {
            "apps/root/dist/index.js"
        } else {
            "dist/index.js"
        });
        let original = fs::read(&payload).unwrap();
        let state = fs::read(root.join("state.json")).unwrap();
        let deployed = fs::read(root.join("dist/acme/root/index.js")).unwrap();
        for staged in [false, true] {
            fs::write(&payload, "local edits").unwrap();
            if staged {
                git(&checkout, &["add", "--all"]);
            }
            for result in [
                manager.update("acme/root").map(|_| ()),
                manager.rollback("acme/root").map(|_| ()),
                manager.uninstall("acme/root", &[], false).map(|_| ()),
            ] {
                assert_eq!(result.unwrap_err().kind(), "local_changes");
            }
            assert_eq!(fs::read(&payload).unwrap(), b"local edits");
            assert_eq!(fs::read(root.join("state.json")).unwrap(), state);
            assert_eq!(
                fs::read(root.join("dist/acme/root/index.js")).unwrap(),
                deployed
            );
            git(&checkout, &["reset", "--hard", &record.current.commit]);
        }
        fs::write(checkout.join("untracked.txt"), "keep").unwrap();
        for result in [
            manager.update("acme/root").map(|_| ()),
            manager.rollback("acme/root").map(|_| ()),
            manager.uninstall("acme/root", &[], false).map(|_| ()),
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
            manager.update("acme/root").map(|_| ()),
            manager.rollback("acme/root").map(|_| ()),
            manager.uninstall("acme/root", &[], false).map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.kind(), "local_changes");
            assert!(
                matches!(error, gitpm::Error::LocalChanges { items, .. } if items.iter().any(|item| matches!(item, gitpm::LostWork::HeadMoved { .. })))
            );
        }
        manager.forget("acme/root").unwrap();
        assert_eq!(
            manager.restore("acme/root").unwrap_err().kind(),
            "local_changes"
        );
        git(&checkout, &["checkout", "--detach", &record.current.commit]);
        manager.restore("acme/root").unwrap();
        manager.uninstall("acme/root", &[], false).unwrap();
        assert!(!checkout.exists());
    }
}

#[test]
fn missing_bare_preserves_orphan_worktree_on_removal() {
    for forget in [false, true] {
        let dir = TempDir::new();
        let source = scaffold::init(&dir.path().join("source"), "acme", "root")
            .unwrap()
            .repo;
        let root = dir.path().join("workspace");
        let manager = Manager::open(&root).unwrap();
        manager.install(source.to_str().unwrap()).unwrap();
        let record = manager.state().unwrap().packages["acme/root"].clone();
        let checkout = root.join("worktrees/acme/root");
        fs::write(checkout.join("scratch.txt"), "keep my work").unwrap();
        fs::remove_dir_all(root.join("repos").join(record.clone.unwrap())).unwrap();
        if forget {
            manager.forget("acme/root").unwrap();
        } else {
            manager.uninstall("acme/root", &[], false).unwrap();
        }
        assert_eq!(
            fs::read(checkout.join("scratch.txt")).unwrap(),
            b"keep my work"
        );
        assert!(!manager.state().unwrap().packages.contains_key("acme/root"));
    }
}

#[test]
fn reinstall_preserves_orphan_worktree_after_missing_bare() {
    let dir = TempDir::new();
    let source = scaffold::init(&dir.path().join("source"), "acme", "root")
        .unwrap()
        .repo;
    let root = dir.path().join("workspace");
    let manager = Manager::open(&root).unwrap();
    manager.install(source.to_str().unwrap()).unwrap();
    let record = manager.state().unwrap().packages["acme/root"].clone();
    let checkout = root.join("worktrees/acme/root");
    fs::write(checkout.join("scratch.txt"), "keep my work").unwrap();
    fs::remove_dir_all(root.join("repos").join(record.clone.unwrap())).unwrap();
    manager.uninstall("acme/root", &[], false).unwrap();
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
