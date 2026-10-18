//! Mandatory product acceptance gates, deliberately separate from regression tests.
//! A green regression suite does not imply dependency replacement is implemented.
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
use gitpm::{Manager, scaffold};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("home")).unwrap();
        fs::create_dir_all(dir.join("tmp")).unwrap();
        fs::write(
            dir.join("home/.gitconfig"),
            "[user]\n\tname = Test\n\temail = test@example.invalid\n[init]\n\tdefaultBranch = main\n[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n",
        )
        .unwrap();
        Self(dir)
    }
}
impl std::ops::Deref for TempDir {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

/// A process with nothing from the developer's environment except PATH.
fn hermetic(dir: &Path, program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", dir.join("home"))
        .env("TMPDIR", dir.join("tmp"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("home/.gitconfig"));
    command
}
fn git(dir: &Path, repo: &Path, args: &[&str]) {
    let out = hermetic(dir, "git")
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

#[test]
fn required_dependent_plugin_installation() {
    let dir = TempDir::new("required-dependencies");
    let base = scaffold::init(&dir.join("base"), "vendor", "base")
        .unwrap()
        .repo;
    let consumer = scaffold::init(&dir.join("consumer"), "app", "consumer")
        .unwrap()
        .repo;
    let m = Manager::open(dir.join("workspace")).unwrap();
    m.install(base.to_str().unwrap()).unwrap();
    fs::write(
        consumer.join("plugin.json"),
        r#"{"id":"app/consumer","version":"0.1.1","dependencies":{"vendor/base":"0.1.0"}}"#,
    )
    .unwrap();
    for args in [
        &["add", "."][..],
        &["commit", "-m", "dependent consumer"][..],
        &["tag", "app/consumer@0.1.1"][..],
    ] {
        git(&dir, &consumer, args);
    }
    let result = m.install(consumer.to_str().unwrap());
    assert!(
        result.is_ok(),
        "REQUIRED CAPABILITY MISSING: install a consumer of an already installed compatible module: {result:?}. Fork replacement, dependent update and switch-back cannot be accepted until this prerequisite works. Artifacts: {}",
        dir.display()
    );
    assert!(m.state().unwrap().packages.contains_key("app/consumer"));
    // Further replacement behavior is mandatory in ATQ UC-053..056; this gate alone
    // must never be reported as proof that replacement works.
}

fn pm(dir: &Path, args: &[&str]) -> std::process::Output {
    hermetic(dir, env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(dir)
        .arg("--root")
        .arg(dir.join("workspace"))
        .arg("--catalog")
        .arg(dir.join("catalog"))
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap()
}
fn release(dir: &Path, repo: &Path, manifest: serde_json::Value) {
    fs::write(repo.join("plugin.json"), manifest.to_string()).unwrap();
    let id = manifest["id"].as_str().unwrap();
    let version = manifest["version"].as_str().unwrap();
    fs::write(repo.join("dist/index.js"), format!("{id} {version}")).unwrap();
    let tag = format!("{id}@{version}");
    for args in [
        &["add", "--all"][..],
        &["commit", "-q", "-m", version][..],
        &["tag", &tag][..],
    ] {
        git(dir, repo, args);
    }
}

#[test]
fn required_global_fork_replacement_through_the_cli() {
    let dir = TempDir::new("required-replace");
    let catalog = dir.join("catalog");
    let author = |id: &str, deps: serde_json::Value, extra: serde_json::Value| {
        let repo = dir.join("authors").join(id);
        fs::create_dir_all(repo.join("dist")).unwrap();
        git(&dir, &repo, &["init", "-q", "-b", "main"]);
        let mut manifest = serde_json::json!({"id": id, "version": "1.0.0", "dependencies": deps});
        for (k, v) in extra.as_object().unwrap() {
            manifest[k] = v.clone();
        }
        release(&dir, &repo, manifest);
        gitpm::catalog::register(
            catalog.to_str().unwrap(),
            repo.to_str().unwrap(),
            &format!("{id}@1.0.0"),
        )
        .unwrap();
        repo
    };
    author("vendor/base", serde_json::json!({}), serde_json::json!({}));
    author(
        "app/a",
        serde_json::json!({"vendor/base": "^1.0"}),
        serde_json::json!({}),
    );
    author(
        "app/b",
        serde_json::json!({"vendor/base": ">=0.9.0"}),
        serde_json::json!({}),
    );
    author(
        "mine/base",
        serde_json::json!({}),
        serde_json::json!({"replaces": {"id": "vendor/base", "version": "1.0.0"}}),
    );
    author(
        "old/base",
        serde_json::json!({}),
        serde_json::json!({"replaces": {"id": "vendor/base", "version": "0.9.0"}}),
    );
    for id in ["app/a", "app/b", "mine/base", "old/base"] {
        let out = pm(&dir, &["install-catalog", id]);
        assert!(
            out.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let list = |dir: &Path| -> serde_json::Value {
        serde_json::from_slice(&pm(dir, &["list"]).stdout).unwrap()
    };
    assert!(
        list(&dir)["packages"]["vendor/base"].is_object(),
        "pulled in as a dependency"
    );

    // Compatible fork: no question.
    let out = pm(&dir, &["replace", "vendor/base", "--with", "mine/base"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let state = list(&dir);
    assert_eq!(state["replacements"]["vendor/base"], "mine/base");
    for consumer in state["resolved"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["id"].as_str().unwrap().starts_with("app/"))
    {
        assert_eq!(consumer["dependencies"][0]["provider"], "mine/base");
    }
    // Incompatible fork without a terminal: refused, nothing changes; --force applies.
    let before = fs::read(dir.join("workspace/state.json")).unwrap();
    let out = pm(&dir, &["replace", "vendor/base", "--with", "old/base"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("app/a needs vendor/base ^1.0") && !stderr.contains("app/b needs"),
        "{stderr}"
    );
    assert_eq!(fs::read(dir.join("workspace/state.json")).unwrap(), before);
    let out = pm(
        &dir,
        &["replace", "vendor/base", "--with", "old/base", "--force"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("Warning"));
    assert_eq!(list(&dir)["replacements"]["vendor/base"], "old/base");
    // The active provider is guarded; switching back frees it.
    let out = pm(&dir, &["disable", "old/base"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("app/a, app/b"));
    let out = pm(&dir, &["replace", "vendor/base", "--back"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(list(&dir).get("replacements").is_none());
    let out = pm(&dir, &["uninstall", "old/base"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn direct_install_without_catalog_names_the_missing_dependency() {
    let dir = TempDir::new("no-catalog");
    let consumer = scaffold::init(&dir.join("consumer"), "app", "consumer")
        .unwrap()
        .repo;
    release(
        &dir,
        &consumer,
        serde_json::json!({"id": "app/consumer", "version": "0.2.0", "dependencies": {"lib/core": "^1.0"}}),
    );
    // No --catalog and no GITPM_CATALOG: the missing dependency cannot be fetched.
    let out = hermetic(&dir, env!("CARGO_BIN_EXE_gitpm"))
        .current_dir(&*dir)
        .arg("--root")
        .arg(dir.join("workspace"))
        .args(["install", consumer.to_str().unwrap()])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    for needle in [
        "lib/core",
        "no catalog configured",
        "Error kind: dependency_conflict",
    ] {
        assert!(stderr.contains(needle), "missing {needle:?}: {stderr}");
    }
    // A catalog location that does not exist is refused, never read as "no catalog".
    let out = pm(&dir, &["install", consumer.to_str().unwrap()]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("does not exist"), "{stderr}");
    assert!(stderr.contains("Error kind: invalid_argument"), "{stderr}");
}

/// `download-catalog` prints one line per package, `enable` refuses the disabled
/// dependency with a hint, `enable --force` enables only the named package.
#[test]
fn download_then_enable_through_the_cli() {
    let dir = TempDir::new("download-enable");
    let catalog = dir.join("catalog");
    for (id, deps) in [
        ("lib/core", serde_json::json!({})),
        ("app/a", serde_json::json!({"lib/core": "^1"})),
    ] {
        let repo = dir.join("authors").join(id);
        fs::create_dir_all(repo.join("dist")).unwrap();
        git(&dir, &repo, &["init", "-q", "-b", "main"]);
        release(
            &dir,
            &repo,
            serde_json::json!({"id": id, "version": "1.0.0", "dependencies": deps}),
        );
        gitpm::catalog::register(
            catalog.to_str().unwrap(),
            repo.to_str().unwrap(),
            &format!("{id}@1.0.0"),
        )
        .unwrap();
    }
    let out = pm(&dir, &["download-catalog", "app/a"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert!(
        lines[0].starts_with("downloaded app/a@1.0.0 from ") && lines[0].ends_with(" (disabled)"),
        "{stdout}"
    );
    assert!(
        lines[1].starts_with("downloaded lib/core@1.0.0 from ")
            && lines[1].ends_with(" (disabled)"),
        "{stdout}"
    );
    assert!(!dir.join("workspace/dist/app").exists());

    let out = pm(&dir, &["enable", "app/a"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("lib/core is disabled; enable it"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Error kind: dependency_conflict"),
        "{stderr}"
    );
    assert!(
        stderr.contains("Hint: `gitpm enable app/a --force` enables it anyway"),
        "{stderr}"
    );
    assert!(!dir.join("workspace/dist/app/a").exists());

    let out = pm(&dir, &["enable", "app/a", "--force"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(String::from_utf8_lossy(&out.stdout), "app/a enabled\n");
    assert!(dir.join("workspace/dist/app/a/index.js").is_file());
    assert!(!dir.join("workspace/dist/lib/core").exists());

    // install over the (disabled) download: refused like enable, --force enables it.
    assert!(pm(&dir, &["disable", "app/a"]).status.success());
    let out = pm(&dir, &["install-catalog", "app/a"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("lib/core is disabled; enable it"),
        "{stderr}"
    );
    let out = pm(&dir, &["install-catalog", "app/a", "--force"]);
    assert!(out.status.success(), "{out:?}");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.starts_with("installed app/a@1.0.0 from "),
        "{stdout}"
    );
    assert!(dir.join("workspace/dist/app/a/index.js").is_file());

    // install-catalog --force of a new package next to the disabled lib/core.
    let repo = dir.join("authors/app/b");
    fs::create_dir_all(repo.join("dist")).unwrap();
    git(&dir, &repo, &["init", "-q", "-b", "main"]);
    release(
        &dir,
        &repo,
        serde_json::json!({"id": "app/b", "version": "1.0.0", "dependencies": {"lib/core": "^1"}}),
    );
    gitpm::catalog::register(
        catalog.to_str().unwrap(),
        repo.to_str().unwrap(),
        "app/b@1.0.0",
    )
    .unwrap();
    assert!(!pm(&dir, &["install-catalog", "app/b"]).status.success());
    let out = pm(&dir, &["install-catalog", "app/b", "--force"]);
    assert!(out.status.success(), "{out:?}");
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("installed app/b@1.0.0 from "));
    assert!(dir.join("workspace/dist/app/b/index.js").is_file());
    assert!(!dir.join("workspace/dist/lib/core").exists());
}
