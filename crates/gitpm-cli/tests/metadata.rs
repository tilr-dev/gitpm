//! End-to-end tests of plugin metadata (`title`, `description`, `provides`, `tags`) and
//! of the authoring/CLI output added in 0.5 (`init --multi`, `init --no-release`,
//! install/update lines, empty folders after removal).
//! Hermetic: the CLI and Git run with a cleared environment, a fixture HOME/TMPDIR and
//! Git config; every path is inside a per-test temp directory removed on drop.
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

const THEME: &str =
    r#","title":"Theme","description":"Dark and light","provides":"theme","tags":["ui","colors"]"#;

struct Fixture {
    dir: PathBuf,
    mono: PathBuf,
}
impl Fixture {
    /// Repository `mono` with `tilr/store` (no metadata) and `tilr/theme` (all four
    /// fields, `provides` as a string) at 1.0.0.
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-cli-meta-{}-{}",
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
        let f = Self {
            mono: dir.join("mono"),
            dir,
        };
        fs::create_dir_all(&f.mono).unwrap();
        f.git(&f.mono, &["init", "-q", "-b", "main"]);
        fs::write(f.mono.join("gitpm.json"), r#"{"packages": ["packages/*"]}"#).unwrap();
        f.write("store", "1.0.0", "");
        f.write("theme", "1.0.0", THEME);
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
    fn git_output(&self, repo: &Path, args: &[&str]) -> String {
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
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
    fn git(&self, repo: &Path, args: &[&str]) {
        self.git_output(repo, args);
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
    fn release(&self, name: &str, version: &str, extra: &str) {
        self.write(name, version, extra);
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
    fn fails(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(!out.status.success(), "{args:?} must fail");
        String::from_utf8(out.stderr).unwrap()
    }
    fn source(&self) -> &str {
        self.mono.to_str().unwrap()
    }
    fn list(&self, args: &[&str]) -> serde_json::Value {
        let mut all = vec!["list"];
        all.extend_from_slice(args);
        serde_json::from_str(&self.ok(&all)).unwrap()
    }
    fn json(&self, path: &Path) -> serde_json::Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::env::var_os("GITPM_KEEP_TEST_ARTIFACTS").is_none() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

fn resolved<'a>(list: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    list["resolved"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in {list:#}"))
}
fn ids(list: &serde_json::Value) -> Vec<String> {
    list["resolved"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn list_reports_metadata_present_or_absent_and_filters_by_provides() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    let list = f.list(&[]);
    let theme = resolved(&list, "tilr/theme");
    assert_eq!(theme["title"], "Theme", "{list:#}");
    assert_eq!(theme["description"], "Dark and light");
    // A string `provides` is reported as an array.
    assert_eq!(theme["provides"], serde_json::json!(["theme"]));
    assert_eq!(theme["tags"], serde_json::json!(["ui", "colors"]));
    // Absent metadata: null title/description, empty arrays.
    let store = resolved(&list, "tilr/store");
    assert!(store["title"].is_null() && store["description"].is_null());
    assert_eq!(store["provides"], serde_json::json!([]));
    assert_eq!(store["tags"], serde_json::json!([]));
    // The shared clone is reported in resolved[] as in plugins{}.
    let clone = &list["plugins"]["tilr/theme"]["clone"];
    assert!(clone.is_string(), "{list:#}");
    assert_eq!(&theme["clone"], clone);
    assert_eq!(store["clone"], *clone);
    // State keeps the metadata of the installed revision.
    assert_eq!(
        list["plugins"]["tilr/theme"]["metadata"]["provides"],
        serde_json::json!(["theme"])
    );

    let only = f.list(&["--provides", "theme"]);
    assert_eq!(ids(&only), ["tilr/theme"]);
    assert_eq!(
        only["plugins"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["tilr/theme"]
    );
    let none = f.list(&["--provides", "store"]);
    assert!(none["forgotten"].is_null() && none["replacements"].is_null());
    assert!(ids(&none).is_empty() && none["plugins"].as_object().unwrap().is_empty());
    assert!(
        f.fails(&["check", "--provides", "theme"])
            .contains("--provides requires list")
    );

    // An update takes the metadata of the new revision.
    f.release("store", "1.1.0", r#","provides":["store","sync"]"#);
    f.ok(&["update", "tilr/store"]);
    assert_eq!(
        ids(&f.list(&["--provides", "sync"])),
        ["tilr/store"],
        "metadata follows the installed revision"
    );

    // A workspace written by gitpm 0.4 has no metadata in state: it is read from the
    // installed commit in the clone.
    let path = f.dir.join("workspace/state.json");
    let mut state = f.json(&path);
    for plugin in state["plugins"].as_object_mut().unwrap().values_mut() {
        plugin.as_object_mut().unwrap().remove("metadata");
    }
    fs::write(&path, serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    let list = f.list(&[]);
    assert!(list["plugins"]["tilr/theme"].get("metadata").is_none());
    assert_eq!(resolved(&list, "tilr/theme")["title"], "Theme");
    assert_eq!(
        resolved(&list, "tilr/store")["provides"],
        serde_json::json!(["store", "sync"])
    );
    // ... and is empty, without an error, when the clone is gone.
    fs::remove_dir_all(f.dir.join("workspace/repos")).unwrap();
    let list = f.list(&[]);
    assert!(resolved(&list, "tilr/theme")["title"].is_null());
    assert_eq!(
        resolved(&list, "tilr/theme")["provides"],
        serde_json::json!([])
    );
}

#[test]
fn wrong_typed_metadata_is_ignored_when_installing_and_refused_when_authoring() {
    let f = Fixture::new();
    // Published before 0.5 checked the types: install/update/rollback/fork accept it.
    let bad = r#","title":"Store","tags":"ui","provides":42"#;
    f.release("store", "1.1.0", bad);
    let out = f.run(&["install", f.source(), "tilr/store"]);
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains(
            "Warning: tilr/store 1.1.0: plugin.json tags must be an array of strings; ignored"
        ) && stderr.contains("provides must be a string or an array of strings"),
        "{stderr}"
    );
    let store = f.list(&[]);
    let store = resolved(&store, "tilr/store");
    assert_eq!(store["title"], "Store");
    assert_eq!(
        (&store["provides"], &store["tags"]),
        (&serde_json::json!([]), &serde_json::json!([]))
    );
    f.release("store", "1.2.0", r#","tags":"still bad""#);
    let out = f.run(&["update", "tilr/store"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("tilr/store 1.2.0: plugin.json tags"));
    f.ok(&["rollback", "tilr/store"]);
    assert_eq!(resolved(&f.list(&[]), "tilr/store")["title"], "Store");
    f.release("store", "1.3.0", r#","tags":7"#);
    f.release("theme", "1.1.0", r#","tags":"x""#);
    f.ok(&["install", f.source(), "tilr/theme"]);
    let out = f.ok(&["update", "all"]);
    assert!(out.contains("tilr/store updated 1.1.0 → 1.3.0"), "{out}");

    // fork copies the keys as they are; commit (authoring) refuses them.
    f.ok(&["fork", "tilr/store", "--ns", "me", "--into", "forks/store"]);
    let fork = f.dir.join("forks/store");
    assert_eq!(f.json(&fork.join("packages/store/plugin.json"))["tags"], 7);
    let error = f.fails(&["commit", fork.to_str().unwrap(), "2.0.0"]);
    assert!(
        error.contains("invalid plugin.json: tags must be an array of strings")
            && error.contains("Error kind: invalid_manifest"),
        "{error}"
    );
    assert_eq!(f.git_output(&fork, &["status", "--porcelain"]), "");
}

#[test]
fn fork_and_commit_carry_the_metadata_unchanged() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/theme"]);
    f.ok(&[
        "fork",
        "tilr/theme",
        "--ns",
        "me",
        "--into",
        "forks/me-theme",
    ]);
    let fork = f.dir.join("forks/me-theme");
    let manifest = fork.join("packages/theme/plugin.json");
    let m = f.json(&manifest);
    assert_eq!(m["id"], "me/theme");
    assert_eq!(m["forkOf"]["id"], "tilr/theme");
    assert_eq!(m["title"], "Theme");
    assert_eq!(m["description"], "Dark and light");
    // Written back as it was: still the string form.
    assert_eq!(m["provides"], "theme");
    assert_eq!(m["tags"], serde_json::json!(["ui", "colors"]));

    fs::write(fork.join("packages/theme/dist/index.js"), "my theme").unwrap();
    f.ok(&[
        "commit",
        fork.to_str().unwrap(),
        "1.1.0",
        "--package",
        "me/theme",
    ]);
    let m = f.json(&manifest);
    assert_eq!(m["version"], "1.1.0");
    assert_eq!(
        (&m["title"], &m["provides"], &m["tags"]),
        (
            &serde_json::json!("Theme"),
            &serde_json::json!("theme"),
            &serde_json::json!(["ui", "colors"])
        )
    );
    f.ok(&["install", fork.to_str().unwrap(), "me/theme"]);
    let list = f.list(&["--provides", "theme"]);
    assert_eq!(ids(&list), ["me/theme", "tilr/theme"]);
    assert_eq!(resolved(&list, "me/theme")["title"], "Theme");
}

#[test]
fn catalog_releases_carry_and_show_the_metadata() {
    let f = Fixture::new();
    fs::write(f.dir.join("sources.txt"), format!("{}\n", f.source())).unwrap();
    f.ok(&["catalog-update"]);
    let release = f.json(
        &f.dir
            .join("catalog/namespaces/tilr/theme/versions/1.0.0/release.json"),
    );
    assert_eq!(release["manifest"]["title"], "Theme");
    assert_eq!(release["manifest"]["provides"], "theme");
    let info = f.ok(&["info"]);
    assert!(
        info.contains("Title: Theme")
            && info.contains("Description: Dark and light")
            && info.contains("Provides: theme")
            && info.contains("Tags: ui, colors"),
        "{info}"
    );
    // Only the latest release counts; a package without metadata shows none.
    let store = info.split("tilr/store").nth(1).unwrap();
    let store = store.split("tilr/theme").next().unwrap();
    assert!(!store.contains("Title:"), "{info}");

    // An installed catalog release has the same metadata.
    f.ok(&["install-catalog", "tilr/theme"]);
    assert_eq!(
        resolved(&f.list(&[]), "tilr/theme")["provides"],
        serde_json::json!(["theme"])
    );

    // A broken latest release.json costs only the metadata in the overview.
    fs::write(
        f.dir
            .join("catalog/namespaces/tilr/theme/versions/1.0.0/release.json"),
        "{",
    )
    .unwrap();
    let info = f.ok(&["info"]);
    assert!(
        info.contains("tilr/theme — latest: 1.0.0") && !info.contains("Title:"),
        "{info}"
    );
}

#[test]
fn catalog_update_publishes_historical_releases_with_wrong_typed_metadata() {
    let f = Fixture::new();
    f.release("store", "1.1.0", r#","title":"Old","tags":"ui""#);
    f.release("store", "1.2.0", r#","title":"New""#);
    fs::write(f.dir.join("sources.txt"), format!("{}\n", f.source())).unwrap();
    let out = f.ok(&["catalog-update"]);
    assert!(out.contains("Catalog ready: 4 release(s)"), "{out}");
    let old = f.json(
        &f.dir
            .join("catalog/namespaces/tilr/store/versions/1.1.0/release.json"),
    );
    // Published as it is; readers ignore the wrong-typed key.
    assert_eq!(old["manifest"]["tags"], "ui");
    f.ok(&["install-catalog", "tilr/store", "--version", "1.1.0"]);
    let list = f.list(&[]);
    let store = resolved(&list, "tilr/store");
    assert_eq!(
        (&store["title"], &store["tags"]),
        (&"Old".into(), &serde_json::json!([]))
    );
    assert!(f.ok(&["info"]).contains("Title: New"));
}

#[test]
fn init_multi_creates_a_multi_package_repository_that_takes_more_packages() {
    let f = Fixture::new();
    let out = f.ok(&["init", "acme/one", "--into", "plugins", "--multi"]);
    assert!(out.contains("Git commit and tag acme/one@0.1.0"), "{out}");
    let repo = f.dir.join("plugins");
    assert_eq!(
        f.json(&repo.join("gitpm.json")),
        serde_json::json!({"packages": ["packages/*"]})
    );
    assert!(!repo.join("plugin.json").exists());
    let m = f.json(&repo.join("packages/one/plugin.json"));
    assert_eq!((&m["id"], &m["title"]), (&"acme/one".into(), &"one".into()));
    f.ok(&["init", "acme/two", "--into", "plugins"]);
    assert!(repo.join("packages/two/dist/index.js").is_file());
    assert_eq!(
        f.git_output(&repo, &["tag", "--list"]),
        "acme/one@0.1.0\nacme/two@0.1.0"
    );
    f.ok(&["install", repo.to_str().unwrap(), "acme/two"]);

    // An empty folder (not a repository yet) works with --multi only.
    fs::create_dir(f.dir.join("empty")).unwrap();
    let error = f.fails(&["init", "acme/three", "--into", "empty"]);
    assert!(error.contains("init --multi"), "{error}");
    f.ok(&["init", "acme/three", "--into", "empty", "--multi"]);
    assert!(f.dir.join("empty/packages/three/plugin.json").is_file());

    // A single-package root points to --multi.
    f.ok(&["init", "acme/single", "--into", "single"]);
    let m = f.json(&f.dir.join("single/plugin.json"));
    assert_eq!(m["title"], "single");
    let error = f.fails(&["init", "acme/other", "--into", "single"]);
    assert!(
        error.contains("move it into a folder") && error.contains("init --multi"),
        "{error}"
    );
    assert!(
        f.fails(&["install", f.source(), "--multi"])
            .contains("--multi/--no-release")
    );
}

#[test]
fn init_no_release_commits_without_a_tag() {
    let f = Fixture::new();
    let out = f.ok(&["init", "acme/draft", "--into", "draft", "--no-release"]);
    assert!(out.contains("no release tag yet"), "{out}");
    let repo = f.dir.join("draft");
    assert_eq!(f.git_output(&repo, &["tag", "--list"]), "");
    assert_eq!(f.git_output(&repo, &["status", "--porcelain"]), "");
    assert_eq!(f.git_output(&repo, &["rev-list", "--count", "HEAD"]), "1");
    let error = f.fails(&["install", repo.to_str().unwrap()]);
    assert!(error.contains("Error kind: no_releases"), "{error}");
    // The first version is released later, as the output says.
    let out = f.ok(&["commit", repo.to_str().unwrap(), "0.1.0"]);
    assert!(out.contains("tag acme/draft@0.1.0"), "{out}");
    assert_eq!(f.git_output(&repo, &["rev-list", "--count", "HEAD"]), "1");
    f.ok(&["install", repo.to_str().unwrap()]);

    f.ok(&[
        "init",
        "acme/a",
        "--into",
        "multi",
        "--multi",
        "--no-release",
    ]);
    assert_eq!(f.git_output(&f.dir.join("multi"), &["tag", "--list"]), "");
    f.ok(&[
        "commit",
        f.dir.join("multi").to_str().unwrap(),
        "0.1.0",
        "--package",
        "acme/a",
    ]);
    assert_eq!(
        f.git_output(&f.dir.join("multi"), &["tag", "--list"]),
        "acme/a@0.1.0"
    );
}

#[test]
fn install_and_update_say_what_they_did() {
    let f = Fixture::new();
    let out = f.ok(&["install", f.source(), "tilr/store"]);
    // The source as recorded (canonical path).
    assert!(
        out.starts_with("installed tilr/store@1.0.0 from /") && out.trim_end().ends_with("/mono"),
        "{out}"
    );
    f.ok(&["install", f.source(), "tilr/theme"]);
    let out = f.ok(&["update", "tilr/store"]);
    assert_eq!(out.trim(), "tilr/store up to date (1.0.0)");
    f.release("store", "1.1.0", "");
    let out = f.ok(&["update", "tilr/store"]);
    assert_eq!(out.trim(), "tilr/store updated 1.0.0 → 1.1.0");
    f.release("store", "1.2.0", "");
    let out = f.ok(&["update", "all"]);
    assert!(
        out.contains("tilr/store updated 1.1.0 → 1.2.0\n")
            && out.contains("tilr/theme up to date (1.0.0)\n")
            && out.contains("1 updates, 0 installs, 0 kept back."),
        "{out}"
    );
}

#[test]
fn removal_leaves_no_empty_namespace_or_clone_folders() {
    let f = Fixture::new();
    f.ok(&["install", f.source(), "tilr/store"]);
    f.ok(&["install", f.source(), "tilr/theme"]);
    let workspace = f.dir.join("workspace");
    f.ok(&["uninstall", "tilr/store"]);
    assert!(workspace.join("dist/tilr/theme").is_dir());
    f.ok(&["forget", "tilr/theme"]);
    f.ok(&["uninstall", "tilr/theme"]);
    assert!(!workspace.join("dist/tilr").exists());
    assert!(!workspace.join("repos").exists());
    assert!(workspace.join("dist").is_dir() && workspace.join("plugins").is_dir());

    // A root package's own clone plugins/<ns>/<name> as well.
    f.ok(&["init", "acme/solo", "--into", "solo"]);
    f.ok(&["install", f.dir.join("solo").to_str().unwrap()]);
    assert!(workspace.join("plugins/acme/solo").is_dir());
    f.ok(&["uninstall", "acme/solo"]);
    assert!(!workspace.join("plugins/acme").exists());
    assert!(!workspace.join("dist/acme").exists());
}

#[test]
fn update_all_reports_kept_back_packages_and_replace_back_without_a_replacement() {
    let f = Fixture::new();
    f.release("store", "1.0.1", r#","dependencies":{"tilr/theme":"~1.0"}"#);
    f.ok(&["install", f.source(), "tilr/theme"]);
    f.ok(&["install", f.source(), "tilr/store"]);
    f.release("theme", "1.1.0", "");
    let out = f.ok(&["update", "all"]);
    assert!(
        out.contains("tilr/theme kept back at 1.0.0 (latest 1.1.0: dependency constraints)")
            && out.contains("tilr/store up to date (1.0.1)")
            && out.contains("0 updates, 0 installs, 1 kept back."),
        "{out}"
    );
    // Nothing to switch back from: a clear refusal, nothing changes.
    let error = f.fails(&["replace", "tilr/theme", "--back"]);
    assert!(
        error.contains("tilr/theme is not replaced")
            && error.contains("Error kind: invalid_argument"),
        "{error}"
    );
}
