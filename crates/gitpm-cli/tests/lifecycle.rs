//! End-to-end tests of the `gitpm` binary on local Git repositories and catalogs.
//! Hermetic: each test runs the CLI with a cleared environment, a fixture HOME/TMPDIR
//! and Git config, and every GITPM_* path inside its own temp directory.
use std::{
    fs,
    path::{Path, PathBuf},
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
        // The catalog location every command gets must exist (it starts empty).
        fs::create_dir(dir.join("catalog")).unwrap();
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
        self.git_in(&self.repo, args);
    }
    fn git_in(&self, repo: &Path, args: &[&str]) {
        let out = self
            .command("git")
            .arg("-C")
            .arg(repo)
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
            format!(r#"{{"id":"dan/hello","version":"{version}"}}"#),
        )
        .unwrap();
        fs::write(self.repo.join("dist/index.js"), contents).unwrap();
        self.git(&["add", "."]);
        self.git(&["commit", "-m", version]);
        self.git(&["tag", "-f", &format!("dan/hello@{version}")]);
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
    /// [`Fixture::cli`] with neither `--catalog` nor `GITPM_CATALOG`.
    fn cli_without_catalog(&self, args: &[&str], success: bool) -> String {
        let out = self
            .pm()
            .env_remove("GITPM_CATALOG")
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
        fs::read_to_string(self.root.join("dist/dan/hello/index.js")).unwrap()
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
    f.cli(&["update", "dan/hello"], true);
    assert_eq!(f.content(), "hello v2");
    assert_eq!(
        f.state()["packages"]["dan/hello"]["current"]["version"],
        "2.0.0"
    );
    f.cli(&["rollback", "dan/hello"], true);
    assert_eq!(f.content(), "hello v1");
    f.cli(&["update", "dan/hello"], true);
    assert_eq!(f.content(), "hello v2");
    f.cli(&["disable", "dan/hello"], true);
    assert_eq!(f.state()["packages"]["dan/hello"]["enabled"], false);
    // dist/ holds only active packages.
    assert!(!f.root.join("dist/dan/hello").exists());
    f.cli(&["enable", "dan/hello"], true);
    assert_eq!(f.content(), "hello v2");
    f.cli(&["forget", "dan/hello"], true);
    assert!(f.state()["packages"].as_object().unwrap().is_empty());
    assert!(f.root.join("packages/dan/hello/.git").exists());
    assert!(!f.root.join("dist/dan/hello").exists());
    f.cli(&["restore", "dan/hello"], true);
    assert_eq!(f.state()["packages"]["dan/hello"]["enabled"], true);
    assert_eq!(f.content(), "hello v2");
    f.cli(&["uninstall", "dan/hello"], true);
    assert!(!f.root.join("packages/dan/hello").exists());
    assert!(!f.root.join("dist/dan/hello").exists());
}
#[test]
fn lifecycle_and_check_without_a_catalog() {
    let f = Fixture::new();
    let row = |out: &str| {
        out.lines()
            .find(|l| l.starts_with("dan/hello "))
            .unwrap_or_else(|| panic!("no row: {out}"))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    f.cli_without_catalog(&["install", f.repo.to_str().unwrap()], true);
    let out = f.cli_without_catalog(&["check"], true);
    assert!(out.contains("Catalog: none"), "{out}");
    assert_eq!(row(&out), "dan/hello 1.0.0 1.0.0 yes origin up to date");
    f.release("2.0.0", "hello v2");
    let state = fs::read(f.root.join("state.json")).unwrap();
    let out = f.cli_without_catalog(&["check"], true);
    assert_eq!(
        row(&out),
        "dan/hello 1.0.0 2.0.0 (major) yes origin update available"
    );
    assert!(out.contains("dan/hello: 1.0.0 → 2.0.0"), "{out}");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    assert_eq!(f.content(), "hello v1", "check never updates");
    f.cli_without_catalog(&["update", "dan/hello"], true);
    assert_eq!(f.content(), "hello v2");
    let out = f.cli_without_catalog(&["check"], true);
    assert_eq!(row(&out), "dan/hello 2.0.0 2.0.0 yes origin up to date");
    f.cli_without_catalog(&["rollback", "dan/hello"], true);
    assert_eq!(f.content(), "hello v1");
    f.cli_without_catalog(&["forget", "dan/hello"], true);
    f.cli_without_catalog(&["restore", "dan/hello"], true);
    assert_eq!(f.content(), "hello v1");
    // An origin that disappeared is a row, not a failed check.
    fs::rename(&f.repo, f.dir.join("moved")).unwrap();
    let out = f.cli_without_catalog(&["check"], true);
    assert!(
        row(&out)
            .starts_with("dan/hello 1.0.0 — yes origin origin unreachable: Repository not found"),
        "{out}"
    );
    f.cli_without_catalog(&["uninstall", "dan/hello"], true);
    assert!(!f.root.join("packages/dan/hello").exists());
}
#[test]
fn failed_update_keeps_files_and_state() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    // The newest release needs a package nobody can install.
    fs::write(f.repo.join("dist/index.js"), "v2").unwrap();
    fs::write(
        f.repo.join("plugin.json"),
        r#"{"id":"dan/hello","version":"2.0.0","dependencies":{"dan/missing":"1"}}"#,
    )
    .unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "dependency"]);
    f.git(&["tag", "dan/hello@2.0.0"]);
    f.cli(&["update", "dan/hello"], false);
    assert_eq!(f.state(), before);
    assert_eq!(f.content(), "hello v1");
}
#[test]
fn protects_local_edits_and_force_is_explicit() {
    let f = Fixture::new();
    f.install();
    fs::write(f.root.join("packages/dan/hello/my-work.txt"), "keep me").unwrap();
    f.cli(&["update", "dan/hello"], false);
    f.cli(&["uninstall", "dan/hello"], false);
    assert!(f.root.join("packages/dan/hello/my-work.txt").exists());
    f.cli(&["uninstall", "dan/hello", "--force"], true);
    assert!(!f.root.join("packages/dan/hello").exists());
}
#[test]
fn rejects_duplicate_path_escape_and_missing_payload() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    let again = f.cli(&["install", f.repo.to_str().unwrap()], true);
    assert_eq!(again, "dan/hello already installed (1.0.0)\n");
    f.cli(&["uninstall", "../upstream", "--force"], false);
    assert!(f.repo.exists());
    assert_eq!(f.state(), before);
    f.cli(&["uninstall", "dan/hello"], true);
    fs::remove_dir_all(f.repo.join("dist")).unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "missing dist"]);
    f.cli(&["install", f.repo.to_str().unwrap(), "--dev"], false);
    assert!(f.state()["packages"].as_object().unwrap().is_empty());
}
#[test]
fn recovery_and_lock() {
    let f = Fixture::new();
    f.install();
    let manager = gitpm::Manager::open(&f.root).unwrap();
    assert!(gitpm::Manager::open(&f.root).is_err());
    drop(manager);
    // An interrupted, uncommitted transaction is discarded by the next command that
    // opens the workspace; the read-only `list` leaves it alone.
    fs::create_dir_all(f.root.join(".pm-txn/new/dist/dan/hello")).unwrap();
    fs::write(f.root.join(".pm-txn/new/dist/dan/hello/index.js"), "half").unwrap();
    f.cli(&["list"], true);
    assert!(f.root.join(".pm-txn").exists(), "list must not recover");
    assert_eq!(f.content_after_recovery(), "hello v1");
    assert!(!f.root.join(".pm-txn").exists());
}
impl Fixture {
    fn content_after_recovery(&self) -> String {
        self.cli(&["enable", "dan/hello"], true);
        self.content()
    }
}

#[test]
fn direct_git_refs_and_local_author_commit() {
    let f = Fixture::new();
    f.git(&["tag", "-f", "dan/hello@1.0.0"]);
    let repo = f.repo.to_str().unwrap();
    f.cli(&["install", repo, "--ref", "dan/hello@1.0.0"], true);
    assert_eq!(
        f.state()["packages"]["dan/hello"]["mode"],
        serde_json::json!({"pinned": "dan/hello@1.0.0"})
    );
    fs::write(f.repo.join("dist/index.js"), "author release").unwrap();
    f.cli(&["commit", repo, "1.0.1"], true);
    f.cli(&["commit", repo, "1.0.1"], false);
    // Pinned: update says so and changes nothing; --release returns to releases.
    assert_eq!(
        f.cli(&["update", "dan/hello"], true),
        "dan/hello pinned at 1.0.0 (dan/hello@1.0.0)\n"
    );
    assert_eq!(f.content(), "hello v1");
    assert_eq!(
        f.cli(&["update", "dan/hello", "--release"], true),
        "dan/hello updated 1.0.0 → 1.0.1\n"
    );
    assert!(f.state()["packages"]["dan/hello"].get("mode").is_none());
    assert_eq!(f.content(), "author release");
    f.cli(&["update", "dan/hello", "--ref", "dan/hello@1.0.1"], true);
    assert_eq!(f.content(), "author release");
    let sha = f.state()["packages"]["dan/hello"]["current"]["commit"]
        .as_str()
        .unwrap()
        .to_owned();
    f.cli(&["rollback", "dan/hello"], true);
    assert_eq!(f.content(), "hello v1");
    f.cli(&["uninstall", "dan/hello"], true);
    f.cli(&["install", repo, "--ref", &sha], true);
    assert_eq!(f.content(), "author release");
}

mod scaffold {
    use super::*;

    #[test]
    fn derived_identity_is_refused() {
        let f = Fixture::new();
        fs::write(
            f.dir.join("home/.gitconfig"),
            "[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n",
        )
        .unwrap();
        f.git(&["config", "--unset", "user.name"]);
        f.git(&["config", "--unset", "user.email"]);
        fs::write(f.repo.join("dist/index.js"), "next release").unwrap();

        // Names are explicit, but EMAIL is Git's implicit email fallback. Plain
        // `git var` accepts it; user.useConfigOnly rejects it.
        for variable in ["GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT"] {
            let ident = f
                .command("git")
                .arg("-C")
                .arg(&f.repo)
                .args(["var", variable])
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("EMAIL", "derived@example.invalid")
                .output()
                .unwrap();
            assert!(
                ident.status.success(),
                "{}",
                String::from_utf8_lossy(&ident.stderr)
            );
        }

        let git_output = |args: &[&str]| {
            let out = f
                .command("git")
                .arg("-C")
                .arg(&f.repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            out.stdout
        };
        let state = || {
            (
                git_output(&["rev-parse", "HEAD"]),
                git_output(&["status", "--porcelain"]),
                git_output(&["tag", "--list"]),
                fs::read(f.repo.join("plugin.json")).unwrap(),
                fs::read(f.repo.join("dist/index.js")).unwrap(),
                fs::read(f.repo.join(".git/config")).unwrap(),
                fs::read(f.repo.join(".git/index")).unwrap(),
            )
        };
        let before = state();
        let out = f
            .pm()
            .args(["commit", f.repo.to_str().unwrap(), "1.0.1"])
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("EMAIL", "derived@example.invalid")
            .output()
            .unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert!(stderr.contains("invalid_argument"), "{stderr}");
        assert!(
            stderr.contains("no Git identity configured; set user.name and user.email"),
            "{stderr}"
        );
        assert_eq!(state(), before);
    }
}

#[test]
fn catalog_manifest_is_verified_before_install() {
    let f = Fixture::new();
    f.git(&["tag", "-f", "dan/hello@1.0.0"]);
    let manager = gitpm::Manager::open(&f.root).unwrap();
    let wrong = gitpm::catalog::Release {
        manifest: gitpm::Manifest {
            id: "dan/hello".into(),
            version: "99.0.0".into(),
            ..Default::default()
        },
        source: f.repo.to_str().unwrap().into(),
        commit: String::from_utf8(
            f.command("git")
                .arg("-C")
                .arg(&f.repo)
                .args(["rev-parse", "dan/hello@1.0.0"])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_owned(),
        reference: "dan/hello@99.0.0".into(),
        files: Default::default(),
        build: None,
        path: None,
    };
    assert!(manager.install_release(&wrong).is_err());
    assert!(manager.state().unwrap().packages.is_empty());
    assert!(!f.root.join("dist/dan/hello").exists());
}

#[test]
fn explicit_same_commit_and_numeric_versions_sort_correctly() {
    let f = Fixture::new();
    f.install();
    f.git(&["tag", "-f", "dan/hello@1.0.0"]);
    f.cli(&["update", "dan/hello", "--ref", "dan/hello@1.0.0"], true);
    assert!(f.state()["packages"]["dan/hello"].get("pinned").is_none());
    use gitpm::catalog::version_key;
    assert!(version_key("1.10.0").unwrap() > version_key("1.9.0").unwrap());
    for invalid in ["01.0.0", "dan/hello@1.0.0", "1.0", "1.0.0+dev", "-1.0.0"] {
        assert!(version_key(invalid).is_err());
    }
    assert!(version_key("1.0.0-dev").unwrap() < version_key("1.0.0").unwrap());
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
fn init_takes_an_explicit_id_and_repository_folder() {
    use std::process::Stdio;
    let f = Fixture::new();
    for (command, id) in [("init", "acme/hello"), ("init", "vendor/demo1")] {
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
        assert_eq!(f.state()["packages"][id]["current"]["version"], "0.1.0");
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
fn init_requires_a_namespace_and_never_prompts_or_reads_the_environment() {
    use std::process::Stdio;
    let f = Fixture::new();
    fs::create_dir_all(f.dir.join("dan/existing")).unwrap();
    fs::write(f.dir.join("dan/existing/keep.txt"), "untouched").unwrap();
    for (args, message) in [
        (
            &["init", "hello", "--into", "hello"][..],
            // The library refuses it (invalid_id); the CLI only prints the error.
            "package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello\nError kind: invalid_id",
        ),
        (&["init"][..], "init: expected 1 argument(s), got 0"),
        (&["init", "dan/a/b", "--into", "x"][..], "namespace/name"),
        (&["init", "../escape", "--into", "x"][..], "namespace/name"),
        (&["init", "/hello", "--into", "x"][..], "namespace/name"),
        (&["init", "dan/", "--into", "x"][..], "namespace/name"),
        (
            &["init", "dan/hello", "--namespace", "me"][..],
            "unknown option: --namespace",
        ),
        // No default folder: the repository is always given.
        (&["init", "dan/hello"][..], "init needs --into DIR"),
        (
            &["init", "dan/existing", "--into", "dan/existing"][..],
            "is not a Git repository",
        ),
        // The former alias is an unknown command.
        (
            &["create", "dan/hello", "--into", "dan/hello"][..],
            "unknown command: create; use --help",
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
    let installed = f.root.join("packages/dan/hello");
    fs::write(installed.join("dist/index.js"), "local edit").unwrap();
    fs::write(installed.join("notes.txt"), "untracked work").unwrap();
    let state = fs::read(f.root.join("state.json")).unwrap();
    let destination = f.dir.join("me/hello");
    let out = f.cli(
        &["fork", "dan/hello", "--ns", "me", "--into", "me/hello"],
        true,
    );
    assert!(
        out.contains("me/hello forked from dan/hello (repository root)"),
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
        &["fork", "dan/hello", "--ns", "me", "--into", "me/hello"],
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
    let installed = f.root.join("packages/dan/hello");
    let out = f
        .pm()
        .current_dir(&installed)
        .args(["fork", "dan/hello", "--ns", "me", "--into", "me-hello"])
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
            &["fork", "dan/hello", "me/hello", "--ns", "me", "--into", "x"][..],
            "package me/hello is not installed",
        ),
        (
            &["fork", "dan/hello", "--into", "x"][..],
            "fork needs --ns NAMESPACE",
        ),
        (
            &["fork", "dan/hello", "--ns", "me"][..],
            "fork needs --into DIR",
        ),
        (
            &["fork", "dan/hello", "--ns", "me/a", "--into", "x"][..],
            "namespace/name",
        ),
        (
            &["fork", "--ns", "me", "--into", "x"][..],
            "fork: expected 1 argument(s), got 0",
        ),
        (
            &["fork", "dan/hello", "--namespace", "x"][..],
            "unknown option: --namespace",
        ),
        (
            &["fork", "dan/hello", "--ns", "me", "--into", "../x"][..],
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
        f.state()["packages"]["vendor/demo"]["current"]["commit"]
    );
    f.cli(&["install", fork.to_str().unwrap(), "--dev"], true);
    assert!(f.root.join("dist/dan/demo/index.js").exists());
    assert_eq!(f.state()["packages"]["dan/demo"]["forkOf"], m["forkOf"]);
    fs::write(fork.join("dist/index.js"), "fork release").unwrap();
    f.cli(&["commit", fork.to_str().unwrap(), "0.1.1"], true);
    f.cli(&["update", "dan/demo", "--ref", "dan/demo@0.1.1"], true);
    assert_eq!(
        fs::read_to_string(f.root.join("dist/dan/demo/index.js")).unwrap(),
        "fork release"
    );
    f.cli(&["rollback", "dan/demo"], true);
    f.cli(&["uninstall", "dan", "--force"], false); // namespace is not a package
    assert!(f.root.join("packages/dan/demo/.git").exists());
    f.cli(&["uninstall", "dan/demo"], true);
    assert!(f.root.join("dist/vendor/demo/index.js").exists());
    assert!(
        config_files(&f.dir).is_empty(),
        "{:?}",
        config_files(&f.dir)
    );
}

#[test]
fn namespaces_reject_path_traversal_and_unscoped_ids() {
    let f = Fixture::new();
    f.install();
    f.cli(&["init", "../bad", "--into", "bad"], false);
    for id in ["../hello", "a/../hello", "/hello", "a/b/c", "a//b", "hello"] {
        f.cli(&["uninstall", id, "--force"], false);
    }
    // A namespace equal to an installed package's name is just another namespace.
    let source = f.dir.join("hello/demo");
    f.cli(&["init", "hello/demo", "--into", "hello/demo"], true);
    assert!(source.join("plugin.json").exists());
    f.cli(&["install", source.to_str().unwrap()], true);
    assert!(f.root.join("packages/dan/hello/.git").exists());
    assert!(f.root.join("packages/hello/demo/.git").exists());
}

#[test]
fn local_catalog_register_read_install_and_immutable_versions() {
    let f = Fixture::new();
    let catalog_dir = f.dir.join("local-catalog");
    let location = catalog_dir.to_str().unwrap();
    let repo = f.repo.to_str().unwrap();
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            repo,
            "--ref",
            "HEAD",
        ],
        true,
    );
    let file = catalog_dir.join("catalog.json");
    let original = fs::read(&file).unwrap();
    assert!(
        f.cli(&["--catalog", location, "catalog", "links"], true)
            .contains("namespaces")
    );
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            repo,
            "--ref",
            "HEAD",
        ],
        true,
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    f.cli(
        &[
            "--catalog",
            file.to_str().unwrap(),
            "install-catalog",
            "dan/hello",
        ],
        true,
    );
    assert!(
        f.cli(&["--catalog", location, "check"], true)
            .contains("dan/hello")
    );
    fs::write(
        f.repo.join("dist/index.js"),
        "changed without a version bump",
    )
    .unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "same version different commit"]);
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            repo,
            "--ref",
            "HEAD",
        ],
        false,
    );
    assert_eq!(fs::read(&file).unwrap(), original);
    fs::write(&file, "not JSON").unwrap();
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            repo,
            "--ref",
            "HEAD",
        ],
        false,
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "not JSON");
    f.cli(
        &[
            "--catalog",
            "https://example.invalid/catalog",
            "catalog",
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
    assert!(manager.state().unwrap().packages.is_empty());
    manager.install_release(&found).unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join("dist/dan/hello/index.js")).unwrap(),
        "abc"
    );
    let selected = catalog::selected(location, &manager.state().unwrap()).unwrap();
    assert_eq!(selected.packages.len(), 1);
    let index_path = dir.join("namespaces/dan/hello/plugin.json");
    let mut index: serde_json::Value =
        serde_json::from_slice(&fs::read(&index_path).unwrap()).unwrap();
    index["versions"]["1.0.0"] = "../../escape.json".into();
    fs::write(index_path, serde_json::to_vec(&index).unwrap()).unwrap();
    assert!(catalog::lookup(location, "dan/hello", None).is_err());
    assert!(catalog::lookup(location, "../escape", None).is_err());
}

#[test]
fn v1_catalog_is_refused_and_left_unchanged() {
    use gitpm::catalog;
    let f = Fixture::new();
    let first = f.dir.join("first");
    let release =
        catalog::register(first.to_str().unwrap(), f.repo.to_str().unwrap(), "HEAD").unwrap();
    let old = serde_json::json!({"plugins": {"dan/hello": {"source": release.source, "versions": {"1.0.0": release}}}});
    let dir = f.dir.join("old");
    fs::create_dir(&dir).unwrap();
    let original = serde_json::to_vec(&old).unwrap();
    fs::write(dir.join("catalog.json"), &original).unwrap();
    let location = dir.to_str().unwrap();
    assert_eq!(
        catalog::lookup(location, "dan/hello", None)
            .unwrap_err()
            .kind(),
        "catalog"
    );
    for args in [
        &["--catalog", location, "catalog", "show"][..],
        &["--catalog", location, "install-catalog", "dan/hello"][..],
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            f.repo.to_str().unwrap(),
            "--ref",
            "HEAD",
        ][..],
        &["--catalog", location, "catalog", "update"][..],
    ] {
        f.cli(args, false);
    }
    assert_eq!(fs::read(dir.join("catalog.json")).unwrap(), original);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "nothing written");
    assert!(
        !f.root.join("dist").exists() || fs::read_dir(f.root.join("dist")).unwrap().count() == 0
    );
    // migrate-catalog is gone.
    let out = f
        .pm()
        .args(["--catalog", location, "migrate-catalog"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("unknown command: migrate-catalog; use --help")
    );
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
        !dir.join("namespaces/dan/hello/versions/1.0.0/release.json")
            .exists()
    );
}

#[test]
fn stable_install_and_update_ignore_untagged_head_and_sort_versions() {
    let f = Fixture::new();
    f.release("1.9.0", "nine");
    f.release("1.10.0", "ten");
    f.git(&["tag", "-d", "dan/hello@1.10.0"]);
    f.git(&["tag", "-a", "dan/hello@1.10.0", "-m", "annotated release"]);
    // A newer-looking version on the branch is not a release.
    fs::write(
        f.repo.join("plugin.json"),
        r#"{"id":"dan/hello","version":"9.0.0"}"#,
    )
    .unwrap();
    fs::write(f.repo.join("dist/index.js"), "unreleased").unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "development"]);
    f.install();
    assert_eq!(f.content(), "ten");
    f.cli(&["update", "dan/hello"], true);
    assert_eq!(f.content(), "ten");
    f.cli(&["update", "dan/hello", "--dev"], true);
    assert_eq!(f.content(), "unreleased");
    let before = f.state();
    // The default branch is followed now.
    f.cli(&["update", "dan/hello"], true);
    assert_eq!(before, f.state());
    // An explicit return to releases may go down to the newest one.
    f.cli(&["update", "dan/hello", "--release"], true);
    assert_eq!(f.content(), "ten");
    f.cli(&["update", "dan/hello", "--dev", "--ref", "HEAD"], false);
}

#[test]
fn catalog_update_keeps_published_records_when_tags_move() {
    use gitpm::catalog;
    let f = Fixture::new();
    let catalog_dir = f.dir.join("generated");
    let location = catalog_dir.to_str().unwrap();
    // Paths are relative to the list file, and duplicate lines are harmless.
    let sources = f.dir.join("sources.txt");
    fs::write(&sources, "# Git sources\n\n./upstream\n./upstream\n").unwrap();
    f.cli(&["--catalog", location, "catalog", "update"], true);
    let first = catalog::lookup(location, "dan/hello", None).unwrap();
    assert_eq!(first.manifest.version, "1.0.0");
    assert!(!first.files.is_empty());
    let original_root = fs::read(catalog_dir.join("catalog.json")).unwrap();
    f.release("1.10.0", "ten");
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "update",
            "--sources",
            sources.to_str().unwrap(),
        ],
        true,
    );
    assert_eq!(
        catalog::lookup(location, "dan/hello", None)
            .unwrap()
            .manifest
            .version,
        "1.10.0"
    );
    assert_eq!(
        fs::read(catalog_dir.join("catalog.json")).unwrap(),
        original_root
    );
    let index = catalog_dir.join("namespaces/dan/hello/plugin.json");
    let original_index = fs::read(&index).unwrap();
    f.cli(&["--catalog", location, "catalog", "update"], true);
    assert_eq!(fs::read(&index).unwrap(), original_index);
    f.release("2.0.0", "two");
    // Move an old tag to a different commit with the same manifest version: the
    // published record is kept with a warning, the new version is published.
    f.release("1.0.0", "replacement");
    let (ok, out, err) = f.output(&["--catalog", location, "catalog", "update"]);
    assert!(ok, "{err}");
    assert!(
        out.contains("Catalog ready: 3 release(s) checked from sources."),
        "{out}"
    );
    assert!(
        err.contains("Warning: release dan/hello@1.0.0: tag now points to ")
            && err.contains("; the published record is kept"),
        "{err}"
    );
    assert!(
        catalog_dir
            .join("namespaces/dan/hello/versions/2.0.0/release.json")
            .exists()
    );
    assert_eq!(
        catalog::lookup(location, "dan/hello", Some("1.0.0"))
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
    f.cli(&["--catalog", location, "catalog", "update"], false);
    assert!(!directory.exists());
    // Stable installs have no implicit fallback to the development branch.
    f.cli(&["install", no_tags.to_str().unwrap()], false);
    f.cli(&["install", no_tags.to_str().unwrap(), "--dev"], true);
    fs::write(&sources, "./upstream\n").unwrap();
    f.cli(&["--catalog", location, "catalog", "update"], true);
    f.cli(&["init", "dan/second", "--into", "dan/second"], true);
    fs::write(&sources, "./dan/second\n").unwrap();
    f.cli(&["--catalog", location, "catalog", "update"], true);
    assert!(directory.join("namespaces/dan/hello/plugin.json").exists());
    assert!(directory.join("namespaces/dan/second/plugin.json").exists());
}

#[test]
fn release_manifest_must_match_tag_and_update_warns_on_retagging() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    // A moved release tag of the installed version: up to date with a warning.
    f.release("1.0.0", "replacement");
    let (ok, out, err) = f.output(&["update", "dan/hello"]);
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "dan/hello up to date (1.0.0)");
    assert!(
        err.contains("Warning: release tag dan/hello@1.0.0 now points to ")
            && err.contains("; the installed commit is kept"),
        "{err}"
    );
    assert_eq!(f.state(), before);
    assert_eq!(f.content(), "hello v1");
    f.git(&["tag", "dan/hello@8.0.0"]); // manifest still says 1.0.0
    let dir = f.dir.join("generated");
    fs::write(f.dir.join("sources.txt"), "./upstream\n").unwrap();
    f.cli(
        &["--catalog", dir.to_str().unwrap(), "catalog", "update"],
        false,
    );
    assert!(!dir.exists());
}

#[test]
fn update_after_a_deleted_release_tag_is_up_to_date_with_a_warning() {
    let f = Fixture::new();
    f.install();
    let before = f.state();
    f.git(&["tag", "-d", "dan/hello@1.0.0"]);
    let (ok, out, err) = f.output(&["update", "dan/hello"]);
    assert!(ok, "{err}");
    assert_eq!(out.trim(), "dan/hello up to date (1.0.0)");
    assert_eq!(
        err.trim(),
        "Warning: installed release dan/hello@1.0.0 no longer exists on the origin"
    );
    let (ok, out, err) = f.output(&["update", "all"]);
    assert!(ok, "{err}");
    assert!(out.contains("dan/hello up to date (1.0.0)"), "{out}");
    assert!(
        err.contains("Warning: installed release dan/hello@1.0.0 no longer exists on the origin"),
        "{err}"
    );
    assert_eq!(f.state(), before);
}

#[test]
fn info_shows_directory_tree_without_installing() {
    use gitpm::catalog;
    use gitpm_cli::catalog::info;
    let f = Fixture::new();
    let dir = f.dir.join("catalog");
    let location = dir.to_str().unwrap();
    let release = catalog::register(location, f.repo.to_str().unwrap(), "HEAD").unwrap();
    let result = f.cli(&["--catalog", location, "catalog", "show"], true);
    assert!(result.contains("Directories and packages:"));
    assert!(result.contains("namespaces/"));
    assert!(result.contains("hello/"));
    assert!(result.contains("dan/hello — latest: 1.0.0, versions: 1"));
    assert!(result.contains(&release.source));
    assert!(!f.root.exists());
    fs::write(
        dir.join("catalog.json"),
        r#"{"schema":1,"directories":{"escape":"../"}}"#,
    )
    .unwrap();
    assert!(info(location).is_err());
    fs::write(
        dir.join("catalog.json"),
        r#"{"schema":1,"directories":{"missing":"missing/"}}"#,
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
    let repo = f.root.join("packages/dan/hello");
    let git = |args: &[&str]| f.git_in(&repo, args);
    git(&["config", "user.name", "Test"]);
    git(&["config", "user.email", "test@example.invalid"]);
    fs::write(repo.join("dist/index.js"), "stashed edit").unwrap();
    git(&["stash", "push", "-m", "wip"]);
    let before = f.state();
    let out = f
        .pm()
        .arg("--root")
        .arg(&f.root)
        .args(["uninstall", "dan/hello"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("stash: stash@{0}"), "{stderr}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("[y/N]"));
    assert_eq!(f.state(), before);
    assert!(repo.join(".git").is_dir());

    // One refusal tells everything: a consumer and the lost work, from the plan.
    let app = f.dir.join("app");
    f.cli(&["init", "app/a", "--into", app.to_str().unwrap()], true);
    let manifest = app.join("plugin.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    json["dependencies"] = serde_json::json!({"dan/hello": "^1"});
    fs::write(&manifest, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    f.cli(&["commit", app.to_str().unwrap(), "0.1.1"], true);
    f.cli(&["install", app.to_str().unwrap()], true);
    let out = f
        .pm()
        .arg("--root")
        .arg(&f.root)
        .args(["uninstall", "dan/hello"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Error: cannot uninstall dan/hello: required by app/a")
            && stderr.contains("  - stash: stash@{0}")
            && stderr.contains("Error kind: has_dependents"),
        "{stderr}"
    );
    assert!(repo.join(".git").is_dir());
    f.cli(&["uninstall", "dan/hello", "--force"], true);
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
    let repo = f.root.join("packages/dan/hello");
    fs::write(repo.join("notes.txt"), "untracked work").unwrap();
    // BSD script(1) takes the command as arguments; util-linux needs -c.
    let util_linux = Command::new("script")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    // stdin is the terminal; stderr goes to a file, so the prompt's stream is visible.
    let script = if util_linux {
        "script -qec \"'$0' --root '$1' uninstall dan/hello 2>'$2'\" /dev/null"
    } else {
        "script -q /dev/null sh -c \"exec '$0' --root '$1' uninstall dan/hello 2>'$2'\""
    };
    let stderr_file = f.dir.join("uninstall.stderr");
    // `before` runs while the prompt waits, before the answer is typed.
    let ask_after = |before: &str, answer: &str| {
        let out = f
            .command("sh")
            .arg("-c")
            .arg(format!(
                "(sleep 1; {before}printf '{answer}\\n'; sleep 1) | {script}"
            ))
            .arg(env!("CARGO_BIN_EXE_gitpm"))
            .arg(&f.root)
            .arg(&stderr_file)
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            fs::read_to_string(&stderr_file).unwrap_or_default(),
        )
    };
    let ask = |answer: &str| ask_after("", answer);
    let (output, stderr) = ask("n");
    assert!(stderr.contains("Delete anyway? [y/N]"), "{stderr}");
    assert!(stderr.contains("untracked: notes.txt"), "{stderr}");
    assert!(
        stderr.contains("uninstall cancelled; nothing was changed"),
        "{stderr}"
    );
    // stdout carries only result lines.
    assert!(!output.contains("Delete anyway"), "{output}");
    assert!(repo.join("notes.txt").exists());
    assert_eq!(f.state()["packages"]["dan/hello"]["enabled"], true);
    // Work that appears while the prompt waits is not covered by the answer.
    let later = repo.join("later.txt");
    let (output, stderr) = ask_after(&format!("touch '{}'; ", later.display()), "y");
    assert!(stderr.contains("Delete anyway? [y/N]"), "{stderr}");
    assert!(
        stderr.contains("  - untracked: later.txt") && stderr.contains("Error kind: local_changes"),
        "{stderr}"
    );
    let refusal = &stderr[stderr.find("Error:").unwrap()..];
    assert!(!refusal.contains("notes.txt"), "{stderr}");
    assert!(!output.contains("uninstalled"), "{output}");
    assert!(repo.join("notes.txt").exists() && later.exists());
    fs::remove_file(&later).unwrap();
    let (output, stderr) = ask("y");
    assert!(stderr.contains("Delete anyway? [y/N]"), "{stderr}");
    assert!(output.contains("dan/hello uninstalled"), "{output}");
    assert!(!repo.exists());
    assert!(f.state()["packages"].as_object().unwrap().is_empty());
}

#[test]
fn usage_errors_name_the_problem_and_create_nothing() {
    let f = Fixture::new();
    for (args, message) in [
        (&["bogus", "qa/demo"][..], "unknown command: bogus"),
        (
            &["catalog", "show", "extra"][..],
            "catalog show: expected 0 argument(s), got 1",
        ),
        (
            &["catalog", "links", "extra"][..],
            "catalog links: expected 0 argument(s), got 1",
        ),
        (
            &["catalog", "update", "extra"][..],
            "catalog update: expected 0 argument(s), got 1",
        ),
        (
            &["catalog", "register"][..],
            "catalog register: expected 1 argument(s), got 0",
        ),
        (
            &["catalog", "show", "--sources", "list.txt"][..],
            "--sources requires catalog update",
        ),
        (
            &["catalog", "links", "--ref", "HEAD"][..],
            "--ref is not valid for catalog links",
        ),
        (
            &["catalog", "update", "--package", "qa/demo"][..],
            "--package requires install/download/commit/catalog register",
        ),
        (
            &[
                "catalog",
                "show",
                "--build-url",
                "https://example.invalid/build",
                "--build-sha256",
                "a",
            ][..],
            "build options require catalog register",
        ),
        (
            &["catalog", "register", "source"][..],
            "catalog register requires --ref",
        ),
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
        (&["init", "hello"][..], "package ID needs a namespace"),
        // Library refusals before anything is created (Options are validated first).
        (
            &["--catalog", "./missing", "update", "qa/demo"][..],
            "catalog location ./missing does not exist",
        ),
        (
            &["list", "--namespace", "me"][..],
            "unknown option: --namespace",
        ),
        // One target per command; `--channel` is gone without an alias.
        (
            &["install", "./x", "--channel", "dev"][..],
            "unknown option: --channel; use --help",
        ),
        (
            &["install", "./x", "--ref", "a", "--dev"][..],
            "--dev conflicts with --ref",
        ),
        (
            &["update", "qa/demo", "--dev", "--release"][..],
            "--release conflicts with --dev",
        ),
        (
            &["rollback", "qa/demo", "--dev"][..],
            "--dev requires install/download/update",
        ),
        (
            &["update", "all", "--dev"][..],
            "update all takes no --ref, --dev or --release; it follows each package's recorded mode",
        ),
        (
            &["download", "./x", "--ref", "a", "--dev"][..],
            "--dev conflicts with --ref",
        ),
        (
            &["download", "./x", "--version", "1.0.0"][..],
            "--version is not valid for download",
        ),
        (
            &["download-catalog", "qa/demo", "--ref", "a"][..],
            "--ref is not valid for download-catalog",
        ),
        (&["download-catalog", "Bad"][..], "invalid"),
        (
            &["list", "--force"][..],
            "--force is only valid with uninstall/disable/enable/install/install-catalog/forget/replace/fork --replace",
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
        &["catalog", "show"][..],
        &["catalog", "links"][..],
        &["catalog", "update"][..],
        &["catalog", "register", "x", "--ref", "HEAD"][..],
        &["install-catalog", "qa/demo"][..],
        &["download-catalog", "qa/demo"][..],
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
            stderr.contains(&format!(
                "{} needs a catalog: pass --catalog",
                if args[0] == "catalog" {
                    format!("catalog {}", args[1])
                } else {
                    args[0].to_owned()
                }
            )),
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
    assert!(
        f.dir
            .join("gitpm-workspace/dist/dan/hello/index.js")
            .exists()
    );
    let out = f
        .command(env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(&f.dir)
        .args(["init", "me/hello", "--into", "me/hello"])
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
    assert!(
        f.dir
            .join("gitpm-workspace/dist/dan/hello/index.js")
            .exists()
    );
    let out = cli(&["catalog", "show"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("needs a catalog: pass --catalog DIR_OR_JSON_OR_URL or set GITPM_CATALOG")
    );
    let out = cli(&["init", "hello", "--into", "hello"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("package ID needs a namespace"));
    let out = cli(&["init", "dan/hello", "--into", "dan/hello"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.join("dan/hello/plugin.json")).unwrap()).unwrap();
    assert_eq!(m["id"], "dan/hello");
    // The workspace is ./gitpm-workspace (PM_ROOT ignored), so hello is found there.
    let out = cli(&["fork", "dan/hello", "--ns", "me", "--into", "me/hello"]);
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
fn init_install_and_fork_into_explicit_folders() {
    let f = Fixture::new();
    f.cli(&["init", "dan/hello", "--into", "dan/hello"], true);
    f.cli(&["install", "./dan/hello"], true);
    let out = f.cli(
        &["fork", "dan/hello", "--ns", "me", "--into", "me/hello"],
        true,
    );
    assert!(
        out.contains("me/hello forked from dan/hello (repository root)"),
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
    f.cli(&["install", "./me/hello", "--dev"], true);
    assert!(f.root.join("dist/dan/hello/index.js").exists());
    assert!(f.root.join("dist/me/hello/index.js").exists());
    assert!(config_files(&f.dir).is_empty());
}

#[test]
fn missing_git_keeps_auth_and_repository_error_kinds() {
    let f = Fixture::new();
    let empty = f.dir.join("empty-path");
    fs::create_dir(&empty).unwrap();
    for (args, kind) in [
        (
            vec!["auth", "check", "https://example.invalid/repo.git"],
            "auth",
        ),
        (vec!["install", f.repo.to_str().unwrap()], "git"),
    ] {
        let output = f.pm().env("PATH", &empty).args(&args).output().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{args:?}");
        assert!(
            stderr.contains(&format!("Error kind: {kind}")),
            "{args:?}: {stderr}"
        );
    }
}

#[cfg(unix)]
#[test]
fn auth_check_reports_config_probe_timeout_without_running_ls_remote() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let bin = f.dir.join("fakebin");
    fs::create_dir(&bin).unwrap();
    let fifo = f.dir.join("probe.fifo");
    assert!(f.command("mkfifo").arg(&fifo).status().unwrap().success());
    let wrapper = bin.join("git");
    // With no writer, opening the FIFO blocks until gitpm kills the probe.
    fs::write(&wrapper, "#!/bin/sh\ncase \" $* \" in\n*' config --get core.sshCommand '*) exec /bin/cat \"$GITPMT_PROBE_FIFO\";;\n*' ls-remote '*) printf fallback > \"$GITPMT_FALLBACK\"; exit 0;;\nesac\nexit 1\n").unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let fallback = f.dir.join("fallback");
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let output = f
        .pm()
        .env("PATH", path)
        .env("GITPMT_PROBE_FIFO", fifo)
        .env("GITPMT_FALLBACK", &fallback)
        .args(["auth", "check", "https://example.invalid/repo.git"])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "probe timeout was swallowed: {stderr}"
    );
    assert!(stderr.contains("Error kind: git"), "{stderr}");
    assert!(
        stderr.contains("git config --get core.sshCommand: timed out after 10s"),
        "{stderr}"
    );
    assert!(!fallback.exists(), "ls-remote ran after the failed probe");
}

#[cfg(unix)]
#[test]
fn custom_ssh_is_shared_by_auth_install_catalog_and_check() {
    use std::os::unix::fs::PermissionsExt;
    for variable in ["GIT_SSH", "GIT_SSH_COMMAND"] {
        let f = Fixture::new();
        let helper = f.dir.join("local-ssh");
        let log = f.dir.join("ssh.log");
        fs::write(&helper, "#!/bin/sh\ncase \"$1\" in -G) exit 1;; esac\nprintf '%s\\n' \"$*\" >> \"$GITPMT_SSH_LOG\"\nexec git-upload-pack \"$GITPMT_SSH_REPO\"\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        let source = "git@example.invalid:repo.git";
        fs::write(f.dir.join("sources.txt"), format!("{source}\n")).unwrap();
        let mut previous = 0;
        for args in [
            vec!["auth", "check", source],
            vec!["install", source],
            vec!["check"],
            vec!["catalog", "update"],
            vec!["catalog", "register", source, "--ref", "dan/hello@1.0.0"],
        ] {
            let mut command = f.pm();
            command
                .current_dir(&f.dir)
                .env(variable, &helper)
                .env("GITPMT_SSH_LOG", &log)
                .env("GITPMT_SSH_REPO", &f.repo)
                .args(&args);
            if args == ["check"] {
                // Exercise origin lookup without the fixture's not-yet-built catalog.
                command.env_remove("GITPM_CATALOG");
            }
            if variable == "GIT_SSH_COMMAND" {
                command.env("GIT_SSH", "/nonexistent-must-not-be-used");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{variable} {args:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let calls = fs::read_to_string(&log).unwrap().lines().count();
            assert!(
                calls > previous,
                "{variable} {args:?} must use the SSH helper"
            );
            previous = calls;
        }
        assert_eq!(f.content(), "hello v1");
    }
}

#[test]
fn gitpm_variables_set_root_catalog_and_sources() {
    let f = Fixture::new();
    fs::write(f.dir.join("sources.txt"), "./upstream\n").unwrap();
    // pm() sets GITPM_ROOT, GITPM_CATALOG and GITPM_SOURCES; no flags here.
    let out = f
        .pm()
        .current_dir(&f.dir)
        .args(["catalog", "update"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        f.dir
            .join("catalog/namespaces/dan/hello/plugin.json")
            .exists()
    );
    let out = f
        .pm()
        .current_dir(&f.dir)
        .args(["install-catalog", "dan/hello"])
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
    // The removed `sync` command (sync-dist is another command).
    assert!(!help.contains("  sync "));
    assert!(help.contains("  sync-dist "));
    assert!(!help.contains("--publish"));
    for args in [
        &["sync"][..],
        &["catalog", "update", "--publish"][..],
        &["sync", "--dry-run"][..],
    ] {
        let out = f.pm().args(args).output().unwrap();
        assert!(!out.status.success(), "{args:?} succeeded");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains("unknown"), "{args:?}: {stderr}");
    }
    // Removed commands and aliases are unknown commands, not hidden ones.
    assert!(
        !help
            .lines()
            .any(|l| l.starts_with("  create ") || l.contains("migrate-catalog"))
    );
    for command in [
        "create",
        "migrate-catalog",
        "info",
        "catalog-update",
        "register",
    ] {
        let out = f
            .pm()
            .args([command, "dan/hello", "--into", "x"])
            .output()
            .unwrap();
        assert!(!out.status.success());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(&format!("unknown command: {command}; use --help")),
            "{stderr}"
        );
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
        preview.contains("dan/hello would be updated 1.0.0 → 2.0.0"),
        "{preview}"
    );
    assert!(preview.contains("Nothing was changed."));
    assert_eq!(f.state(), before);
    assert_eq!(f.content(), "hello v1");
    for args in [
        vec!["update", "all", "--ref", "main"],
        vec!["update", "all", "--dev"],
        vec!["update", "dan/hello", "--dry-run"],
    ] {
        f.cli(&args, false);
        assert_eq!(f.state(), before);
    }
    let out = f.cli(&["update", "all"], true);
    assert!(out.contains("1 updates, 0 installs"), "{out}");
    assert!(out.contains("dan/hello updated 1.0.0 → 2.0.0"), "{out}");
    assert_eq!(f.content(), "hello v2");
    let out = f.cli(&["update", "all"], true);
    assert!(out.contains("0 updates, 0 installs"), "{out}");
    assert!(out.contains("dan/hello up to date (2.0.0)"), "{out}");
}

#[test]
fn install_again_reports_already_installed_and_changes_nothing() {
    let f = Fixture::new();
    f.install();
    let source = f.repo.to_str().unwrap();
    let state = || fs::read(f.root.join("state.json")).unwrap();
    let dist = f.root.join("dist/dan/hello/index.js");
    let (before, modified) = (state(), fs::metadata(&dist).unwrap().modified().unwrap());
    // Same source, same revision.
    assert_eq!(
        f.cli(&["install", source], true),
        "dan/hello already installed (1.0.0)\n"
    );
    assert_eq!(state(), before);
    assert_eq!(fs::metadata(&dist).unwrap().modified().unwrap(), modified);
    // Same source, a newer release: not installed, update moves to it.
    f.release("1.1.0", "hello v2");
    assert_eq!(
        f.cli(&["install", source], true),
        "dan/hello already installed (1.0.0); run update\n"
    );
    assert_eq!(state(), before);
    assert!(f.state()["packages"]["dan/hello"]["previous"].is_null());
    // An explicit --ref or --dev is named in the hint, never silently ignored.
    assert_eq!(
        f.cli(&["install", source, "--ref", "dan/hello@1.1.0"], true),
        "dan/hello already installed (1.0.0); requested --ref dan/hello@1.1.0 — run update dan/hello --ref dan/hello@1.1.0\n"
    );
    // Same revision, another mode (the tag would pin it): named too.
    assert_eq!(
        f.cli(&["install", source, "--ref", "dan/hello@1.0.0"], true),
        "dan/hello already installed (1.0.0); requested --ref dan/hello@1.0.0 — run update dan/hello --ref dan/hello@1.0.0\n"
    );
    assert_eq!(
        f.cli(&["install", source, "--dev"], true),
        "dan/hello already installed (1.0.0); requested --dev — run update dan/hello --dev\n"
    );
    assert_eq!(state(), before);
    assert_eq!(f.content(), "hello v1");
    assert!(!f.root.join(".pm-txn").exists());
    // Another source with the same ID: refused as before.
    let other = f.dir.join("other");
    let out = f
        .command("git")
        .args(["clone", "-q", source])
        .arg(&other)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = f.pm().arg("install").arg(&other).output().unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("already installed: dan/hello")
            && stderr.contains("Error kind: already_installed"),
        "{stderr}"
    );
    assert_eq!(state(), before);
    assert_eq!(
        f.cli(&["update", "dan/hello"], true),
        "dan/hello updated 1.0.0 → 1.1.0\n"
    );
}

#[test]
fn mutating_commands_print_one_line_per_package_and_no_ok_lines() {
    let f = Fixture::new();
    f.install();
    f.release("1.1.0", "hello v2");
    f.cli(&["update", "dan/hello"], true);
    for (args, expected) in [
        (
            &["rollback", "dan/hello"][..],
            "dan/hello rolled back 1.1.0 → 1.0.0\n",
        ),
        (
            &["update", "dan/hello", "--ref", "dan/hello@1.0.0"][..],
            "dan/hello pinned at 1.0.0 (dan/hello@1.0.0)\n",
        ),
        (
            &["update", "dan/hello"][..],
            "dan/hello pinned at 1.0.0 (dan/hello@1.0.0)\n",
        ),
        (
            &["update", "dan/hello", "--release"][..],
            "dan/hello updated 1.0.0 → 1.1.0\n",
        ),
        (
            &["rollback", "dan/hello"][..],
            "dan/hello rolled back 1.1.0 → 1.0.0\n",
        ),
        (&["disable", "dan/hello"][..], "dan/hello disabled\n"),
        (&["enable", "dan/hello"][..], "dan/hello enabled\n"),
        (
            &["forget", "dan/hello"][..],
            "dan/hello forgotten (clone kept; restore re-registers it)\n",
        ),
        (&["restore", "dan/hello"][..], "dan/hello restored 1.0.0\n"),
        (&["uninstall", "dan/hello"][..], "dan/hello uninstalled\n"),
    ] {
        assert_eq!(f.cli(args, true), expected, "{args:?}");
    }
    let source = fs::canonicalize(&f.repo).unwrap();
    let source = source.to_str().unwrap();
    assert_eq!(
        f.cli(&["download", source], true),
        format!("downloaded dan/hello@1.1.0 from {source} (disabled)\n")
    );
    assert!(!f.root.join("dist/dan/hello").exists());
    assert_eq!(f.cli(&["enable", "dan/hello"], true), "dan/hello enabled\n");
    assert_eq!(f.content(), "hello v2");
}

#[test]
fn command_help_prints_only_that_command() {
    let f = Fixture::new();
    let full = f.cli(&["--help"], true);
    for flag in ["--help", "-h"] {
        let init = f.cli(&["init", flag], true);
        assert!(
            init.contains("--multi") && init.contains("--no-release"),
            "{init}"
        );
        assert!(
            init.starts_with("  init ") && !init.contains("sync-dist"),
            "{init}"
        );
        assert!(full.contains(init.trim_end()));
    }
    let sync = f.cli(&["sync-dist", "--help"], true);
    assert!(
        sync.contains("--dry-run") && sync.starts_with("  sync-dist "),
        "{sync}"
    );
    let list = f.cli(&["list", "--help"], true);
    assert!(
        list.contains("--provides") && !list.contains("install GIT_SOURCE"),
        "{list}"
    );
    let update = f.cli(&["update", "-h"], true);
    assert!(
        update.contains("update ID") && update.contains("update all"),
        "{update}"
    );
    for flag in ["--help", "-h"] {
        let group = f.cli(&["catalog", flag], true);
        for subcommand in ["show", "links", "update", "register"] {
            let single = f.cli(&["catalog", subcommand, flag], true);
            assert!(
                single.starts_with(&format!("  catalog {subcommand} ")),
                "{single}"
            );
            assert!(group.contains(single.trim_end()));
            assert!(full.contains(single.trim_end()));
            assert_eq!(
                single
                    .lines()
                    .filter(|l| l.starts_with("  catalog "))
                    .count(),
                1
            );
        }
    }
    let out = f.pm().args(["nope", "--help"]).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown command: nope"));
    assert!(!f.root.exists());
}

impl Fixture {
    /// Run the CLI; returns (success, stdout, stderr).
    fn output(&self, args: &[&str]) -> (bool, String, String) {
        let out = self
            .pm()
            .current_dir(&self.dir)
            .arg("--root")
            .arg(&self.root)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

/// The CLI passes the catalog location to the library unchanged: a missing local catalog
/// is a library error with a kind; `--ref`, `--dev` and `--release` exclude each other
/// as usage errors, and `--channel` is gone.
#[test]
fn a_missing_catalog_is_a_library_error_and_targets_are_exclusive() {
    let f = Fixture::new();
    let source = f.repo.to_str().unwrap();
    let (ok, _, stderr) = f.output(&["--catalog", "./missing", "install", source]);
    assert!(!ok);
    assert!(
        stderr.contains("Error: catalog location ./missing does not exist"),
        "{stderr}"
    );
    assert!(stderr.contains("Error kind: invalid_argument"), "{stderr}");
    assert!(
        !f.root.exists(),
        "a refused catalog location created the workspace"
    );
    for (args, message) in [
        (
            vec!["install", source, "--ref", "x", "--dev"],
            "Error: --dev conflicts with --ref",
        ),
        (
            vec!["update", "dan/hello", "--dev", "--release"],
            "Error: --release conflicts with --dev",
        ),
        (
            vec!["update", "dan/hello", "--ref", "x", "--release"],
            "Error: --release conflicts with --ref",
        ),
        (
            vec!["install", source, "--release"],
            "Error: --release requires update",
        ),
        (
            vec!["install", source, "--channel", "dev"],
            "Error: unknown option: --channel; use --help",
        ),
        (
            vec!["update", "all", "--release"],
            "Error: update all takes no --ref, --dev or --release; it follows each package's recorded mode",
        ),
    ] {
        let (ok, _, stderr) = f.output(&args);
        assert!(!ok, "{args:?}");
        assert!(stderr.contains(message), "{args:?}: {stderr}");
        assert!(!stderr.contains("Error kind"), "a usage error: {stderr}");
        assert!(!f.root.exists(), "{args:?} created the workspace");
    }
    // A tag pins the package.
    let (ok, stdout, stderr) = f.output(&["install", source, "--ref", "dan/hello@1.0.0"]);
    assert!(ok, "{stderr}");
    assert!(
        stdout.starts_with("installed dan/hello@1.0.0 from "),
        "{stdout}"
    );
}

/// `list` reads the committed state while another process holds the workspace lock.
#[test]
fn list_works_while_the_workspace_is_locked() {
    let f = Fixture::new();
    f.install();
    let manager = gitpm::Manager::open(&f.root).unwrap();
    let state = f.state();
    assert_eq!(state["resolved"][0]["id"], "dan/hello");
    let (ok, _, stderr) = f.output(&["enable", "dan/hello"]);
    assert!(!ok);
    assert!(stderr.contains("Error kind: locked"), "{stderr}");
    drop(manager);
}

/// Library messages are neutral; the CLI adds the command that helps as a `Hint:` line.
#[test]
fn errors_print_a_command_hint_by_kind() {
    let f = Fixture::new();
    f.install();
    f.cli(&["forget", "dan/hello"], true);
    let (ok, _, stderr) = f.output(&["install", f.repo.to_str().unwrap()]);
    assert!(!ok);
    assert!(
        stderr.contains("Error: dan/hello was forgotten and its clone is kept\n"),
        "{stderr}"
    );
    assert!(stderr.contains("Error kind: already_installed"), "{stderr}");
    assert!(
        stderr.contains("Hint: ")
            && stderr.contains("`gitpm restore dan/hello`")
            && stderr.contains("`gitpm uninstall dan/hello`"),
        "{stderr}"
    );
    // A usage error has no kind and no hint.
    let (ok, _, stderr) = f.output(&["list", "--force"]);
    assert!(!ok);
    assert!(
        !stderr.contains("Error kind") && !stderr.contains("Hint:"),
        "{stderr}"
    );
}

/// `update` reports broken release tags it passed over, as install does.
#[test]
fn update_past_a_broken_historical_tag_warns_about_it() {
    let f = Fixture::new();
    f.install();
    // 1.5.0: a tag whose manifest is invalid; 2.0.0: valid.
    fs::write(f.repo.join("plugin.json"), "{broken").unwrap();
    f.git(&["add", "."]);
    f.git(&["commit", "-m", "broken"]);
    f.git(&["tag", "dan/hello@1.5.0"]);
    f.release("2.0.0", "hello v2");
    let (ok, stdout, stderr) = f.output(&["update", "dan/hello"]);
    assert!(ok, "{stderr}");
    assert_eq!(stdout, "dan/hello updated 1.0.0 → 2.0.0 (major)\n");
    assert!(
        stderr.contains("Warning: skipped release tag dan/hello@1.5.0: invalid manifest"),
        "{stderr}"
    );
    f.cli(&["rollback", "dan/hello"], true);
    f.release("2.1.0", "hello v2.1");
    let (ok, stdout, stderr) = f.output(&["update", "all"]);
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains("dan/hello updated 1.0.0 → 2.1.0"),
        "{stdout}"
    );
    assert!(
        stderr.contains("Warning: skipped release tag dan/hello@1.5.0: invalid manifest"),
        "{stderr}"
    );
}

/// `--ref` names exact refs only; the already-installed line says the recorded mode.
#[test]
fn refs_are_exact_and_the_mode_is_named() {
    let f = Fixture::new();
    let source = f.repo.to_str().unwrap();
    for bad in ["HEAD~1", "main~1", "dan/hello@1.0.0~1"] {
        let (ok, _, stderr) = f.output(&["install", source, "--ref", bad]);
        assert!(!ok, "{bad}");
        assert!(
            stderr.contains("Error kind: invalid_argument"),
            "{bad}: {stderr}"
        );
    }
    f.cli(&["install", source, "--ref", "dan/hello@1.0.0"], true);
    assert_eq!(
        f.cli(&["install", source, "--ref", "dan/hello@1.0.0"], true),
        "dan/hello already installed (1.0.0, pinned at dan/hello@1.0.0)\n"
    );
    assert_eq!(
        f.cli(&["install", source, "--ref", "HEAD"], true),
        "dan/hello already installed (1.0.0, pinned at dan/hello@1.0.0); requested --ref HEAD — run update dan/hello --ref HEAD\n"
    );
    assert_eq!(
        f.cli(&["update", "dan/hello", "--ref", "HEAD"], true),
        "dan/hello follows branch main at 1.0.0\n"
    );
}

#[test]
fn verified_catalog_protects_updates_and_detach_drops_it() {
    let f = Fixture::new();
    let catalog_dir = f.dir.join("verified-catalog");
    let location = catalog_dir.to_str().unwrap();
    let repo = f.repo.to_str().unwrap();
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            repo,
            "--ref",
            "dan/hello@1.0.0",
        ],
        true,
    );
    // Unverified: discovery only.
    f.cli(
        &["--catalog", location, "install-catalog", "dan/hello"],
        true,
    );
    let out = f.cli(&["--catalog", location, "check"], true);
    assert!(
        out.contains(&format!("Catalog: {location} (unverified: discovery only)")),
        "{out}"
    );
    assert!(f.state()["resolved"][0].get("catalog").is_none());
    f.cli(&["uninstall", "dan/hello"], true);

    // The owner marks the catalog verified by hand.
    let file = catalog_dir.join("catalog.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    value["verified"] = serde_json::Value::Bool(true);
    fs::write(&file, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    fs::write(f.dir.join("sources.txt"), format!("{repo}\n")).unwrap();
    let before = fs::read(&file).unwrap();
    f.cli(&["--catalog", location, "catalog", "update"], false);
    assert_eq!(fs::read(&file).unwrap(), before);
    let info = f.cli(&["--catalog", location, "catalog", "show"], true);
    assert!(info.contains("Verified: yes"), "{info}");

    f.cli(
        &["--catalog", location, "install-catalog", "dan/hello"],
        true,
    );
    assert!(f.state()["resolved"][0]["catalog"].is_string());
    f.release("1.1.0", "hello v2");
    // No catalog given: the recorded verified catalog is read.
    let out = f.cli_without_catalog(&["update", "dan/hello"], true);
    assert!(
        out.contains("dan/hello kept at 1.0.0 (upstream 1.1.0 not in catalog)"),
        "{out}"
    );
    let out = f.cli(&["--catalog", location, "check"], true);
    assert!(
        out.contains(&format!("Catalog: {location} (verified)")),
        "{out}"
    );
    assert!(out.contains("upstream 1.1.0 not in catalog"), "{out}");
    f.cli(&["update", "all", "--detach"], false);
    f.cli(&["install", repo, "--detach"], false);

    let out = f.cli_without_catalog(&["update", "dan/hello", "--detach"], true);
    assert!(
        out.contains("dan/hello detached from catalog") && out.contains("updated 1.0.0 → 1.1.0"),
        "{out}"
    );
    assert!(f.state()["resolved"][0].get("catalog").is_none());
    assert_eq!(f.content(), "hello v2");
    f.cli(&["update", "dan/hello", "--detach"], false);
}

/// `update` marks a major update, passes over pre-releases (`check` names them) and
/// `install-catalog --version` takes one by name; `update` then follows its track.
#[test]
fn update_lines_mark_major_updates() {
    let f = Fixture::new();
    let row = |out: &str| {
        out.lines()
            .find(|l| l.starts_with("dan/hello "))
            .unwrap_or_else(|| panic!("no row: {out}"))
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    f.cli_without_catalog(&["install", f.repo.to_str().unwrap()], true);
    f.release("1.1.0", "hello v1.1");
    assert_eq!(
        f.cli_without_catalog(&["update", "dan/hello"], true),
        "dan/hello updated 1.0.0 → 1.1.0\n"
    );
    f.release("2.0.0-beta.1", "hello v2 beta");
    assert_eq!(
        f.cli_without_catalog(&["update", "dan/hello"], true),
        "dan/hello up to date (1.1.0)\n"
    );
    let out = f.cli_without_catalog(&["check"], true);
    assert_eq!(
        row(&out),
        "dan/hello 1.1.0 1.1.0 yes origin up to date; pre-release 2.0.0-beta.1"
    );
    f.release("2.0.0", "hello v2");
    let out = f.cli_without_catalog(&["check"], true);
    assert_eq!(
        row(&out),
        "dan/hello 1.1.0 2.0.0 (major) yes origin update available"
    );
    assert_eq!(
        f.cli_without_catalog(&["update", "all"], true),
        "dan/hello updated 1.1.0 → 2.0.0 (major)\n1 updates, 0 installs, 0 kept back.\n"
    );

    // A catalog pre-release is installed by name and then followed to its release.
    let other = Fixture::new();
    let location = other.dir.join("cat");
    let location = location.to_str().unwrap();
    other.release("1.1.0-rc.1", "hello rc1");
    other.release("1.1.0-rc.2", "hello rc2");
    let repo = other.repo.to_str().unwrap();
    for tag in [
        "dan/hello@1.0.0",
        "dan/hello@1.1.0-rc.1",
        "dan/hello@1.1.0-rc.2",
    ] {
        other.cli(
            &[
                "--catalog",
                location,
                "catalog",
                "register",
                repo,
                "--ref",
                tag,
            ],
            true,
        );
    }
    let out = other.cli(
        &["--catalog", location, "install-catalog", "dan/hello"],
        true,
    );
    assert!(out.contains("installed dan/hello@1.0.0 from"), "{out}");
    other.cli(&["--catalog", location, "uninstall", "dan/hello"], true);
    let out = other.cli(
        &[
            "--catalog",
            location,
            "install-catalog",
            "dan/hello",
            "--version",
            "1.1.0-rc.1",
        ],
        true,
    );
    assert!(out.contains("installed dan/hello@1.1.0-rc.1 from"), "{out}");
    assert_eq!(
        other.cli(&["--catalog", location, "update", "dan/hello"], true),
        "dan/hello updated 1.1.0-rc.1 → 1.1.0-rc.2\n"
    );
    other.cli(
        &[
            "--catalog",
            location,
            "install-catalog",
            "dan/hello",
            "--version",
            "1.1.0+build",
        ],
        false,
    );
}

/// A catalog package with pre-releases only: `info` says so, an install without
/// `--version` is refused with `catalog`, `--version` installs the pre-release.
#[test]
fn catalog_package_with_pre_releases_only() {
    let f = Fixture::new();
    f.git(&["tag", "-d", "dan/hello@1.0.0"]);
    f.release("2.0.0-beta.1", "hello beta");
    let location = f.dir.join("cat");
    let location = location.to_str().unwrap();
    let repo = f.repo.to_str().unwrap();
    f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            repo,
            "--ref",
            "dan/hello@2.0.0-beta.1",
        ],
        true,
    );
    let info = f.cli(&["--catalog", location, "catalog", "show"], true);
    assert!(info.contains("latest: pre-releases only"), "{info}");
    let out = f
        .pm()
        .current_dir(&f.dir)
        .args(["--root", f.root.to_str().unwrap(), "--catalog", location])
        .args(["install-catalog", "dan/hello"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("only pre-release versions"), "{stderr}");
    let out = f.cli(
        &[
            "--catalog",
            location,
            "install-catalog",
            "dan/hello",
            "--version",
            "2.0.0-beta.1",
        ],
        true,
    );
    assert!(
        out.contains("installed dan/hello@2.0.0-beta.1 from"),
        "{out}"
    );
}

#[test]
fn update_all_local_work_prints_the_library_message_unchanged() {
    let f = Fixture::new();
    f.install();
    let repo = f.root.join("packages/dan/hello");
    let git = |args: &[&str]| f.git_in(&repo, args);
    fs::write(repo.join("local.txt"), "local work").unwrap();
    git(&["add", "local.txt"]);
    git(&["commit", "-qm", "local"]);
    git(&["tag", "local-only"]);
    git(&["reset", "--hard", "HEAD~1"]);
    let error = {
        let manager = gitpm::Manager::open(&f.root).unwrap();
        manager.update_all().unwrap_err()
    };
    let (ok, out, err) = f.output(&["update", "all"]);
    assert!(!ok);
    assert!(out.is_empty(), "{out}");
    assert_eq!(err, format!("Error: {error}\nError kind: local_changes\n"));
    assert!(
        err.contains(
            "dan/hello: unpublished local work blocks update all; unpublished tag: local-only"
        ),
        "{err}"
    );
}

#[test]
fn catalog_group_runs_every_subcommand() {
    let f = Fixture::new();
    f.release("1.1.0", "hello v2");
    let sources = f.dir.join("group-sources.txt");
    fs::write(&sources, format!("{}\n", f.repo.display())).unwrap();
    let location = f.dir.join("group-catalog");
    let location = location.to_str().unwrap();
    let out = f.cli(
        &[
            "catalog",
            "update",
            "--catalog",
            location,
            "--sources",
            sources.to_str().unwrap(),
        ],
        true,
    );
    assert_eq!(out, "Catalog ready: 2 release(s) checked from sources.\n");
    f.release("1.2.0", "hello v3");
    let out = f.cli(
        &[
            "--catalog",
            location,
            "catalog",
            "register",
            f.repo.to_str().unwrap(),
            "--ref",
            "dan/hello@1.2.0",
            "--package",
            "dan/hello",
            "--build-url",
            "https://example.invalid/build.tar.gz",
            "--build-sha256",
            &"a".repeat(64),
        ],
        true,
    );
    let release: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(release["manifest"]["id"], "dan/hello");
    let show = f.cli(&["--catalog", location, "catalog", "show"], true);
    assert!(
        show.contains("dan/hello — latest: 1.2.0, versions: 3"),
        "{show}"
    );
    let links = f.cli(&["--catalog", location, "catalog", "links"], true);
    let links: serde_json::Value = serde_json::from_str(&links).unwrap();
    assert_eq!(links["schema"], 1);
    assert!(!f.root.exists());
}

#[test]
fn catalog_group_errors_create_nothing() {
    let f = Fixture::new();
    let empty = f.dir.join("empty");
    fs::create_dir(&empty).unwrap();
    for (args, message) in [
        (
            vec!["catalog"],
            "catalog needs a subcommand: show, links, update or register; use gitpm catalog --help",
        ),
        (
            vec!["catalog", "publish"],
            "unknown catalog command: publish; use gitpm catalog --help",
        ),
        (
            vec!["catalog", "publish", "--help"],
            "unknown catalog command: publish; use gitpm catalog --help",
        ),
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_gitpm"))
            .env_clear()
            .current_dir(&empty)
            .args(["--root", "workspace", "--catalog", "catalog-dir"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            format!("Error: {message}\n")
        );
        assert!(out.stdout.is_empty());
        assert_eq!(fs::read_dir(&empty).unwrap().count(), 0);
    }
}

#[test]
fn catalog_update_http_error_has_a_command_hint() {
    let f = Fixture::new();
    let (ok, out, err) = f.output(&[
        "--catalog",
        "https://example.invalid/catalog.json",
        "catalog",
        "update",
    ]);
    assert!(!ok);
    assert!(out.is_empty());
    assert!(
        err.contains("writing a catalog requires a local folder; HTTP(S) catalogs are read-only"),
        "{err}"
    );
    assert!(err.contains("Error kind: invalid_argument"), "{err}");
    assert!(
        err.contains(
            "Hint: `gitpm catalog update` writes a local catalog folder; upload its files yourself"
        ),
        "{err}"
    );
    assert!(!f.root.exists());
}

#[test]
fn file_formats_use_version_one_and_refuse_other_versions() {
    let f = Fixture::new();
    fs::write(f.dir.join("sources.txt"), f.repo.to_str().unwrap()).unwrap();
    f.cli(&["catalog", "update"], true);
    f.cli(&["install", f.repo.to_str().unwrap()], true);
    for (path, field, args, kind) in [
        (
            f.root.join("state.json"),
            "format",
            vec!["list"],
            "unsupported",
        ),
        (
            f.dir.join("catalog/catalog.json"),
            "schema",
            vec!["catalog", "links"],
            "catalog",
        ),
        (
            f.dir.join("catalog/namespaces/dan/hello/plugin.json"),
            "schema",
            vec!["catalog", "show"],
            "catalog",
        ),
    ] {
        let original = fs::read(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
        assert_eq!(value[field], 1, "{}", path.display());
        f.cli(&args, true);
        for version in [0, 2, 3, 99] {
            value[field] = version.into();
            let bytes = serde_json::to_vec_pretty(&value).unwrap();
            fs::write(&path, &bytes).unwrap();
            let out = f.pm().args(&args).output().unwrap();
            assert!(!out.status.success(), "{args:?}: version {version}");
            let error = String::from_utf8(out.stderr).unwrap();
            assert!(error.contains(kind), "{error}");
            assert_eq!(
                fs::read(&path).unwrap(),
                bytes,
                "input must not be rewritten"
            );
        }
        fs::write(&path, original).unwrap();
    }
}
