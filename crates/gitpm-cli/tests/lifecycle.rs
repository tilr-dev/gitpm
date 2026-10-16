//! End-to-end tests of the `gitpm` binary on local Git repositories and catalogs.
//! Hermetic: each test runs the CLI with a cleared environment, a fixture HOME/TMPDIR
//! and Git config, and every PM_* path inside its own temp directory.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    repo: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        fs::create_dir(dir.join("tmp")).unwrap();
        fs::create_dir(dir.join("home")).unwrap();
        fs::write(
            dir.join("home/.gitconfig"),
            "[user]\n\tname = Test\n\temail = test@example.invalid\n[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n",
        )
        .unwrap();
        let f = Self {
            repo: dir.join("upstream"),
            root: dir.join("workspace"),
            dir,
        };
        fs::create_dir(&f.repo).unwrap();
        f.git(&["init", "-b", "main"]);
        f.git(&["config", "user.name", "Test"]);
        f.git(&["config", "user.email", "test@example.invalid"]);
        f.release("1.0.0", "hello v1");
        f
    }
    /// Process with a hermetic environment: nothing from the developer's shell
    /// (PM_* exported by .env, global/system Git config, TERM, NO_COLOR) reaches it.
    /// Only PATH, a fixture HOME/TMPDIR with its own Git config and what the test
    /// sets explicitly.
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
    /// The CLI under test, with every PM_* path inside the fixture.
    fn pm(&self) -> Command {
        let mut command = self.command(env!("CARGO_BIN_EXE_gitpm"));
        command
            .env("PM_ROOT", &self.root)
            .env("PM_CONFIG", self.dir.join("pm.json"))
            .env("PM_SOURCES", self.dir.join("sources.txt"))
            .env("PM_CATALOG", self.dir.join("catalog"));
        command
    }
    fn git(&self, args: &[&str]) {
        let out = self
            .command("git")
            .arg("-C")
            .arg(&self.repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    fn release(&self, version: &str, contents: &str) {
        fs::create_dir_all(self.repo.join("dist")).unwrap();
        fs::write(
            self.repo.join("plugin.json"),
            format!(r#"{{"id":"hello","version":"{version}"}}"#),
        )
        .unwrap();
        fs::write(self.repo.join("dist/index.js"), contents).unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "-m", version]);
        self.git(&["tag", "-f", &format!("v{version}")]);
    }
    fn cli(&self, args: &[&str], success: bool) -> String {
        let out = self
            .pm()
            .current_dir(&self.dir)
            .arg("--root")
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            success,
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }
    fn install(&self) {
        self.cli(&["install", self.repo.to_str().unwrap()], true);
    }
    fn state(&self) -> serde_json::Value {
        serde_json::from_str(&self.cli(&["list"], true)).unwrap()
    }
    fn content(&self) -> String {
        fs::read_to_string(self.root.join("dist/hello/index.js")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("PM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

#[test]
fn lifecycle_and_three_removal_levels() {
    let f = Fixture::new();
    f.install();
    assert_eq!(f.content(), "hello v1");
    f.release("2.0.0", "hello v2");
    f.cli(&["update", "hello"], true);
    assert_eq!(f.content(), "hello v2");
    assert_eq!(f.state()["plugins"]["hello"]["current"]["version"], "2.0.0");
    f.cli(&["rollback", "hello"], true);
    assert_eq!(f.content(), "hello v1");
    f.cli(&["update", "hello"], true);
    assert_eq!(f.content(), "hello v2");
    f.cli(&["disable", "hello"], true);
    assert_eq!(f.state()["plugins"]["hello"]["enabled"], false);
    assert!(f.root.join("dist/hello").exists());
    f.cli(&["enable", "hello"], true);
    f.cli(&["forget", "hello"], true);
    assert!(f.state()["plugins"].as_object().unwrap().is_empty());
    assert!(f.root.join("plugins/hello/.git").exists());
    assert!(f.root.join("dist/hello").exists());
    f.cli(&["restore", "hello"], true);
    assert_eq!(f.state()["plugins"]["hello"]["enabled"], true);
    f.cli(&["uninstall", "hello"], true);
    assert!(!f.root.join("plugins/hello").exists());
    assert!(!f.root.join("dist/hello").exists());
}
#[test]
fn failed_update_keeps_files_and_state() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    f.release("1.0.0", "changed without version bump");
    f.cli(&["update", "hello"], false);
    assert_eq!(f.state(), before);
    assert_eq!(f.content(), "hello v1");
    f.release("2.0.0", "v2");
    fs::write(
        f.repo.join("plugin.json"),
        r#"{"id":"hello","version":"3.0.0","dependencies":{"missing":"1"}}"#,
    )
    .unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "dependency"]);
    f.git(&["tag", "v3.0.0"]);
    f.cli(&["update", "hello"], false);
    assert_eq!(f.state(), before);
    assert_eq!(f.content(), "hello v1");
}
#[test]
fn protects_local_edits_and_force_is_explicit() {
    let f = Fixture::new();
    f.install();
    fs::write(f.root.join("plugins/hello/my-work.txt"), "keep me").unwrap();
    f.cli(&["update", "hello"], false);
    f.cli(&["uninstall", "hello"], false);
    assert!(f.root.join("plugins/hello/my-work.txt").exists());
    f.cli(&["uninstall", "hello", "--force"], true);
    assert!(!f.root.join("plugins/hello").exists());
}
#[test]
fn rejects_duplicate_path_escape_and_missing_payload() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    f.cli(&["install", f.repo.to_str().unwrap()], false);
    f.cli(&["uninstall", "../upstream", "--force"], false);
    assert!(f.repo.exists());
    assert_eq!(f.state(), before);
    f.cli(&["uninstall", "hello"], true);
    fs::remove_dir_all(f.repo.join("dist")).unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "missing dist"]);
    f.cli(
        &["install", f.repo.to_str().unwrap(), "--channel", "dev"],
        false,
    );
    assert!(f.state()["plugins"].as_object().unwrap().is_empty());
}
#[test]
fn recovery_and_lock() {
    let f = Fixture::new();
    f.install();
    let manager = gitpm::Manager::open(&f.root).unwrap();
    assert!(gitpm::Manager::open(&f.root).is_err());
    drop(manager);
    // An interrupted, uncommitted transaction is discarded on the next run.
    fs::create_dir_all(f.root.join(".pm-txn/new/dist/hello")).unwrap();
    fs::write(f.root.join(".pm-txn/new/dist/hello/index.js"), "half").unwrap();
    assert_eq!(f.content_after_recovery(), "hello v1");
    assert!(!f.root.join(".pm-txn").exists());
}
impl Fixture {
    fn content_after_recovery(&self) -> String {
        self.cli(&["list"], true);
        self.content()
    }
}

#[test]
fn direct_git_refs_and_local_author_commit() {
    let f = Fixture::new();
    f.git(&["tag", "-f", "v1.0.0"]);
    let repo = f.repo.to_str().unwrap();
    f.cli(&["install", repo, "--ref", "v1.0.0"], true);
    assert!(f.state()["plugins"]["hello"].get("pinned").is_none());
    fs::write(f.repo.join("dist/index.js"), "author release").unwrap();
    f.cli(&["commit", repo, "1.0.1"], true);
    f.cli(&["commit", repo, "1.0.1"], false);
    f.cli(&["update", "hello"], true);
    assert_eq!(f.content(), "author release");
    f.cli(&["update", "hello", "--ref", "v1.0.1"], true);
    assert_eq!(f.content(), "author release");
    let sha = f.state()["plugins"]["hello"]["current"]["commit"]
        .as_str()
        .unwrap()
        .to_owned();
    f.cli(&["rollback", "hello"], true);
    assert_eq!(f.content(), "hello v1");
    f.cli(&["uninstall", "hello"], true);
    f.cli(&["install", repo, "--ref", &sha], true);
    assert_eq!(f.content(), "author release");
}

#[test]
fn catalog_manifest_is_verified_before_install() {
    let f = Fixture::new();
    f.git(&["tag", "-f", "v1.0.0"]);
    let manager = gitpm::Manager::open(&f.root).unwrap();
    let wrong = gitpm::Manifest {
        id: "hello".into(),
        version: "99.0.0".into(),
        ..Default::default()
    };
    assert!(
        manager
            .install_ref(f.repo.to_str().unwrap(), Some("v1.0.0"), Some(&wrong))
            .is_err()
    );
    assert!(manager.state().unwrap().plugins.is_empty());
    assert!(!f.root.join("dist/hello").exists());
}

#[test]
fn explicit_same_commit_and_numeric_versions_sort_correctly() {
    let f = Fixture::new();
    f.install();
    f.git(&["tag", "-f", "v1.0.0"]);
    f.cli(&["update", "hello", "--ref", "v1.0.0"], true);
    assert!(f.state()["plugins"]["hello"].get("pinned").is_none());
    use gitpm::catalog::version_key;
    assert!(version_key("1.10.0").unwrap() > version_key("1.9.0").unwrap());
    for invalid in ["01.0.0", "v1.0.0", "1.0", "1.0.0-dev", "-1.0.0"] {
        assert!(version_key(invalid).is_err());
    }
}

#[test]
fn interactive_init_reprompts_and_creates_installable_repository() {
    use std::io::Write;
    use std::process::Stdio;
    let f = Fixture::new();
    let mut child = f
        .pm()
        .current_dir(&f.dir)
        .arg("init")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\n../escape\nmy-plugin\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = f.dir.join("my-plugin");
    assert!(source.join(".git").is_dir());
    assert!(!f.dir.join("gitpm-workspace").exists());
    f.cli(
        &[
            "install",
            source.to_str().unwrap(),
            "--ref",
            "local/my-plugin@0.1.0",
        ],
        true,
    );
    assert_eq!(
        f.state()["plugins"]["local/my-plugin"]["current"]["version"],
        "0.1.0"
    );
    assert!(f.root.join("dist/local/my-plugin/index.js").exists());
}

#[test]
fn interactive_create_preserves_existing_path_and_handles_eof() {
    use std::io::Write;
    use std::process::Stdio;
    let f = Fixture::new();
    fs::create_dir(f.dir.join("existing")).unwrap();
    fs::write(f.dir.join("existing/keep.txt"), "untouched").unwrap();
    for input in ["existing\n", ""] {
        let mut child = f
            .pm()
            .current_dir(&f.dir)
            .arg("create")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        assert!(!child.wait_with_output().unwrap().status.success());
    }
    assert_eq!(
        fs::read_to_string(f.dir.join("existing/keep.txt")).unwrap(),
        "untouched"
    );
    assert!(!f.dir.join("existing/.git").exists());
}

#[test]
fn create_at_nested_and_absolute_paths_without_prompting() {
    use std::process::Stdio;
    let f = Fixture::new();
    for (command, path) in [
        ("init", "demo/demo1".to_owned()),
        (
            "create",
            f.dir
                .join("other folder/demo2")
                .to_string_lossy()
                .into_owned(),
        ),
    ] {
        let output = f
            .pm()
            .current_dir(&f.dir)
            .args([command, &path])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let repo = f.dir.join(&path);
        let m: serde_json::Value =
            serde_json::from_slice(&fs::read(repo.join("plugin.json")).unwrap()).unwrap();
        assert_eq!(
            m["id"].as_str().unwrap(),
            format!("local/{}", repo.file_name().unwrap().to_str().unwrap())
        );
        assert!(repo.join(".git").is_dir());
        f.cli(
            &[
                "install",
                repo.to_str().unwrap(),
                "--ref",
                &format!("{}@0.1.0", m["id"].as_str().unwrap()),
            ],
            true,
        );
        let retry = f
            .pm()
            .current_dir(&f.dir)
            .args([command, &path])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!retry.status.success());
        assert!(repo.join(".git").is_dir());
    }
}

#[test]
fn fork_preserves_history_edits_and_original_installation() {
    let f = Fixture::new();
    f.install();
    let installed = f.root.join("plugins/hello");
    fs::write(installed.join("dist/index.js"), "local edit").unwrap();
    fs::write(installed.join("notes.txt"), "untracked work").unwrap();
    let state = fs::read(f.root.join("state.json")).unwrap();
    let destination = f.dir.join("authors/my-hello");
    f.cli(&["fork", "hello", destination.to_str().unwrap()], true);
    let git_at = |path: &std::path::Path, args: &[&str]| {
        let out = f
            .command("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(
        git_at(&installed, &["rev-parse", "HEAD"]),
        git_at(&destination, &["rev-parse", "HEAD^"])
    );
    assert_eq!(git_at(&destination, &["branch", "--show-current"]), "fork");
    assert_eq!(git_at(&destination, &["remote"]), "upstream");
    assert_eq!(
        git_at(&destination, &["remote", "get-url", "upstream"]),
        fs::canonicalize(&f.repo).unwrap().to_string_lossy()
    );
    assert_eq!(
        fs::read_to_string(destination.join("dist/index.js")).unwrap(),
        "local edit"
    );
    assert_eq!(
        fs::read_to_string(destination.join("notes.txt")).unwrap(),
        "untracked work"
    );
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    assert_eq!(f.content(), "hello v1");
    fs::write(destination.join("dist/index.js"), "independent edit").unwrap();
    assert_eq!(
        fs::read_to_string(installed.join("dist/index.js")).unwrap(),
        "local edit"
    );
    f.cli(&["fork", "hello", destination.to_str().unwrap()], false);
    assert_eq!(
        fs::read_to_string(destination.join("dist/index.js")).unwrap(),
        "independent edit"
    );
}

#[test]
fn fork_rejects_managed_destinations_and_unknown_plugins() {
    let f = Fixture::new();
    f.install();
    let nested = f.root.join("plugins/hello/nested/fork");
    f.cli(&["fork", "hello", nested.to_str().unwrap()], false);
    assert!(!f.root.join("plugins/hello/nested").exists());
    let outside = f.dir.join("outside");
    f.cli(&["fork", "unknown", outside.to_str().unwrap()], false);
    assert!(!outside.exists());
}

#[test]
fn namespaced_fork_and_original_coexist_and_release_independently() {
    let f = Fixture::new();
    f.cli(&["namespace", "vendor"], true);
    assert_eq!(f.cli(&["namespace"], true).trim(), "vendor");
    let source = f.dir.join("author/demo");
    f.cli(&["init", source.to_str().unwrap()], true);
    f.cli(&["install", source.to_str().unwrap()], true);
    assert!(f.root.join("dist/vendor/demo/index.js").exists());
    let fork = f.dir.join("forks/different-directory-name");
    f.cli(&["fork", "vendor/demo", fork.to_str().unwrap()], false); // same namespace
    f.cli(
        &[
            "fork",
            "vendor/demo",
            fork.to_str().unwrap(),
            "--namespace",
            "dan",
        ],
        true,
    );
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(fork.join("plugin.json")).unwrap()).unwrap();
    assert_eq!(m["id"], "dan/demo");
    assert_eq!(m["forkOf"]["id"], "vendor/demo");
    assert_eq!(
        m["forkOf"]["commit"],
        f.state()["plugins"]["vendor/demo"]["current"]["commit"]
    );
    f.cli(
        &["install", fork.to_str().unwrap(), "--channel", "dev"],
        true,
    );
    assert!(f.root.join("dist/dan/demo/index.js").exists());
    assert_eq!(f.state()["plugins"]["dan/demo"]["forkOf"], m["forkOf"]);
    fs::write(fork.join("dist/index.js"), "fork release").unwrap();
    f.cli(&["commit", fork.to_str().unwrap(), "0.1.1"], true);
    f.cli(&["update", "dan/demo", "--ref", "dan/demo@0.1.1"], true);
    assert_eq!(
        fs::read_to_string(f.root.join("dist/dan/demo/index.js")).unwrap(),
        "fork release"
    );
    f.cli(&["rollback", "dan/demo"], true);
    f.cli(&["uninstall", "dan", "--force"], false); // namespace is not a plugin
    assert!(f.root.join("plugins/dan/demo/.git").exists());
    f.cli(&["uninstall", "dan/demo"], true);
    assert!(f.root.join("dist/vendor/demo/index.js").exists());
    assert_eq!(f.cli(&["namespace"], true).trim(), "vendor"); // override not persisted
}

#[test]
fn namespaces_reject_path_traversal_and_legacy_prefix_collisions() {
    let f = Fixture::new();
    f.install();
    f.cli(&["namespace", "../bad"], false);
    for id in ["../hello", "a/../hello", "/hello", "a/b/c", "a//b"] {
        f.cli(&["uninstall", id, "--force"], false);
    }
    let source = f.dir.join("author/demo");
    f.cli(
        &["init", source.to_str().unwrap(), "--namespace", "hello"],
        true,
    );
    f.cli(&["install", source.to_str().unwrap()], false);
    assert!(f.root.join("plugins/hello/.git").exists());
}

#[test]
fn local_catalog_register_read_install_and_immutable_versions() {
    let f = Fixture::new();
    let catalog_dir = f.dir.join("local-catalog");
    let location = catalog_dir.to_str().unwrap();
    let repo = f.repo.to_str().unwrap();
    f.cli(
        &["--catalog", location, "register", repo, "--ref", "HEAD"],
        true,
    );
    let file = catalog_dir.join("catalog.json");
    let original = fs::read(&file).unwrap();
    assert!(
        f.cli(&["--catalog", location, "catalog"], true)
            .contains("legacy")
    );
    f.cli(
        &["--catalog", location, "register", repo, "--ref", "HEAD"],
        true,
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    f.cli(
        &[
            "--catalog",
            file.to_str().unwrap(),
            "install-catalog",
            "hello",
        ],
        true,
    );
    assert!(
        f.cli(&["--catalog", location, "check"], true)
            .contains("hello")
    );
    fs::write(
        f.repo.join("dist/index.js"),
        "changed without a version bump",
    )
    .unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "same version different commit"]);
    f.cli(
        &["--catalog", location, "register", repo, "--ref", "HEAD"],
        false,
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::write(&file, "not JSON").unwrap();
    f.cli(
        &["--catalog", location, "register", repo, "--ref", "HEAD"],
        false,
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "not JSON");
    f.cli(
        &[
            "--catalog",
            "https://example.invalid/catalog",
            "register",
            repo,
            "--ref",
            "HEAD",
        ],
        false,
    );
}

#[test]
fn split_catalog_hashes_and_direct_lookup_are_independent_of_directory_indexes() {
    use gitpm::{Manager, catalog};
    let f = Fixture::new();
    fs::write(
        f.repo.join("plugin.json"),
        r#"{"id":"dan/hello","version":"1.0.0"}"#,
    )
    .unwrap();
    fs::write(f.repo.join("dist/index.js"), "abc").unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "scoped"]);
    let dir = f.dir.join("catalog");
    let location = dir.to_str().unwrap();
    let release = catalog::register_build(
        location,
        f.repo.to_str().unwrap(),
        "HEAD",
        Some(catalog::Build {
            url: "https://example.invalid/builds/hello.tar.zst".into(),
            sha256: "a".repeat(64),
        }),
    )
    .unwrap();
    assert_eq!(
        release.files["index.js"].sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(release.files["index.js"].size, 3);
    let root = fs::read(dir.join("catalog.json")).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&root).unwrap();
    assert_eq!(value["directories"]["namespaces"], "namespaces/");
    assert!(value.get("plugins").is_none());
    assert!(
        dir.join("namespaces/dan/hello/versions/1.0.0/release.json")
            .exists()
    );
    catalog::register_build(
        location,
        f.repo.to_str().unwrap(),
        "HEAD",
        release.build.clone(),
    )
    .unwrap();
    assert_eq!(fs::read(dir.join("catalog.json")).unwrap(), root);
    fs::remove_file(dir.join("catalog.json")).unwrap();
    fs::remove_file(dir.join("namespaces/dan/catalog.json")).unwrap();
    let found = catalog::lookup(location, "dan/hello", None).unwrap();
    assert_eq!(found.build.as_ref().unwrap().sha256, "a".repeat(64));
    let manager = Manager::open(&f.root).unwrap();
    let mut tampered = found.clone();
    tampered.files.get_mut("index.js").unwrap().sha256 = "0".repeat(64);
    assert!(
        manager
            .install_release(&tampered)
            .unwrap_err()
            .to_string()
            .contains("hashes")
    );
    assert!(manager.state().unwrap().plugins.is_empty());
    manager.install_release(&found).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("dist/dan/hello/index.js")).unwrap(),
        "abc"
    );
    let selected = catalog::selected(location, &manager.state().unwrap()).unwrap();
    assert_eq!(selected.plugins.len(), 1);
    let index_path = dir.join("namespaces/dan/hello/plugin.json");
    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
    index["versions"]["1.0.0"] = "../../escape.json".into();
    fs::write(index_path, serde_json::to_vec(&index).unwrap()).unwrap();
    assert!(catalog::lookup(location, "dan/hello", None).is_err());
    assert!(catalog::lookup(location, "../escape", None).is_err());
}

#[test]
fn migration_preserves_legacy_releases_and_backup() {
    use gitpm::catalog;
    let f = Fixture::new();
    let first = f.dir.join("first");
    let mut release =
        catalog::register(first.to_str().unwrap(), f.repo.to_str().unwrap(), "HEAD").unwrap();
    release.files.clear();
    let old = serde_json::json!({"plugins": {"hello": {"source": release.source, "versions": {"1.0.0": release}}}});
    let dir = f.dir.join("old");
    fs::create_dir(&dir).unwrap();
    let original = serde_json::to_vec(&old).unwrap();
    fs::write(dir.join("catalog.json"), &original).unwrap();
    let location = dir.to_str().unwrap();
    let before = catalog::lookup(location, "hello", None).unwrap();
    catalog::migrate(location).unwrap();
    let after = catalog::lookup(location, "hello", None).unwrap();
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(after).unwrap()
    );
    assert_eq!(fs::read(dir.join(".catalog-v1.json")).unwrap(), original);
    catalog::migrate(location).unwrap();
    let retry = catalog::register(location, f.repo.to_str().unwrap(), "HEAD").unwrap();
    assert!(retry.files.is_empty());
    f.cli(&["--catalog", location, "migrate-catalog"], true);
    assert_eq!(fs::read(dir.join(".catalog-v1.json")).unwrap(), original);
}

#[test]
fn invalid_build_metadata_does_not_publish_release() {
    use gitpm::catalog;
    let f = Fixture::new();
    let dir = f.dir.join("catalog");
    let result = catalog::register_build(
        dir.to_str().unwrap(),
        f.repo.to_str().unwrap(),
        "HEAD",
        Some(catalog::Build {
            url: "file:///tmp/build".into(),
            sha256: "a".repeat(64),
        }),
    );
    assert!(result.is_err());
    assert!(
        !dir.join("legacy/hello/versions/1.0.0/release.json")
            .exists()
    );
}

#[test]
fn stable_install_and_update_ignore_untagged_head_and_sort_versions() {
    let f = Fixture::new();
    f.release("1.9.0", "nine");
    f.release("1.10.0", "ten");
    f.git(&["tag", "-d", "v1.10.0"]);
    f.git(&["tag", "-a", "v1.10.0", "-m", "annotated release"]);
    // A newer-looking version on the branch is not a release.
    fs::write(
        f.repo.join("plugin.json"),
        r#"{"id":"hello","version":"9.0.0"}"#,
    )
    .unwrap();
    fs::write(f.repo.join("dist/index.js"), "unreleased").unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "development"]);
    f.install();
    assert_eq!(f.content(), "ten");
    f.cli(&["update", "hello"], true);
    assert_eq!(f.content(), "ten");
    f.cli(&["update", "hello", "--channel", "dev"], true);
    assert_eq!(f.content(), "unreleased");
    let before = f.state();
    f.cli(&["update", "hello"], false); // no automatic downgrade
    assert_eq!(before, f.state());
    f.cli(
        &["update", "hello", "--channel", "dev", "--ref", "HEAD"],
        false,
    );
}

#[test]
fn catalog_update_generates_all_tags_and_rejects_moved_releases_without_partial_write() {
    use gitpm::catalog;
    let f = Fixture::new();
    let catalog_dir = f.dir.join("generated");
    let location = catalog_dir.to_str().unwrap();
    // Paths are relative to the list file, and duplicate lines are harmless.
    let sources = f.dir.join("sources.txt");
    fs::write(&sources, "# Git sources\n\n./upstream\n./upstream\n").unwrap();
    f.cli(&["--catalog", location, "catalog-update"], true);
    let first = catalog::lookup(location, "hello", None).unwrap();
    assert_eq!(first.manifest.version, "1.0.0");
    assert!(!first.files.is_empty());
    let original_root = fs::read(catalog_dir.join("catalog.json")).unwrap();
    f.release("1.10.0", "ten");
    f.cli(
        &[
            "--catalog",
            location,
            "catalog-update",
            "--sources",
            sources.to_str().unwrap(),
        ],
        true,
    );
    assert_eq!(
        catalog::lookup(location, "hello", None)
            .unwrap()
            .manifest
            .version,
        "1.10.0"
    );
    assert_eq!(
        fs::read(catalog_dir.join("catalog.json")).unwrap(),
        original_root
    );
    let index = catalog_dir.join("legacy/hello/plugin.json");
    let original_index = fs::read(&index).unwrap();
    f.cli(&["--catalog", location, "catalog-update"], true);
    assert_eq!(fs::read(&index).unwrap(), original_index);
    f.release("2.0.0", "two");
    // Move an old tag to a different commit with the same manifest version.
    f.release("1.0.0", "replacement");
    f.cli(&["--catalog", location, "catalog-update"], false);
    assert_eq!(fs::read(index).unwrap(), original_index);
    assert!(
        !catalog_dir
            .join("legacy/hello/versions/2.0.0/release.json")
            .exists()
    );
    assert_eq!(
        catalog::lookup(location, "hello", Some("1.0.0"))
            .unwrap()
            .commit,
        first.commit
    );
}

#[test]
fn catalog_update_validates_every_source_before_writing_and_retains_removed_sources() {
    let f = Fixture::new();
    let directory = f.dir.join("generated");
    let location = directory.to_str().unwrap();
    let sources = f.dir.join("sources.txt");
    let no_tags = f.dir.join("no-tags");
    f.cli(
        &["init", no_tags.to_str().unwrap(), "--namespace", "dan"],
        true,
    );
    assert!(
        f.command("git")
            .arg("-C")
            .arg(&no_tags)
            .args(["tag", "-d", "dan/no-tags@0.1.0"])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(&sources, "./upstream\n./no-tags\n").unwrap();
    f.cli(&["--catalog", location, "catalog-update"], false);
    assert!(!directory.exists());
    // Stable installs have no implicit fallback to the development branch.
    f.cli(&["install", no_tags.to_str().unwrap()], false);
    f.cli(
        &["install", no_tags.to_str().unwrap(), "--channel", "dev"],
        true,
    );
    fs::write(&sources, "./upstream\n").unwrap();
    f.cli(&["--catalog", location, "catalog-update"], true);
    let source2 = f.dir.join("second");
    f.cli(
        &["init", source2.to_str().unwrap(), "--namespace", "dan"],
        true,
    );
    fs::write(&sources, "./second\n").unwrap();
    f.cli(&["--catalog", location, "catalog-update"], true);
    assert!(directory.join("legacy/hello/plugin.json").exists());
    assert!(directory.join("namespaces/dan/second/plugin.json").exists());
}

#[test]
fn release_manifest_must_match_tag_and_update_rejects_retagging() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    f.release("1.0.0", "replacement");
    f.cli(&["update", "hello"], false);
    assert_eq!(f.state(), before);
    f.git(&["tag", "v8.0.0"]); // manifest still says 1.0.0
    let dir = f.dir.join("generated");
    fs::write(f.dir.join("sources.txt"), "./upstream\n").unwrap();
    f.cli(
        &["--catalog", dir.to_str().unwrap(), "catalog-update"],
        false,
    );
    assert!(!dir.exists());
}

#[test]
fn info_shows_directory_tree_and_legacy_catalog_without_installing() {
    use gitpm::catalog;
    use gitpm_cli::catalog::info;
    let f = Fixture::new();
    let dir = f.dir.join("catalog");
    let location = dir.to_str().unwrap();
    let release = catalog::register(location, f.repo.to_str().unwrap(), "HEAD").unwrap();
    let result = f.cli(&["--catalog", location, "info"], true);
    assert!(result.contains("Format: v2"));
    assert!(result.contains("legacy/"));
    assert!(result.contains("hello/"));
    assert!(result.contains("latest: 1.0.0, versions: 1"));
    assert!(result.contains(&release.source));
    assert!(!f.root.exists());
    let old = serde_json::json!({"plugins": {"hello": {"source": release.source, "versions": {"1.0.0": release}}}});
    fs::write(dir.join("catalog.json"), serde_json::to_vec(&old).unwrap()).unwrap();
    let result = info(location).unwrap();
    assert!(result.contains("Format: v1"));
    assert!(result.contains("hello — latest: 1.0.0"));
    fs::write(
        dir.join("catalog.json"),
        r#"{"schema":2,"directories":{"escape":"../"}}"#,
    )
    .unwrap();
    assert!(info(location).is_err());
    fs::write(
        dir.join("catalog.json"),
        r#"{"schema":2,"directories":{"missing":"missing/"}}"#,
    )
    .unwrap();
    assert!(info(location).is_err());
}

#[test]
fn auth_check_is_read_only_and_accepts_empty_repositories() {
    let f = Fixture::new();
    let before = fs::read(f.repo.join(".git/config")).unwrap();
    let out = f.cli(&["auth", "check", f.repo.to_str().unwrap()], true);
    assert!(out.contains("Read access confirmed"));
    assert!(!f.root.exists());
    assert_eq!(fs::read(f.repo.join(".git/config")).unwrap(), before);
    let empty = f.dir.join("empty.git");
    assert!(
        f.command("git")
            .args(["init", "--bare"])
            .arg(&empty)
            .output()
            .unwrap()
            .status
            .success()
    );
    f.cli(&["auth", "check", empty.to_str().unwrap()], true);
    f.cli(
        &["auth", "check", f.dir.join("missing.git").to_str().unwrap()],
        false,
    );
    f.cli(&["auth", "login", f.repo.to_str().unwrap()], false);
    f.cli(&["auth", "check"], false);
    assert!(!f.root.exists());
    let output = f
        .pm()
        .args([
            "auth",
            "check",
            "https://user:supersecret@example.com/repo.git",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("supersecret"));
}

#[test]
fn uninstall_without_tty_refuses_and_lists_lost_work_including_stash() {
    let f = Fixture::new();
    f.install();
    let repo = f.root.join("plugins/hello");
    let git = |args: &[&str]| {
        let out = f
            .command("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["config", "user.name", "Test"]);
    git(&["config", "user.email", "test@example.invalid"]);
    fs::write(repo.join("dist/index.js"), "stashed edit").unwrap();
    git(&["stash", "push", "-m", "wip"]);
    let before = f.state();
    let out = f
        .pm()
        .arg("--root")
        .arg(&f.root)
        .args(["uninstall", "hello"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("stash: stash@{0}"), "{stderr}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("[y/N]"));
    assert_eq!(f.state(), before);
    assert!(repo.join(".git").is_dir());
    f.cli(&["uninstall", "hello", "--force"], true);
    assert!(!repo.exists());
}

#[test]
fn uninstall_on_tty_asks_before_losing_work() {
    // A pseudo-terminal via script(1); skipped where it is unavailable.
    if !std::path::Path::new("/usr/bin/script").exists() {
        return;
    }
    let f = Fixture::new();
    f.install();
    let repo = f.root.join("plugins/hello");
    fs::write(repo.join("notes.txt"), "untracked work").unwrap();
    // BSD script(1) takes the command as arguments; util-linux needs -c.
    let util_linux = Command::new("script")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let script = if util_linux {
        "script -qec \"'$0' --root '$1' uninstall hello\" /dev/null"
    } else {
        "script -q /dev/null \"$0\" --root \"$1\" uninstall hello"
    };
    let ask = |answer: &str| {
        let out = f
            .command("sh")
            .arg("-c")
            .arg(format!(
                "(sleep 1; printf '{answer}\\n'; sleep 1) | {script}"
            ))
            .arg(env!("CARGO_BIN_EXE_gitpm"))
            .arg(&f.root)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let output = ask("n");
    assert!(output.contains("Delete anyway? [y/N]"), "{output}");
    assert!(output.contains("untracked: notes.txt"), "{output}");
    assert!(repo.join("notes.txt").exists());
    assert_eq!(f.state()["plugins"]["hello"]["enabled"], true);
    let output = ask("y");
    assert!(output.contains("OK: uninstall hello"), "{output}");
    assert!(!repo.exists());
    assert!(f.state()["plugins"].as_object().unwrap().is_empty());
}

#[test]
fn usage_errors_name_the_problem_and_create_nothing() {
    let f = Fixture::new();
    for (args, message) in [
        (&["bogus", "qa/demo"][..], "unknown command: bogus"),
        (&["list", "--bogus"][..], "unknown option: --bogus"),
        (
            &["update", "qa/demo", "--ref", "a", "--ref", "b"][..],
            "--ref given more than once",
        ),
        (
            &["uninstall", "qa/demo", "--force", "--force"][..],
            "--force given more than once",
        ),
        (&["update"][..], "update: expected 1 argument(s), got 0"),
        (
            &["rollback", "a/b", "c"][..],
            "rollback: expected 1 argument(s), got 2",
        ),
        (&["uninstall", "bad id"][..], "namespace/name"),
        (
            &["install-catalog", "qa/demo", "--version", "1.x"][..],
            "1.x",
        ),
        (&["namespace", ""][..], "namespace/name"),
    ] {
        let out = f
            .pm()
            .current_dir(&f.dir)
            .arg("--root")
            .arg(&f.root)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} succeeded");
        assert!(stderr.contains(message), "{args:?}: {stderr}");
        assert!(!f.root.exists(), "{args:?} created the workspace");
        assert!(!f.dir.join("pm.json").exists(), "{args:?} wrote config");
    }
}

#[test]
fn catalog_commands_name_the_missing_catalog_and_create_nothing() {
    let f = Fixture::new();
    for args in [
        &["info"][..],
        &["catalog"][..],
        &["catalog-update"][..],
        &["migrate-catalog"][..],
        &["register", "x", "--ref", "HEAD"][..],
        &["install-catalog", "qa/demo"][..],
    ] {
        let out = f
            .pm()
            .env_remove("PM_CATALOG")
            .current_dir(&f.dir)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} succeeded");
        assert!(
            stderr.contains("needs a catalog: pass --catalog"),
            "{args:?}: {stderr}"
        );
        assert!(!f.root.exists(), "{args:?} created the workspace");
    }
    // A direct Git install, list and check of an empty workspace need no catalog.
    let out = f
        .pm()
        .env_remove("PM_CATALOG")
        .args(["check"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = f
        .pm()
        .env_remove("PM_CATALOG")
        .args(["install", f.repo.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(f.content(), "hello v1");
}

#[test]
fn defaults_are_relative_to_the_current_directory() {
    let f = Fixture::new();
    let out = f
        .command(env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(&f.dir)
        .args(["install", "./upstream"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.dir.join("gitpm-workspace/dist/hello/index.js").exists());
    let out = f
        .command(env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(&f.dir)
        .args(["namespace", "me"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.dir.join(".gitpm.json").exists());
}

#[test]
fn help_names_the_binary_and_removed_commands_are_unknown() {
    let f = Fixture::new();
    let help = f.cli(&["--help"], true);
    assert!(help.starts_with("gitpm "));
    assert!(!help.contains("sync"));
    assert!(!help.contains("--publish"));
    for args in [
        &["sync"][..],
        &["catalog-update", "--publish"][..],
        &["sync", "--dry-run"][..],
    ] {
        let out = f.pm().args(args).output().unwrap();
        assert!(!out.status.success(), "{args:?} succeeded");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("unknown"), "{args:?}: {stderr}");
    }
    assert!(!f.root.exists());
}
