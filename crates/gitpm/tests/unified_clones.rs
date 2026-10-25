//! A root package and later nested siblings share one source-keyed workspace clone.
mod common;
use gitpm::{InstallTarget, Manager, scaffold};
use std::{
    fs,
    path::{Path, PathBuf},
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
    fs::write(clone.join("local.txt"), "local work").unwrap();
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
    fs::remove_file(clone.join("local.txt")).unwrap();

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
    manager
        .rollback("365ade82-57cd-53a7-a61f-c790d46cba87")
        .unwrap();
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
        clone.join(".git").is_dir(),
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
