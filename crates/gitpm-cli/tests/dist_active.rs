//! End-to-end tests of the `dist/` rule: `dist/<id>` exists exactly for the active
//! packages (installed, enabled, not replaced), whatever command changed the state.
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
mod common;
use common::clone_path;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    dir: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-dist-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("home")).unwrap();
        fs::create_dir_all(dir.join("tmp")).unwrap();
        // The catalog location every command gets must exist (it starts empty).
        fs::create_dir_all(dir.join("catalog")).unwrap();
        fs::write(
            dir.join("home/.gitconfig"),
            "[user]\n\tname = Test\n\temail = test@example.invalid\n[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n",
        )
        .unwrap();
        Self {
            dir: fs::canonicalize(dir).unwrap(),
        }
    }
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.dir.join("home"))
            .env("TMPDIR", self.dir.join("tmp"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.dir.join("home/.gitconfig"));
        command
    }
    fn git(&self, repo: &Path, args: &[&str]) {
        let out = self
            .command("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_gitpm"))
            .current_dir(&self.dir)
            .arg("--root")
            .arg(self.root())
            .arg("--catalog")
            .arg(self.dir.join("catalog"))
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    /// Failing command: returns stderr.
    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?} must fail");
        String::from_utf8(out.stderr).unwrap()
    }
    fn root(&self) -> PathBuf {
        self.dir.join("workspace")
    }
    fn path(&self, relative: &str) -> PathBuf {
        self.dir.join(relative)
    }
    fn state(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["list"])).unwrap()
    }
    /// `dist/<id>/index.js`, `None` when `dist/<id>` does not exist.
    fn dist(&self, id: &str) -> Option<String> {
        let folder = self.root().join("dist").join(id);
        folder
            .exists()
            .then(|| fs::read_to_string(folder.join("index.js")).unwrap())
    }
    /// Commit `contents` as `dist/index.js` of the single-package repository `repo` and
    /// release it as `version`.
    fn release(&self, repo: &str, contents: &str, version: &str) {
        fs::write(self.path(repo).join("dist/index.js"), contents).unwrap();
        self.ok(&["commit", self.path(repo).to_str().unwrap(), version]);
    }
    /// `dd3387ed-a5a3-5059-81ad-b1f6987a28f2` installed from `dd3387ed-a5a3-5059-81ad-b1f6987a28f2` (dist "vendor 1"), and its fork
    /// `826a054b-0fad-5e5a-8890-f87c6452943b` released from `826a054b-0fad-5e5a-8890-f87c6452943b` (dist "fork") and installed.
    fn create(&self, folder: &str, id: &str, name: &str) {
        let repo = self.path(folder);
        fs::create_dir_all(repo.join("dist")).unwrap();
        fs::write(
            repo.join("plugin.json"),
            serde_json::json!({"id":id, "name":name, "version":"0.1.0"}).to_string(),
        )
        .unwrap();
        fs::write(repo.join("dist/index.js"), "hello").unwrap();
        self.git(&repo, &["init", "-q", "-b", "main"]);
        self.git(&repo, &["add", "--all"]);
        self.git(&repo, &["commit", "-qm", "Fixture package"]);
        self.git(&repo, &["tag", "v0.1.0"]);
    }
    fn original_and_fork(&self) {
        self.create(
            "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
            "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
            "demo",
        );
        self.release("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "vendor 1", "0.1.1");
        self.ok(&[
            "install",
            self.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
                .to_str()
                .unwrap(),
        ]);
        self.create(
            "826a054b-0fad-5e5a-8890-f87c6452943b",
            "826a054b-0fad-5e5a-8890-f87c6452943b",
            "demo",
        );
        let source = self.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2");
        let output = self
            .command("git")
            .arg("-C")
            .arg(&source)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let commit = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        let path = self.path("826a054b-0fad-5e5a-8890-f87c6452943b/plugin.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["forkOf"] = serde_json::json!({"id":"dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "version":"0.1.1", "commit":commit, "source":source});
        manifest["replaces"] =
            serde_json::json!({"id":"dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "version":"0.1.1"});
        fs::write(path, manifest.to_string()).unwrap();
        self.release("826a054b-0fad-5e5a-8890-f87c6452943b", "fork", "0.2.0");
        self.ok(&[
            "install",
            self.path("826a054b-0fad-5e5a-8890-f87c6452943b")
                .to_str()
                .unwrap(),
        ]);
        assert_eq!(
            self.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
            Some("vendor 1")
        );
        assert_eq!(
            self.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
            Some("fork")
        );
    }
}
impl Fixture {
    /// `9692fc07-23c7-5f20-8779-f09374f1a571` (needs `dd3387ed-a5a3-5059-81ad-b1f6987a28f2 >=0.1`) released from `9692fc07-23c7-5f20-8779-f09374f1a571` and installed.
    fn consumer(&self) {
        self.create(
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "9692fc07-23c7-5f20-8779-f09374f1a571",
            "a",
        );
        let manifest = self.path("9692fc07-23c7-5f20-8779-f09374f1a571/plugin.json");
        let mut json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
        json["dependencies"] = serde_json::json!({"dd3387ed-a5a3-5059-81ad-b1f6987a28f2": ">=0.1"});
        fs::write(&manifest, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        self.release("9692fc07-23c7-5f20-8779-f09374f1a571", "app", "0.1.1");
        self.ok(&[
            "install",
            self.path("9692fc07-23c7-5f20-8779-f09374f1a571")
                .to_str()
                .unwrap(),
        ]);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

#[test]
fn replace_removes_the_original_dist_and_back_restores_it_from_the_clone() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    assert!(
        !f.root()
            .join("dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
            .exists(),
        "original UUID entry removed"
    );
    // The fork keeps its own dist; nothing is copied into the original's folder.
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    // list still reports the original as installed and enabled.
    let state = f.state();
    assert_eq!(
        state["packages"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]["enabled"],
        true
    );
    assert_eq!(
        state["replacements"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"],
        "826a054b-0fad-5e5a-8890-f87c6452943b"
    );
    // resolved[].active is the dist rule; enabled alone is not.
    let resolved = |state: &serde_json::Value, id: &str| {
        let entry = state["resolved"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == id)
            .unwrap()
            .clone();
        (entry["enabled"].clone(), entry["active"].clone())
    };
    assert_eq!(
        resolved(&state, "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        (true.into(), false.into())
    );
    assert_eq!(
        resolved(&state, "826a054b-0fad-5e5a-8890-f87c6452943b"),
        (true.into(), true.into())
    );
    let out = f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert!(
        out.contains(
            "; 826a054b-0fad-5e5a-8890-f87c6452943b stays installed and enabled (disable or uninstall it if not wanted)"
        ),
        "{out}"
    );
    assert_eq!(
        resolved(&f.state(), "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        (true.into(), true.into())
    );
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
}

#[test]
fn disable_removes_and_enable_restores_also_around_a_replacement() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["disable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&["enable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    // Returning to a disabled original is refused until it is explicitly enabled.
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&["disable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert!(
        f.fails(&[
            "replace",
            "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
            "--with",
            "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"
        ])
        .contains("enable it first")
    );
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&["enable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    // Disabling the replacement drops it: the installed original takes over and is
    // deployed again; a later enable does not bring the binding back.
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&["disable", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert_eq!(f.dist("826a054b-0fad-5e5a-8890-f87c6452943b"), None);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert!(f.state().get("replacements").is_none());
    f.ok(&["enable", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    assert!(f.state().get("replacements").is_none());
    // A consumer the original cannot serve (it is uninstalled) guards the fork.
    f.consumer();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&["uninstall", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    let error = f.fails(&["disable", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert!(
        error.contains("cannot disable 826a054b-0fad-5e5a-8890-f87c6452943b: required by 9692fc07-23c7-5f20-8779-f09374f1a571")
            && error.contains("Error kind: has_dependents")
            && error.contains("Hint: `gitpm disable 826a054b-0fad-5e5a-8890-f87c6452943b --force` does it anyway"),
        "{error}"
    );
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    // Forced: the binding is dropped and nothing is deployed for the original.
    f.ok(&["disable", "826a054b-0fad-5e5a-8890-f87c6452943b", "--force"]);
    assert_eq!(f.dist("826a054b-0fad-5e5a-8890-f87c6452943b"), None);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    assert!(
        !f.root()
            .join("dist/826a054b-0fad-5e5a-8890-f87c6452943b")
            .exists()
    );
    assert!(f.state().get("replacements").is_none());
    // A disabled package cannot become a replacement.
    f.ok(&[
        "install",
        f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
            .to_str()
            .unwrap(),
    ]);
    let error = f.fails(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert!(
        error.contains("826a054b-0fad-5e5a-8890-f87c6452943b is disabled; enable it first"),
        "{error}"
    );
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
}

#[test]
fn update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.release("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "vendor 2", "0.3.0");
    f.ok(&["update", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(
        f.state()["packages"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]["current"]["version"],
        "0.3.0"
    );
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 2")
    );
    // Active again: rollback deploys as usual.
    f.ok(&["rollback", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    // Replaced: rollback only changes state and clone.
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&["rollback", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 2")
    );
}

#[test]
fn installing_a_package_that_is_replaced_deploys_nothing() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    // Uninstalling the original keeps the replacement binding.
    f.ok(&["uninstall", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(
        f.state()["replacements"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"],
        "826a054b-0fad-5e5a-8890-f87c6452943b"
    );
    f.ok(&[
        "install",
        f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
            .to_str()
            .unwrap(),
    ]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
}

#[test]
fn explicit_fork_install_and_replace_deploys_only_the_fork() {
    let f = Fixture::new();
    f.create(
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "demo",
    );
    f.ok(&[
        "install",
        f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
            .to_str()
            .unwrap(),
    ]);
    let original = f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").unwrap();
    f.ok(&[
        "fork",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--into",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    let fork_id = common::manifest_id(&f.path("826a054b-0fad-5e5a-8890-f87c6452943b"));

    f.ok(&[
        "commit",
        f.path("826a054b-0fad-5e5a-8890-f87c6452943b")
            .to_str()
            .unwrap(),
        "0.2.0",
    ]);
    f.ok(&[
        "install",
        f.path("826a054b-0fad-5e5a-8890-f87c6452943b")
            .to_str()
            .unwrap(),
    ]);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        fork_id.as_str(),
    ]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    assert_eq!(f.dist(fork_id.as_str()).as_deref(), Some(original.as_str()));
    assert_eq!(
        f.state()["replacements"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"],
        fork_id.as_str()
    );
}

#[test]
fn uninstalling_or_forgetting_the_replacement_deploys_the_original_again() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    // forget drops the replacement (and the fork's dist); restore keeps it dropped.
    f.ok(&["forget", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert!(f.state().get("replacements").is_none());
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert_eq!(f.dist("826a054b-0fad-5e5a-8890-f87c6452943b"), None);
    f.ok(&["restore", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    // uninstall drops the replacement too.
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    f.ok(&["uninstall", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert!(f.state().get("replacements").is_none());
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert!(
        !f.root()
            .join("dist/826a054b-0fad-5e5a-8890-f87c6452943b")
            .exists()
    );
}

/// Repository `mono` with packages `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` and `f851f609-bb81-52d6-bfef-33798624782c` (dist "NAME VERSION").
fn mono(f: &Fixture) -> PathBuf {
    let mono = f.path("mono");
    fs::create_dir_all(&mono).unwrap();
    f.git(&mono, &["init", "-q", "-b", "main"]);
    fs::write(mono.join("gitpm.json"), r#"{"packages": ["packages/*"]}"#).unwrap();
    for name in ["store", "theme"] {
        write_package(&mono, name, "1.0.0", &format!("{name} 1.0.0"));
    }
    f.git(&mono, &["add", "--all"]);
    f.git(&mono, &["commit", "-q", "-m", "first"]);
    f.git(&mono, &["tag", "v1.0.0"]);
    mono
}
fn write_package(mono: &Path, name: &str, version: &str, contents: &str) {
    let id = match name {
        "store" => "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "theme" => "f851f609-bb81-52d6-bfef-33798624782c",
        _ => panic!("unknown package"),
    };
    let folder = mono.join("packages").join(name);
    fs::create_dir_all(folder.join("dist")).unwrap();
    fs::write(
        folder.join("plugin.json"),
        format!(r#"{{"id":"{id}","name": "fixture", "version":"{version}"}}"#),
    )
    .unwrap();
    fs::write(folder.join("dist/index.js"), contents).unwrap();
}
fn release_package(f: &Fixture, mono: &Path, name: &str, version: &str) {
    write_package(mono, name, version, &format!("{name} {version}"));
    for entry in fs::read_dir(mono.join("packages")).unwrap() {
        let path = entry.unwrap().path().join("plugin.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["version"] = version.into();
        fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    }
    f.git(mono, &["add", "--all"]);
    f.git(mono, &["commit", "-q", "-m", &format!("{name} {version}")]);
    f.git(mono, &["tag", &format!("v{version}")]);
}

#[test]
fn a_package_of_a_shared_clone_is_deployed_from_its_own_commit() {
    let f = Fixture::new();
    let mono = mono(&f);
    let source = mono.to_str().unwrap();
    f.ok(&["install", source, "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    f.ok(&["install", source, "f851f609-bb81-52d6-bfef-33798624782c"]);
    f.ok(&[
        "fork",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--into",
        "7200845d-5956-5ec3-85d1-a7161d3592de",
    ]);
    let fork = f.path("7200845d-5956-5ec3-85d1-a7161d3592de");
    let fork_store = common::manifest_id(&fork.join("packages/store"));
    let fork_theme = common::manifest_id(&fork.join("packages/theme"));

    f.ok(&[
        "commit",
        fork.to_str().unwrap(),
        "1.0.1",
        "--package",
        fork_theme.as_str(),
    ]);
    f.ok(&["install", fork.to_str().unwrap(), fork_theme.as_str()]);
    f.ok(&[
        "replace",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--with",
        fork_theme.as_str(),
    ]);
    assert_eq!(f.dist("f851f609-bb81-52d6-bfef-33798624782c"), None);
    assert_eq!(f.dist(fork_theme.as_str()).as_deref(), Some("theme 1.0.0"));
    // Only theme was explicitly installed and replaced; store keeps its original provider.
    assert_eq!(f.dist(fork_store.as_str()), None);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd").as_deref(),
        Some("store 1.0.0")
    );
    // theme 1.1.0 is installed while replaced; then store moves the shared clone's HEAD
    // to a commit whose theme files differ from theme's installed revision.
    release_package(&f, &mono, "theme", "1.1.0");
    f.ok(&["update", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(f.dist("f851f609-bb81-52d6-bfef-33798624782c"), None);
    fs::write(
        mono.join("packages/theme/dist/index.js"),
        "theme unreleased",
    )
    .unwrap();
    release_package(&f, &mono, "store", "1.2.0");
    f.ok(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd").as_deref(),
        Some("store 1.2.0")
    );
    f.ok(&[
        "replace",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--with",
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c").as_deref(),
        Some("theme 1.1.0")
    );
    assert_eq!(f.dist(fork_theme.as_str()).as_deref(), Some("theme 1.0.0"));
    f.ok(&["disable", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(f.dist("f851f609-bb81-52d6-bfef-33798624782c"), None);
    f.ok(&["enable", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c").as_deref(),
        Some("theme 1.1.0")
    );
}

#[test]
fn sync_dist_repairs_a_tree_written_by_an_older_gitpm() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    // As gitpm 0.4 left it: the replaced original deployed, the fork's dist missing,
    // plus a folder of no known package.
    let dist = f.root().join("dist");
    fs::create_dir_all(dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")).unwrap();
    fs::write(
        dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2/index.js"),
        "stale",
    )
    .unwrap();
    fs::remove_dir_all(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b")).unwrap();
    fs::create_dir_all(dist.join("stray")).unwrap();
    // Read-only commands never rewrite dist.
    f.state();
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("stale")
    );
    let out = f.run(&["sync-dist", "--dry-run"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("826a054b-0fad-5e5a-8890-f87c6452943b would be deployed to dist/826a054b-0fad-5e5a-8890-f87c6452943b"),
        "{stdout}"
    );
    assert!(
        stdout.contains("dd3387ed-a5a3-5059-81ad-b1f6987a28f2 would be removed from dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        "{stdout}"
    );
    assert!(stdout.contains("Nothing was changed"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dist/stray belongs to no known package"),
        "{stderr}"
    );
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("stale")
    );
    assert_eq!(f.dist("826a054b-0fad-5e5a-8890-f87c6452943b"), None);
    let out = f.ok(&["sync-dist"]);
    assert!(out.contains("826a054b-0fad-5e5a-8890-f87c6452943b deployed to dist/826a054b-0fad-5e5a-8890-f87c6452943b"), "{out}");
    assert!(
        out.contains("dd3387ed-a5a3-5059-81ad-b1f6987a28f2 removed from dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        "{out}"
    );
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    assert!(!dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").exists());
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    assert!(
        dist.join("stray").is_dir(),
        "unmanaged folders are left alone"
    );
    let out = f.ok(&["sync-dist"]);
    assert!(
        out.contains("already holds exactly the active packages"),
        "{out}"
    );
    // A disabled package's leftover dist is removed as well.
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    f.ok(&["disable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    fs::create_dir_all(dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")).unwrap();
    let out = f.ok(&["sync-dist"]);
    assert!(
        out.contains("dd3387ed-a5a3-5059-81ad-b1f6987a28f2 removed from dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        "{out}"
    );
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    // Other options are refused.
    assert!(!f.run(&["sync-dist", "--force"]).status.success());
    assert!(
        !f.run(&["sync-dist", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"])
            .status
            .success()
    );
}

#[test]
fn any_mutating_command_reconciles_the_whole_dist() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["disable", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    // Drift no command of this test touches: an active package's dist missing, a
    // disabled package deployed, a folder of no known package.
    let dist = f.root().join("dist");
    fs::remove_dir_all(dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")).unwrap();
    fs::create_dir_all(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b")).unwrap();
    fs::write(
        dist.join("826a054b-0fad-5e5a-8890-f87c6452943b/index.js"),
        "stale",
    )
    .unwrap();
    fs::create_dir_all(dist.join("stray")).unwrap();
    f.state();
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        None,
        "list never changes dist"
    );
    // An unrelated install fixes all of it.
    f.create(
        "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f",
        "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f",
        "pkg",
    );
    let out = f.run(&[
        "install",
        f.path("fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f")
            .to_str()
            .unwrap(),
    ]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dist/stray belongs to no known package"),
        "{stderr}"
    );
    assert!(f.dist("fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f").is_some());
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert_eq!(f.dist("826a054b-0fad-5e5a-8890-f87c6452943b"), None);
    assert!(!dist.join("826a054b-0fad-5e5a-8890-f87c6452943b").exists());
    assert!(
        dist.join("stray").is_dir(),
        "unmanaged folders are left alone"
    );
    let out = f.ok(&["sync-dist", "--dry-run"]);
    assert!(
        out.contains("already holds exactly the active packages"),
        "{out}"
    );
}

#[test]
fn a_uuid_path_that_is_not_a_folder_blocks_nothing_else() {
    let f = Fixture::new();
    f.original_and_fork();
    // dist/826a054b-0fad-5e5a-8890-f87c6452943b becomes a symlink to a folder outside the workspace.
    let dist = f.root().join("dist");
    let outside = f.path("outside");
    fs::rename(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, dist.join("826a054b-0fad-5e5a-8890-f87c6452943b"))
        .unwrap();
    fs::remove_dir_all(dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")).unwrap();
    f.create(
        "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f",
        "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f",
        "pkg",
    );
    let out = f.run(&[
        "install",
        f.path("fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f")
            .to_str()
            .unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("dist/826a054b-0fad-5e5a-8890-f87c6452943b belongs to no known package or is not a folder"),
        "{stderr}"
    );
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert!(
        fs::symlink_metadata(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(outside.join("index.js")).unwrap(),
        "fork",
        "never written through"
    );
    // sync-dist reports it and syncs the rest.
    fs::remove_dir_all(dist.join("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")).unwrap();
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(
        stderr.contains("dist/826a054b-0fad-5e5a-8890-f87c6452943b belongs to no known package or is not a folder"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Warning: 826a054b-0fad-5e5a-8890-f87c6452943b: dist not deployed (dist/826a054b-0fad-5e5a-8890-f87c6452943b is not a folder)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error: 1 package(s) not deployed: 826a054b-0fad-5e5a-8890-f87c6452943b"),
        "{stderr}"
    );
    assert!(stderr.contains("Error kind: not_deployed"), "{stderr}");
    assert!(
        stderr.contains("Hint: remove or rename `dist/826a054b-0fad-5e5a-8890-f87c6452943b` and run `gitpm sync-dist` again"),
        "{stderr}"
    );
    assert!(!stderr.contains("clone is gone"), "{stderr}");
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    // A plain file there behaves the same.
    fs::remove_file(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b")).unwrap();
    fs::write(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b"), "file").unwrap();
    f.ok(&["disable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    assert_eq!(
        fs::read_to_string(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b")).unwrap(),
        "file"
    );
    let out = f.run(&[
        "uninstall",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
        "--force",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("Error kind: workspace"), "{stderr}");
    assert_eq!(
        fs::read_to_string(dist.join("826a054b-0fad-5e5a-8890-f87c6452943b")).unwrap(),
        "file"
    );
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(
        stderr.contains("Warning: 826a054b-0fad-5e5a-8890-f87c6452943b: dist not deployed (dist/826a054b-0fad-5e5a-8890-f87c6452943b is not a folder)"),
        "{stderr}"
    );
    assert!(stderr.contains("Error kind: not_deployed"), "{stderr}");
    assert!(
        stderr.contains("Hint: remove or rename `dist/826a054b-0fad-5e5a-8890-f87c6452943b` and run `gitpm sync-dist` again"),
        "{stderr}"
    );
    assert!(!stderr.contains("clone is gone"), "{stderr}");
    // Following the hint repairs deployment without dropping the package's record.
    fs::rename(
        dist.join("826a054b-0fad-5e5a-8890-f87c6452943b"),
        dist.join("dan-blocked"),
    )
    .unwrap();
    f.ok(&["sync-dist"]);
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    assert_eq!(
        fs::read_to_string(dist.join("dan-blocked")).unwrap(),
        "file"
    );
}

#[test]
fn a_missing_clone_is_reported_and_blocks_only_its_own_dist() {
    let f = Fixture::new();
    f.original_and_fork();
    fs::remove_dir_all(clone_path(
        &f.root(),
        &f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
    ))
    .unwrap();
    fs::remove_dir_all(f.root().join("dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2")).unwrap();
    f.create(
        "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f",
        "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f",
        "pkg",
    );
    let out = f.run(&[
        "install",
        f.path("fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f")
            .to_str()
            .unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "Warning: dd3387ed-a5a3-5059-81ad-b1f6987a28f2: dist not deployed (clone missing); uninstall or forget dd3387ed-a5a3-5059-81ad-b1f6987a28f2 to drop its record"
        ),
        "{stderr}"
    );
    assert!(f.dist("fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f").is_some());
    // sync-dist removes stale dists anyway, then fails naming the package.
    f.ok(&["disable", "fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f"]);
    fs::create_dir_all(f.root().join("dist/fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f")).unwrap();
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("Error: 1 package(s) not deployed: dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
        "{stderr}"
    );
    // A library error now: the kind and the command that helps.
    assert!(stderr.contains("Error kind: not_deployed"), "{stderr}");
    assert!(
        stderr.contains("Hint: `gitpm uninstall dd3387ed-a5a3-5059-81ad-b1f6987a28f2` or `gitpm forget dd3387ed-a5a3-5059-81ad-b1f6987a28f2` drops a package whose clone is gone"),
        "{stderr}"
    );
    // The way out works without confirmation: uninstall drops the record and dist.
    let out = f.ok(&["uninstall", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(out, "dd3387ed-a5a3-5059-81ad-b1f6987a28f2 uninstalled\n");
    let out = f.run(&["sync-dist"]);
    assert!(out.status.success());
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("Warning"),
        "later commands warn no more"
    );
    assert_eq!(f.dist("fa5ae48c-53cd-5b99-8cc1-246d4ce19c1f"), None);
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
}

#[test]
fn every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.original_and_fork();
    let repo = f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2");
    fs::create_dir_all(repo.join("dist/bin")).unwrap();
    fs::create_dir_all(repo.join("dist/75494f24-1c7e-5480-9e39-738ff3d038c4")).unwrap();
    fs::write(repo.join("dist/bin/run.sh"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(
        repo.join("dist/bin/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(
        repo.join("dist/75494f24-1c7e-5480-9e39-738ff3d038c4/c.txt"),
        "nested",
    )
    .unwrap();
    f.release("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "vendor 2", "0.3.0");
    let check = |when: &str| {
        let dist = f.root().join("dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2");
        assert_eq!(
            fs::read_to_string(dist.join("75494f24-1c7e-5480-9e39-738ff3d038c4/c.txt")).unwrap(),
            "nested",
            "{when}"
        );
        let mode = fs::metadata(dist.join("bin/run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_ne!(mode & 0o111, 0, "{when}: executable bit");
        let mode = fs::metadata(dist.join("index.js"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0, "{when}: plain file");
    };
    f.ok(&["update", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    check("update");
    // Uncommitted edits in the clone are never deployed.
    let clone = clone_path(&f.root(), &f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"));
    fs::write(clone.join("dist/index.js"), "edited").unwrap();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    check("--back");
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 2")
    );
    f.ok(&["disable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    f.ok(&["enable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    check("enable");
    fs::write(clone.join("dist/index.js"), "vendor 2").unwrap();
    f.ok(&["rollback", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
    assert!(
        !f.root()
            .join("dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2/bin")
            .exists()
    );
}

#[test]
fn replace_with_absent_original_is_refused() {
    let f = Fixture::new();
    f.original_and_fork();
    f.consumer();
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&["uninstall", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    // --with needs the original installed.
    let error = f.fails(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert!(
        error.contains("Error: package dd3387ed-a5a3-5059-81ad-b1f6987a28f2 is not installed")
            && error.contains("Error kind: not_installed"),
        "{error}"
    );
    // Returning requires an installed original, with no force bypass.
    let error = f.fails(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert!(error.contains("Error kind: not_installed"), "{error}");
    assert_eq!(
        f.state()["replacements"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"],
        "826a054b-0fad-5e5a-8890-f87c6452943b"
    );
    let error = f.fails(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--force",
    ]);
    assert!(error.contains("--force is only valid"), "{error}");
    assert_eq!(
        f.state()["replacements"]["dd3387ed-a5a3-5059-81ad-b1f6987a28f2"],
        "826a054b-0fad-5e5a-8890-f87c6452943b"
    );
    assert_eq!(
        f.dist("826a054b-0fad-5e5a-8890-f87c6452943b").as_deref(),
        Some("fork")
    );
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
}

#[test]
fn forget_says_whether_the_clone_was_kept() {
    let f = Fixture::new();
    f.original_and_fork();
    let out = f.ok(&["forget", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    assert_eq!(
        out,
        "826a054b-0fad-5e5a-8890-f87c6452943b forgotten (clone kept; restore re-registers it)\n"
    );
    fs::remove_dir_all(clone_path(
        &f.root(),
        &f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"),
    ))
    .unwrap();
    let out = f.ok(&["forget", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    assert_eq!(
        out,
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2 forgotten (no clone; record and dist removed)\n"
    );
    assert_eq!(f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), None);
    assert!(
        f.state()["forgotten"]
            .get("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
            .is_none()
    );
}

/// Uninstalling a fork that would reactivate an original with a disabled dependency is
/// refused from the plan, before any question about lost work (also on a TTY).
#[test]
fn uninstall_refusal_of_a_reactivation_comes_before_the_prompt() {
    if !Path::new("/usr/bin/script").exists() {
        return;
    }
    let f = Fixture::new();
    f.original_and_fork();
    f.create(
        "7bb3523b-dc85-5ff2-a33f-55b071c001cb",
        "7bb3523b-dc85-5ff2-a33f-55b071c001cb",
        "x",
    );
    f.ok(&[
        "install",
        f.path("7bb3523b-dc85-5ff2-a33f-55b071c001cb")
            .to_str()
            .unwrap(),
    ]);
    let manifest = f.path("dd3387ed-a5a3-5059-81ad-b1f6987a28f2/plugin.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    json["dependencies"] = serde_json::json!({"7bb3523b-dc85-5ff2-a33f-55b071c001cb": ">=0.1"});
    fs::write(&manifest, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    f.release("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "vendor 2", "0.1.2");
    f.ok(&["update", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"]);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&["disable", "7bb3523b-dc85-5ff2-a33f-55b071c001cb", "--force"]);
    fs::write(
        clone_path(&f.root(), &f.path("826a054b-0fad-5e5a-8890-f87c6452943b")).join("notes.txt"),
        "work",
    )
    .unwrap();
    let util_linux = Command::new("script")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let script = if util_linux {
        "script -qec \"'$0' --root '$1' --catalog '$2' uninstall 826a054b-0fad-5e5a-8890-f87c6452943b 2>'$3'\" /dev/null"
    } else {
        "script -q /dev/null sh -c \"exec '$0' --root '$1' --catalog '$2' uninstall 826a054b-0fad-5e5a-8890-f87c6452943b 2>'$3'\""
    };
    let stderr_file = f.path("uninstall.stderr");
    let out = f
        .command("sh")
        .arg("-c")
        .arg(format!("(sleep 1; printf 'y\\n'; sleep 1) | {script}"))
        .arg(env!("CARGO_BIN_EXE_gitpm"))
        .arg(f.root())
        .arg(f.path("catalog"))
        .arg(&stderr_file)
        .output()
        .unwrap();
    let stderr = fs::read_to_string(&stderr_file).unwrap_or_default();
    assert!(!stderr.contains("Delete anyway"), "{stderr}");
    assert!(
        stderr.contains("would reactivate dd3387ed-a5a3-5059-81ad-b1f6987a28f2: 7bb3523b-dc85-5ff2-a33f-55b071c001cb is disabled; enable it")
            && stderr.contains("Error kind: dependency_conflict"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("uninstalled"));
    assert!(
        clone_path(&f.root(), &f.path("826a054b-0fad-5e5a-8890-f87c6452943b"))
            .join("notes.txt")
            .exists()
    );
}

/// A release with a configurable output folder, either at root or inside a monorepo.
fn output_release(f: &Fixture, folder: Option<&str>, mono: bool) -> PathBuf {
    let repo = f.path("source");
    let package = if mono {
        repo.join("packages/demo")
    } else {
        repo.clone()
    };
    fs::create_dir_all(&package).unwrap();
    let mut manifest = serde_json::json!({"id":"be228981-2f39-55b3-b613-46b8d654dcaa", "name": "fixture", "version":"0.1.0",
        "buildCommand":"touch BUILD_MUST_NOT_RUN", "buildRequired":true});
    if let Some(folder) = folder {
        manifest["dist"] = folder.into();
    }
    fs::write(package.join("plugin.json"), manifest.to_string()).unwrap();
    let output = package.join(folder.unwrap_or("dist"));
    fs::create_dir_all(output.join("nested")).unwrap();
    fs::write(output.join("index.js"), "committed output").unwrap();
    fs::write(output.join("nested/data"), "nested output").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(output.join("index.js"), fs::Permissions::from_mode(0o755)).unwrap();
    if folder != Some(".") {
        fs::write(package.join("outside"), "not deployed").unwrap();
    }
    if mono {
        fs::write(repo.join("gitpm.json"), r#"{"packages":["packages/*"]}"#).unwrap();
        fs::write(repo.join("sibling"), "not deployed").unwrap();
    }
    f.git(&repo, &["init"]);
    f.git(&repo, &["add", "."]);
    f.git(&repo, &["commit", "-m", "release"]);
    f.git(&repo, &["tag", "v0.1.0"]);
    repo
}

#[test]
fn declared_output_folders_install_redeploy_and_verify_catalog() {
    for mono in [false, true] {
        for folder in [None, Some("dist"), Some("."), Some("output/vtf")] {
            let f = Fixture::new();
            let repo = output_release(&f, folder, mono);
            let source = repo.to_str().unwrap();
            f.ok(&["install", source]);
            assert_eq!(
                f.dist("be228981-2f39-55b3-b613-46b8d654dcaa").as_deref(),
                Some("committed output")
            );
            let deployed = f.root().join("dist/be228981-2f39-55b3-b613-46b8d654dcaa");
            assert_eq!(
                fs::read_to_string(deployed.join("nested/data")).unwrap(),
                "nested output"
            );
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(deployed.join("index.js"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o111,
                0
            );
            assert!(!deployed.join("outside").exists());
            assert!(!deployed.join("sibling").exists());
            assert!(!deployed.join(".git").exists());
            let state = f.state();
            let key = state["packages"]["be228981-2f39-55b3-b613-46b8d654dcaa"]["clone"]
                .as_str()
                .unwrap();
            let clone = f.root().join("repos").join(key);
            let package = if mono {
                clone.join("packages/demo")
            } else {
                clone
            };
            fs::write(
                package.join(folder.unwrap_or("dist")).join("index.js"),
                "dirty",
            )
            .unwrap();
            f.ok(&["disable", "be228981-2f39-55b3-b613-46b8d654dcaa"]);
            f.ok(&["enable", "be228981-2f39-55b3-b613-46b8d654dcaa"]);
            assert_eq!(
                f.dist("be228981-2f39-55b3-b613-46b8d654dcaa").as_deref(),
                Some("committed output")
            );
            f.ok(&[
                "uninstall",
                "be228981-2f39-55b3-b613-46b8d654dcaa",
                "--force",
            ]);
            fs::write(f.path("sources.txt"), format!("{source}\n")).unwrap();
            f.ok(&[
                "catalog",
                "update",
                "--sources",
                f.path("sources.txt").to_str().unwrap(),
            ]);
            let release: serde_json::Value = serde_json::from_slice(
                &fs::read(f.path("catalog/packages/be228981-2f39-55b3-b613-46b8d654dcaa/versions/0.1.0/release.json"))
                    .unwrap(),
            )
            .unwrap();
            assert!(release["files"].get("index.js").is_some());
            assert!(release["files"].get("outside").is_none());
            f.ok(&["install-catalog", "be228981-2f39-55b3-b613-46b8d654dcaa"]);
            assert_eq!(
                f.dist("be228981-2f39-55b3-b613-46b8d654dcaa").as_deref(),
                Some("committed output")
            );
            assert!(!repo.join("BUILD_MUST_NOT_RUN").exists());
            assert!(!f.root().join("BUILD_MUST_NOT_RUN").exists());
        }
    }
}

#[test]
fn declared_output_missing_required_build_is_actionable_and_atomic() {
    for mono in [false, true] {
        let f = Fixture::new();
        let repo = output_release(&f, Some("output/vtf"), mono);
        let package = if mono {
            repo.join("packages/demo")
        } else {
            repo.clone()
        };
        fs::remove_dir_all(package.join("output")).unwrap();
        f.git(&repo, &["add", "."]);
        f.git(&repo, &["commit", "-m", "remove build output"]);
        f.git(&repo, &["tag", "-f", "v0.1.0"]);
        let error = f.fails(&["install", repo.to_str().unwrap()]);
        assert!(error.contains("invalid_manifest"), "{error}");
        assert!(error.contains("output/vtf"), "{error}");
        assert!(
            error.contains("buildCommand: touch BUILD_MUST_NOT_RUN"),
            "{error}"
        );
        assert!(
            !f.root()
                .join("dist/be228981-2f39-55b3-b613-46b8d654dcaa")
                .exists()
        );
        assert!(!package.join("BUILD_MUST_NOT_RUN").exists());
    }
}

#[test]
fn declared_output_fork_preserves_build_metadata_without_running_it() {
    let f = Fixture::new();
    let repo = output_release(&f, Some("output/vtf"), false);
    f.ok(&["install", repo.to_str().unwrap()]);
    f.ok(&[
        "fork",
        "be228981-2f39-55b3-b613-46b8d654dcaa",
        "--into",
        "fork",
    ]);
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("fork/plugin.json")).unwrap()).unwrap();
    assert_eq!(manifest["dist"], "output/vtf");
    assert_eq!(manifest["buildRequired"], true);
    assert_eq!(manifest["buildCommand"], "touch BUILD_MUST_NOT_RUN");
    assert!(!f.path("fork/BUILD_MUST_NOT_RUN").exists());
}

#[test]
fn declared_output_rejects_invalid_fields() {
    for (key, value) in [
        ("dist", serde_json::json!("../escape")),
        ("dist", serde_json::json!("/absolute")),
        ("dist", serde_json::json!("")),
        ("dist", serde_json::json!("output\\escape")),
        ("dist", serde_json::json!(42)),
        ("dist", serde_json::Value::Null),
        ("buildCommand", serde_json::Value::Null),
        ("buildRequired", serde_json::json!("true")),
        ("buildCommand", serde_json::json!(["build"])),
    ] {
        let f = Fixture::new();
        let repo = output_release(&f, None, false);
        let path = repo.join("plugin.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest[key] = value;
        fs::write(path, manifest.to_string()).unwrap();
        f.git(&repo, &["add", "."]);
        f.git(&repo, &["commit", "-m", "invalid manifest"]);
        f.git(&repo, &["tag", "-f", "v0.1.0"]);
        let error = f.fails(&["install", repo.to_str().unwrap()]);
        assert!(error.contains("invalid_manifest"), "{error}");
    }
}

#[test]
fn declared_output_update_error_names_package() {
    for mono in [false, true] {
        let f = Fixture::new();
        let repo = output_release(&f, None, mono);
        let package = if mono {
            repo.join("packages/demo")
        } else {
            repo.clone()
        };
        f.ok(&["install", repo.to_str().unwrap()]);
        let path = package.join("plugin.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["version"] = serde_json::json!("0.2.0");
        manifest["dist"] = serde_json::json!("../escape");
        fs::write(path, manifest.to_string()).unwrap();
        f.git(&repo, &["add", "."]);
        f.git(&repo, &["commit", "-m", "invalid output path"]);
        f.git(&repo, &["tag", "v0.2.0"]);
        let error = f.fails(&["update", "be228981-2f39-55b3-b613-46b8d654dcaa"]);
        assert!(
            error.contains("be228981-2f39-55b3-b613-46b8d654dcaa: invalid manifest dist path"),
            "monorepo={mono}: {error}"
        );
        assert_eq!(
            f.dist("be228981-2f39-55b3-b613-46b8d654dcaa").as_deref(),
            Some("committed output")
        );
    }
}

#[test]
fn declared_output_update_and_rollback_use_each_commits_manifest() {
    for mono in [false, true] {
        let f = Fixture::new();
        let repo = output_release(&f, Some("output/vtf"), mono);
        let package = if mono {
            repo.join("packages/demo")
        } else {
            repo.clone()
        };
        f.ok(&["install", repo.to_str().unwrap()]);
        let path = package.join("plugin.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["dist"] = serde_json::json!("alternate");
        fs::write(&path, manifest.to_string()).unwrap();
        fs::create_dir_all(package.join("alternate")).unwrap();
        fs::write(package.join("alternate/index.js"), "new output").unwrap();
        f.ok(&["commit", repo.to_str().unwrap(), "0.2.0"]);
        let rewritten: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(rewritten["dist"], "alternate");
        assert_eq!(rewritten["buildCommand"], "touch BUILD_MUST_NOT_RUN");
        assert_eq!(rewritten["buildRequired"], true);
        f.ok(&["update", "be228981-2f39-55b3-b613-46b8d654dcaa"]);
        assert_eq!(
            f.dist("be228981-2f39-55b3-b613-46b8d654dcaa").as_deref(),
            Some("new output")
        );
        f.ok(&["rollback", "be228981-2f39-55b3-b613-46b8d654dcaa"]);
        assert_eq!(
            f.dist("be228981-2f39-55b3-b613-46b8d654dcaa").as_deref(),
            Some("committed output")
        );
        assert!(!package.join("BUILD_MUST_NOT_RUN").exists());
    }
}

#[test]
fn replacement_check_resolve_cli_and_removed_options() {
    let f = Fixture::new();
    f.original_and_fork();
    f.consumer();
    let before = f.state();
    let out = f.ok(&[
        "replace-check",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    let check: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(
        check["plan"]["provider"],
        "826a054b-0fad-5e5a-8890-f87c6452943b"
    );
    assert_eq!(check["issues"], serde_json::json!([]));
    assert_eq!(check["missing"], serde_json::json!({}));
    assert_eq!(before, f.state());
    assert!(
        f.ok(&[
            "replace-resolve",
            "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
            "--with",
            "826a054b-0fad-5e5a-8890-f87c6452943b"
        ])
        .is_empty()
    );
    assert_eq!(before, f.state());
    assert!(
        f.fails(&["replace", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "--back"])
            .contains("unknown option: --back")
    );
    assert!(
        f.fails(&[
            "replace",
            "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
            "--with",
            "826a054b-0fad-5e5a-8890-f87c6452943b",
            "--force"
        ])
        .contains("--force is only valid")
    );
    assert_eq!(before, f.state());
}

#[test]
fn replacement_missing_dependencies_suggest_explicit_resolution() {
    let f = Fixture::new();
    f.create(
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "new",
    );
    f.release(
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "dependency",
        "1.0.0",
    );
    f.ok(&[
        "catalog",
        "register",
        f.path("61cfe0c5-cc76-56a1-8a43-f9ca4faad534")
            .to_str()
            .unwrap(),
        "--ref",
        "HEAD",
    ]);
    f.original_and_fork();
    let path = f.path("826a054b-0fad-5e5a-8890-f87c6452943b/plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["dependencies"] = serde_json::json!({"61cfe0c5-cc76-56a1-8a43-f9ca4faad534": "^1"});
    fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    f.release("826a054b-0fad-5e5a-8890-f87c6452943b", "fork", "0.2.1");
    f.ok(&["update", "826a054b-0fad-5e5a-8890-f87c6452943b"]);
    f.ok(&[
        "uninstall",
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "--force",
    ]);
    let before = f.state();
    let error = f.fails(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert!(!error.contains("no catalog configured"), "{error}");
    assert!(
        error.contains(
            "826a054b-0fad-5e5a-8890-f87c6452943b needs 61cfe0c5-cc76-56a1-8a43-f9ca4faad534 ^1"
        ),
        "{error}"
    );
    assert!(
        error.contains("61cfe0c5-cc76-56a1-8a43-f9ca4faad534 1.0.0"),
        "{error}"
    );
    assert!(
        error.contains("gitpm replace-check dd3387ed-a5a3-5059-81ad-b1f6987a28f2 --with 826a054b-0fad-5e5a-8890-f87c6452943b"),
        "{error}"
    );
    assert!(
        error.contains("gitpm replace-resolve dd3387ed-a5a3-5059-81ad-b1f6987a28f2 --with 826a054b-0fad-5e5a-8890-f87c6452943b"),
        "{error}"
    );
    assert_eq!(before, f.state());
    assert!(f.dist("61cfe0c5-cc76-56a1-8a43-f9ca4faad534").is_none());
    f.ok(&[
        "replace-resolve",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    // Resolution cannot change an incompatible installed version.
    f.ok(&[
        "uninstall",
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "--force",
    ]);
    f.release(
        "61cfe0c5-cc76-56a1-8a43-f9ca4faad534",
        "incompatible dependency",
        "2.0.0",
    );
    f.ok(&[
        "install",
        f.path("61cfe0c5-cc76-56a1-8a43-f9ca4faad534")
            .to_str()
            .unwrap(),
    ]);
    let before = f.state();
    let error = f.fails(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert!(error.contains("Error kind: dependency_conflict"), "{error}");
    assert!(error.contains("shows dependency issues"), "{error}");
    assert!(
        error.contains("installs missing dependencies when resolvable"),
        "{error}"
    );
    assert!(
        error.contains("fix incompatible or disabled installed dependencies first"),
        "{error}"
    );
    f.fails(&[
        "replace-resolve",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "826a054b-0fad-5e5a-8890-f87c6452943b",
    ]);
    assert_eq!(before, f.state());
}

#[test]
fn replacing_original_with_itself_without_binding_reports_no_change() {
    let f = Fixture::new();
    f.original_and_fork();
    let before = f.state();
    let out = f.ok(&[
        "replace",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
        "--with",
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2",
    ]);
    assert_eq!(
        out.trim(),
        "dd3387ed-a5a3-5059-81ad-b1f6987a28f2 already uses original 0.1.1"
    );
    assert_eq!(before, f.state());
    assert_eq!(
        f.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").as_deref(),
        Some("vendor 1")
    );
}
