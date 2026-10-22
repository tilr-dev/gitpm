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
    /// Repository `mono` with `tilr/store` and `tilr/theme` at 1.0.0 (one commit, one repository tag).
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
        f.git(&f.mono, &["tag", "v1.0.0"]);
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
    /// Commit one payload change, bump every package and tag the repository.
    fn release(&self, name: &str, version: &str) {
        self.write(name, version, "");
        for entry in fs::read_dir(self.mono.join("packages")).unwrap() {
            let path = entry.unwrap().path().join("plugin.json");
            if path.is_file() {
                let mut manifest: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                manifest["version"] = version.into();
                fs::write(path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
            }
        }
        self.git(&self.mono, &["add", "--all"]);
        self.git(
            &self.mono,
            &["commit", "-q", "-m", &format!("{name} {version}")],
        );
        self.git(&self.mono, &["tag", &format!("v{version}")]);
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
    for (name, script) in [
        ("prepare-commit-msg", post_commit),
        ("pre-commit", pre_commit),
    ] {
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
        .arg("v2.0.0")
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
    release_hooks(&f, ":", "exit 1");
    let (fork, stderr) = release_failure(&f);
    assert!(stderr.contains("kind: git"), "{stderr}");
    for name in ["store", "theme"] {
        let path = format!("packages/{name}/plugin.json");
        let expected = f
            .command("git")
            .arg("-C")
            .arg(&fork)
            .args(["show", &format!("HEAD:{path}")])
            .output()
            .unwrap();
        assert!(expected.status.success());
        assert_eq!(fs::read(fork.join(&path)).unwrap(), expected.stdout);
    }
    f.git(&fork, &["diff", "--cached", "--exit-code"]);
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
    f.git(&f.mono, &["tag", "-f", "v1.0.0"]);
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
    use std::os::unix::fs::PermissionsExt;
    // Cloning the recorded local-path source needs file transport; disable it after checkout.
    let hooks = f.dir.join("transport-hooks");
    fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("post-checkout");
    fs::write(
        &hook,
        "#!/bin/sh\ngit config --global protocol.file.allow never\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    f.git(
        &f.mono,
        &["config", "--global", "protocol.file.allow", "always"],
    );
    f.git(
        &f.mono,
        &[
            "config",
            "--global",
            "core.hooksPath",
            hooks.to_str().unwrap(),
        ],
    );
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
    f.git(&fork, &["diff", "--exit-code", "HEAD", "v2.0.0"]);
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
: > .git/refs/tags/v2.0.0.lock
"#,
    );
    let (fork, stderr) = release_failure(&f);
    assert!(stderr.contains("kind: git"), "{stderr}");
    assert!(stderr.contains("v2.0.0.lock"), "{stderr}");
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
#[cfg(unix)]
fn fork_replace_single_package_tag_failure_gives_no_commit_retry_hint() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    // A checkout hook places a tag lock after the clone has been created.
    // It runs on checkout of the fork branch, before the initial commit.
    use std::os::unix::fs::PermissionsExt;
    release_hooks(&f, ":", ":");
    let hook = f.dir.join("hooks/post-checkout");
    fs::write(
        &hook,
        r#"#!/bin/sh
set -eu
case "$PWD" in
*/forks/me/mono)
    : > .git/refs/tags/v1.0.1.lock
    ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    let out = f.run(&[
        "fork",
        "tilr/store",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
        "--replace",
        "--version",
        "1.0.1",
    ]);
    assert!(!out.status.success(), "release must fail");
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("me/store: the release step failed"),
        "{stderr}"
    );
    assert!(stderr.contains("no release tags were created"), "{stderr}");
    assert!(!stderr.contains("commit succeeded"), "{stderr}");
    assert!(
        !stderr.contains("releasing the same version again"),
        "{stderr}"
    );
    assert!(f.state()["packages"].get("me/store").is_none());
}

#[test]
fn two_packages_share_one_clone_and_follow_repository_releases() {
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

    // Publishing a shared version still lets the caller update one installation.
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
    assert_eq!(
        f.state()["packages"]["tilr/store"]["current"]["version"],
        "1.2.0"
    );
    assert_eq!(f.dist("tilr/theme"), "theme 1.2.0");

    // --dev works per package (HEAD of the default branch, then followed).
    f.write("theme", "1.3.0", "");
    f.write("store", "1.3.0", "");
    f.git(&f.mono, &["commit", "-q", "-am", "theme work"]);
    f.ok(&["update", "tilr/theme", "--dev"]);
    assert_eq!(f.dist("tilr/theme"), "theme 1.3.0");
    // An explicit ref names a release tag of the package.
    f.ok(&["update", "tilr/store", "--ref", "v1.0.0"]);
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
    // A repository tag selects a revision, so a package ID is still required.
    f.fails(&["install", f.source(), "--ref", "v1.0.0"]);
    f.ok(&["install", f.source(), "tilr/theme", "--ref", "v1.0.0"]);
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
    f.git(&f.mono, &["tag", "v2.0.0"]);
    let error = f.fails(&["install", f.source(), "tilr/store"]);
    assert!(
        error.contains("v2.0.0 points to commit") && error.contains("without package"),
        "{error}"
    );
    let error = f.fails(&["install", f.source(), "tilr/store", "--ref", "v2.0.0"]);
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
        f.git(&legacy, &["tag", &format!("v{version}")]);
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
    assert!(out.contains("v1.2.0"), "{out}");
    fs::write(f.dir.join("sources.txt"), "hello\n").unwrap();
    let out = f.ok(&["catalog", "update"]);
    assert!(out.contains("3 release(s)"), "{out}");
    let release = &f.ok(&[
        "catalog",
        "register",
        legacy.to_str().unwrap(),
        "--ref",
        "v1.2.0",
    ]);
    assert!(!release.contains("\"path\""), "{release}");
}

#[test]
fn catalog_update_publishes_every_package_with_its_path() {
    let f = Fixture::new();
    f.release("store", "1.1.0");
    fs::write(f.dir.join("sources.txt"), "mono\n").unwrap();
    let out = f.ok(&["catalog", "update"]);
    assert!(out.contains("4 release(s)"), "{out}");
    let info = f.ok(&["catalog", "show"]);
    assert!(
        info.contains("tilr/store — latest: 1.1.0") && info.contains("tilr/theme — latest: 1.1.0"),
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
    let error = f.fails(&["catalog", "register", f.source(), "--ref", "main"]);
    assert!(error.contains("choose one by ID"), "{error}");
    let out = f.ok(&[
        "catalog",
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
fn init_into_and_commit_releases_all_packages_and_additions_inherit_version() {
    let f = Fixture::new();
    let repo = f.dir.join("apps");
    fs::create_dir_all(&repo).unwrap();
    f.git(&repo, &["init", "-q", "-b", "main"]);
    f.ok(&["init", "acme/one", "--into", repo.to_str().unwrap()]);
    f.ok(&["commit", repo.to_str().unwrap(), "0.2.0"]);
    let out = f.ok(&["init", "acme/two", "--into", repo.to_str().unwrap()]);
    assert!(!out.contains("Git commit and tag"), "{out}");
    assert_eq!(manifest(&repo.join("packages/two"))["version"], "0.2.0");
    let tags = f
        .command("git")
        .arg("-C")
        .arg(&repo)
        .args(["tag", "--list"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(tags.stdout).unwrap(), "v0.2.0\n");
    let error = f.fails(&["init", "other/two", "--into", repo.to_str().unwrap()]);
    assert!(
        error.contains("package acme/two already uses that folder"),
        "{error}"
    );
    let error = f.fails(&["init", "Acme/One", "--into", repo.to_str().unwrap()]);
    assert!(error.contains("acme/one already exists"), "{error}");
    fs::write(repo.join("packages/two/dist/index.js"), "two v2").unwrap();
    let out = f.ok(&["commit", repo.to_str().unwrap(), "0.3.0"]);
    assert!(out.contains("v0.3.0"), "{out}");
    for name in ["one", "two"] {
        assert_eq!(
            manifest(&repo.join("packages").join(name))["version"],
            "0.3.0"
        );
    }
    f.ok(&["install", repo.to_str().unwrap(), "acme/two"]);
    f.ok(&["install", repo.to_str().unwrap(), "acme/one"]);
    assert_eq!(f.dist("acme/two"), "two v2");
    assert_eq!(f.clones(), 1);
    let root = f.dir.join("single");
    let out = f.ok(&["init", "acme/root", "--into", root.to_str().unwrap()]);
    assert!(out.contains("v0.1.0"), "{out}");
    assert!(root.join("plugin.json").is_file() && !root.join("gitpm.json").exists());
    let error = f.fails(&["init", "acme/three", "--into", root.to_str().unwrap()]);
    assert!(error.contains("repository root is a package"), "{error}");
}

#[test]
fn commit_honors_ssh_tag_signing_without_an_editor() {
    let f = Fixture::new();
    let key = f.dir.join("signing-key");
    let generated = f
        .command("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let public = fs::read_to_string(key.with_extension("pub")).unwrap();
    let allowed = f.dir.join("allowed-signers");
    fs::write(&allowed, format!("test@example.invalid {public}")).unwrap();
    f.git(&f.mono, &["config", "gpg.format", "ssh"]);
    f.git(
        &f.mono,
        &["config", "user.signingkey", key.to_str().unwrap()],
    );
    f.git(
        &f.mono,
        &[
            "config",
            "gpg.ssh.allowedSignersFile",
            allowed.to_str().unwrap(),
        ],
    );
    f.git(&f.mono, &["config", "tag.gpgSign", "true"]);
    let out = f
        .command(env!("CARGO_BIN_EXE_gitpm"))
        .env("GIT_EDITOR", "false")
        .args(["commit", f.source(), "1.1.0"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let kind = f
        .command("git")
        .arg("-C")
        .arg(&f.mono)
        .args(["cat-file", "-t", "refs/tags/v1.1.0"])
        .output()
        .unwrap();
    assert!(kind.status.success());
    assert_eq!(String::from_utf8(kind.stdout).unwrap().trim(), "tag");
    f.git(&f.mono, &["verify-tag", "v1.1.0"]);
    for name in ["store", "theme"] {
        assert_eq!(
            manifest(&f.mono.join("packages").join(name))["version"],
            "1.1.0"
        );
    }
}

#[test]
fn commit_package_selects_result_and_still_releases_every_package() {
    let f = Fixture::new();
    let out = f.ok(&["commit", f.source(), "1.1.0", "--package", "tilr/store"]);
    assert!(out.contains("tilr/store committed 1.1.0"), "{out}");
    for name in ["store", "theme"] {
        assert_eq!(
            manifest(&f.mono.join("packages").join(name))["version"],
            "1.1.0"
        );
    }
    f.git(&f.mono, &["tag", "tilr/store@9.0.0"]);
    f.ok(&["install", f.source(), "tilr/store"]);
    assert_eq!(
        f.state()["packages"]["tilr/store"]["current"]["version"],
        "1.1.0"
    );
}

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
    f.git(&repo, &["tag", "v1.0.0"]);
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
    // Local installation edits stay local; the fork releases the recorded source.
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
        "--version",
        "1.0.1",
    ]);
    assert!(out.contains("Released: v1.0.1"), "{out}");
    assert!(out.contains("Installed: me/theme 1.0.1"), "{out}");
    assert!(
        out.contains(
            "tilr/theme replaced by me/theme 1.0.1 (as tilr/theme 1.0.0) for: app/consumer"
        ),
        "{out}"
    );
    let fork = f.dir.join("forks/me/theme");
    assert!(fork.join("packages/theme/plugin.json").is_file());
    assert_eq!(f.dist("me/theme"), "theme 1.0.0");
    assert_eq!(
        fs::read_to_string(clone.join("packages/theme/dist/index.js")).unwrap(),
        "my theme"
    );
    // dist/ holds only active packages: the replaced original is not deployed.
    assert!(!f.dir.join("workspace/dist/tilr/theme").exists());
    let state = f.state();
    assert_eq!(state["replacements"]["tilr/theme"], "me/theme");
    assert_eq!(provider(&state), "me/theme");
    assert_eq!(state["packages"]["me/store"]["current"]["version"], "1.0.1");
    assert_eq!(f.dist("me/store"), "store 1.0.0");
    assert!(state["packages"].get("tilr/store").is_none());
    assert!(state["replacements"].get("tilr/store").is_none());
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
    f.git(&legacy, &["tag", "v1.0.0"]);
    f.ok(&["install", legacy.to_str().unwrap()]);
    // A release hook introduces an uninstallable dist after the source clone.
    release_hooks(&f, ":", "ln -s index.js dist/link.js\ngit add dist/link.js");
    let error = f.fails(&[
        "fork",
        "acme/hello",
        "--ns",
        "me",
        "--into",
        "me/hello",
        "--replace",
        "--version",
        "1.0.1",
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
    release_hooks(&f, ":", ":");
    // Without --replace the output names the next steps.
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
    f.git(&legacy, &["tag", "v2.0.0"]);
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
    f.git(&app, &["tag", "v1.0.0"]);
    f.ok(&["update", "acme/hello"]);
    f.ok(&["install", app.to_str().unwrap()]);
    f.ok(&["uninstall", "acme/hello", "--force"]);
    f.ok(&["install", legacy.to_str().unwrap(), "--ref", "v1.0.0"]);
    let error = f.fails(&[
        "fork",
        "acme/hello",
        "--ns",
        "pending",
        "--into",
        "pending/hello",
        "--replace",
        "--version",
        "3.0.0",
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
    f.git(&f.mono, &["tag", "-f", "v1.0.0"]);
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
    f.git(&repo, &["tag", "v1.0.0"]);
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
fn fork_ids_select_the_repository_and_rewrite_all_package_dependencies() {
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
    assert_eq!(config(&fork)["packages"], serde_json::json!(["packages/*"]));
    assert_eq!(manifest(&fork.join("packages/store"))["id"], "me/store");
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
    f.git(&fork, &["cat-file", "-e", "v1.0.0"]);
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
    // Every package in the source repository is available in the fork.
    let error = f.fails(&["install", fork.to_str().unwrap(), "--dev"]);
    assert!(
        error.contains("me/panel (packages/panel)")
            && error.contains("me/theme (packages/theme)")
            && error.contains("me/store (packages/store)"),
        "{error}"
    );
    f.ok(&["install", fork.to_str().unwrap(), "me/theme", "--dev"]);
    f.ok(&["install", fork.to_str().unwrap(), "me/panel", "--dev"]);
    assert_eq!(f.dist("me/panel"), "panel 1.0.0");

    // A single ID selects the same repository and rewrites every internal dependency.
    f.ok(&["fork", "tilr/panel", "--ns", "solo", "--into", "forks/solo"]);
    let solo = f.dir.join("forks/solo");
    assert_eq!(config(&solo)["packages"], serde_json::json!(["packages/*"]));
    assert_eq!(
        manifest(&solo.join("packages/panel"))["dependencies"],
        serde_json::json!({"solo/theme": "^1.0"})
    );
    assert_eq!(manifest(&solo.join("packages/theme"))["id"], "solo/theme");
    assert_eq!(manifest(&solo.join("packages/store"))["id"], "solo/store");
}

#[test]
fn fork_one_id_preserves_uninstalled_siblings_layout_and_installed_revisions() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    let store_commit = f.state()["packages"]["tilr/store"]["current"]["commit"].clone();
    f.write("store", "1.1.0", "");
    f.write("theme", "1.1.0", r#","dependencies":{"tilr/store":"^1.0"}"#);
    f.write("panel", "1.1.0", r#","dependencies":{"tilr/theme":"^1.0"}"#);
    fs::create_dir_all(f.mono.join("tools")).unwrap();
    fs::create_dir_all(f.mono.join("packages/support")).unwrap();
    fs::write(f.mono.join("tools/build.sh"), "echo build\n").unwrap();
    fs::write(f.mono.join("packages/support/data.txt"), "shared data\n").unwrap();
    f.git(&f.mono, &["add", "--all"]);
    f.git(
        &f.mono,
        &["commit", "-q", "-m", "Add siblings and shared tooling"],
    );
    f.git(&f.mono, &["tag", "v1.1.0"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    let before = f.state();
    let theme_commit = before["packages"]["tilr/theme"]["current"]["commit"].clone();
    assert!(before["packages"].get("tilr/panel").is_none());

    f.ok(&["fork", "tilr/store", "--ns", "me", "--into", "forks/whole"]);
    let fork = f.dir.join("forks/whole");
    assert_eq!(config(&fork), config(&f.mono));
    assert_eq!(
        fs::read(fork.join("tools/build.sh")).unwrap(),
        b"echo build\n"
    );
    assert_eq!(
        fs::read(fork.join("packages/support/data.txt")).unwrap(),
        b"shared data\n"
    );
    let store = manifest(&fork.join("packages/store"));
    let theme = manifest(&fork.join("packages/theme"));
    let panel = manifest(&fork.join("packages/panel"));
    assert_eq!(store["id"], "me/store");
    assert_eq!(store["version"], "1.0.0");
    assert_eq!(store["forkOf"]["commit"], store_commit);
    assert_eq!(theme["id"], "me/theme");
    assert_eq!(theme["version"], "1.1.0");
    assert_eq!(theme["forkOf"]["commit"], theme_commit);
    assert_eq!(
        theme["dependencies"],
        serde_json::json!({"me/store": "^1.0"})
    );
    assert_eq!(panel["id"], "me/panel");
    assert_eq!(panel["forkOf"]["id"], "tilr/panel");
    assert_eq!(panel["forkOf"]["commit"], theme_commit);
    assert_eq!(
        panel["dependencies"],
        serde_json::json!({"me/theme": "^1.0"})
    );
    assert_eq!(f.state(), before);
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
    f.git(&legacy, &["tag", "v1.0.0"]);
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
    assert!(error.contains("destination already exists"), "{error}");
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
        out.contains("Released: v2.0.0") && out.contains("Released: v2.0.0"),
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
    f.ok(&["replace", "tilr/store", "--back"]);
    let state = f.state();
    assert_eq!(provider_of(&state, "app/uses-theme"), "tilr/theme");
    assert_eq!(provider_of(&state, "app/uses-panel"), "tilr/panel");
    assert!(state.get("replacements").is_none(), "{state:#}");
}

#[test]
fn an_existing_fork_is_refused_without_rewriting_dependencies() {
    let f = Fixture::new();
    three_installed(&f);
    f.ok(&[
        "fork",
        "tilr/panel",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
    ]);
    let fork = f.dir.join("forks/me/mono");
    let before = fs::read(fork.join("packages/panel/plugin.json")).unwrap();
    let config_before = fs::read(fork.join("gitpm.json")).unwrap();
    let error = f.fails(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
    ]);
    assert!(error.contains("destination already exists"), "{error}");
    assert_eq!(
        fs::read(fork.join("packages/panel/plugin.json")).unwrap(),
        before
    );
    assert_eq!(fs::read(fork.join("gitpm.json")).unwrap(), config_before);
    assert_eq!(manifest(&fork.join("packages/theme"))["id"], "me/theme");
    assert_eq!(
        manifest(&fork.join("packages/panel"))["dependencies"],
        serde_json::json!({"me/theme": "^1.0"})
    );
}

#[test]
fn fork_replace_of_several_packages_reports_each_failure_and_keeps_the_fork() {
    let f = Fixture::new();
    three_installed(&f);
    // Introduce an uninstallable dist only during the release, after source cloning.
    release_hooks(
        &f,
        ":",
        "ln -s index.js packages/panel/dist/link.js\ngit add packages/panel/dist/link.js",
    );
    let out = f.run(&[
        "fork",
        "tilr/theme",
        "tilr/panel",
        "--ns",
        "me",
        "--into",
        "me/mono",
        "--replace",
        "--version",
        "1.0.1",
    ]);
    assert!(!out.status.success());
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(stdout.contains("Released: v1.0.1"), "{stdout}");
    assert!(stdout.contains("Released: v1.0.1"), "{stdout}");
    assert!(
        stdout.contains("tilr/theme replaced by me/theme 1.0.1"),
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
fn legacy_id_version_tags_are_ignored_without_a_warning() {
    let f = Fixture::new();
    // Legacy per-package tags never participate in release discovery.
    f.release("store", "1.1.0");
    f.git(&f.mono, &["tag", "tilr/store@9.0.0"]);
    let quiet = |args: &[&str]| {
        let out = f.run(args);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("tilr/store@9.0.0") && !stdout.contains("tilr/store@9.0.0"),
            "{args:?}: {stdout}{stderr}"
        );
        stdout
    };
    quiet(&["install", f.source(), "tilr/store", "--ref", "v1.0.0"]);
    quiet(&["install", f.source(), "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), "theme 1.0.0");
    let out = quiet(&["update", "tilr/store", "--release"]);
    assert!(out.contains("tilr/store updated 1.0.0 → 1.1.0"), "{out}");
    quiet(&["update", "all"]);
    fs::write(f.dir.join("sources.txt"), "mono\n").unwrap();
    let out = quiet(&["catalog", "update"]);
    assert!(out.contains("4 release(s)"), "{out}");
    assert!(
        !f.dir
            .join("catalog/namespaces/tilr/store/versions/9.0.0")
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
        f.git(&hello, &["tag", &format!("v{version}")]);
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
    f.git(&app, &["tag", "v1.0.0"]);
    f.ok(&["install", hello.to_str().unwrap()]);
    f.ok(&["install", app.to_str().unwrap()]);
    f.ok(&["uninstall", "acme/hello", "--force"]);
    f.ok(&["install", hello.to_str().unwrap(), "--ref", "v1.0.0"]);
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
            "'{}' --root '{}' --catalog '{}' fork acme/hello --ns {ns} --into {ns}/hello --replace --version 3.0.0 2>'{}'",
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
    assert!(stdout.contains("Released: v3.0.0"), "{stdout}");
    assert!(stdout.contains("Installed: me/hello 3.0.0"), "{stdout}");
    let state = f.state();
    assert!(state["packages"].get("me/hello").is_some());
    assert!(state.get("replacements").is_none());
    let (stdout, stderr) = run("you", "y");
    assert!(
        stdout
            .contains("acme/hello replaced by you/hello 3.0.0 (as acme/hello 1.0.0) for: acme/app"),
        "{stdout}\n{stderr}"
    );
    assert_eq!(f.state()["replacements"]["acme/hello"], "you/hello");
}

#[test]
fn fork_replace_without_a_new_version_preserves_inherited_release_tag() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    let source_tag = f
        .command("git")
        .arg("-C")
        .arg(&f.mono)
        .args(["rev-parse", "refs/tags/v1.0.0"])
        .output()
        .unwrap();
    assert!(source_tag.status.success());
    let error = f.fails(&[
        "fork",
        "tilr/store",
        "--ns",
        "me",
        "--into",
        "forks/default",
        "--replace",
    ]);
    assert!(error.contains("the release step failed"), "{error}");
    assert!(error.contains("tag already exists: v1.0.0"), "{error}");
    assert!(error.contains("version_conflict"), "{error}");
    let inherited = f
        .command("git")
        .arg("-C")
        .arg(f.dir.join("forks/default"))
        .args(["rev-parse", "refs/tags/v1.0.0"])
        .output()
        .unwrap();
    assert!(inherited.status.success());
    assert_eq!(inherited.stdout, source_tag.stdout);
    assert!(f.state()["packages"].get("me/store").is_none());
    assert!(f.state()["packages"].get("me/theme").is_none());
    assert!(f.state().get("replacements").is_none());
}
