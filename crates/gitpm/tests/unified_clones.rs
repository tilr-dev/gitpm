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

fn git(repo: &Path, args: &[&str]) {
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
    assert!(clone.join(".git").is_dir());

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
    git(&source, &["tag", "acme/sibling@0.2.0"]);

    // Refused mutations leave both state and deployed output unchanged.
    let before = fs::read(root.join("state.json")).unwrap();
    fs::write(clone.join("local.txt"), "local work").unwrap();
    assert_eq!(
        manager.update("acme/root").unwrap_err().kind(),
        "local_changes"
    );
    assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
    assert!(root.join("dist/acme/root/index.js").is_file());
    fs::remove_file(clone.join("local.txt")).unwrap();

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
    manager.rollback("acme/root").unwrap();
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
        clone.join(".git").is_dir(),
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
