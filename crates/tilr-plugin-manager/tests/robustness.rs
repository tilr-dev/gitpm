use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
use tilr_plugin_manager::{Manager, scaffold};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    author: PathBuf,
    root: PathBuf,
}
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pm-robustness-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let author = scaffold::create_namespaced(&dir.join("module"), "vendor").unwrap();
        let root = dir.join("workspace");
        Self { dir, author, root }
    }
    fn installed(&self) -> Manager {
        let m = Manager::open(&self.root).unwrap();
        m.install(self.author.to_str().unwrap()).unwrap();
        m
    }
    fn release(&self, version: &str) {
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(self.author.join("plugin.json")).unwrap()).unwrap();
        manifest["version"] = version.into();
        fs::write(
            self.author.join("plugin.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(self.author.join("dist/index.js"), version).unwrap();
        git(&self.author, &["add", "."]);
        git(&self.author, &["commit", "-m", version]);
        git(&self.author, &["tag", &format!("vendor/module@{version}")]);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("PM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}
fn state(m: &Manager) -> serde_json::Value {
    serde_json::to_value(m.state().unwrap()).unwrap()
}

#[test]
fn ignored_work_survives_rejected_operations_and_fork() {
    let f = Fixture::new();
    let m = f.installed();
    f.release("0.1.1");
    m.update("vendor/module").unwrap();
    let repo = f.root.join("plugins/vendor/module");
    fs::create_dir_all(repo.join("node_modules")).unwrap();
    fs::write(repo.join("node_modules/private.txt"), "irreplaceable").unwrap();
    let before = state(&m);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let payload = fs::read(f.root.join("dist/vendor/module/index.js")).unwrap();
    assert!(m.update("vendor/module").is_err());
    assert!(m.rollback("vendor/module").is_err());
    assert!(m.uninstall("vendor/module", false).is_err());
    assert_eq!(state(&m), before);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read(f.root.join("dist/vendor/module/index.js")).unwrap(),
        payload
    );
    let fork = m
        .fork_in_namespace("vendor/module", &f.dir.join("fork"), "mine")
        .unwrap();
    assert_eq!(
        fs::read_to_string(fork.join("node_modules/private.txt")).unwrap(),
        "irreplaceable"
    );
    assert_eq!(
        fs::read_to_string(repo.join("node_modules/private.txt")).unwrap(),
        "irreplaceable"
    );
}

#[test]
fn fork_records_local_head_preserves_index_and_immediate_parent() {
    let f = Fixture::new();
    let m = f.installed();
    let repo = f.root.join("plugins/vendor/module");
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    fs::write(repo.join("local.txt"), "local commit").unwrap();
    git(&repo, &["add", "local.txt"]);
    git(&repo, &["commit", "-m", "local"]);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    fs::write(repo.join("staged.txt"), "not a release").unwrap();
    git(&repo, &["add", "staged.txt"]);
    let status = git(&repo, &["status", "--porcelain"]);
    let fork = m
        .fork_in_namespace("vendor/module", &f.dir.join("fork"), "mine")
        .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fork.join("plugin.json")).unwrap()).unwrap();
    assert_eq!(manifest["forkOf"]["commit"], head);
    assert_eq!(
        git(&fork, &["diff", "--cached", "--name-only"]),
        "staged.txt"
    );
    assert_eq!(
        git(
            &fork,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        "plugin.json"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&repo, &["status", "--porcelain"]), status);
    m.install_ref(fork.to_str().unwrap(), Some("HEAD"), None)
        .unwrap();
    let second = m
        .fork_in_namespace("mine/module", &f.dir.join("second"), "third")
        .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(second.join("plugin.json")).unwrap()).unwrap();
    assert_eq!(manifest["forkOf"]["id"], "mine/module");
    assert_eq!(
        manifest["forkOf"]["commit"],
        git(&fork, &["rev-parse", "HEAD"])
    );
}

#[test]
fn disabled_update_rollback_and_offline_restore() {
    let f = Fixture::new();
    let m = f.installed();
    m.set_enabled("vendor/module", false).unwrap();
    f.release("0.1.1");
    m.update("vendor/module").unwrap();
    assert!(!m.state().unwrap().plugins["vendor/module"].enabled);
    fs::rename(&f.author, f.dir.join("unavailable-origin")).unwrap();
    m.rollback("vendor/module").unwrap();
    assert!(!m.state().unwrap().plugins["vendor/module"].enabled);
    assert_eq!(
        m.state().unwrap().plugins["vendor/module"].current.version,
        "0.1.0"
    );
    m.forget("vendor/module").unwrap();
    m.restore("vendor/module").unwrap();
    let s = m.state().unwrap();
    assert!(s.plugins["vendor/module"].enabled);
    assert!(s.plugins["vendor/module"].previous.is_none());
    m.uninstall("vendor/module", false).unwrap();
    assert!(m.state().unwrap().plugins.is_empty());
}

#[test]
fn corrupt_state_is_not_overwritten() {
    let f = Fixture::new();
    drop(f.installed());
    let payload = fs::read(f.root.join("dist/vendor/module/index.js")).unwrap();
    fs::write(f.root.join("state.json"), b"{truncated").unwrap();
    assert!(Manager::open(&f.root).is_err());
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), b"{truncated");
    assert_eq!(
        fs::read(f.root.join("dist/vendor/module/index.js")).unwrap(),
        payload
    );
}

#[test]
fn forgotten_dirty_clone_cannot_be_overwritten_by_install_or_restore() {
    let f = Fixture::new();
    let m = f.installed();
    let local = f.root.join("plugins/vendor/module/local.txt");
    fs::write(&local, "keep").unwrap();
    m.forget("vendor/module").unwrap();
    assert!(m.install(f.author.to_str().unwrap()).is_err());
    assert!(m.restore("vendor/module").is_err());
    assert_eq!(fs::read_to_string(local).unwrap(), "keep");
    assert!(m.state().unwrap().plugins.is_empty());
}

#[test]
fn explicit_fork_toggle_update_rollback_does_not_replace_original_identity() {
    let f = Fixture::new();
    let m = f.installed();
    let original = state(&m)["plugins"]["vendor/module"].clone();
    let fork = m
        .fork_in_namespace("vendor/module", &f.dir.join("fork"), "mine")
        .unwrap();
    m.install_ref(fork.to_str().unwrap(), Some("HEAD"), None)
        .unwrap();
    m.set_enabled("vendor/module", false).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(fork.join("plugin.json")).unwrap()).unwrap();
    manifest["version"] = "0.1.1".into();
    fs::write(
        fork.join("plugin.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::write(fork.join("dist/index.js"), "fork release").unwrap();
    git(&fork, &["add", "."]);
    git(&fork, &["commit", "-m", "release"]);
    git(&fork, &["tag", "mine/module@0.1.1"]);
    m.update("mine/module").unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("dist/mine/module/index.js")).unwrap(),
        "fork release"
    );
    m.rollback("mine/module").unwrap();
    m.set_enabled("mine/module", false).unwrap();
    m.set_enabled("vendor/module", true).unwrap();
    assert_eq!(state(&m)["plugins"]["vendor/module"], original);
    assert_eq!(
        m.state().unwrap().plugins["mine/module"]
            .fork_of
            .as_ref()
            .unwrap()
            .id,
        "vendor/module"
    );
    // Enabled flags are not dependency replacement: that is a separate acceptance requirement.
}
