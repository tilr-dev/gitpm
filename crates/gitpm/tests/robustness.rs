use gitpm::{Manager, scaffold};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

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
        let author = scaffold::init(&dir.join("module"), "vendor", "module")
            .unwrap()
            .repo;
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
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
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
    let repo = f.root.join("packages/vendor/module");
    fs::create_dir_all(repo.join("node_modules")).unwrap();
    fs::write(repo.join("node_modules/private.txt"), "irreplaceable").unwrap();
    let before = state(&m);
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let payload = fs::read(f.root.join("dist/vendor/module/index.js")).unwrap();
    assert!(m.update("vendor/module").is_err());
    assert!(m.rollback("vendor/module").is_err());
    assert!(m.uninstall("vendor/module", &[], false).is_err());
    assert_eq!(state(&m), before);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read(f.root.join("dist/vendor/module/index.js")).unwrap(),
        payload
    );
    let fork = m
        .fork_in_namespace("vendor/module", &f.dir.join("fork"), "mine")
        .unwrap();
    assert!(!fork.join("node_modules/private.txt").exists());
    assert_eq!(
        fs::read_to_string(repo.join("node_modules/private.txt")).unwrap(),
        "irreplaceable"
    );
}

#[test]
fn fork_records_source_revision_and_immediate_parent() {
    let f = Fixture::new();
    let m = f.installed();
    let repo = f.root.join("packages/vendor/module");
    git(&repo, &["config", "user.name", "Test"]);
    git(&repo, &["config", "user.email", "test@example.invalid"]);
    let installed = git(&repo, &["rev-parse", "HEAD"]);
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
    assert_eq!(manifest["forkOf"]["commit"], installed);
    assert_eq!(git(&fork, &["rev-parse", "HEAD^"]), installed);
    assert!(!fork.join("local.txt").exists());
    assert!(!fork.join("staged.txt").exists());
    assert_eq!(git(&fork, &["diff", "--cached", "--name-only"]), "");
    assert_eq!(
        git(
            &fork,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        ),
        "plugin.json"
    );
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&repo, &["status", "--porcelain"]), status);
    m.install_package(
        fork.to_str().unwrap(),
        None,
        gitpm::InstallTarget::Ref("HEAD".into()),
    )
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
fn fork_clones_source_submodules_and_installs_only_committed_dist() {
    let f = Fixture::new();
    let module = f.dir.join("dependency");
    fs::create_dir_all(&module).unwrap();
    git(&module, &["init", "-q"]);
    git(&module, &["config", "user.name", "Test"]);
    git(&module, &["config", "user.email", "test@example.invalid"]);
    fs::write(module.join("library.js"), "source dependency").unwrap();
    git(&module, &["add", "."]);
    git(&module, &["commit", "-qm", "dependency"]);
    let module_commit = git(&module, &["rev-parse", "HEAD"]);
    git(
        &f.author,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "../dependency",
            "src/vendor",
        ],
    );
    fs::write(f.author.join("src/main.js"), "source, not distribution").unwrap();
    git(&f.author, &["add", "."]);
    git(&f.author, &["commit", "-qm", "source and submodule"]);
    git(&f.author, &["tag", "-f", "vendor/module@0.1.0"]);
    let m = f.installed();
    let original_dist = fs::read(f.root.join("dist/vendor/module/index.js")).unwrap();
    let before = state(&m);
    // Fork needs only recorded metadata and the source, even without an installed clone.
    fs::remove_dir_all(f.root.join("packages/vendor/module")).unwrap();
    let fork = m
        .fork_in_namespace("vendor/module", &f.dir.join("fork"), "mine")
        .unwrap();
    assert!(fork.join("src/main.js").is_file());
    assert!(
        git(&fork, &["ls-tree", "HEAD", "src/vendor"])
            .starts_with(&format!("160000 commit {module_commit}"))
    );
    git(
        &fork,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "update",
            "--init",
        ],
    );
    assert_eq!(
        fs::read_to_string(fork.join("src/vendor/library.js")).unwrap(),
        "source dependency"
    );
    assert_eq!(state(&m), before);
    m.install_package(
        fork.to_str().unwrap(),
        None,
        gitpm::InstallTarget::Ref("HEAD".into()),
    )
    .unwrap();
    assert_eq!(
        fs::read(f.root.join("dist/mine/module/index.js")).unwrap(),
        original_dist
    );
    assert!(!f.root.join("dist/mine/module/src").exists());
}

#[test]
fn disabled_update_rollback_and_offline_restore() {
    let f = Fixture::new();
    let m = f.installed();
    m.set_enabled("vendor/module", false).unwrap();
    f.release("0.1.1");
    m.update("vendor/module").unwrap();
    assert!(!m.state().unwrap().packages["vendor/module"].enabled);
    fs::rename(&f.author, f.dir.join("unavailable-origin")).unwrap();
    m.rollback("vendor/module").unwrap();
    assert!(!m.state().unwrap().packages["vendor/module"].enabled);
    assert_eq!(
        m.state().unwrap().packages["vendor/module"].current.version,
        "0.1.0"
    );
    m.forget("vendor/module").unwrap();
    m.restore("vendor/module").unwrap();
    // Restored as recorded (offline): still disabled, with its previous revision.
    let s = m.state().unwrap();
    assert!(!s.packages["vendor/module"].enabled);
    assert_eq!(
        s.packages["vendor/module"]
            .previous
            .as_ref()
            .unwrap()
            .version,
        "0.1.1"
    );
    assert!(!f.root.join("dist/vendor/module").exists());
    m.uninstall("vendor/module", &[], false).unwrap();
    assert!(m.state().unwrap().packages.is_empty());
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
    let local = f.root.join("packages/vendor/module/local.txt");
    fs::write(&local, "keep").unwrap();
    m.forget("vendor/module").unwrap();
    assert!(m.install(f.author.to_str().unwrap()).is_err());
    assert!(m.restore("vendor/module").is_err());
    assert_eq!(fs::read_to_string(local).unwrap(), "keep");
    assert!(m.state().unwrap().packages.is_empty());
}

#[test]
fn explicit_fork_toggle_update_rollback_does_not_replace_original_identity() {
    let f = Fixture::new();
    let m = f.installed();
    let original = state(&m)["packages"]["vendor/module"].clone();
    let fork = m
        .fork_in_namespace("vendor/module", &f.dir.join("fork"), "mine")
        .unwrap();
    m.install_package(
        fork.to_str().unwrap(),
        None,
        gitpm::InstallTarget::Ref("HEAD".into()),
    )
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
    assert_eq!(state(&m)["packages"]["vendor/module"], original);
    assert_eq!(
        m.state().unwrap().packages["mine/module"]
            .fork_of
            .as_ref()
            .unwrap()
            .id,
        "vendor/module"
    );
    // Enabled flags are not dependency replacement: that is a separate acceptance requirement.
}
