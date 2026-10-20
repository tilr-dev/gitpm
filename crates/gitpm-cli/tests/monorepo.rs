//! End-to-end tests of multi-package repositories (`gitpm.json`) and `fork --replace`.
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    dir: PathBuf,
    mono: PathBuf,
}
impl Fixture {
    /// Repository `mono` with `tilr/store` and `tilr/theme` at 1.0.0 (one commit, two tags).
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-mono-{}-{}",
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
        let f = Self {
            mono: dir.join("mono"),
            dir,
        };
        fs::create_dir_all(&f.mono).unwrap();
        f.git(&f.mono, &["init", "-q", "-b", "main"]);
        fs::write(f.mono.join("gitpm.json"), r#"{"packages": ["packages/*"]}"#).unwrap();
        f.write("store", "1.0.0", "");
        f.write("theme", "1.0.0", "");
        f.git(&f.mono, &["add", "--all"]);
        f.git(&f.mono, &["commit", "-q", "-m", "first"]);
        f.git(&f.mono, &["tag", "tilr/store@1.0.0"]);
        f.git(&f.mono, &["tag", "tilr/theme@1.0.0"]);
        f
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
    fn write(&self, name: &str, version: &str, extra: &str) {
        let folder = self.mono.join("packages").join(name);
        fs::create_dir_all(folder.join("dist")).unwrap();
        fs::write(
            folder.join("plugin.json"),
            format!(r#"{{"id":"tilr/{name}","version":"{version}"{extra}}}"#),
        )
        .unwrap();
        fs::write(folder.join("dist/index.js"), format!("{name} {version}")).unwrap();
    }
    /// Commit a new version of one package and tag only that package.
    fn release(&self, name: &str, version: &str) {
        self.write(name, version, "");
        self.git(&self.mono, &["add", "--all"]);
        self.git(
            &self.mono,
            &["commit", "-q", "-m", &format!("{name} {version}")],
        );
        self.git(&self.mono, &["tag", &format!("tilr/{name}@{version}")]);
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_gitpm"))
            .current_dir(&self.dir)
            .arg("--root")
            .arg(self.dir.join("workspace"))
            .arg("--catalog")
            .arg(self.dir.join("catalog"))
            .env("GITPM_SOURCES", self.dir.join("sources.txt"))
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
    fn source(&self) -> &str {
        self.mono.to_str().unwrap()
    }
    fn state(&self) -> serde_json::Value {
        serde_json::from_str(&self.ok(&["list"])).unwrap()
    }
    fn dist(&self, id: &str) -> String {
        fs::read_to_string(self.dir.join("workspace/dist").join(id).join("index.js")).unwrap()
    }
    fn clones(&self) -> usize {
        fs::read_dir(self.dir.join("workspace/repos"))
            .map(|entries| entries.count())
            .unwrap_or(0)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Hooks live outside the copied repository and apply to its initial and release commits.
#[cfg(unix)]
fn release_hooks(f: &Fixture, post_commit: &str, pre_commit: &str) {
    use std::os::unix::fs::PermissionsExt;
    let hooks = f.dir.join("hooks");
    fs::create_dir_all(&hooks).unwrap();
    for (name, script) in [("post-commit", post_commit), ("pre-commit", pre_commit)] {
        let path = hooks.join(name);
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{script}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    f.git(
        &f.mono,
        &[
            "config",
            "--global",
            "core.hooksPath",
            hooks.to_str().unwrap(),
        ],
    );
}

fn release_failure(f: &Fixture) -> (PathBuf, String) {
    let out = f.run(&[
        "fork",
        "tilr/store",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
        "--replace",
        "--version",
        "2.0.0",
    ]);
    assert!(!out.status.success(), "release must fail");
    let stderr = String::from_utf8(out.stderr).unwrap();
    for id in ["me/store", "me/theme"] {
        assert!(
            stderr.contains(&format!("{id}: the release step failed")),
            "{stderr}"
        );
        assert!(f.state()["packages"].get(id).is_none());
    }
    assert!(f.state().get("replacements").is_none());
    assert_eq!(f.dist("tilr/store"), "store 1.0.0");
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    let fork = f.dir.join("forks/me/mono");
    let tags = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .arg("tag")
        .arg("--list")
        .arg("me/*")
        .output()
        .unwrap();
    assert!(tags.status.success());
    assert!(
        tags.stdout.is_empty(),
        "partial tags: {}",
        String::from_utf8_lossy(&tags.stdout)
    );
    (fork, stderr)
}

#[test]
#[cfg(unix)]
fn fork_replace_release_rejecting_hook_restores_manifests_and_index() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    release_hooks(
        &f,
        r#"
case "$(git log -1 --format=%s)" in
    Fork*)
        printf 'staged before release' > unrelated.txt
        git add unrelated.txt
        cp packages/store/plugin.json .git/store.before
        cp packages/theme/plugin.json .git/theme.before
        cp .git/index .git/index.before
        ;;
esac
"#,
        "exit 1",
    );
    let (fork, stderr) = release_failure(&f);
    assert!(stderr.contains("kind: git"), "{stderr}");
    for name in ["store", "theme"] {
        assert_eq!(
            fs::read(fork.join(format!("packages/{name}/plugin.json"))).unwrap(),
            fs::read(fork.join(format!(".git/{name}.before"))).unwrap()
        );
    }
    // Read raw index before any Git status invocation can refresh its stat cache.
    assert!(
        fs::read(fork.join(".git/index")).unwrap()
            == fs::read(fork.join(".git/index.before")).unwrap(),
        "index differs from its pre-release snapshot"
    );
    let staged = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .args(["diff", "--cached", "--name-only"])
        .output()
        .unwrap();
    assert!(staged.status.success());
    assert_eq!(
        String::from_utf8(staged.stdout).unwrap().trim(),
        "unrelated.txt"
    );
    let log = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .args(["log", "-1", "--format=%s"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).starts_with("Fork"));
}

#[test]
fn fork_replace_release_rejects_wrong_metadata_type() {
    let f = Fixture::new();
    // Lenient install accepts this metadata; strict release must reject it even
    // when it belongs to the last package in the batch.
    f.write("theme", "1.0.0", r#", "tags": 42"#);
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-q", "-m", "bad metadata"]);
    f.git(&f.mono, &["tag", "-f", "tilr/theme@1.0.0"]);
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    let (fork, stderr) = release_failure(&f);
    assert!(stderr.contains("kind: invalid_manifest"), "{stderr}");
    for name in ["store", "theme"] {
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(fork.join(format!("packages/{name}/plugin.json"))).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["version"], "1.0.0");
    }
}

#[test]
#[cfg(unix)]
fn fork_replace_release_without_file_transport() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    let output = f
        .command(env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(&f.dir)
        .arg("--root")
        .arg(f.dir.join("workspace"))
        .arg("--catalog")
        .arg(f.dir.join("catalog"))
        .env("GITPM_SOURCES", f.dir.join("sources.txt"))
        .env("GIT_PROTOCOL_FROM_USER", "0")
        .args([
            "fork",
            "tilr/store",
            "tilr/theme",
            "--ns",
            "me",
            "--into",
            "forks/me/mono",
            "--replace",
            "--version",
            "2.0.0",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    // The following install needs file transport; release itself must complete.
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("the install step failed"), "{stderr}");
    assert!(!stderr.contains("the release step failed"), "{stderr}");
    let fork = f.dir.join("forks/me/mono");
    for name in ["store", "theme"] {
        f.git(
            &fork,
            &["diff", "--exit-code", "HEAD", &format!("me/{name}@2.0.0")],
        );
    }
}

#[test]
#[cfg(unix)]
fn fork_replace_release_late_tag_lock_leaves_no_tags() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    release_hooks(
        &f,
        ":",
        r#"
mkdir -p .git/refs/tags/me
: > .git/refs/tags/me/theme@2.0.0.lock
"#,
    );
    let (fork, stderr) = release_failure(&f);
    assert!(stderr.contains("kind: git"), "{stderr}");
    assert!(stderr.contains("theme@2.0.0.lock"), "{stderr}");
    let log = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .args(["log", "-1", "--format=%s"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&log.stdout).starts_with("Release"));
}

#[test]
fn two_packages_of_one_repository_share_one_clone_and_update_independently() {
    let f = Fixture::new();
    // Positional ID and --package are equivalent.
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "--package", "tilr/theme"]);
    assert_eq!(f.dist("tilr/store"), "store 1.0.0");
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    assert_eq!(f.clones(), 1);
    assert!(!f.dir.join("workspace/packages/tilr").exists());
    let state = f.state();
    assert_eq!(
        state["packages"]["tilr/store"]["path"], "packages/store",
        "{state:#}"
    );
    assert_eq!(
        state["packages"]["tilr/store"]["repo"],
        state["packages"]["tilr/theme"]["repo"]
    );

    // Releasing A moves only A.
    f.release("store", "1.1.0");
    f.ok(&["update", "tilr/store"]);
    assert_eq!(f.dist("tilr/store"), "store 1.1.0");
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    assert_eq!(
        f.state()["packages"]["tilr/theme"]["current"]["version"],
        "1.0.0"
    );
    f.ok(&["rollback", "tilr/store"]);
    assert_eq!(f.dist("tilr/store"), "store 1.0.0");
    f.release("theme", "1.2.0");
    let out = f.ok(&["update", "all"]);
    assert!(out.contains("2 updates"), "{out}");
    assert_eq!(f.dist("tilr/store"), "store 1.1.0");
    assert_eq!(f.dist("tilr/theme"), "theme 1.2.0");

    // --dev works per package (HEAD of the default branch, then followed).
    f.write("theme", "1.3.0", "");
    f.git(&f.mono, &["commit", "-q", "-am", "theme work"]);
    f.ok(&["update", "tilr/theme", "--dev"]);
    assert_eq!(f.dist("tilr/theme"), "theme 1.3.0");
    // An explicit ref names a release tag of the package.
    f.ok(&["update", "tilr/store", "--ref", "tilr/store@1.0.0"]);
    assert_eq!(f.dist("tilr/store"), "store 1.0.0");
}

#[test]
fn installing_a_monorepo_without_a_package_id_lists_the_packages() {
    let f = Fixture::new();
    let error = f.fails(&["install", f.source()]);
    assert!(
        error.contains("choose one by ID")
            && error.contains("tilr/store (packages/store)")
            && error.contains("tilr/theme (packages/theme)"),
        "{error}"
    );
    assert!(error.contains("Error kind: invalid_argument"), "{error}");
    let error = f.fails(&["install", f.source(), "tilr/nothing"]);
    assert!(error.contains("not in this repository"), "{error}");
    f.fails(&[
        "install",
        f.source(),
        "tilr/store",
        "--package",
        "tilr/store",
    ]);
    // --ref with an ID@VERSION tag names the package; --dev needs an ID.
    f.ok(&["install", f.source(), "--ref", "tilr/theme@1.0.0"]);
    f.fails(&["install", f.source(), "--dev"]);
    f.ok(&["install", f.source(), "tilr/store", "--dev"]);
    assert_eq!(f.dist("tilr/store"), "store 1.0.0");
}

#[test]
fn uninstalling_one_package_keeps_the_other_working() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    f.ok(&["uninstall", "tilr/store"]);
    assert!(!f.dir.join("workspace/dist/tilr/store").exists());
    assert_eq!(f.clones(), 1);
    f.release("theme", "1.1.0");
    f.ok(&["update", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), "theme 1.1.0");
    f.ok(&["uninstall", "tilr/theme"]);
    assert_eq!(f.clones(), 0);
    assert!(
        f.state()["packages"].as_object().unwrap().is_empty(),
        "nothing left"
    );
}

#[test]
fn a_tag_on_a_commit_without_its_package_is_a_clear_error() {
    let f = Fixture::new();
    fs::remove_dir_all(f.mono.join("packages/store")).unwrap();
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-q", "-m", "drop store"]);
    f.git(&f.mono, &["tag", "tilr/store@2.0.0"]);
    let error = f.fails(&["install", f.source(), "tilr/store"]);
    assert!(
        error.contains("tilr/store@2.0.0 points to commit") && error.contains("without package"),
        "{error}"
    );
    let error = f.fails(&["install", f.source(), "--ref", "tilr/store@2.0.0"]);
    assert!(error.contains("not in this repository"), "{error}");
    assert!(!f.dir.join("workspace/dist/tilr").exists());
}

#[test]
fn legacy_root_repository_installs_and_updates_as_before() {
    let f = Fixture::new();
    let legacy = f.dir.join("hello");
    fs::create_dir_all(legacy.join("dist")).unwrap();
    f.git(&legacy, &["init", "-q", "-b", "main"]);
    let release = |version: &str| {
        fs::write(
            legacy.join("plugin.json"),
            format!(r#"{{"id":"acme/hello","version":"{version}"}}"#),
        )
        .unwrap();
        fs::write(legacy.join("dist/index.js"), format!("hello {version}")).unwrap();
        f.git(&legacy, &["add", "--all"]);
        f.git(&legacy, &["commit", "-q", "-m", version]);
        f.git(&legacy, &["tag", &format!("acme/hello@{version}")]);
    };
    release("1.0.0");
    f.ok(&["install", legacy.to_str().unwrap()]);
    assert!(f.dir.join("workspace/packages/acme/hello/.git").is_dir());
    assert_eq!(f.clones(), 0);
    let entry = &f.state()["packages"]["acme/hello"];
    assert!(
        entry.get("path").is_none() && entry.get("repo").is_none(),
        "{entry}"
    );
    release("1.1.0");
    f.ok(&["update", "acme/hello"]);
    assert_eq!(f.dist("acme/hello"), "hello 1.1.0");
    // A matching ID is accepted for a root repository too, a different one is not.
    f.ok(&["uninstall", "acme/hello"]);
    f.fails(&["install", legacy.to_str().unwrap(), "acme/other"]);
    f.ok(&["install", legacy.to_str().unwrap(), "acme/hello"]);
    // commit and catalog-update keep working without gitpm.json.
    fs::write(legacy.join("dist/index.js"), "hello 1.2.0").unwrap();
    let out = f.ok(&["commit", legacy.to_str().unwrap(), "1.2.0"]);
    assert!(out.contains("acme/hello@1.2.0"), "{out}");
    fs::write(f.dir.join("sources.txt"), "hello\n").unwrap();
    let out = f.ok(&["catalog-update"]);
    assert!(out.contains("3 release(s)"), "{out}");
    let release = &f.ok(&[
        "register",
        legacy.to_str().unwrap(),
        "--ref",
        "acme/hello@1.2.0",
    ]);
    assert!(!release.contains("\"path\""), "{release}");
}

#[test]
fn catalog_update_publishes_every_package_with_its_path() {
    let f = Fixture::new();
    f.release("store", "1.1.0");
    fs::write(f.dir.join("sources.txt"), "mono\n").unwrap();
    let out = f.ok(&["catalog-update"]);
    assert!(out.contains("3 release(s)"), "{out}");
    let info = f.ok(&["info"]);
    assert!(
        info.contains("tilr/store — latest: 1.1.0") && info.contains("tilr/theme — latest: 1.0.0"),
        "{info}"
    );
    let release: serde_json::Value = serde_json::from_slice(
        &fs::read(
            f.dir
                .join("catalog/namespaces/tilr/store/versions/1.1.0/release.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(release["path"], "packages/store");
    f.ok(&["install-catalog", "tilr/store"]);
    f.ok(&["install-catalog", "tilr/theme"]);
    assert_eq!(f.dist("tilr/store"), "store 1.1.0");
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    assert_eq!(f.clones(), 1);
    let check = f.ok(&["check"]);
    assert!(check.contains("Updates: 0."), "{check}");
    // register needs the package for a ref that does not name it.
    let error = f.fails(&["register", f.source(), "--ref", "main"]);
    assert!(error.contains("choose one by ID"), "{error}");
    let out = f.ok(&[
        "register",
        f.source(),
        "--ref",
        "main",
        "--package",
        "tilr/store",
    ]);
    assert!(out.contains("\"path\": \"packages/store\""), "{out}");
}

#[test]
fn init_into_and_commit_with_a_package() {
    let f = Fixture::new();
    let repo = f.dir.join("apps");
    // An existing repository without a root package becomes a multi-package one.
    fs::create_dir_all(&repo).unwrap();
    f.git(&repo, &["init", "-q", "-b", "main"]);
    let out = f.ok(&["init", "acme/one", "--into", repo.to_str().unwrap()]);
    assert!(out.contains("acme/one@0.1.0"), "{out}");
    assert!(repo.join("gitpm.json").is_file());
    assert!(repo.join("packages/one/dist/index.js").is_file());
    f.ok(&["init", "acme/two", "--into", repo.to_str().unwrap()]);
    // Folders are named after the package name: other/two cannot join acme/two, and
    // IDs are unique in any letter case.
    let error = f.fails(&["init", "other/two", "--into", repo.to_str().unwrap()]);
    assert!(
        error.contains("package acme/two already uses that folder"),
        "{error}"
    );
    let error = f.fails(&["init", "Acme/One", "--into", repo.to_str().unwrap()]);
    assert!(error.contains("acme/one already exists"), "{error}");
    fs::write(repo.join("packages/two/dist/index.js"), "two v2").unwrap();
    let error = f.fails(&["commit", repo.to_str().unwrap(), "0.2.0"]);
    assert!(error.contains("acme/one (packages/one)"), "{error}");
    let out = f.ok(&[
        "commit",
        repo.to_str().unwrap(),
        "0.2.0",
        "--package",
        "acme/two",
    ]);
    assert!(out.contains("acme/two@0.2.0"), "{out}");
    f.ok(&["install", repo.to_str().unwrap(), "acme/two"]);
    assert_eq!(f.dist("acme/two"), "two v2");
    f.ok(&["install", repo.to_str().unwrap(), "acme/one"]);
    assert_eq!(f.clones(), 1);
    // A new folder becomes a single-package repository (the root is the package);
    // nothing is derived from the ID.
    let root = f.dir.join("single");
    let out = f.ok(&["init", "acme/root", "--into", root.to_str().unwrap()]);
    assert!(out.contains("acme/root@0.1.0"), "{out}");
    assert!(root.join("plugin.json").is_file() && !root.join("gitpm.json").exists());
    assert!(!f.dir.join("acme").exists());
    // A root package is not converted to a multi-package repository implicitly.
    let error = f.fails(&["init", "acme/three", "--into", root.to_str().unwrap()]);
    assert!(error.contains("repository root is a package"), "{error}");
    assert!(!root.join("gitpm.json").exists());
    let error = f.fails(&["init", "acme/four", "--in", root.to_str().unwrap()]);
    assert!(error.contains("unknown option: --in"), "{error}");
}

/// A consumer repository that depends on `tilr/theme`.
fn consumer(f: &Fixture) -> PathBuf {
    let repo = f.dir.join("consumer");
    fs::create_dir_all(repo.join("dist")).unwrap();
    f.git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(
        repo.join("plugin.json"),
        r#"{"id":"app/consumer","version":"1.0.0","dependencies":{"tilr/theme":"^1.0"}}"#,
    )
    .unwrap();
    fs::write(repo.join("dist/index.js"), "consumer").unwrap();
    f.git(&repo, &["add", "--all"]);
    f.git(&repo, &["commit", "-q", "-m", "consumer"]);
    f.git(&repo, &["tag", "app/consumer@1.0.0"]);
    repo
}
fn provider(state: &serde_json::Value) -> String {
    let consumer = state["resolved"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "app/consumer")
        .unwrap();
    consumer["dependencies"][0]["provider"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn fork_replace_releases_installs_and_replaces_in_one_step() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/theme"]);
    let consumer = consumer(&f);
    f.ok(&["install", consumer.to_str().unwrap()]);
    assert_eq!(provider(&f.state()), "tilr/theme");
    // Local edits of the installed package go into the fork's first release.
    let clone = fs::read_dir(f.dir.join("workspace/repos"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::write(clone.join("packages/theme/dist/index.js"), "my theme").unwrap();
    let out = f.ok(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me/theme",
        "--replace",
    ]);
    assert!(out.contains("Released: me/theme@1.0.0"), "{out}");
    assert!(out.contains("Installed: me/theme 1.0.0"), "{out}");
    assert!(
        out.contains("tilr/theme replaced by me/theme 1.0.0 for: app/consumer"),
        "{out}"
    );
    let fork = f.dir.join("forks/me/theme");
    assert!(fork.join("packages/theme/plugin.json").is_file());
    assert_eq!(f.dist("me/theme"), "my theme");
    // dist/ holds only active packages: the replaced original is not deployed.
    assert!(!f.dir.join("workspace/dist/tilr/theme").exists());
    let state = f.state();
    assert_eq!(state["replacements"]["tilr/theme"], "me/theme");
    assert_eq!(provider(&state), "me/theme");
    // A later fork release with another version shows the installed version.
    fs::write(fork.join("packages/theme/dist/index.js"), "my theme 2").unwrap();
    f.ok(&["commit", fork.to_str().unwrap(), "1.5.0"]);
    f.ok(&["update", "me/theme"]);
    let out = f.ok(&["replace", "tilr/theme", "--with", "me/theme"]);
    assert!(
        out.contains(
            "tilr/theme replaced by me/theme 1.5.0 (as tilr/theme 1.0.0) for: app/consumer"
        ),
        "{out}"
    );
    // Switching back restores the original for the consumer.
    let out = f.ok(&["replace", "tilr/theme", "--back"]);
    assert!(
        out.contains(
            "tilr/theme switched back from me/theme (tilr/theme 1.0.0 for: app/consumer); me/theme stays installed and enabled (disable or uninstall it if not wanted)"
        ),
        "{out}"
    );
    let state = f.state();
    assert!(state.get("replacements").is_none(), "{state:#}");
    assert_eq!(provider(&state), "tilr/theme");
    // Deployed again from the installed commit (the clone's local edit is not released).
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    assert_eq!(f.dist("me/theme"), "my theme 2");
}

#[test]
fn fork_replace_keeps_the_fork_and_says_how_to_finish_when_a_step_fails() {
    let f = Fixture::new();
    let legacy = f.dir.join("hello");
    fs::create_dir_all(legacy.join("dist")).unwrap();
    f.git(&legacy, &["init", "-q", "-b", "main"]);
    fs::write(
        legacy.join("plugin.json"),
        r#"{"id":"acme/hello","version":"1.0.0"}"#,
    )
    .unwrap();
    fs::write(legacy.join("dist/index.js"), "hello").unwrap();
    f.git(&legacy, &["add", "--all"]);
    f.git(&legacy, &["commit", "-q", "-m", "hello"]);
    f.git(&legacy, &["tag", "acme/hello@1.0.0"]);
    f.ok(&["install", legacy.to_str().unwrap()]);
    // A symlink in the edited dist gets released with the fork but cannot be installed.
    let installed = f.dir.join("workspace/packages/acme/hello");
    std::os::unix::fs::symlink("index.js", installed.join("dist/link.js")).unwrap();
    let error = f.fails(&[
        "fork",
        "acme/hello",
        "--ns",
        "me",
        "--into",
        "me/hello",
        "--replace",
    ]);
    assert!(
        error.contains("me/hello: the install step failed"),
        "{error}"
    );
    assert!(error.contains("gitpm install"), "{error}");
    assert!(
        error.contains("To undo everything: uninstall the installed forks, then delete"),
        "{error}"
    );
    assert!(error.contains("Error kind: unsupported"), "{error}");
    let fork = f.dir.join("me/hello");
    assert!(fork.join("plugin.json").is_file(), "fork folder is kept");
    let state = f.state();
    assert!(state["packages"].get("me/hello").is_none());
    assert!(state.get("replacements").is_none());
    // Without --replace the output names the next steps.
    fs::remove_file(installed.join("dist/link.js")).unwrap();
    let out = f.ok(&["fork", "acme/hello", "--ns", "me", "--into", "other"]);
    assert!(
        out.contains("Next steps") && out.contains("gitpm replace acme/hello --with me/hello"),
        "{out}"
    );
    assert!(f.dir.join("other/plugin.json").is_file());
    // Invalid input is refused before any folder is created.
    f.fails(&[
        "fork",
        "acme/hello",
        "--ns",
        "you",
        "--into",
        "you",
        "--replace",
        "--version",
        "1.0",
    ]);
    assert!(!f.dir.join("you").exists());
    f.fails(&[
        "fork",
        "acme/hello",
        "--ns",
        "you",
        "--into",
        "you",
        "--version",
        "2.0.0",
    ]);
    assert!(!f.dir.join("you").exists());

    // An incompatible replacement needs confirmation: without a terminal the library
    // leaves it pending, so the fork stays released and installed and the original is
    // not replaced. acme/app needs acme/hello ^2, which a fork of 1.0.0 does not give.
    fs::write(
        legacy.join("plugin.json"),
        r#"{"id":"acme/hello","version":"2.0.0"}"#,
    )
    .unwrap();
    f.git(&legacy, &["commit", "-q", "-am", "2.0.0"]);
    f.git(&legacy, &["tag", "acme/hello@2.0.0"]);
    let app = f.dir.join("app");
    fs::create_dir_all(app.join("dist")).unwrap();
    f.git(&app, &["init", "-q", "-b", "main"]);
    fs::write(
        app.join("plugin.json"),
        r#"{"id":"acme/app","version":"1.0.0","dependencies":{"acme/hello":"^2"}}"#,
    )
    .unwrap();
    fs::write(app.join("dist/index.js"), "app").unwrap();
    f.git(&app, &["add", "--all"]);
    f.git(&app, &["commit", "-q", "-m", "app"]);
    f.git(&app, &["tag", "acme/app@1.0.0"]);
    f.ok(&["update", "acme/hello"]);
    f.ok(&["install", app.to_str().unwrap()]);
    f.ok(&["uninstall", "acme/hello", "--force"]);
    f.ok(&[
        "install",
        legacy.to_str().unwrap(),
        "--ref",
        "acme/hello@1.0.0",
    ]);
    let error = f.fails(&[
        "fork",
        "acme/hello",
        "--ns",
        "pending",
        "--into",
        "pending/hello",
        "--replace",
    ]);
    for needle in [
        "Warning: pending/hello provides acme/hello 1.0.0, which these consumers do not accept:",
        "  - acme/app needs acme/hello ^2",
        "No terminal to confirm; use --force to replace anyway.",
        "pending/hello: the replace step failed",
        "gitpm replace acme/hello --with pending/hello [--force]",
        "Error kind: incompatible_replacement",
    ] {
        assert!(error.contains(needle), "missing {needle:?}: {error}");
    }
    let state = f.state();
    assert!(state["packages"].get("pending/hello").is_some());
    assert!(state.get("replacements").is_none());
}

// ---- forks of several packages ------------------------------------------------------

/// Add `tilr/panel` (depends on `tilr/theme ^1.0`) to the fixture repository, released
/// as 1.0.0, and install all three packages.
fn three_installed(f: &Fixture) {
    f.write("panel", "1.0.0", r#","dependencies":{"tilr/theme":"^1.0"}"#);
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-q", "-m", "panel"]);
    f.git(&f.mono, &["tag", "tilr/panel@1.0.0"]);
    for id in ["tilr/store", "tilr/theme", "tilr/panel"] {
        f.ok(&["install", f.source(), id]);
    }
}
fn manifest(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path.join("plugin.json")).unwrap()).unwrap()
}
fn config(repo: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(repo.join("gitpm.json")).unwrap()).unwrap()
}
/// A root consumer `app/NAME` depending on `DEPENDENCY ^1.0`, installed.
fn install_consumer(f: &Fixture, name: &str, dependency: &str) {
    let repo = f.dir.join(name);
    fs::create_dir_all(repo.join("dist")).unwrap();
    f.git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(
        repo.join("plugin.json"),
        format!(
            r#"{{"id":"app/{name}","version":"1.0.0","dependencies":{{"{dependency}":"^1.0"}}}}"#
        ),
    )
    .unwrap();
    fs::write(repo.join("dist/index.js"), name).unwrap();
    f.git(&repo, &["add", "--all"]);
    f.git(&repo, &["commit", "-q", "-m", name]);
    f.git(&repo, &["tag", &format!("app/{name}@1.0.0")]);
    f.ok(&["install", repo.to_str().unwrap()]);
}
fn provider_of(state: &serde_json::Value, consumer: &str) -> String {
    let entry = state["resolved"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == consumer)
        .unwrap();
    entry["dependencies"][0]["provider"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn fork_of_two_packages_keeps_exactly_those_and_rewrites_their_dependency() {
    let f = Fixture::new();
    three_installed(&f);
    let out = f.ok(&[
        "fork",
        "tilr/theme",
        "tilr/panel",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
    ]);
    let fork = f.dir.join("forks/me/mono");
    assert!(
        out.contains("Fork created: /") && out.contains("forks/me/mono\n"),
        "{out}"
    );
    assert!(
        out.contains("me/panel forked from tilr/panel (packages/panel)"),
        "{out}"
    );
    assert!(
        out.contains("Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)"),
        "{out}"
    );
    assert_eq!(
        config(&fork)["packages"],
        serde_json::json!(["packages/panel", "packages/theme"])
    );
    assert!(
        !fork.join("packages/store").exists(),
        "unforked sibling removed"
    );
    let panel = manifest(&fork.join("packages/panel"));
    assert_eq!(panel["id"], "me/panel");
    assert_eq!(
        panel["dependencies"],
        serde_json::json!({"me/theme": "^1.0"})
    );
    assert_eq!(panel["forkOf"]["id"], "tilr/panel");
    assert_eq!(panel["replaces"]["id"], "tilr/panel");
    assert_eq!(manifest(&fork.join("packages/theme"))["id"], "me/theme");
    // History is kept, the change is one commit, nothing is left uncommitted.
    f.git(&fork, &["cat-file", "-e", "tilr/store@1.0.0"]);
    let out = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        out.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    // The fork lists only its two packages.
    let error = f.fails(&["install", fork.to_str().unwrap(), "--dev"]);
    assert!(
        error.contains("me/panel (packages/panel)")
            && error.contains("me/theme (packages/theme)")
            && !error.contains("store"),
        "{error}"
    );
    f.ok(&["install", fork.to_str().unwrap(), "me/theme", "--dev"]);
    f.ok(&["install", fork.to_str().unwrap(), "me/panel", "--dev"]);
    assert_eq!(f.dist("me/panel"), "panel 1.0.0");

    // Forking only the dependent package keeps its dependency on the original.
    f.ok(&["fork", "tilr/panel", "--ns", "solo", "--into", "forks/solo"]);
    let solo = f.dir.join("forks/solo");
    assert_eq!(
        config(&solo)["packages"],
        serde_json::json!(["packages/panel"])
    );
    assert_eq!(
        manifest(&solo.join("packages/panel"))["dependencies"],
        serde_json::json!({"tilr/theme": "^1.0"})
    );
    assert!(!solo.join("packages/theme").exists());
}

#[test]
fn packages_forked_together_must_come_from_one_repository() {
    let f = Fixture::new();
    three_installed(&f);
    let legacy = f.dir.join("hello");
    fs::create_dir_all(legacy.join("dist")).unwrap();
    f.git(&legacy, &["init", "-q", "-b", "main"]);
    fs::write(
        legacy.join("plugin.json"),
        r#"{"id":"acme/hello","version":"1.0.0"}"#,
    )
    .unwrap();
    fs::write(legacy.join("dist/index.js"), "hello").unwrap();
    f.git(&legacy, &["add", "--all"]);
    f.git(&legacy, &["commit", "-q", "-m", "hello"]);
    f.git(&legacy, &["tag", "acme/hello@1.0.0"]);
    f.ok(&["install", legacy.to_str().unwrap()]);
    let error = f.fails(&[
        "fork",
        "tilr/store",
        "acme/hello",
        "--ns",
        "me",
        "--into",
        "me/mono",
    ]);
    assert!(
        error.contains("must come from one repository")
            && error.contains("tilr/store from /")
            && error.contains("/mono; acme/hello from /")
            && error.ends_with("/hello\nError kind: invalid_argument\n"),
        "{error}"
    );
    assert!(error.contains("Error kind: invalid_argument"), "{error}");
    let error = f.fails(&[
        "fork",
        "tilr/store",
        "tilr/nothing",
        "--ns",
        "me",
        "--into",
        "me/mono",
    ]);
    assert!(error.contains("Error kind: not_installed"), "{error}");
    let error = f.fails(&[
        "fork",
        "tilr/store",
        "tilr/store",
        "--ns",
        "me",
        "--into",
        "me/mono",
    ]);
    assert!(error.contains("more than once"), "{error}");
    let error = f.fails(&["fork", "tilr/store", "--ns", "tilr", "--into", "tilr"]);
    assert!(error.contains("different namespace"), "{error}");
    // The source repository is never the fork repository.
    let error = f.fails(&["fork", "tilr/store", "--ns", "me", "--into", "mono"]);
    assert!(error.contains("is the source repository"), "{error}");
    assert!(!f.dir.join("me").exists() && !f.dir.join("tilr").exists());
    // A root package forks through the same command (no gitpm.json in the fork).
    let out = f.ok(&["fork", "acme/hello", "--ns", "me", "--into", "me/hello"]);
    assert!(
        out.contains("me/hello forked from acme/hello (repository root)"),
        "{out}"
    );
    assert!(f.dir.join("me/hello/plugin.json").is_file());
    assert!(!f.dir.join("me/hello/gitpm.json").exists());
    // ...but cannot be added to a multi-package repository.
    f.ok(&["fork", "tilr/store", "--ns", "me", "--into", "me/mono"]);
    let error = f.fails(&["fork", "acme/hello", "--ns", "you", "--into", "me/mono"]);
    assert!(
        error.contains("is the root package of its repository"),
        "{error}"
    );
}

#[test]
fn fork_replace_of_two_packages_replaces_both_and_switches_back() {
    let f = Fixture::new();
    three_installed(&f);
    install_consumer(&f, "uses-theme", "tilr/theme");
    install_consumer(&f, "uses-panel", "tilr/panel");
    let out = f.ok(&[
        "fork",
        "tilr/panel",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
        "--replace",
        "--version",
        "2.0.0",
    ]);
    // me/panel needs me/theme; ^1.0 does not accept the released 2.0.0.
    assert!(
        out.contains("Dependency: me/panel now depends on me/theme ^2.0.0 (was tilr/theme ^1.0"),
        "{out}"
    );
    assert!(
        out.contains("Released: me/theme@2.0.0") && out.contains("Released: me/panel@2.0.0"),
        "{out}"
    );
    // Dependencies are installed first.
    assert!(
        out.find("Installed: me/theme").unwrap() < out.find("Installed: me/panel").unwrap(),
        "{out}"
    );
    assert!(
        out.contains(
            "tilr/theme replaced by me/theme 2.0.0 (as tilr/theme 1.0.0) for: app/uses-theme"
        ),
        "{out}"
    );
    let state = f.state();
    assert_eq!(provider_of(&state, "app/uses-theme"), "me/theme");
    assert_eq!(provider_of(&state, "app/uses-panel"), "me/panel");
    assert_eq!(state["replacements"]["tilr/panel"], "me/panel");
    let fork = f.dir.join("forks/me/mono");
    assert_eq!(manifest(&fork.join("packages/theme"))["version"], "2.0.0");
    f.ok(&["replace", "tilr/theme", "--back"]);
    f.ok(&["replace", "tilr/panel", "--back"]);
    let state = f.state();
    assert_eq!(provider_of(&state, "app/uses-theme"), "tilr/theme");
    assert_eq!(provider_of(&state, "app/uses-panel"), "tilr/panel");
    assert!(state.get("replacements").is_none(), "{state:#}");
}

#[test]
fn packages_are_added_to_an_existing_fork_with_dependencies_rewritten_both_ways() {
    let f = Fixture::new();
    three_installed(&f);
    f.write("dock", "1.0.0", r#","dependencies":{"tilr/theme":"^1.0"}"#);
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-q", "-m", "dock"]);
    f.git(&f.mono, &["tag", "tilr/dock@1.0.0"]);
    f.ok(&["install", f.source(), "tilr/dock"]);
    // Only the dependent first: it keeps depending on the original.
    f.ok(&[
        "fork",
        "tilr/panel",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
    ]);
    let fork = f.dir.join("forks/me/mono");
    assert_eq!(
        manifest(&fork.join("packages/panel"))["dependencies"],
        serde_json::json!({"tilr/theme": "^1.0"})
    );
    // Adding the dependency later rewrites the existing package (explicit repository).
    let out = f.ok(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        fork.to_str().unwrap(),
    ]);
    assert!(
        out.contains("Packages added to: /") && out.contains("forks/me/mono\n"),
        "{out}"
    );
    assert!(
        out.contains("Dependency: me/panel now depends on me/theme ^1.0 (was tilr/theme)"),
        "{out}"
    );
    assert_eq!(
        manifest(&fork.join("packages/panel"))["dependencies"],
        serde_json::json!({"me/theme": "^1.0"})
    );
    assert_eq!(
        config(&fork)["packages"],
        serde_json::json!(["packages/panel", "packages/theme"])
    );
    // A new package depending on an original that the fork already has: rewritten to the
    // fork.
    let out = f.ok(&["fork", "tilr/dock", "--ns", "me", "--into", "forks/me/mono"]);
    assert!(out.contains("Packages added to"), "{out}");
    let dock = manifest(&fork.join("packages/dock"));
    assert_eq!(dock["id"], "me/dock");
    assert_eq!(dock["forkOf"]["id"], "tilr/dock");
    assert_eq!(
        dock["dependencies"],
        serde_json::json!({"me/theme": "^1.0"})
    );
    assert_eq!(
        config(&fork)["packages"],
        serde_json::json!(["packages/panel", "packages/theme", "packages/dock"])
    );
    let out = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .args(["log", "--format=%s", "-3"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Fork tilr/dock as me/dock\nFork tilr/theme as me/theme\nFork tilr/panel as me/panel\n"
    );
    // IDs stay unique.
    let error = f.fails(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
    ]);
    assert!(error.contains("me/theme already exists"), "{error}");
    // The added packages release and install like any other.
    f.ok(&[
        "commit",
        fork.to_str().unwrap(),
        "1.0.1",
        "--package",
        "me/theme",
    ]);
    f.ok(&["install", fork.to_str().unwrap(), "me/theme"]);
    assert_eq!(f.dist("me/theme"), "theme 1.0.0");
}

#[test]
fn fork_replace_of_several_packages_reports_each_failure_and_keeps_the_fork() {
    let f = Fixture::new();
    three_installed(&f);
    // A symlink in panel's edited dist is released with the fork but cannot be installed.
    let clone = fs::read_dir(f.dir.join("workspace/repos"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::os::unix::fs::symlink("index.js", clone.join("packages/panel/dist/link.js")).unwrap();
    let out = f.run(&[
        "fork",
        "tilr/theme",
        "tilr/panel",
        "--ns",
        "me",
        "--into",
        "me/mono",
        "--replace",
    ]);
    assert!(!out.status.success());
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(stdout.contains("Released: me/theme@1.0.0"), "{stdout}");
    assert!(stdout.contains("Released: me/panel@1.0.0"), "{stdout}");
    assert!(
        stdout.contains("tilr/theme replaced by me/theme 1.0.0"),
        "{stdout}"
    );
    assert!(
        stderr.contains("me/panel: the install step failed")
            && stderr.contains("kind: unsupported"),
        "{stderr}"
    );
    assert!(stderr.contains("gitpm install"), "{stderr}");
    assert!(
        stderr.contains("gitpm replace tilr/panel --with me/panel"),
        "{stderr}"
    );
    assert!(stderr.contains("is kept"), "{stderr}");
    assert!(stderr.contains("Error kind: unsupported"), "{stderr}");
    let fork = f.dir.join("me/mono");
    assert!(
        fork.join("packages/panel/plugin.json").is_file(),
        "fork kept"
    );
    let state = f.state();
    assert_eq!(state["replacements"]["tilr/theme"], "me/theme");
    assert!(state["packages"].get("me/panel").is_none());
    assert!(state["replacements"].get("tilr/panel").is_none());
}

#[test]
fn a_url_rewrite_in_the_git_config_does_not_hide_the_shared_clone() {
    // B3: `git remote get-url` applies insteadOf; the shared clone must still be found.
    let f = Fixture::new();
    let real = fs::canonicalize(&f.mono).unwrap();
    let mut config = fs::read_to_string(f.dir.join("home/.gitconfig")).unwrap();
    config.push_str(&format!(
        "[url \"file://{}\"]\n\tinsteadOf = {}\n",
        real.display(),
        real.display()
    ));
    fs::write(f.dir.join("home/.gitconfig"), config).unwrap();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    assert_eq!(f.clones(), 1);
    f.release("theme", "1.1.0");
    f.ok(&["update", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), "theme 1.1.0");
    // forget/restore of a shared package goes through the same clone.
    f.ok(&["forget", "tilr/store"]);
    f.ok(&["restore", "tilr/store"]);
    assert_eq!(f.dist("tilr/store"), "store 1.0.0");
}

#[test]
fn a_v_version_tag_in_a_multi_package_repository_is_ignored_without_a_warning() {
    let f = Fixture::new();
    // `v0.2.0` on a later commit: no release of any package, and never mentioned.
    f.release("store", "1.1.0");
    f.git(&f.mono, &["tag", "v0.2.0"]);
    let quiet = |args: &[&str]| {
        let out = f.run(args);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("v0.2.0") && !stdout.contains("v0.2.0"),
            "{args:?}: {stdout}{stderr}"
        );
        stdout
    };
    quiet(&[
        "install",
        f.source(),
        "tilr/store",
        "--ref",
        "tilr/store@1.0.0",
    ]);
    quiet(&["install", f.source(), "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    let out = quiet(&["update", "tilr/store", "--release"]);
    assert!(out.contains("tilr/store updated 1.0.0 → 1.1.0"), "{out}");
    quiet(&["update", "all"]);
    fs::write(f.dir.join("sources.txt"), "mono\n").unwrap();
    let out = quiet(&["catalog-update"]);
    assert!(out.contains("3 release(s)"), "{out}");
    assert!(
        !f.dir
            .join("catalog/namespaces/tilr/store/versions/0.2.0")
            .exists()
    );
}

/// `acme/hello` installed at 1.0.0 while `acme/app` needs `acme/hello ^2`: a fork of it is
/// an incompatible replacement.
fn incompatible_consumer(f: &Fixture) -> std::path::PathBuf {
    let hello = f.dir.join("hello");
    fs::create_dir_all(hello.join("dist")).unwrap();
    f.git(&hello, &["init", "-q", "-b", "main"]);
    for version in ["1.0.0", "2.0.0"] {
        fs::write(
            hello.join("plugin.json"),
            format!(r#"{{"id":"acme/hello","version":"{version}"}}"#),
        )
        .unwrap();
        fs::write(hello.join("dist/index.js"), version).unwrap();
        f.git(&hello, &["add", "--all"]);
        f.git(&hello, &["commit", "-q", "-m", version]);
        f.git(&hello, &["tag", &format!("acme/hello@{version}")]);
    }
    let app = f.dir.join("app");
    fs::create_dir_all(app.join("dist")).unwrap();
    f.git(&app, &["init", "-q", "-b", "main"]);
    fs::write(
        app.join("plugin.json"),
        r#"{"id":"acme/app","version":"1.0.0","dependencies":{"acme/hello":"^2"}}"#,
    )
    .unwrap();
    fs::write(app.join("dist/index.js"), "app").unwrap();
    f.git(&app, &["add", "--all"]);
    f.git(&app, &["commit", "-q", "-m", "app"]);
    f.git(&app, &["tag", "acme/app@1.0.0"]);
    f.ok(&["install", hello.to_str().unwrap()]);
    f.ok(&["install", app.to_str().unwrap()]);
    f.ok(&["uninstall", "acme/hello", "--force"]);
    f.ok(&[
        "install",
        hello.to_str().unwrap(),
        "--ref",
        "acme/hello@1.0.0",
    ]);
    hello
}

/// On a TTY a declined pending replacement does not cancel `fork --replace`: the fork
/// stays released and installed, the original is not replaced, and the command says
/// how to finish; a confirmed one is applied.
#[test]
fn fork_replace_on_a_tty_keeps_a_declined_replacement_pending() {
    if !Path::new("/usr/bin/script").exists() {
        return;
    }
    let f = Fixture::new();
    incompatible_consumer(&f);
    let util_linux = Command::new("script")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let stderr_file = f.dir.join("fork.stderr");
    let run = |ns: &str, answer: &str| {
        let gitpm = format!(
            "'{}' --root '{}' --catalog '{}' fork acme/hello --ns {ns} --into {ns}/hello --replace 2>'{}'",
            env!("CARGO_BIN_EXE_gitpm"),
            f.dir.join("workspace").display(),
            f.dir.join("catalog").display(),
            stderr_file.display()
        );
        let script = if util_linux {
            format!("script -qec \"{gitpm}\" /dev/null")
        } else {
            format!("script -q /dev/null sh -c \"exec {gitpm}\"")
        };
        let out = f
            .command("sh")
            .current_dir(&f.dir)
            .arg("-c")
            .arg(format!(
                "(sleep 2; printf '{answer}\\n'; sleep 1) | {script}"
            ))
            .output()
            .unwrap();
        (
            String::from_utf8_lossy(&out.stdout).into_owned(),
            fs::read_to_string(&stderr_file).unwrap_or_default(),
        )
    };
    let (stdout, stderr) = run("me", "n");
    for needle in [
        "  - acme/app needs acme/hello ^2",
        "Replace anyway? [y/N]",
        "acme/hello is not replaced.",
        "me/hello: the replace step failed",
        "To finish:\n    gitpm replace acme/hello --with me/hello [--force]",
        "The fork ",
        "Error kind: incompatible_replacement",
    ] {
        assert!(stderr.contains(needle), "missing {needle:?}: {stderr}");
    }
    assert!(!stderr.contains("nothing was changed"), "{stderr}");
    assert!(stdout.contains("Released: me/hello@1.0.0"), "{stdout}");
    assert!(stdout.contains("Installed: me/hello 1.0.0"), "{stdout}");
    let state = f.state();
    assert!(state["packages"].get("me/hello").is_some());
    assert!(state.get("replacements").is_none());
    let (stdout, stderr) = run("you", "y");
    assert!(
        stdout.contains("acme/hello replaced by you/hello 1.0.0 for: acme/app"),
        "{stdout}\n{stderr}"
    );
    assert_eq!(f.state()["replacements"]["acme/hello"], "you/hello");
}
