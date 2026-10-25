//! End-to-end tests of multi-package repositories (`gitpm.json`) and explicit fork workflows.
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
        let dir = fs::canonicalize(dir).unwrap();
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
fn repository_tags_without_the_package_are_ignored_without_warnings() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    fs::remove_dir_all(f.mono.join("packages/store")).unwrap();
    f.write("theme", "2.0.0", "");
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-q", "-m", "drop store"]);
    f.git(&f.mono, &["tag", "v2.0.0"]);
    let check = f
        .command(env!("CARGO_BIN_EXE_gitpm"))
        .args(["--root", f.dir.join("workspace").to_str().unwrap(), "check"])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert!(!String::from_utf8_lossy(&check.stdout).contains("update available"));
    let update = f.run(&["update", "tilr/store"]);
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("Warning:"));
    f.ok(&["uninstall", "tilr/store"]);
    let install = f.run(&["install", f.source(), "tilr/store"]);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(!String::from_utf8_lossy(&install.stderr).contains("Warning:"));
    assert_eq!(
        f.state()["packages"]["tilr/store"]["current"]["version"],
        "1.0.0"
    );
    let error = f.fails(&["install", f.source(), "tilr/store", "--ref", "v2.0.0"]);
    assert!(error.contains("not in this repository"), "{error}");
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
    assert!(f.dir.join("workspace/worktrees/acme/hello/.git").is_file());
    assert_eq!(f.clones(), 1);
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
    let install = f.run(&["install", repo.to_str().unwrap(), "acme/two"]);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(!String::from_utf8_lossy(&install.stderr).contains("Warning:"));
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
fn explicit_forks_replace_two_packages_and_switch_back() {
    let f = Fixture::new();
    three_installed(&f);
    install_consumer(&f, "uses-theme", "tilr/theme");
    install_consumer(&f, "uses-panel", "tilr/panel");
    f.ok(&[
        "fork",
        "tilr/panel",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me/mono",
    ]);
    let fork = f.dir.join("forks/me/mono");
    f.ok(&["commit", fork.to_str().unwrap(), "1.0.1"]);
    for name in ["store", "theme", "panel"] {
        let id = format!("me/{name}");
        f.ok(&["install", fork.to_str().unwrap(), &id]);
        f.ok(&["replace", &format!("tilr/{name}"), "--with", &id]);
    }
    let state = f.state();
    assert_eq!(provider_of(&state, "app/uses-theme"), "me/theme");
    assert_eq!(provider_of(&state, "app/uses-panel"), "me/panel");
    assert_eq!(state["replacements"]["tilr/panel"], "me/panel");
    let fork = f.dir.join("forks/me/mono");
    assert_eq!(manifest(&fork.join("packages/theme"))["version"], "1.0.1");
    f.ok(&["replace", "tilr/theme", "--with", "tilr/theme"]);
    f.ok(&["replace", "tilr/panel", "--with", "tilr/panel"]);
    f.ok(&["replace", "tilr/store", "--with", "tilr/store"]);
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

/// A repository-wide fork remains separate from releasing or activating one package.
#[test]
fn plain_fork_preserves_workspace_until_explicit_install_and_replace() {
    let f = Fixture::new();
    for (name, id) in [("store", "demo/a"), ("theme", "demo/b")] {
        let path = f.mono.join(format!("packages/{name}/plugin.json"));
        fs::write(
            path,
            serde_json::json!({"id": id, "version": "0.2.0"}).to_string(),
        )
        .unwrap();
    }
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-qm", "Independent A and B"]);
    for tag in ["demo/a@0.2.0", "demo/b@0.2.0", "v0.2.0"] {
        f.git(&f.mono, &["tag", tag]);
    }
    for id in ["demo/a", "demo/b"] {
        f.ok(&["install", f.source(), id]);
    }
    let state_path = f.dir.join("workspace/state.json");
    let state_before = fs::read(&state_path).unwrap();
    let before = f.state();
    let a_dist = f.dist("demo/a");
    let b_dist = f.dist("demo/b");
    let tags = |repo: &Path| {
        let output = f
            .command("git")
            .arg("-C")
            .arg(repo)
            .args(["show-ref", "--tags"])
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    let source_tags = tags(&f.mono);
    let out = f.ok(&["fork", "demo/a", "--ns", "my", "--into", "my-fork"]);
    assert!(
        out.contains("Next steps (choose the packages to install and replace):"),
        "{out}"
    );
    assert!(
        out.contains("gitpm commit")
            && out.contains("gitpm install")
            && out.contains("gitpm replace demo/a --with my/a"),
        "{out}"
    );
    let fork = f.dir.join("my-fork");
    assert_eq!(tags(&fork), source_tags);
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    assert_eq!(f.state(), before);
    assert_eq!(f.dist("demo/a"), a_dist);
    assert_eq!(f.dist("demo/b"), b_dist);
    assert!(!f.dir.join("workspace/dist/my").exists());
    for (name, id, original) in [("store", "my/a", "demo/a"), ("theme", "my/b", "demo/b")] {
        let manifest = manifest(&fork.join(format!("packages/{name}")));
        assert_eq!(manifest["id"], id);
        assert_eq!(manifest["forkOf"]["id"], original);
        assert_eq!(manifest["replaces"]["id"], original);
    }
    f.ok(&[
        "commit",
        fork.to_str().unwrap(),
        "0.3.0",
        "--package",
        "my/a",
    ]);
    let release_head = f
        .command("git")
        .arg("-C")
        .arg(&fork)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap()
        .stdout;
    let error = f.fails(&[
        "commit",
        fork.to_str().unwrap(),
        "0.3.0",
        "--package",
        "my/a",
    ]);
    assert!(error.contains("tag already exists"), "{error}");
    assert_eq!(
        f.command("git")
            .arg("-C")
            .arg(&fork)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
        release_head
    );
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    f.ok(&["install", fork.to_str().unwrap(), "my/a"]);
    assert!(f.state().get("replacements").is_none());
    f.ok(&["replace", "demo/a", "--with", "my/a"]);
    let after = f.state();
    assert_eq!(after["packages"]["demo/b"], before["packages"]["demo/b"]);
    assert_eq!(after["replacements"], serde_json::json!({"demo/a": "my/a"}));
    assert!(after["packages"].get("my/b").is_none());
    assert_eq!(f.dist("demo/b"), b_dist);
    assert_eq!(f.dist("my/a"), a_dist);
    assert!(!f.dir.join("workspace/dist/demo/a").exists());
    assert!(!f.dir.join("workspace/dist/my/b").exists());
}

#[test]
fn removed_fork_options_have_no_side_effects() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/theme"]);
    let state = fs::read(f.dir.join("workspace/state.json")).unwrap();
    let dist = f.dist("tilr/theme");
    let clones = f.clones();
    for options in [
        vec!["--replace"],
        vec!["--version", "0.3.0"],
        vec!["--force"],
    ] {
        let mut args = vec![
            "fork",
            "tilr/theme",
            "--ns",
            "my",
            "--into",
            "rejected/fork",
        ];
        args.extend(options);
        f.fails(&args);
        assert!(!f.dir.join("rejected").exists());
        assert_eq!(fs::read(f.dir.join("workspace/state.json")).unwrap(), state);
        assert_eq!(f.dist("tilr/theme"), dist);
        assert_eq!(f.clones(), clones);
    }
    // Validation also happens before creating an otherwise absent workspace.
    let fresh = Fixture::new();
    for option in ["--replace", "--version", "--force"] {
        let mut args = vec![
            "fork",
            "demo/a",
            "--ns",
            "my",
            "--into",
            "rejected/fork",
            option,
        ];
        if option == "--version" {
            args.push("0.3.0");
        }
        fresh.fails(&args);
        assert!(!fresh.dir.join("workspace").exists());
        assert!(!fresh.dir.join("rejected").exists());
    }
}

#[test]
fn explicit_fork_release_preserves_inherited_release_tag() {
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
    f.ok(&[
        "fork",
        "tilr/store",
        "--ns",
        "me",
        "--into",
        "forks/default",
    ]);
    let error = f.fails(&[
        "commit",
        f.dir.join("forks/default").to_str().unwrap(),
        "1.0.0",
    ]);
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

#[test]
fn check_uses_catalog_then_forced_partial_origin_fallback() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&[
        "catalog",
        "register",
        f.source(),
        "--ref",
        "v1.0.0",
        "--package",
        "tilr/store",
    ]);
    f.release("store", "2.0.0");
    // The newest repository release no longer contains the installed package.
    fs::remove_dir_all(f.mono.join("packages/store")).unwrap();
    f.release("theme", "3.0.0");
    let catalog = f.ok(&["check"]);
    assert!(catalog.contains("Updates: 0."), "{catalog}");
    assert!(catalog.contains("catalog"), "{catalog}");
    let forced = f.ok(&["check", "--no-catalog"]);
    assert!(
        forced.contains("2.0.0") && forced.contains("update available"),
        "{forced}"
    );
    assert!(!forced.contains("3.0.0"), "{forced}");
    assert!(forced.contains("origin"), "{forced}");
    // A source missing from the selected catalog takes the same fallback.
    fs::remove_dir_all(f.dir.join("catalog/namespaces")).unwrap();
    let fallback = f.ok(&["check"]);
    assert!(
        fallback.contains("2.0.0") && fallback.contains("update available"),
        "{fallback}"
    );
    // Explicit bypass also ignores an invalid configured catalog.
    fs::remove_dir_all(f.dir.join("catalog")).unwrap();
    let bypass = f.ok(&["check", "--no-catalog"]);
    assert!(bypass.contains("2.0.0"), "{bypass}");
    assert_eq!(
        f.state()["packages"]["tilr/store"]["current"]["version"],
        "1.0.0"
    );
}

#[test]
fn fork_source_without_workspace_matches_installed_fork_output() {
    let f = Fixture::new();
    let dest = f.dir.join("source-fork");
    let dest_text = dest.to_str().unwrap();
    let source_output = f.ok(&[
        "fork",
        "tilr/theme",
        "--from",
        f.source(),
        "--ns",
        "me",
        "--into",
        dest_text,
    ]);
    assert!(!f.dir.join("workspace").exists());
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(dest.join("packages/store/plugin.json")).unwrap())
            .unwrap();
    assert_eq!(m["id"], "me/store");
    assert_eq!(m["forkOf"]["id"], "tilr/store");
    f.ok(&["install", f.source(), "tilr/theme"]);
    let installed_dest = f.dir.join("installed-fork");
    let installed_output = f.ok(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        installed_dest.to_str().unwrap(),
    ]);
    assert_eq!(
        source_output.replace(dest_text, "DEST"),
        installed_output.replace(installed_dest.to_str().unwrap(), "DEST")
    );
}

#[test]
fn fork_source_ref_and_usage_errors_do_not_open_workspace() {
    let f = Fixture::new();
    let dest = f.dir.join("fork");
    let dest_text = dest.to_str().unwrap();
    let e = f.fails(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        dest_text,
        "--ref",
        "main",
    ]);
    assert!(e.contains("--ref"), "{e}");
    let e = f.fails(&[
        "fork",
        "tilr/theme",
        "--from",
        f.source(),
        "--ns",
        "me",
        "--into",
        dest_text,
        "--replace",
    ]);
    assert!(e.contains("unknown option: --replace"), "{e}");
    let e = f.fails(&["list", "--from", f.source()]);
    assert!(e.contains("--from is only valid with fork"), "{e}");
    let e = f.fails(&[
        "fork",
        "--from",
        f.source(),
        "--ns",
        "me",
        "--into",
        dest_text,
    ]);
    assert!(e.contains("choose one by ID"), "{e}");
    assert!(!dest.exists());
    assert!(!f.dir.join("workspace").exists());
    // A pre-existing locked workspace must not be opened by source fork.
    let manager = gitpm::Manager::open(f.dir.join("workspace")).unwrap();
    let before = fs::read(f.dir.join("workspace/state.json")).ok();
    f.ok(&[
        "fork",
        "tilr/theme",
        "--from",
        f.source(),
        "--ref",
        "main",
        "--ns",
        "me",
        "--into",
        dest_text,
    ]);
    assert_eq!(fs::read(f.dir.join("workspace/state.json")).ok(), before);
    drop(manager);
}

#[test]
fn fork_source_readme_example() {
    let f = Fixture::new();
    let e = f.ok(&["init", "acme/hello", "--into", "./hello"]);
    assert!(e.contains("acme/hello"));
    f.ok(&[
        "fork",
        "acme/hello",
        "--from",
        "./hello",
        "--ns",
        "me",
        "--into",
        "./hello-fork",
    ]);
    assert!(!f.dir.join("workspace").exists());
    let m: serde_json::Value =
        serde_json::from_slice(&fs::read(f.dir.join("hello-fork/plugin.json")).unwrap()).unwrap();
    assert_eq!(m["id"], "me/hello");
}
