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
    /// `vendor/demo` installed from `vendor/demo` (dist "vendor 1"), and its fork
    /// `dan/demo` released from `dan/demo` (dist "fork") and installed.
    fn original_and_fork(&self) {
        self.ok(&["init", "vendor/demo", "--into", "vendor/demo"]);
        self.release("vendor/demo", "vendor 1", "0.1.1");
        self.ok(&["install", self.path("vendor/demo").to_str().unwrap()]);
        self.ok(&["fork", "vendor/demo", "--ns", "dan", "--into", "dan/demo"]);
        self.release("dan/demo", "fork", "0.2.0");
        self.ok(&["install", self.path("dan/demo").to_str().unwrap()]);
        assert_eq!(self.dist("vendor/demo").as_deref(), Some("vendor 1"));
        assert_eq!(self.dist("dan/demo").as_deref(), Some("fork"));
    }
}
impl Fixture {
    /// `app/a` (needs `vendor/demo >=0.1`) released from `app/a` and installed.
    fn consumer(&self) {
        self.ok(&["init", "app/a", "--into", "app/a"]);
        let manifest = self.path("app/a/plugin.json");
        let mut json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
        json["dependencies"] = serde_json::json!({"vendor/demo": ">=0.1"});
        fs::write(&manifest, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        self.release("app/a", "app", "0.1.1");
        self.ok(&["install", self.path("app/a").to_str().unwrap()]);
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
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(
        !f.root().join("dist/vendor").exists(),
        "empty namespace removed"
    );
    // The fork keeps its own dist; nothing is copied into the original's folder.
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    // list still reports the original as installed and enabled.
    let state = f.state();
    assert_eq!(state["packages"]["vendor/demo"]["enabled"], true);
    assert_eq!(state["replacements"]["vendor/demo"], "dan/demo");
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
    assert_eq!(resolved(&state, "vendor/demo"), (true.into(), false.into()));
    assert_eq!(resolved(&state, "dan/demo"), (true.into(), true.into()));
    let out = f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert!(
        out.contains(
            "; dan/demo stays installed and enabled (disable or uninstall it if not wanted)"
        ),
        "{out}"
    );
    assert_eq!(
        resolved(&f.state(), "vendor/demo"),
        (true.into(), true.into())
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
}

#[test]
fn disable_removes_and_enable_restores_also_around_a_replacement() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["disable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["enable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // Returning to a disabled original is refused until it is explicitly enabled.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["disable", "vendor/demo"]);
    assert!(
        f.fails(&["replace", "vendor/demo", "--with", "vendor/demo"])
            .contains("enable it first")
    );
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["enable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // Disabling the replacement drops it: the installed original takes over and is
    // deployed again; a later enable does not bring the binding back.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["disable", "dan/demo"]);
    assert_eq!(f.dist("dan/demo"), None);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(f.state().get("replacements").is_none());
    f.ok(&["enable", "dan/demo"]);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    assert!(f.state().get("replacements").is_none());
    // A consumer the original cannot serve (it is uninstalled) guards the fork.
    f.consumer();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["uninstall", "vendor/demo"]);
    let error = f.fails(&["disable", "dan/demo"]);
    assert!(
        error.contains("cannot disable dan/demo: required by app/a")
            && error.contains("Error kind: has_dependents")
            && error.contains("Hint: `gitpm disable dan/demo --force` does it anyway"),
        "{error}"
    );
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    // Forced: the binding is dropped and nothing is deployed for the original.
    f.ok(&["disable", "dan/demo", "--force"]);
    assert_eq!(f.dist("dan/demo"), None);
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(!f.root().join("dist/dan").exists());
    assert!(f.state().get("replacements").is_none());
    // A disabled package cannot become a replacement.
    f.ok(&["install", f.path("vendor/demo").to_str().unwrap()]);
    let error = f.fails(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert!(
        error.contains("dan/demo is disabled; enable it first"),
        "{error}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
}

#[test]
fn update_and_rollback_of_a_replaced_package_leave_dist_alone_until_back() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.release("vendor/demo", "vendor 2", "0.3.0");
    f.ok(&["update", "vendor/demo"]);
    assert_eq!(
        f.state()["packages"]["vendor/demo"]["current"]["version"],
        "0.3.0"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 2"));
    // Active again: rollback deploys as usual.
    f.ok(&["rollback", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // Replaced: rollback only changes state and clone.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["rollback", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 2"));
}

#[test]
fn installing_a_package_that_is_replaced_deploys_nothing() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    // Uninstalling the original keeps the replacement binding.
    f.ok(&["uninstall", "vendor/demo"]);
    assert_eq!(f.state()["replacements"]["vendor/demo"], "dan/demo");
    f.ok(&["install", f.path("vendor/demo").to_str().unwrap()]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
}

#[test]
fn explicit_fork_install_and_replace_deploys_only_the_fork() {
    let f = Fixture::new();
    f.ok(&["init", "vendor/demo", "--into", "vendor/demo"]);
    f.ok(&["install", f.path("vendor/demo").to_str().unwrap()]);
    let original = f.dist("vendor/demo").unwrap();
    f.ok(&["fork", "vendor/demo", "--ns", "dan", "--into", "dan/demo"]);
    f.ok(&["commit", f.path("dan/demo").to_str().unwrap(), "0.2.0"]);
    f.ok(&["install", f.path("dan/demo").to_str().unwrap()]);
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    assert_eq!(f.dist("dan/demo").as_deref(), Some(original.as_str()));
    assert_eq!(f.state()["replacements"]["vendor/demo"], "dan/demo");
}

#[test]
fn uninstalling_or_forgetting_the_replacement_deploys_the_original_again() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    // forget drops the replacement (and the fork's dist); restore keeps it dropped.
    f.ok(&["forget", "dan/demo"]);
    assert!(f.state().get("replacements").is_none());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert_eq!(f.dist("dan/demo"), None);
    f.ok(&["restore", "dan/demo"]);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // uninstall drops the replacement too.
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    f.ok(&["uninstall", "dan/demo"]);
    assert!(f.state().get("replacements").is_none());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(!f.root().join("dist/dan").exists());
}

/// Repository `mono` with packages `tilr/store` and `tilr/theme` (dist "NAME VERSION").
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
    let folder = mono.join("packages").join(name);
    fs::create_dir_all(folder.join("dist")).unwrap();
    fs::write(
        folder.join("plugin.json"),
        format!(r#"{{"id":"tilr/{name}","version":"{version}"}}"#),
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
    f.ok(&["install", source, "tilr/store"]);
    f.ok(&["install", source, "tilr/theme"]);
    f.ok(&["fork", "tilr/theme", "--ns", "me", "--into", "forks/me"]);
    let fork = f.path("forks/me");
    f.ok(&[
        "commit",
        fork.to_str().unwrap(),
        "1.0.1",
        "--package",
        "me/theme",
    ]);
    f.ok(&["install", fork.to_str().unwrap(), "me/theme"]);
    f.ok(&["replace", "tilr/theme", "--with", "me/theme"]);
    assert_eq!(f.dist("tilr/theme"), None);
    assert_eq!(f.dist("me/theme").as_deref(), Some("theme 1.0.0"));
    // Only theme was explicitly installed and replaced; store keeps its original provider.
    assert_eq!(f.dist("me/store"), None);
    assert_eq!(f.dist("tilr/store").as_deref(), Some("store 1.0.0"));
    // theme 1.1.0 is installed while replaced; then store moves the shared clone's HEAD
    // to a commit whose theme files differ from theme's installed revision.
    release_package(&f, &mono, "theme", "1.1.0");
    f.ok(&["update", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), None);
    fs::write(
        mono.join("packages/theme/dist/index.js"),
        "theme unreleased",
    )
    .unwrap();
    release_package(&f, &mono, "store", "1.2.0");
    f.ok(&["update", "tilr/store"]);
    assert_eq!(f.dist("tilr/store").as_deref(), Some("store 1.2.0"));
    f.ok(&["replace", "tilr/theme", "--with", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme").as_deref(), Some("theme 1.1.0"));
    assert_eq!(f.dist("me/theme").as_deref(), Some("theme 1.0.0"));
    f.ok(&["disable", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme"), None);
    f.ok(&["enable", "tilr/theme"]);
    assert_eq!(f.dist("tilr/theme").as_deref(), Some("theme 1.1.0"));
}

#[test]
fn sync_dist_repairs_a_tree_written_by_an_older_gitpm() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    // As gitpm 0.4 left it: the replaced original deployed, the fork's dist missing,
    // plus a folder of no known package.
    let dist = f.root().join("dist");
    fs::create_dir_all(dist.join("vendor/demo")).unwrap();
    fs::write(dist.join("vendor/demo/index.js"), "stale").unwrap();
    fs::remove_dir_all(dist.join("dan")).unwrap();
    fs::create_dir_all(dist.join("stray")).unwrap();
    // Read-only commands never rewrite dist.
    f.state();
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("stale"));
    let out = f.run(&["sync-dist", "--dry-run"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("dan/demo would be deployed to dist/dan/demo"),
        "{stdout}"
    );
    assert!(
        stdout.contains("vendor/demo would be removed from dist/vendor/demo"),
        "{stdout}"
    );
    assert!(stdout.contains("Nothing was changed"), "{stdout}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dist/stray belongs to no known package"),
        "{stderr}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("stale"));
    assert_eq!(f.dist("dan/demo"), None);
    let out = f.ok(&["sync-dist"]);
    assert!(out.contains("dan/demo deployed to dist/dan/demo"), "{out}");
    assert!(
        out.contains("vendor/demo removed from dist/vendor/demo"),
        "{out}"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(!dist.join("vendor").exists());
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
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
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    f.ok(&["disable", "vendor/demo"]);
    fs::create_dir_all(dist.join("vendor/demo")).unwrap();
    let out = f.ok(&["sync-dist"]);
    assert!(
        out.contains("vendor/demo removed from dist/vendor/demo"),
        "{out}"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    // Other options are refused.
    assert!(!f.run(&["sync-dist", "--force"]).status.success());
    assert!(!f.run(&["sync-dist", "vendor/demo"]).status.success());
}

#[test]
fn any_mutating_command_reconciles_the_whole_dist() {
    let f = Fixture::new();
    f.original_and_fork();
    f.ok(&["disable", "dan/demo"]);
    // Drift no command of this test touches: an active package's dist missing, a
    // disabled package deployed, a folder of no known package.
    let dist = f.root().join("dist");
    fs::remove_dir_all(dist.join("vendor")).unwrap();
    fs::create_dir_all(dist.join("dan/demo")).unwrap();
    fs::write(dist.join("dan/demo/index.js"), "stale").unwrap();
    fs::create_dir_all(dist.join("stray")).unwrap();
    f.state();
    assert_eq!(f.dist("vendor/demo"), None, "list never changes dist");
    // An unrelated install fixes all of it.
    f.ok(&["init", "other/pkg", "--into", "other/pkg"]);
    let out = f.run(&["install", f.path("other/pkg").to_str().unwrap()]);
    assert!(out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("dist/stray belongs to no known package"),
        "{stderr}"
    );
    assert!(f.dist("other/pkg").is_some());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert_eq!(f.dist("dan/demo"), None);
    assert!(!dist.join("dan").exists());
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
fn a_namespace_path_that_is_not_a_folder_blocks_nothing_else() {
    let f = Fixture::new();
    f.original_and_fork();
    // dist/dan becomes a symlink to a folder outside the workspace.
    let dist = f.root().join("dist");
    let outside = f.path("outside");
    fs::rename(dist.join("dan"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, dist.join("dan")).unwrap();
    fs::remove_dir_all(dist.join("vendor")).unwrap();
    f.ok(&["init", "other/pkg", "--into", "other/pkg"]);
    let out = f.run(&["install", f.path("other/pkg").to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains("dist/dan belongs to no known package or is not a folder"),
        "{stderr}"
    );
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(
        fs::symlink_metadata(dist.join("dan"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(outside.join("demo/index.js")).unwrap(),
        "fork",
        "never written through"
    );
    // sync-dist reports it and syncs the rest.
    fs::remove_dir_all(dist.join("vendor")).unwrap();
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(
        stderr.contains("dist/dan belongs to no known package or is not a folder"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Warning: dan/demo: dist not deployed (dist/dan is not a folder)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error: 1 package(s) not deployed: dan/demo"),
        "{stderr}"
    );
    assert!(stderr.contains("Error kind: not_deployed"), "{stderr}");
    assert!(
        stderr.contains("Hint: remove or rename `dist/dan` and run `gitpm sync-dist` again"),
        "{stderr}"
    );
    assert!(!stderr.contains("clone is gone"), "{stderr}");
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    // A plain file there behaves the same.
    fs::remove_file(dist.join("dan")).unwrap();
    fs::write(dist.join("dan"), "file").unwrap();
    f.ok(&["disable", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo"), None);
    assert_eq!(fs::read_to_string(dist.join("dan")).unwrap(), "file");
    let out = f.run(&["uninstall", "dan/demo", "--force"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("Error kind: workspace"), "{stderr}");
    assert_eq!(fs::read_to_string(dist.join("dan")).unwrap(), "file");
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(
        stderr.contains("Warning: dan/demo: dist not deployed (dist/dan is not a folder)"),
        "{stderr}"
    );
    assert!(stderr.contains("Error kind: not_deployed"), "{stderr}");
    assert!(
        stderr.contains("Hint: remove or rename `dist/dan` and run `gitpm sync-dist` again"),
        "{stderr}"
    );
    assert!(!stderr.contains("clone is gone"), "{stderr}");
    // Following the hint repairs deployment without dropping the package's record.
    fs::rename(dist.join("dan"), dist.join("dan-blocked")).unwrap();
    f.ok(&["sync-dist"]);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    assert_eq!(
        fs::read_to_string(dist.join("dan-blocked")).unwrap(),
        "file"
    );
}

#[test]
fn a_missing_clone_is_reported_and_blocks_only_its_own_dist() {
    let f = Fixture::new();
    f.original_and_fork();
    fs::remove_dir_all(clone_path(&f.root(), &f.path("vendor/demo"))).unwrap();
    fs::remove_dir_all(f.root().join("dist/vendor")).unwrap();
    f.ok(&["init", "other/pkg", "--into", "other/pkg"]);
    let out = f.run(&["install", f.path("other/pkg").to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(
        stderr.contains(
            "Warning: vendor/demo: dist not deployed (clone missing); uninstall or forget vendor/demo to drop its record"
        ),
        "{stderr}"
    );
    assert!(f.dist("other/pkg").is_some());
    // sync-dist removes stale dists anyway, then fails naming the package.
    f.ok(&["disable", "other/pkg"]);
    fs::create_dir_all(f.root().join("dist/other/pkg")).unwrap();
    let out = f.run(&["sync-dist"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(
        stderr.contains("Error: 1 package(s) not deployed: vendor/demo"),
        "{stderr}"
    );
    // A library error now: the kind and the command that helps.
    assert!(stderr.contains("Error kind: not_deployed"), "{stderr}");
    assert!(
        stderr.contains("Hint: `gitpm uninstall vendor/demo` or `gitpm forget vendor/demo` drops a package whose clone is gone"),
        "{stderr}"
    );
    // The way out works without confirmation: uninstall drops the record and dist.
    let out = f.ok(&["uninstall", "vendor/demo"]);
    assert_eq!(out, "vendor/demo uninstalled\n");
    let out = f.run(&["sync-dist"]);
    assert!(out.status.success());
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("Warning"),
        "later commands warn no more"
    );
    assert_eq!(f.dist("other/pkg"), None);
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
}

#[test]
fn every_deploy_copies_the_committed_tree_with_nested_files_and_exec_bits() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.original_and_fork();
    let repo = f.path("vendor/demo");
    fs::create_dir_all(repo.join("dist/bin")).unwrap();
    fs::create_dir_all(repo.join("dist/a/b")).unwrap();
    fs::write(repo.join("dist/bin/run.sh"), "#!/bin/sh\n").unwrap();
    fs::set_permissions(
        repo.join("dist/bin/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(repo.join("dist/a/b/c.txt"), "nested").unwrap();
    f.release("vendor/demo", "vendor 2", "0.3.0");
    let check = |when: &str| {
        let dist = f.root().join("dist/vendor/demo");
        assert_eq!(
            fs::read_to_string(dist.join("a/b/c.txt")).unwrap(),
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
    f.ok(&["update", "vendor/demo"]);
    check("update");
    // Uncommitted edits in the clone are never deployed.
    let clone = f.root().join("worktrees/vendor/demo");
    fs::write(clone.join("dist/index.js"), "edited").unwrap();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    check("--back");
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 2"));
    f.ok(&["disable", "vendor/demo"]);
    f.ok(&["enable", "vendor/demo"]);
    check("enable");
    fs::write(clone.join("dist/index.js"), "vendor 2").unwrap();
    f.ok(&["rollback", "vendor/demo"]);
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
    assert!(!f.root().join("dist/vendor/demo/bin").exists());
}

#[test]
fn replace_with_absent_original_is_refused() {
    let f = Fixture::new();
    f.original_and_fork();
    f.consumer();
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["uninstall", "vendor/demo"]);
    // --with needs the original installed.
    let error = f.fails(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert!(
        error.contains("Error: package vendor/demo is not installed")
            && error.contains("Error kind: not_installed"),
        "{error}"
    );
    // Returning requires an installed original, with no force bypass.
    let error = f.fails(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert!(error.contains("Error kind: not_installed"), "{error}");
    assert_eq!(f.state()["replacements"]["vendor/demo"], "dan/demo");
    let error = f.fails(&["replace", "vendor/demo", "--with", "vendor/demo", "--force"]);
    assert!(error.contains("--force is only valid"), "{error}");
    assert_eq!(f.state()["replacements"]["vendor/demo"], "dan/demo");
    assert_eq!(f.dist("dan/demo").as_deref(), Some("fork"));
    assert_eq!(f.dist("vendor/demo"), None);
}

#[test]
fn forget_says_whether_the_clone_was_kept() {
    let f = Fixture::new();
    f.original_and_fork();
    let out = f.ok(&["forget", "dan/demo"]);
    assert_eq!(
        out,
        "dan/demo forgotten (clone kept; restore re-registers it)\n"
    );
    fs::remove_dir_all(clone_path(&f.root(), &f.path("vendor/demo"))).unwrap();
    let out = f.ok(&["forget", "vendor/demo"]);
    assert_eq!(
        out,
        "vendor/demo forgotten (no clone; record and dist removed)\n"
    );
    assert_eq!(f.dist("vendor/demo"), None);
    assert!(f.state()["forgotten"].get("vendor/demo").is_none());
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
    f.ok(&["init", "lib/x", "--into", "lib/x"]);
    f.ok(&["install", f.path("lib/x").to_str().unwrap()]);
    let manifest = f.path("vendor/demo/plugin.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    json["dependencies"] = serde_json::json!({"lib/x": ">=0.1"});
    fs::write(&manifest, serde_json::to_string_pretty(&json).unwrap()).unwrap();
    f.release("vendor/demo", "vendor 2", "0.1.2");
    f.ok(&["update", "vendor/demo"]);
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["disable", "lib/x", "--force"]);
    fs::write(
        f.root().join("worktrees/dan/demo").join("notes.txt"),
        "work",
    )
    .unwrap();
    let util_linux = Command::new("script")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    let script = if util_linux {
        "script -qec \"'$0' --root '$1' --catalog '$2' uninstall dan/demo 2>'$3'\" /dev/null"
    } else {
        "script -q /dev/null sh -c \"exec '$0' --root '$1' --catalog '$2' uninstall dan/demo 2>'$3'\""
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
        stderr.contains("would reactivate vendor/demo: lib/x is disabled; enable it")
            && stderr.contains("Error kind: dependency_conflict"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("uninstalled"));
    assert!(
        f.root()
            .join("worktrees/dan/demo")
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
    let mut manifest = serde_json::json!({"id":"test/output", "version":"0.1.0",
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
            assert_eq!(f.dist("test/output").as_deref(), Some("committed output"));
            let deployed = f.root().join("dist/test/output");
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
            let clone = f.root().join("worktrees/test/output");
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
            f.ok(&["disable", "test/output"]);
            f.ok(&["enable", "test/output"]);
            assert_eq!(f.dist("test/output").as_deref(), Some("committed output"));
            f.ok(&["uninstall", "test/output", "--force"]);
            fs::write(f.path("sources.txt"), format!("{source}\n")).unwrap();
            f.ok(&[
                "catalog",
                "update",
                "--sources",
                f.path("sources.txt").to_str().unwrap(),
            ]);
            let release: serde_json::Value = serde_json::from_slice(
                &fs::read(f.path("catalog/namespaces/test/output/versions/0.1.0/release.json"))
                    .unwrap(),
            )
            .unwrap();
            assert!(release["files"].get("index.js").is_some());
            assert!(release["files"].get("outside").is_none());
            f.ok(&["install-catalog", "test/output"]);
            assert_eq!(f.dist("test/output").as_deref(), Some("committed output"));
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
        assert!(!f.root().join("dist/test/output").exists());
        assert!(!package.join("BUILD_MUST_NOT_RUN").exists());
    }
}

#[test]
fn declared_output_fork_preserves_build_metadata_without_running_it() {
    let f = Fixture::new();
    let repo = output_release(&f, Some("output/vtf"), false);
    f.ok(&["install", repo.to_str().unwrap()]);
    f.ok(&["fork", "test/output", "--ns", "mine", "--into", "fork"]);
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
        let error = f.fails(&["update", "test/output"]);
        assert!(
            error.contains("test/output: invalid manifest dist path"),
            "monorepo={mono}: {error}"
        );
        assert_eq!(f.dist("test/output").as_deref(), Some("committed output"));
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
        f.ok(&["update", "test/output"]);
        assert_eq!(f.dist("test/output").as_deref(), Some("new output"));
        f.ok(&["rollback", "test/output"]);
        assert_eq!(f.dist("test/output").as_deref(), Some("committed output"));
        assert!(!package.join("BUILD_MUST_NOT_RUN").exists());
    }
}

#[test]
fn replacement_check_resolve_cli_and_removed_options() {
    let f = Fixture::new();
    f.original_and_fork();
    f.consumer();
    let before = f.state();
    let out = f.ok(&["replace-check", "vendor/demo", "--with", "dan/demo"]);
    let check: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(check["plan"]["provider"], "dan/demo");
    assert_eq!(check["issues"], serde_json::json!([]));
    assert_eq!(check["missing"], serde_json::json!({}));
    assert_eq!(before, f.state());
    assert!(
        f.ok(&["replace-resolve", "vendor/demo", "--with", "dan/demo"])
            .is_empty()
    );
    assert_eq!(before, f.state());
    assert!(
        f.fails(&["replace", "vendor/demo", "--back"])
            .contains("unknown option: --back")
    );
    assert!(
        f.fails(&["replace", "vendor/demo", "--with", "dan/demo", "--force"])
            .contains("--force is only valid")
    );
    assert_eq!(before, f.state());
}

#[test]
fn replacement_missing_dependencies_suggest_explicit_resolution() {
    let f = Fixture::new();
    f.ok(&["init", "lib/new", "--into", "lib/new"]);
    f.release("lib/new", "dependency", "1.0.0");
    f.ok(&[
        "catalog",
        "register",
        f.path("lib/new").to_str().unwrap(),
        "--ref",
        "HEAD",
    ]);
    f.original_and_fork();
    let path = f.path("dan/demo/plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["dependencies"] = serde_json::json!({"lib/new": "^1"});
    fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();
    f.release("dan/demo", "fork", "0.2.1");
    f.ok(&["update", "dan/demo"]);
    f.ok(&["uninstall", "lib/new", "--force"]);
    let before = f.state();
    let error = f.fails(&["replace", "vendor/demo", "--with", "dan/demo"]);
    assert!(!error.contains("no catalog configured"), "{error}");
    assert!(error.contains("dan/demo needs lib/new ^1"), "{error}");
    assert!(error.contains("lib/new 1.0.0"), "{error}");
    assert!(
        error.contains("gitpm replace-check vendor/demo --with dan/demo"),
        "{error}"
    );
    assert!(
        error.contains("gitpm replace-resolve vendor/demo --with dan/demo"),
        "{error}"
    );
    assert_eq!(before, f.state());
    assert!(f.dist("lib/new").is_none());
    f.ok(&["replace-resolve", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["replace", "vendor/demo", "--with", "dan/demo"]);
    f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    // Resolution cannot change an incompatible installed version.
    f.ok(&["uninstall", "lib/new", "--force"]);
    f.release("lib/new", "incompatible dependency", "2.0.0");
    f.ok(&["install", f.path("lib/new").to_str().unwrap()]);
    let before = f.state();
    let error = f.fails(&["replace", "vendor/demo", "--with", "dan/demo"]);
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
    f.fails(&["replace-resolve", "vendor/demo", "--with", "dan/demo"]);
    assert_eq!(before, f.state());
}

#[test]
fn replacing_original_with_itself_without_binding_reports_no_change() {
    let f = Fixture::new();
    f.original_and_fork();
    let before = f.state();
    let out = f.ok(&["replace", "vendor/demo", "--with", "vendor/demo"]);
    assert_eq!(out.trim(), "vendor/demo already uses original 0.1.1");
    assert_eq!(before, f.state());
    assert_eq!(f.dist("vendor/demo").as_deref(), Some("vendor 1"));
}
