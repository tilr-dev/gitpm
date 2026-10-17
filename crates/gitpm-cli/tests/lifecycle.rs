//! End-to-end tests of the `gitpm` binary on local Git repositories and catalogs.
//! Hermetic: each test runs the CLI with a cleared environment, a fixture HOME/TMPDIR
//! and Git config, and every GITPM_* path inside its own temp directory.
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
    /// (GITPM_*/PM_* exported by .env, global/system Git config, TERM, NO_COLOR) reaches it.
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
    /// The CLI under test, with every GITPM_* path inside the fixture.
    fn pm(&self) -> Command {
        let mut command = self.command(env!("CARGO_BIN_EXE_gitpm"));
        command
            .env("GITPM_ROOT", &self.root)
            .env("GITPM_SOURCES", self.dir.join("sources.txt"))
            .env("GITPM_CATALOG", self.dir.join("catalog"));
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
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
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

/// Every `.gitpm.json` / `pm.json` under `dir`: the CLI has no config file.
fn config_files(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name == ".gitpm.json" || name == "pm.json" || name == ".pm.json" {
            found.push(path.clone());
        }
        if path.is_dir() && !path.is_symlink() {
            found.extend(config_files(&path));
        }
    }
    found
}

#[test]
fn create_and_init_take_an_explicit_id_and_repository_folder() {
    use std::process::Stdio;
    let f = Fixture::new();
    for (command, id) in [("create", "dan/hello"), ("init", "vendor/demo1")] {
        let output = f
            .pm()
            .current_dir(&f.dir)
            .args([command, id, "--into", id])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(&format!("ID: {id}\n")), "{stdout}");
        let repo = f.dir.join(id);
        let m: serde_json::Value =
            serde_json::from_slice(&fs::read(repo.join("plugin.json")).unwrap()).unwrap();
        assert_eq!(m["id"], id);
        assert!(repo.join(".git").is_dir());
        f.cli(
            &[
                "install",
                repo.to_str().unwrap(),
                "--ref",
                &format!("{id}@0.1.0"),
            ],
            true,
        );
        assert_eq!(f.state()["plugins"][id]["current"]["version"], "0.1.0");
        // Never overwrites: the same folder again fails and keeps the repository.
        let retry = f
            .pm()
            .current_dir(&f.dir)
            .args([command, id, "--into", id])
            .output()
            .unwrap();
        assert!(!retry.status.success());
        assert!(repo.join(".git").is_dir());
    }
    assert!(
        config_files(&f.dir).is_empty(),
        "{:?}",
        config_files(&f.dir)
    );
}

#[test]
fn create_requires_a_namespace_and_never_prompts_or_reads_the_environment() {
    use std::process::Stdio;
    let f = Fixture::new();
    fs::create_dir_all(f.dir.join("dan/existing")).unwrap();
    fs::write(f.dir.join("dan/existing/keep.txt"), "untouched").unwrap();
    for (args, message) in [
        (
            &["create", "hello", "--into", "hello"][..],
            "plugin id needs a namespace: use NAMESPACE/NAME, for example acme/hello",
        ),
        (
            &["init", "hello", "--into", "hello"][..],
            "plugin id needs a namespace",
        ),
        (&["create"][..], "create: expected 1 argument(s), got 0"),
        (&["init"][..], "init: expected 1 argument(s), got 0"),
        (&["create", "dan/a/b", "--into", "x"][..], "namespace/name"),
        (
            &["create", "../escape", "--into", "x"][..],
            "namespace/name",
        ),
        (&["create", "/hello", "--into", "x"][..], "namespace/name"),
        (&["create", "dan/", "--into", "x"][..], "namespace/name"),
        (
            &["create", "dan/hello", "--namespace", "me"][..],
            "unknown option: --namespace",
        ),
        // No default folder: the repository is always given.
        (&["create", "dan/hello"][..], "create needs --into DIR"),
        (&["init", "dan/hello"][..], "init needs --into DIR"),
        (
            &["create", "dan/existing", "--into", "dan/existing"][..],
            "is not a Git repository",
        ),
    ] {
        // Legacy and removed namespace variables must not supply a default.
        let mut child = f
            .pm()
            .env("GITPM_NAMESPACE", "me")
            .env("PM_NAMESPACE", "me")
            .current_dir(&f.dir)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        // Input is never read: no interactive name prompt.
        use std::io::Write;
        let _ = child.stdin.take().unwrap().write_all(b"me/typed\n");
        let out = child.wait_with_output().unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} succeeded");
        assert!(stderr.contains(message), "{args:?}: {stderr}");
    }
    assert!(!f.dir.join("hello").exists() && !f.dir.join("x").exists());
    assert!(!f.dir.join("dan/hello").exists());
    assert!(!f.dir.join("me").exists());
    assert_eq!(
        fs::read_to_string(f.dir.join("dan/existing/keep.txt")).unwrap(),
        "untouched"
    );
    assert!(!f.dir.join("dan/existing/.git").exists());
    assert!(!f.root.exists());
    assert!(config_files(&f.dir).is_empty());
}

#[test]
fn fork_preserves_history_edits_and_original_installation() {
    let f = Fixture::new();
    f.install();
    let installed = f.root.join("plugins/hello");
    fs::write(installed.join("dist/index.js"), "local edit").unwrap();
    fs::write(installed.join("notes.txt"), "untracked work").unwrap();
    let state = fs::read(f.root.join("state.json")).unwrap();
    let destination = f.dir.join("me/hello");
    let out = f.cli(&["fork", "hello", "--ns", "me", "--into", "me/hello"], true);
    assert!(
        out.contains("Forked: hello → me/hello (repository root)"),
        "{out}"
    );
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
    // An existing folder that is not a multi-package repository is refused.
    f.cli(
        &["fork", "hello", "--ns", "me", "--into", "me/hello"],
        false,
    );
    assert_eq!(
        fs::read_to_string(destination.join("dist/index.js")).unwrap(),
        "independent edit"
    );
}

#[test]
fn fork_rejects_managed_destinations_and_unknown_plugins() {
    let f = Fixture::new();
    f.install();
    // A fork repository inside a managed repository is refused.
    let installed = f.root.join("plugins/hello");
    let out = f
        .pm()
        .current_dir(&installed)
        .args(["fork", "hello", "--ns", "me", "--into", "me-hello"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(!installed.join("me-hello").exists());
    f.cli(
        &["fork", "unknown", "--ns", "me", "--into", "me/unknown"],
        false,
    );
    assert!(!f.dir.join("me/unknown").exists());
    for (args, message) in [
        // The old form `fork ID NAMESPACE/NAME` is gone: a second argument is an ID.
        (
            &["fork", "hello", "me/hello", "--ns", "me", "--into", "x"][..],
            "plugin me/hello is not installed",
        ),
        (
            &["fork", "hello", "--into", "x"][..],
            "fork needs --ns NAMESPACE",
        ),
        (
            &["fork", "hello", "--ns", "me"][..],
            "fork needs --into DIR",
        ),
        (
            &["fork", "hello", "--ns", "me/a", "--into", "x"][..],
            "namespace/name",
        ),
        (
            &["fork", "--ns", "me", "--into", "x"][..],
            "fork: expected 1 argument(s), got 0",
        ),
        (
            &["fork", "hello", "--namespace", "x"][..],
            "unknown option: --namespace",
        ),
        (
            &["fork", "hello", "--ns", "me", "--into", "../x"][..],
            "without '..'",
        ),
    ] {
        let out = f
            .pm()
            .env("GITPM_NAMESPACE", "me")
            .current_dir(&f.dir)
            .args(args)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "{args:?} succeeded");
        assert!(stderr.contains(message), "{args:?}: {stderr}");
    }
    assert!(!f.dir.join("me").exists());
}

#[test]
fn namespaced_fork_and_original_coexist_and_release_independently() {
    let f = Fixture::new();
    let source = f.dir.join("vendor/demo");
    f.cli(&["init", "vendor/demo", "--into", "vendor/demo"], true);
    f.cli(&["install", source.to_str().unwrap()], true);
    assert!(f.root.join("dist/vendor/demo/index.js").exists());
    let fork = f.dir.join("dan/demo");
    // Same namespace.
    f.cli(
        &[
            "fork",
            "vendor/demo",
            "--ns",
            "vendor",
            "--into",
            "dan/demo",
        ],
        false,
    );
    assert!(!fork.exists());
    assert!(source.join("plugin.json").exists());
    // The source repository itself is never the fork repository.
    let before = fs::read(source.join("plugin.json")).unwrap();
    f.cli(
        &[
            "fork",
            "vendor/demo",
            "--ns",
            "dan",
            "--into",
            "vendor/demo",
        ],
        false,
    );
    assert_eq!(fs::read(source.join("plugin.json")).unwrap(), before);
    f.cli(
        &["fork", "vendor/demo", "--ns", "dan", "--into", "dan/demo"],
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
    assert!(
        config_files(&f.dir).is_empty(),
        "{:?}",
        config_files(&f.dir)
    );
}

#[test]
fn namespaces_reject_path_traversal_and_legacy_prefix_collisions() {
    let f = Fixture::new();
    f.install();
    f.cli(&["create", "../bad", "--into", "bad"], false);
    for id in ["../hello", "a/../hello", "/hello", "a/b/c", "a//b"] {
        f.cli(&["uninstall", id, "--force"], false);
    }
    let source = f.dir.join("hello/demo");
    f.cli(&["init", "hello/demo", "--into", "hello/demo"], true);
    assert!(source.join("plugin.json").exists());
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
    let no_tags = f.dir.join("dan/no-tags");
    f.cli(&["init", "dan/no-tags", "--into", "dan/no-tags"], true);
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
    fs::write(&sources, "./upstream\n./dan/no-tags\n").unwrap();
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
    f.cli(&["init", "dan/second", "--into", "dan/second"], true);
    fs::write(&sources, "./dan/second\n").unwrap();
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
        (&["namespace"][..], "unknown command: namespace"),
        (&["namespace", "me"][..], "unknown command: namespace"),
        (&["create", "hello"][..], "plugin id needs a namespace"),
        (
            &["list", "--namespace", "me"][..],
            "unknown option: --namespace",
        ),
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
        assert!(config_files(&f.dir).is_empty(), "{args:?} wrote config");
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
            .env_remove("GITPM_CATALOG")
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
        .env_remove("GITPM_CATALOG")
        .args(["check"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let out = f
        .pm()
        .env_remove("GITPM_CATALOG")
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
        .args(["create", "me/hello", "--into", "me/hello"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.dir.join("me/hello/plugin.json").exists());
    assert!(config_files(&f.dir).is_empty());
}

#[test]
fn legacy_pm_variables_are_ignored() {
    let f = Fixture::new();
    let legacy = f.dir.join("legacy");
    // Only legacy names: they must not act as GITPM_ROOT/CATALOG/SOURCES/NAMESPACE.
    let cli = |args: &[&str]| {
        f.command(env!("CARGO_BIN_EXE_gitpm"))
            .current_dir(&f.dir)
            .env("PM_ROOT", legacy.join("root"))
            .env("PM_CATALOG", legacy.join("catalog"))
            .env("PM_SOURCES", legacy.join("sources.txt"))
            .env("PM_CONFIG", legacy.join("pm.json"))
            .env("PM_NAMESPACE", "legacy")
            .args(args)
            .output()
            .unwrap()
    };
    let out = cli(&["install", "./upstream"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.dir.join("gitpm-workspace/dist/hello/index.js").exists());
    let out = cli(&["info"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG")
    );
    let out = cli(&["create", "hello", "--into", "hello"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("plugin id needs a namespace"));
    let out = cli(&["create", "dan/hello", "--into", "dan/hello"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.join("dan/hello/plugin.json")).unwrap()).unwrap();
    assert_eq!(m["id"], "dan/hello");
    // The workspace is ./gitpm-workspace (PM_ROOT ignored), so hello is found there.
    let out = cli(&["fork", "hello", "--ns", "me", "--into", "me/hello"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.dir.join("me/hello/plugin.json").exists());
    assert!(!legacy.exists());
    assert!(config_files(&f.dir).is_empty());
}

#[test]
fn create_install_and_fork_into_explicit_folders() {
    let f = Fixture::new();
    f.cli(&["create", "dan/hello", "--into", "dan/hello"], true);
    f.cli(&["install", "./dan/hello"], true);
    let out = f.cli(
        &["fork", "dan/hello", "--ns", "me", "--into", "me/hello"],
        true,
    );
    assert!(
        out.contains("Forked: dan/hello → me/hello (repository root)"),
        "{out}"
    );
    assert!(
        out.contains("gitpm replace dan/hello --with me/hello"),
        "{out}"
    );
    for (folder, id) in [("dan/hello", "dan/hello"), ("me/hello", "me/hello")] {
        let m: serde_json::Value =
            serde_json::from_slice(&fs::read(f.dir.join(folder).join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(m["id"], id);
    }
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.join("me/hello/plugin.json")).unwrap()).unwrap();
    assert_eq!(m["forkOf"]["id"], "dan/hello");
    f.cli(&["install", "./me/hello", "--channel", "dev"], true);
    assert!(f.root.join("dist/dan/hello/index.js").exists());
    assert!(f.root.join("dist/me/hello/index.js").exists());
    assert!(config_files(&f.dir).is_empty());
}

#[test]
fn gitpm_variables_set_root_catalog_and_sources() {
    let f = Fixture::new();
    fs::write(f.dir.join("sources.txt"), "./upstream\n").unwrap();
    // pm() sets GITPM_ROOT, GITPM_CATALOG and GITPM_SOURCES; no flags here.
    let out = f
        .pm()
        .current_dir(&f.dir)
        .args(["catalog-update"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(f.dir.join("catalog/legacy/hello/plugin.json").exists());
    let out = f
        .pm()
        .current_dir(&f.dir)
        .args(["install-catalog", "hello"])
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

#[test]
fn update_all_previews_applies_and_rejects_unsupported_options() {
    let f = Fixture::new();
    f.install();
    f.release("2.0.0", "hello v2");
    let before = f.state();
    let preview = f.cli(&["update", "all", "--dry-run"], true);
    assert!(
        preview.contains("hello would be updated 1.0.0 → 2.0.0"),
        "{preview}"
    );
    assert!(preview.contains("Nothing was changed."));
    assert_eq!(f.state(), before);
    assert_eq!(f.content(), "hello v1");
    for args in [
        vec!["update", "all", "--ref", "main"],
        vec!["update", "all", "--channel", "dev"],
        vec!["update", "hello", "--dry-run"],
    ] {
        f.cli(&args, false);
        assert_eq!(f.state(), before);
    }
    let out = f.cli(&["update", "all"], true);
    assert!(out.contains("1 updates, 0 installs"), "{out}");
    assert!(out.contains("hello updated 1.0.0 → 2.0.0"), "{out}");
    assert_eq!(f.content(), "hello v2");
    let out = f.cli(&["update", "all"], true);
    assert!(out.contains("0 updates, 0 installs"), "{out}");
    assert!(out.contains("hello up to date (2.0.0)"), "{out}");
}
