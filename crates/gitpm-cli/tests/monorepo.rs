//! End-to-end tests of multi-package repositories (`gitpm.json`) and explicit fork workflows.
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
mod common;
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
    /// Repository `mono` with `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` and `f851f609-bb81-52d6-bfef-33798624782c` at 1.0.0 (one commit, one repository tag).
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
        let id = match name {
            "store" => "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
            "theme" => "f851f609-bb81-52d6-bfef-33798624782c",
            "panel" => "955bf29f-2893-56ef-9439-369c50455ae7",
            _ => panic!("unknown fixture package"),
        };
        let folder = self.mono.join("packages").join(name);
        fs::create_dir_all(folder.join("dist")).unwrap();
        fs::write(
            folder.join("plugin.json"),
            format!(r#"{{"id":"{id}","name": "fixture", "version":"{version}"{extra}}}"#),
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
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    f.ok(&[
        "install",
        f.source(),
        "--package",
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.0.0"
    );
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.0.0"
    );
    assert_eq!(f.clones(), 1);
    assert!(!f.dir.join("workspace/packages/tilr").exists());
    let state = f.state();
    assert_eq!(
        state["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["path"], "packages/store",
        "{state:#}"
    );
    assert_eq!(
        state["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["repo"],
        state["packages"]["f851f609-bb81-52d6-bfef-33798624782c"]["repo"]
    );

    // Publishing a shared version still lets the caller update one installation.
    f.release("store", "1.1.0");
    f.ok(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.1.0"
    );
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.0.0"
    );
    assert_eq!(
        f.state()["packages"]["f851f609-bb81-52d6-bfef-33798624782c"]["current"]["version"],
        "1.0.0"
    );
    f.ok(&["rollback", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.0.0"
    );
    f.release("theme", "1.2.0");
    let out = f.ok(&["update", "all"]);
    assert!(out.contains("2 updates"), "{out}");
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.1.0"
    );
    assert_eq!(
        f.state()["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["current"]["version"],
        "1.2.0"
    );
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.2.0"
    );

    // --dev works per package (HEAD of the default branch, then followed).
    f.write("theme", "1.3.0", "");
    f.write("store", "1.3.0", "");
    f.git(&f.mono, &["commit", "-q", "-am", "theme work"]);
    f.ok(&["update", "f851f609-bb81-52d6-bfef-33798624782c", "--dev"]);
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.3.0"
    );
    // An explicit ref names a release tag of the package.
    f.ok(&[
        "update",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--ref",
        "v1.0.0",
    ]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.0.0"
    );
}

#[test]
fn installing_a_monorepo_without_a_package_id_lists_the_packages() {
    let f = Fixture::new();
    let error = f.fails(&["install", f.source()]);
    assert!(
        error.contains("choose one by ID")
            && error.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd (packages/store)")
            && error.contains("f851f609-bb81-52d6-bfef-33798624782c (packages/theme)"),
        "{error}"
    );
    assert!(error.contains("Error kind: invalid_argument"), "{error}");
    let error = f.fails(&[
        "install",
        f.source(),
        "0d443f2c-f4a2-5909-811c-922e87fc056a",
    ]);
    assert!(error.contains("not in this repository"), "{error}");
    f.fails(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--package",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    // A repository tag selects a revision, so a package ID is still required.
    f.fails(&["install", f.source(), "--ref", "v1.0.0"]);
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--ref",
        "v1.0.0",
    ]);
    f.fails(&["install", f.source(), "--dev"]);
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--dev",
    ]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.0.0"
    );
}

#[test]
fn uninstalling_one_package_keeps_the_other_working() {
    let f = Fixture::new();
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    f.ok(&["uninstall", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert!(
        !f.dir
            .join("workspace/dist/76a9b591-60f3-5c7f-9e1c-31648fc73bfd")
            .exists()
    );
    assert_eq!(f.clones(), 1);
    f.release("theme", "1.1.0");
    f.ok(&["update", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.1.0"
    );
    f.ok(&["uninstall", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(f.clones(), 0);
    assert!(
        f.state()["packages"].as_object().unwrap().is_empty(),
        "nothing left"
    );
}

#[test]
fn repository_tags_without_the_package_are_ignored_without_warnings() {
    let f = Fixture::new();
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
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
    let update = f.run(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert!(
        update.status.success(),
        "{}",
        String::from_utf8_lossy(&update.stderr)
    );
    assert!(!String::from_utf8_lossy(&update.stderr).contains("Warning:"));
    f.ok(&["uninstall", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    let install = f.run(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    assert!(!String::from_utf8_lossy(&install.stderr).contains("Warning:"));
    assert_eq!(
        f.state()["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["current"]["version"],
        "1.0.0"
    );
    let error = f.fails(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--ref",
        "v2.0.0",
    ]);
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
            format!(r#"{{"id":"7e6e8392-7be8-55ae-bd03-2b430b1c5d59","name": "fixture", "version":"{version}"}}"#),
        )
        .unwrap();
        fs::write(legacy.join("dist/index.js"), format!("hello {version}")).unwrap();
        f.git(&legacy, &["add", "--all"]);
        f.git(&legacy, &["commit", "-q", "-m", version]);
        f.git(&legacy, &["tag", &format!("v{version}")]);
    };
    release("1.0.0");
    f.ok(&["install", legacy.to_str().unwrap()]);
    assert!(
        f.dir
            .join("workspace/worktrees/7e6e8392-7be8-55ae-bd03-2b430b1c5d59/.git")
            .is_file()
    );
    assert_eq!(f.clones(), 1);
    let entry = &f.state()["packages"]["7e6e8392-7be8-55ae-bd03-2b430b1c5d59"];
    assert!(
        entry.get("path").is_none() && entry.get("repo").is_none(),
        "{entry}"
    );
    release("1.1.0");
    f.ok(&["update", "7e6e8392-7be8-55ae-bd03-2b430b1c5d59"]);
    assert_eq!(
        f.dist("7e6e8392-7be8-55ae-bd03-2b430b1c5d59"),
        "hello 1.1.0"
    );
    // A matching ID is accepted for a root repository too, a different one is not.
    f.ok(&["uninstall", "7e6e8392-7be8-55ae-bd03-2b430b1c5d59"]);
    f.fails(&[
        "install",
        legacy.to_str().unwrap(),
        "cde71d60-4de0-5f8d-a3c9-26b40d4af02a",
    ]);
    f.ok(&[
        "install",
        legacy.to_str().unwrap(),
        "7e6e8392-7be8-55ae-bd03-2b430b1c5d59",
    ]);
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
        info.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd — latest: 1.1.0")
            && info.contains("f851f609-bb81-52d6-bfef-33798624782c — latest: 1.1.0"),
        "{info}"
    );
    let release: serde_json::Value = serde_json::from_slice(
        &fs::read(f.dir.join(
            "catalog/packages/76a9b591-60f3-5c7f-9e1c-31648fc73bfd/versions/1.1.0/release.json",
        ))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(release["path"], "packages/store");
    f.ok(&["install-catalog", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    f.ok(&["install-catalog", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.1.0"
    );
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.0.0"
    );
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
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    assert!(out.contains("\"path\": \"packages/store\""), "{out}");
}

#[test]
fn init_into_and_commit_releases_all_packages_and_additions_inherit_version() {
    let f = Fixture::new();
    let repo = f.dir.join("apps");
    fs::create_dir_all(&repo).unwrap();
    f.git(&repo, &["init", "-q", "-b", "main"]);
    f.ok(&["init", "one", "--into", repo.to_str().unwrap()]);
    let one = common::package_by_name(&repo, "one");
    let one_id = common::manifest_id(&one);
    assert_eq!(one.file_name().unwrap(), one_id.as_str());
    f.ok(&["commit", repo.to_str().unwrap(), "0.2.0"]);
    let out = f.ok(&["init", "two", "--into", repo.to_str().unwrap()]);
    assert!(!out.contains("Git commit and tag"), "{out}");
    let two = common::package_by_name(&repo, "two");
    let two_id = common::manifest_id(&two);
    assert_eq!(manifest(&two)["version"], "0.2.0");
    // The alias is nonunique even inside the same repository.
    f.ok(&["init", "two", "--into", repo.to_str().unwrap()]);
    assert_eq!(fs::read_dir(repo.join("packages")).unwrap().count(), 3);
    let tags = f
        .command("git")
        .arg("-C")
        .arg(&repo)
        .args(["tag", "--list"])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8(tags.stdout).unwrap(), "v0.2.0\n");
    fs::write(two.join("dist/index.js"), "two v2").unwrap();
    f.ok(&["commit", repo.to_str().unwrap(), "0.3.0"]);
    assert_eq!(manifest(&one)["version"], "0.3.0");
    assert_eq!(manifest(&two)["version"], "0.3.0");
    f.ok(&["install", repo.to_str().unwrap(), &two_id]);
    f.ok(&["install", repo.to_str().unwrap(), &one_id]);
    assert_eq!(f.dist(&two_id), "two v2");
    assert_eq!(f.clones(), 1);
    let root = f.dir.join("single");
    f.ok(&["init", "single", "--into", root.to_str().unwrap()]);
    assert!(root.join("plugin.json").is_file() && !root.join("gitpm.json").exists());
    assert!(
        f.fails(&["init", "three", "--into", root.to_str().unwrap()])
            .contains("repository root is a package")
    );
}

fn three_installed(f: &Fixture) {
    f.write(
        "panel",
        "1.0.0",
        r#","dependencies":{"f851f609-bb81-52d6-bfef-33798624782c":"^1.0"}"#,
    );
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-q", "-m", "panel"]);
    f.git(&f.mono, &["tag", "-f", "v1.0.0"]);
    for id in [
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "955bf29f-2893-56ef-9439-369c50455ae7",
    ] {
        f.ok(&["install", f.source(), id]);
    }
}
fn manifest(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path.join("plugin.json")).unwrap()).unwrap()
}
fn config(repo: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(repo.join("gitpm.json")).unwrap()).unwrap()
}
/// A root consumer `4248595a-3373-52d6-b64e-fbc505c1b144` depending on `DEPENDENCY ^1.0`, installed.
fn install_consumer(f: &Fixture, name: &str, dependency: &str) {
    let id = match name {
        "uses-theme" => "4c71f036-f988-52f8-87db-af543d39cac7",
        "uses-panel" => "42361f17-3146-5bb1-a7d6-04973b2aac4a",
        _ => panic!("unknown consumer fixture"),
    };
    let repo = f.dir.join(name);
    fs::create_dir_all(repo.join("dist")).unwrap();
    f.git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(
        repo.join("plugin.json"),
        format!(
            r#"{{"id":"{id}","name":"{name}","version":"1.0.0","dependencies":{{"{dependency}":"^1.0"}}}}"#
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
        "f851f609-bb81-52d6-bfef-33798624782c",
        "955bf29f-2893-56ef-9439-369c50455ae7",
        "--into",
        "7200845d-5956-5ec3-85d1-a7161d3592de/mono",
    ]);
    let fork = f.dir.join("7200845d-5956-5ec3-85d1-a7161d3592de/mono");
    let fork_store = common::manifest_id(&fork.join("packages/store"));
    let fork_theme = common::manifest_id(&fork.join("packages/theme"));
    let fork_panel = common::manifest_id(&fork.join("packages/panel"));

    assert!(
        out.contains("Fork created: /")
            && out.contains("7200845d-5956-5ec3-85d1-a7161d3592de/mono\n"),
        "{out}"
    );
    assert!(
        out.contains(
            format!(
                "{fork_panel} forked from 955bf29f-2893-56ef-9439-369c50455ae7 (packages/panel)"
            )
            .as_str()
        ),
        "{out}"
    );
    assert!(
        out.contains(format!("Dependency: {fork_panel} now depends on {fork_theme} ^1.0 (was f851f609-bb81-52d6-bfef-33798624782c)").as_str()),
        "{out}"
    );
    assert_eq!(config(&fork)["packages"], serde_json::json!(["packages/*"]));
    assert_eq!(
        manifest(&fork.join("packages/store"))["id"],
        fork_store.as_str()
    );
    let panel = manifest(&fork.join("packages/panel"));
    assert_eq!(panel["id"], fork_panel.as_str());
    assert_eq!(
        panel["dependencies"],
        serde_json::json!({(fork_theme.as_str()): "^1.0"})
    );
    assert_eq!(
        panel["forkOf"]["id"],
        "955bf29f-2893-56ef-9439-369c50455ae7"
    );
    assert_eq!(
        panel["replaces"]["id"],
        "955bf29f-2893-56ef-9439-369c50455ae7"
    );
    assert_eq!(
        manifest(&fork.join("packages/theme"))["id"],
        fork_theme.as_str()
    );
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
        error.contains(format!("{fork_panel} (packages/panel)").as_str())
            && error.contains(format!("{fork_theme} (packages/theme)").as_str())
            && error.contains(format!("{fork_store} (packages/store)").as_str()),
        "{error}"
    );
    f.ok(&[
        "install",
        fork.to_str().unwrap(),
        fork_theme.as_str(),
        "--dev",
    ]);
    f.ok(&[
        "install",
        fork.to_str().unwrap(),
        fork_panel.as_str(),
        "--dev",
    ]);
    assert_eq!(f.dist(fork_panel.as_str()), "panel 1.0.0");

    // A single ID selects the same repository and rewrites every internal dependency.
    f.ok(&[
        "fork",
        "955bf29f-2893-56ef-9439-369c50455ae7",
        "--into",
        "a4210d34-bc7b-5749-b830-728c343a20e5",
    ]);
    let solo = f.dir.join("a4210d34-bc7b-5749-b830-728c343a20e5");
    let solo_store = common::manifest_id(&solo.join("packages/store"));
    let solo_theme = common::manifest_id(&solo.join("packages/theme"));

    assert_eq!(config(&solo)["packages"], serde_json::json!(["packages/*"]));
    assert_eq!(
        manifest(&solo.join("packages/panel"))["dependencies"],
        serde_json::json!({(solo_theme.as_str()): "^1.0"})
    );
    assert_eq!(
        manifest(&solo.join("packages/theme"))["id"],
        solo_theme.as_str()
    );
    assert_eq!(
        manifest(&solo.join("packages/store"))["id"],
        solo_store.as_str()
    );
}

#[test]
fn fork_one_id_preserves_uninstalled_siblings_layout_and_installed_revisions() {
    let f = Fixture::new();
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    let store_commit =
        f.state()["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["current"]["commit"].clone();
    f.write("store", "1.1.0", "");
    f.write(
        "theme",
        "1.1.0",
        r#","dependencies":{"76a9b591-60f3-5c7f-9e1c-31648fc73bfd":"^1.0"}"#,
    );
    f.write(
        "panel",
        "1.1.0",
        r#","dependencies":{"f851f609-bb81-52d6-bfef-33798624782c":"^1.0"}"#,
    );
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
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    let before = f.state();
    let theme_commit =
        before["packages"]["f851f609-bb81-52d6-bfef-33798624782c"]["current"]["commit"].clone();
    assert!(
        before["packages"]
            .get("955bf29f-2893-56ef-9439-369c50455ae7")
            .is_none()
    );

    f.ok(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--into",
        "d5f9ad6e-280b-5cc4-958d-30e4356fae7a",
    ]);
    let fork = f.dir.join("d5f9ad6e-280b-5cc4-958d-30e4356fae7a");
    let fork_store = common::manifest_id(&fork.join("packages/store"));
    let fork_theme = common::manifest_id(&fork.join("packages/theme"));
    let fork_panel = common::manifest_id(&fork.join("packages/panel"));

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
    assert_eq!(store["id"], fork_store.as_str());
    assert_eq!(store["version"], "1.0.0");
    assert_eq!(store["forkOf"]["commit"], store_commit);
    assert_eq!(theme["id"], fork_theme.as_str());
    assert_eq!(theme["version"], "1.1.0");
    assert_eq!(theme["forkOf"]["commit"], theme_commit);
    assert_eq!(
        theme["dependencies"],
        serde_json::json!({(fork_store.as_str()): "^1.0"})
    );
    assert_eq!(panel["id"], fork_panel.as_str());
    assert_eq!(
        panel["forkOf"]["id"],
        "955bf29f-2893-56ef-9439-369c50455ae7"
    );
    assert_eq!(panel["forkOf"]["commit"], theme_commit);
    assert_eq!(
        panel["dependencies"],
        serde_json::json!({(fork_theme.as_str()): "^1.0"})
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
        r#"{"id":"7e6e8392-7be8-55ae-bd03-2b430b1c5d59","name": "fixture", "version":"1.0.0"}"#,
    )
    .unwrap();
    fs::write(legacy.join("dist/index.js"), "hello").unwrap();
    f.git(&legacy, &["add", "--all"]);
    f.git(&legacy, &["commit", "-q", "-m", "hello"]);
    f.git(&legacy, &["tag", "v1.0.0"]);
    f.ok(&["install", legacy.to_str().unwrap()]);
    let error = f.fails(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "7e6e8392-7be8-55ae-bd03-2b430b1c5d59",
        "--into",
        "4abc5c91-33eb-5b47-bb82-5c8b4a6dae44",
    ]);
    assert!(
        error.contains("must come from one repository")
            && error.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd from /")
            && error.contains("/mono; 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 from /")
            && error.ends_with("/hello\nError kind: invalid_argument\n"),
        "{error}"
    );
    assert!(error.contains("Error kind: invalid_argument"), "{error}");
    let error = f.fails(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "0d443f2c-f4a2-5909-811c-922e87fc056a",
        "--into",
        "4abc5c91-33eb-5b47-bb82-5c8b4a6dae44",
    ]);
    assert!(error.contains("Error kind: not_installed"), "{error}");
    let error = f.fails(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--into",
        "4abc5c91-33eb-5b47-bb82-5c8b4a6dae44",
    ]);
    assert!(error.contains("more than once"), "{error}");
    // The source repository is never the fork repository.
    let error = f.fails(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--into",
        "mono",
    ]);
    assert!(error.contains("is the source repository"), "{error}");
    assert!(!f.dir.join("me").exists() && !f.dir.join("tilr").exists());
    // A root package forks through the same command (no gitpm.json in the fork).
    let out = f.ok(&[
        "fork",
        "7e6e8392-7be8-55ae-bd03-2b430b1c5d59",
        "--into",
        "ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4",
    ]);
    let root_fork_id = common::manifest_id(&f.dir.join("ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4"));

    assert!(
        out.contains(
            format!(
                "{root_fork_id} forked from 7e6e8392-7be8-55ae-bd03-2b430b1c5d59 (repository root)"
            )
            .as_str()
        ),
        "{out}"
    );
    assert!(
        f.dir
            .join("ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4/plugin.json")
            .is_file()
    );
    assert!(
        !f.dir
            .join("ce6c8a3b-b2f8-5391-8ef8-4cbb7873f5d4/gitpm.json")
            .exists()
    );
    // ...but cannot be added to a multi-package repository.
    f.ok(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--into",
        "4abc5c91-33eb-5b47-bb82-5c8b4a6dae44",
    ]);
    let error = f.fails(&[
        "fork",
        "7e6e8392-7be8-55ae-bd03-2b430b1c5d59",
        "--into",
        "4abc5c91-33eb-5b47-bb82-5c8b4a6dae44",
    ]);
    assert!(error.contains("destination already exists"), "{error}");
}

#[test]
fn explicit_forks_replace_two_packages_and_switch_back() {
    let f = Fixture::new();
    three_installed(&f);
    install_consumer(&f, "uses-theme", "f851f609-bb81-52d6-bfef-33798624782c");
    install_consumer(&f, "uses-panel", "955bf29f-2893-56ef-9439-369c50455ae7");
    f.ok(&[
        "fork",
        "955bf29f-2893-56ef-9439-369c50455ae7",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--into",
        "7200845d-5956-5ec3-85d1-a7161d3592de/mono",
    ]);
    let fork = f.dir.join("7200845d-5956-5ec3-85d1-a7161d3592de/mono");
    let fork_theme = common::manifest_id(&fork.join("packages/theme"));
    let fork_panel = common::manifest_id(&fork.join("packages/panel"));

    f.ok(&["commit", fork.to_str().unwrap(), "1.0.1"]);
    for name in ["store", "theme", "panel"] {
        let package = manifest(&fork.join(format!("packages/{name}")));
        let id = package["id"].as_str().unwrap();
        let original = package["forkOf"]["id"].as_str().unwrap();
        f.ok(&["install", fork.to_str().unwrap(), id]);
        f.ok(&["replace", original, "--with", id]);
    }
    let state = f.state();
    assert_eq!(
        provider_of(&state, "4c71f036-f988-52f8-87db-af543d39cac7"),
        fork_theme.as_str()
    );
    assert_eq!(
        provider_of(&state, "42361f17-3146-5bb1-a7d6-04973b2aac4a"),
        fork_panel.as_str()
    );
    assert_eq!(
        state["replacements"]["955bf29f-2893-56ef-9439-369c50455ae7"],
        fork_panel.as_str()
    );
    let fork = f.dir.join("7200845d-5956-5ec3-85d1-a7161d3592de/mono");
    assert_eq!(manifest(&fork.join("packages/theme"))["version"], "1.0.1");
    f.ok(&[
        "replace",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--with",
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    f.ok(&[
        "replace",
        "955bf29f-2893-56ef-9439-369c50455ae7",
        "--with",
        "955bf29f-2893-56ef-9439-369c50455ae7",
    ]);
    f.ok(&[
        "replace",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--with",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    let state = f.state();
    assert_eq!(
        provider_of(&state, "4c71f036-f988-52f8-87db-af543d39cac7"),
        "f851f609-bb81-52d6-bfef-33798624782c"
    );
    assert_eq!(
        provider_of(&state, "42361f17-3146-5bb1-a7d6-04973b2aac4a"),
        "955bf29f-2893-56ef-9439-369c50455ae7"
    );
    assert!(state.get("replacements").is_none(), "{state:#}");
}

#[test]
fn an_existing_fork_is_refused_without_rewriting_dependencies() {
    let f = Fixture::new();
    three_installed(&f);
    f.ok(&[
        "fork",
        "955bf29f-2893-56ef-9439-369c50455ae7",
        "--into",
        "7200845d-5956-5ec3-85d1-a7161d3592de/mono",
    ]);
    let fork = f.dir.join("7200845d-5956-5ec3-85d1-a7161d3592de/mono");
    let fork_theme = common::manifest_id(&fork.join("packages/theme"));

    let before = fs::read(fork.join("packages/panel/plugin.json")).unwrap();
    let config_before = fs::read(fork.join("gitpm.json")).unwrap();
    let error = f.fails(&[
        "fork",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--into",
        "7200845d-5956-5ec3-85d1-a7161d3592de/mono",
    ]);
    assert!(error.contains("destination already exists"), "{error}");
    assert_eq!(
        fs::read(fork.join("packages/panel/plugin.json")).unwrap(),
        before
    );
    assert_eq!(fs::read(fork.join("gitpm.json")).unwrap(), config_before);
    assert_eq!(
        manifest(&fork.join("packages/theme"))["id"],
        fork_theme.as_str()
    );
    assert_eq!(
        manifest(&fork.join("packages/panel"))["dependencies"],
        serde_json::json!({(fork_theme.as_str()): "^1.0"})
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
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    assert_eq!(f.clones(), 1);
    f.release("theme", "1.1.0");
    f.ok(&["update", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.1.0"
    );
    // forget/restore of a shared package goes through the same clone.
    f.ok(&["forget", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    f.ok(&["restore", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        f.dist("76a9b591-60f3-5c7f-9e1c-31648fc73bfd"),
        "store 1.0.0"
    );
}

#[test]
fn legacy_id_version_tags_are_ignored_without_a_warning() {
    let f = Fixture::new();
    // Legacy per-package tags never participate in release discovery.
    f.release("store", "1.1.0");
    f.git(
        &f.mono,
        &["tag", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd@9.0.0"],
    );
    let quiet = |args: &[&str]| {
        let out = f.run(args);
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(out.status.success(), "{args:?}: {stderr}");
        assert!(
            !stderr.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd@9.0.0")
                && !stdout.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd@9.0.0"),
            "{args:?}: {stdout}{stderr}"
        );
        stdout
    };
    quiet(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--ref",
        "v1.0.0",
    ]);
    quiet(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    assert_eq!(
        f.dist("f851f609-bb81-52d6-bfef-33798624782c"),
        "theme 1.0.0"
    );
    let out = quiet(&[
        "update",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--release",
    ]);
    assert!(
        out.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd updated 1.0.0 → 1.1.0"),
        "{out}"
    );
    quiet(&["update", "all"]);
    fs::write(f.dir.join("sources.txt"), "mono\n").unwrap();
    let out = quiet(&["catalog", "update"]);
    assert!(out.contains("4 release(s)"), "{out}");
    assert!(
        !f.dir
            .join("catalog/packages/76a9b591-60f3-5c7f-9e1c-31648fc73bfd/versions/9.0.0")
            .exists()
    );
}

/// A repository-wide fork remains separate from releasing or activating one package.
#[test]
fn plain_fork_preserves_workspace_until_explicit_install_and_replace() {
    let f = Fixture::new();
    for (name, id) in [
        ("store", "d63a8a6b-85d6-509c-87db-9a6096daa66c"),
        ("theme", "e66d8976-2209-50e5-abaf-9360c0a03411"),
    ] {
        let path = f.mono.join(format!("packages/{name}/plugin.json"));
        fs::write(
            path,
            serde_json::json!({"id": id, "name": "fixture", "version": "0.2.0"}).to_string(),
        )
        .unwrap();
    }
    f.git(&f.mono, &["add", "--all"]);
    f.git(&f.mono, &["commit", "-qm", "Independent A and B"]);
    for tag in [
        "d63a8a6b-85d6-509c-87db-9a6096daa66c@0.2.0",
        "e66d8976-2209-50e5-abaf-9360c0a03411@0.2.0",
        "v0.2.0",
    ] {
        f.git(&f.mono, &["tag", tag]);
    }
    for id in [
        "d63a8a6b-85d6-509c-87db-9a6096daa66c",
        "e66d8976-2209-50e5-abaf-9360c0a03411",
    ] {
        f.ok(&["install", f.source(), id]);
    }
    let state_path = f.dir.join("workspace/state.json");
    let state_before = fs::read(&state_path).unwrap();
    let before = f.state();
    let a_dist = f.dist("d63a8a6b-85d6-509c-87db-9a6096daa66c");
    let b_dist = f.dist("e66d8976-2209-50e5-abaf-9360c0a03411");
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
    let out = f.ok(&[
        "fork",
        "d63a8a6b-85d6-509c-87db-9a6096daa66c",
        "--into",
        "my-fork",
    ]);
    let fork = f.dir.join("my-fork");
    let fork_a = common::manifest_id(&fork.join("packages/store"));
    let fork_b = common::manifest_id(&fork.join("packages/theme"));

    assert!(
        out.contains("Next steps (choose the packages to install and replace):"),
        "{out}"
    );
    assert!(
        out.contains("gitpm commit")
            && out.contains("gitpm install")
            && out.contains(
                format!("gitpm replace d63a8a6b-85d6-509c-87db-9a6096daa66c --with {fork_a}")
                    .as_str()
            ),
        "{out}"
    );
    assert_eq!(tags(&fork), source_tags);
    assert_eq!(fs::read(&state_path).unwrap(), state_before);
    assert_eq!(f.state(), before);
    assert_eq!(f.dist("d63a8a6b-85d6-509c-87db-9a6096daa66c"), a_dist);
    assert_eq!(f.dist("e66d8976-2209-50e5-abaf-9360c0a03411"), b_dist);
    assert!(!f.dir.join("workspace/dist/my").exists());
    for (name, id, original) in [
        (
            "store",
            fork_a.as_str(),
            "d63a8a6b-85d6-509c-87db-9a6096daa66c",
        ),
        (
            "theme",
            fork_b.as_str(),
            "e66d8976-2209-50e5-abaf-9360c0a03411",
        ),
    ] {
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
        fork_a.as_str(),
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
        fork_a.as_str(),
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
    f.ok(&["install", fork.to_str().unwrap(), fork_a.as_str()]);
    assert!(f.state().get("replacements").is_none());
    f.ok(&[
        "replace",
        "d63a8a6b-85d6-509c-87db-9a6096daa66c",
        "--with",
        fork_a.as_str(),
    ]);
    let after = f.state();
    assert_eq!(
        after["packages"]["e66d8976-2209-50e5-abaf-9360c0a03411"],
        before["packages"]["e66d8976-2209-50e5-abaf-9360c0a03411"]
    );
    assert_eq!(
        after["replacements"],
        serde_json::json!({"d63a8a6b-85d6-509c-87db-9a6096daa66c": fork_a.as_str()})
    );
    assert!(after["packages"].get(fork_b.as_str()).is_none());
    assert_eq!(f.dist("e66d8976-2209-50e5-abaf-9360c0a03411"), b_dist);
    assert_eq!(f.dist(fork_a.as_str()), a_dist);
    assert!(
        !f.dir
            .join("workspace/dist/d63a8a6b-85d6-509c-87db-9a6096daa66c")
            .exists()
    );
    assert!(
        !f.dir
            .join(format!("workspace/dist/{fork_b}").as_str())
            .exists()
    );
}

#[test]
fn removed_fork_options_have_no_side_effects() {
    let f = Fixture::new();
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    let state = fs::read(f.dir.join("workspace/state.json")).unwrap();
    let dist = f.dist("f851f609-bb81-52d6-bfef-33798624782c");
    let clones = f.clones();
    for options in [
        vec!["--replace"],
        vec!["--version", "0.3.0"],
        vec!["--force"],
    ] {
        let mut args = vec![
            "fork",
            "f851f609-bb81-52d6-bfef-33798624782c",
            "--into",
            "rejected/fork",
        ];
        args.extend(options);
        f.fails(&args);
        assert!(!f.dir.join("rejected").exists());
        assert_eq!(fs::read(f.dir.join("workspace/state.json")).unwrap(), state);
        assert_eq!(f.dist("f851f609-bb81-52d6-bfef-33798624782c"), dist);
        assert_eq!(f.clones(), clones);
    }
    // Validation also happens before creating an otherwise absent workspace.
    let fresh = Fixture::new();
    for option in ["--replace", "--version", "--force"] {
        let mut args = vec![
            "fork",
            "d63a8a6b-85d6-509c-87db-9a6096daa66c",
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
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
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
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--into",
        "656af2d4-f94e-5ede-a8ae-e70fb52fef03",
    ]);
    let error = f.fails(&[
        "commit",
        f.dir
            .join("656af2d4-f94e-5ede-a8ae-e70fb52fef03")
            .to_str()
            .unwrap(),
        "1.0.0",
    ]);
    assert!(error.contains("tag already exists: v1.0.0"), "{error}");
    assert!(error.contains("version_conflict"), "{error}");
    let inherited = f
        .command("git")
        .arg("-C")
        .arg(f.dir.join("656af2d4-f94e-5ede-a8ae-e70fb52fef03"))
        .args(["rev-parse", "refs/tags/v1.0.0"])
        .output()
        .unwrap();
    assert!(inherited.status.success());
    assert_eq!(inherited.stdout, source_tag.stdout);
    assert!(
        f.state()["packages"]
            .get("f86c5a66-031a-5b0e-9a54-7ceefc6fd352")
            .is_none()
    );
    assert!(
        f.state()["packages"]
            .get("d4377c45-ac7f-55f6-97f8-bd1c9ae7ca50")
            .is_none()
    );
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
    let out = f.ok(&[
        "commit",
        f.source(),
        "1.1.0",
        "--package",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    assert!(
        out.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd committed 1.1.0"),
        "{out}"
    );
    for name in ["store", "theme"] {
        assert_eq!(
            manifest(&f.mono.join("packages").join(name))["version"],
            "1.1.0"
        );
    }
    f.git(
        &f.mono,
        &["tag", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd@9.0.0"],
    );
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    assert_eq!(
        f.state()["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["current"]["version"],
        "1.1.0"
    );
}

#[test]
fn check_uses_catalog_then_forced_partial_origin_fallback() {
    let f = Fixture::new();
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    f.ok(&[
        "catalog",
        "register",
        f.source(),
        "--ref",
        "v1.0.0",
        "--package",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
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
    fs::remove_dir_all(f.dir.join("catalog/packages")).unwrap();
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
        f.state()["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["current"]["version"],
        "1.0.0"
    );
}
