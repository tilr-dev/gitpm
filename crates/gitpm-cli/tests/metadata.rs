//! End-to-end tests of package metadata (`title`, `description`, `provides`, `tags`) and
//! of the authoring/CLI output added in 0.5 (`init --multi`, `init --no-release`,
//! install/update lines, empty folders after removal).
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

const THEME: &str =
    r#","title":"Theme","description":"Dark and light","provides":"theme","tags":["ui","colors"]"#;

struct Fixture {
    dir: PathBuf,
    mono: PathBuf,
}
impl Fixture {
    /// Repository `mono` with `76a9b591-60f3-5c7f-9e1c-31648fc73bfd` (no metadata) and `f851f609-bb81-52d6-bfef-33798624782c` (all four
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
        f.write("theme", "1.0.0", THEME);
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
    fn release(&self, name: &str, version: &str, extra: &str) {
        self.write(name, version, extra);
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
    let list = f.list(&[]);
    let theme = resolved(&list, "f851f609-bb81-52d6-bfef-33798624782c");
    assert_eq!(theme["title"], "Theme", "{list:#}");
    assert_eq!(theme["description"], "Dark and light");
    // A string `provides` is reported as an array.
    assert_eq!(theme["provides"], serde_json::json!(["theme"]));
    assert_eq!(theme["tags"], serde_json::json!(["ui", "colors"]));
    // Absent metadata: null title/description, empty arrays.
    let store = resolved(&list, "76a9b591-60f3-5c7f-9e1c-31648fc73bfd");
    assert!(store["title"].is_null() && store["description"].is_null());
    assert_eq!(store["provides"], serde_json::json!([]));
    assert_eq!(store["tags"], serde_json::json!([]));
    // The shared clone is reported in resolved[] as in packages{}.
    let clone = &list["packages"]["f851f609-bb81-52d6-bfef-33798624782c"]["clone"];
    assert!(clone.is_string(), "{list:#}");
    assert_eq!(&theme["clone"], clone);
    assert_eq!(store["clone"], *clone);
    // State keeps the metadata of the installed revision.
    assert_eq!(
        list["packages"]["f851f609-bb81-52d6-bfef-33798624782c"]["metadata"]["provides"],
        serde_json::json!(["theme"])
    );
    // Same shape in both places: absent strings are null, absent lists [].
    let stored = &list["packages"]["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]["metadata"];
    for key in ["title", "description", "provides", "tags"] {
        assert_eq!(stored[key], store[key], "{key}: {list:#}");
        assert!(stored.get(key).is_some(), "{key} missing: {list:#}");
    }
    assert!(stored["title"].is_null() && stored["description"].is_null());
    assert_eq!(stored["tags"], serde_json::json!([]));
    assert_eq!(stored["provides"], serde_json::json!([]));
    let stored = &list["packages"]["f851f609-bb81-52d6-bfef-33798624782c"]["metadata"];
    for key in ["title", "description", "provides", "tags"] {
        assert_eq!(stored[key], theme[key], "{key}: {list:#}");
    }

    let only = f.list(&["--provides", "theme"]);
    assert_eq!(ids(&only), ["f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(
        only["packages"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["f851f609-bb81-52d6-bfef-33798624782c"]
    );
    let none = f.list(&["--provides", "store"]);
    assert!(none["forgotten"].is_null() && none["replacements"].is_null());
    assert!(ids(&none).is_empty() && none["packages"].as_object().unwrap().is_empty());
    assert!(
        f.fails(&["check", "--provides", "theme"])
            .contains("--provides requires list")
    );

    // An update takes the metadata of the new revision.
    f.release("store", "1.1.0", r#","provides":["store","sync"]"#);
    f.ok(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        ids(&f.list(&["--provides", "sync"])),
        ["76a9b591-60f3-5c7f-9e1c-31648fc73bfd"],
        "metadata follows the installed revision"
    );

    // metadata is required in a format-1 record: a record without it is invalid data,
    // and state.json is not rewritten.
    let path = f.dir.join("workspace/state.json");
    let mut state = f.json(&path);
    for record in state["packages"].as_object_mut().unwrap().values_mut() {
        record.as_object_mut().unwrap().remove("metadata");
    }
    let bytes = serde_json::to_vec_pretty(&state).unwrap();
    fs::write(&path, &bytes).unwrap();
    let err = f.fails(&["list"]);
    assert!(err.contains("Error kind: invalid_data"), "{err}");
    assert!(err.contains("missing field `metadata`"), "{err}");
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn wrong_typed_metadata_is_ignored_when_installing_and_refused_when_authoring() {
    let f = Fixture::new();
    // Published before 0.5 checked the types: install/update/rollback/fork accept it.
    let bad = r#","title":"Store","tags":"ui","provides":42"#;
    f.release("store", "1.1.0", bad);
    let out = f.run(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    assert!(out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains(
            "Warning: 76a9b591-60f3-5c7f-9e1c-31648fc73bfd 1.1.0: plugin.json tags must be an array of strings; ignored"
        ) && stderr.contains("provides must be a string or an array of strings"),
        "{stderr}"
    );
    let store = f.list(&[]);
    let store = resolved(&store, "76a9b591-60f3-5c7f-9e1c-31648fc73bfd");
    assert_eq!(store["title"], "Store");
    assert_eq!(
        (&store["provides"], &store["tags"]),
        (&serde_json::json!([]), &serde_json::json!([]))
    );
    f.release("store", "1.2.0", r#","tags":"still bad""#);
    let out = f.run(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd 1.2.0: plugin.json tags")
    );
    f.ok(&["rollback", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        resolved(&f.list(&[]), "76a9b591-60f3-5c7f-9e1c-31648fc73bfd")["title"],
        "Store"
    );
    f.write("theme", "1.2.0", r#","tags":"x""#);
    f.release("store", "1.3.0", r#","tags":7"#);
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    let out = f.ok(&["update", "all"]);
    assert!(
        out.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd updated 1.1.0 → 1.3.0"),
        "{out}"
    );

    // fork copies the keys as they are; commit (authoring) refuses them.
    f.ok(&[
        "fork",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--into",
        "66747cfe-15d5-50e5-bd73-8281dacc1346",
    ]);
    let fork = f.dir.join("66747cfe-15d5-50e5-bd73-8281dacc1346");
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
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    f.ok(&[
        "fork",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--into",
        "2ee1ec40-461b-54c1-aa7e-00706359e184",
    ]);
    let fork = f.dir.join("2ee1ec40-461b-54c1-aa7e-00706359e184");
    let fork_theme = common::manifest_id(&fork.join("packages/theme"));

    let manifest = fork.join("packages/theme/plugin.json");
    let m = f.json(&manifest);
    assert_eq!(m["id"], fork_theme.as_str());
    assert_eq!(m["forkOf"]["id"], "f851f609-bb81-52d6-bfef-33798624782c");
    assert_eq!(m["title"], "Theme");
    assert_eq!(m["description"], "Dark and light");
    // Written back as it was: still the string form.
    assert_eq!(m["provides"], "theme");
    assert_eq!(m["tags"], serde_json::json!(["ui", "colors"]));

    fs::write(fork.join("packages/theme/dist/index.js"), "my theme").unwrap();
    f.ok(&["commit", fork.to_str().unwrap(), "1.1.0"]);
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
    f.ok(&["install", fork.to_str().unwrap(), fork_theme.as_str()]);
    let list = f.list(&["--provides", "theme"]);
    assert_eq!(
        ids(&list),
        [fork_theme.as_str(), "f851f609-bb81-52d6-bfef-33798624782c"]
    );
    assert_eq!(resolved(&list, fork_theme.as_str())["title"], "Theme");
}

#[test]
fn catalog_releases_carry_and_show_the_metadata() {
    let f = Fixture::new();
    fs::write(f.dir.join("sources.txt"), format!("{}\n", f.source())).unwrap();
    f.ok(&["catalog", "update"]);
    let release =
        f.json(&f.dir.join(
            "catalog/packages/f851f609-bb81-52d6-bfef-33798624782c/versions/1.0.0/release.json",
        ));
    assert_eq!(release["manifest"]["title"], "Theme");
    assert_eq!(release["manifest"]["provides"], "theme");
    let info = f.ok(&["catalog", "show"]);
    assert!(
        info.contains("Title: Theme")
            && info.contains("Description: Dark and light")
            && info.contains("Provides: theme")
            && info.contains("Tags: ui, colors"),
        "{info}"
    );
    // Only the latest release counts; a package without metadata shows none.
    let store = info
        .split("76a9b591-60f3-5c7f-9e1c-31648fc73bfd")
        .nth(1)
        .unwrap();
    let store = store
        .split("f851f609-bb81-52d6-bfef-33798624782c")
        .next()
        .unwrap();
    assert!(!store.contains("Title:"), "{info}");

    // An installed catalog release has the same metadata.
    f.ok(&["install-catalog", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert_eq!(
        resolved(&f.list(&[]), "f851f609-bb81-52d6-bfef-33798624782c")["provides"],
        serde_json::json!(["theme"])
    );

    // Display metadata comes from the index even if the release record is broken.
    fs::write(
        f.dir.join(
            "catalog/packages/f851f609-bb81-52d6-bfef-33798624782c/versions/1.0.0/release.json",
        ),
        "{",
    )
    .unwrap();
    let info = f.ok(&["catalog", "show"]);
    assert!(
        info.contains("f851f609-bb81-52d6-bfef-33798624782c — latest: 1.0.0")
            && info.contains("Title: Theme"),
        "{info}"
    );
    let index_path = f
        .dir
        .join("catalog/packages/f851f609-bb81-52d6-bfef-33798624782c/plugin.json");
    let mut index = f.json(&index_path);
    index["latest"]["provides"] = serde_json::json!(false);
    fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
    let info = f.ok(&["catalog", "show"]);
    assert!(
        info.contains("f851f609-bb81-52d6-bfef-33798624782c — latest: 1.0.0")
            && !info.contains("Title:"),
        "{info}"
    );
}

#[test]
fn catalog_update_publishes_historical_releases_with_wrong_typed_metadata() {
    let f = Fixture::new();
    f.release("store", "1.1.0", r#","title":"Old","tags":"ui""#);
    f.release("store", "1.2.0", r#","title":"New""#);
    fs::write(f.dir.join("sources.txt"), format!("{}\n", f.source())).unwrap();
    let out = f.ok(&["catalog", "update"]);
    assert!(out.contains("Catalog ready: 6 release(s)"), "{out}");
    let old =
        f.json(&f.dir.join(
            "catalog/packages/76a9b591-60f3-5c7f-9e1c-31648fc73bfd/versions/1.1.0/release.json",
        ));
    // Published as it is; readers ignore the wrong-typed key.
    assert_eq!(old["manifest"]["tags"], "ui");
    f.ok(&[
        "install-catalog",
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
        "--version",
        "1.1.0",
    ]);
    let list = f.list(&[]);
    let store = resolved(&list, "76a9b591-60f3-5c7f-9e1c-31648fc73bfd");
    assert_eq!(
        (&store["title"], &store["tags"]),
        (&"Old".into(), &serde_json::json!([]))
    );
    assert!(f.ok(&["catalog", "show"]).contains("Title: New"));
}

#[test]
fn init_multi_creates_a_multi_package_repository_that_takes_more_packages() {
    let f = Fixture::new();
    let out = f.ok(&["init", "one", "--into", "packages", "--multi"]);
    assert!(out.contains("Git commit and tag v0.1.0"), "{out}");
    let repo = f.dir.join("packages");
    assert_eq!(
        f.json(&repo.join("gitpm.json")),
        serde_json::json!({"packages": ["packages/*"]})
    );
    assert!(!repo.join("plugin.json").exists());
    let one = common::package_by_name(&repo, "one");
    let one_id = common::manifest_id(&one);
    let m = f.json(&one.join("plugin.json"));
    assert_eq!(
        (&m["id"], &m["title"]),
        (&one_id.clone().into(), &"one".into())
    );
    f.ok(&["init", "two", "--into", "packages"]);
    let two = common::package_by_name(&repo, "two");
    let two_id = common::manifest_id(&two);
    assert!(two.join("dist/index.js").is_file());
    assert_eq!(f.git_output(&repo, &["tag", "--list"]), "v0.1.0");
    f.ok(&["commit", repo.to_str().unwrap(), "0.1.1"]);
    f.ok(&["install", repo.to_str().unwrap(), two_id.as_str()]);

    // An empty folder (not a repository yet) works with --multi only.
    fs::create_dir(f.dir.join("empty")).unwrap();
    let error = f.fails(&["init", "three", "--into", "empty"]);
    // The library message states the problem; the CLI names the command as a hint.
    assert!(
        error.contains("an empty folder can become a multi-package repository"),
        "{error}"
    );
    assert!(
        error.contains("Hint: `gitpm init three --into DIR --multi`"),
        "{error}"
    );
    assert!(
        !error.contains(
            "Error: empty exists but is not a Git repository; pass a new folder or a repository (or"
        ),
        "{error}"
    );
    f.ok(&["init", "three", "--into", "empty", "--multi"]);
    assert!(
        common::package_by_name(&f.dir.join("empty"), "three")
            .join("plugin.json")
            .is_file()
    );

    // A single-package root points to --multi.
    f.ok(&["init", "single", "--into", "single"]);
    let m = f.json(&f.dir.join("single/plugin.json"));
    assert_eq!(m["title"], "single");
    let error = f.fails(&["init", "other", "--into", "single"]);
    assert!(
        error.contains("a multi-package repository needs gitpm.json")
            && error.contains("Hint: `gitpm init other --into DIR --multi`"),
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
    let out = f.ok(&["init", "draft", "--into", "draft", "--no-release"]);
    assert!(out.contains("no release tag yet"), "{out}");
    let repo = f.dir.join("draft");
    assert_eq!(f.git_output(&repo, &["tag", "--list"]), "");
    assert_eq!(f.git_output(&repo, &["status", "--porcelain"]), "");
    assert_eq!(f.git_output(&repo, &["rev-list", "--count", "HEAD"]), "1");
    let error = f.fails(&["install", repo.to_str().unwrap()]);
    assert!(error.contains("Error kind: no_releases"), "{error}");
    // The first version is released later, as the output says.
    let out = f.ok(&["commit", repo.to_str().unwrap(), "0.1.0"]);
    assert!(out.contains("tag v0.1.0"), "{out}");
    assert_eq!(f.git_output(&repo, &["rev-list", "--count", "HEAD"]), "1");
    f.ok(&["install", repo.to_str().unwrap()]);

    f.ok(&["init", "a", "--into", "multi", "--multi", "--no-release"]);
    assert_eq!(f.git_output(&f.dir.join("multi"), &["tag", "--list"]), "");
    f.ok(&["commit", f.dir.join("multi").to_str().unwrap(), "0.1.0"]);
    assert_eq!(
        f.git_output(&f.dir.join("multi"), &["tag", "--list"]),
        "v0.1.0"
    );
}

#[test]
fn install_and_update_say_what_they_did() {
    let f = Fixture::new();
    let out = f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    // The source as recorded (canonical path).
    assert!(
        out.starts_with("installed 76a9b591-60f3-5c7f-9e1c-31648fc73bfd@1.0.0 from /")
            && out.trim_end().ends_with("/mono"),
        "{out}"
    );
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    let out = f.ok(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        out.trim(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd up to date (1.0.0)"
    );
    f.release("store", "1.1.0", "");
    let out = f.ok(&["update", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert_eq!(
        out.trim(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd updated 1.0.0 → 1.1.0"
    );
    f.release("store", "1.2.0", "");
    let out = f.ok(&["update", "all"]);
    assert!(
        out.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd updated 1.1.0 → 1.2.0\n")
            && out.contains("f851f609-bb81-52d6-bfef-33798624782c updated 1.0.0 → 1.2.0\n")
            && out.contains("2 updates, 0 installs, 0 kept back."),
        "{out}"
    );
}

#[test]
fn removal_leaves_no_empty_namespace_or_clone_folders() {
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
    let workspace = f.dir.join("workspace");
    f.ok(&["uninstall", "76a9b591-60f3-5c7f-9e1c-31648fc73bfd"]);
    assert!(
        workspace
            .join("dist/f851f609-bb81-52d6-bfef-33798624782c")
            .is_dir()
    );
    f.ok(&["forget", "f851f609-bb81-52d6-bfef-33798624782c"]);
    f.ok(&["uninstall", "f851f609-bb81-52d6-bfef-33798624782c"]);
    assert!(!workspace.join("dist/tilr").exists());
    assert!(!workspace.join("repos").exists());
    assert!(workspace.join("dist").is_dir() && !workspace.join("packages").exists());

    // A root package uses the same source-keyed clone layout.
    f.ok(&["init", "solo", "--into", "solo"]);
    let solo_id = common::manifest_id(&f.dir.join("solo"));

    f.ok(&["install", f.dir.join("solo").to_str().unwrap()]);
    assert!(clone_path(&workspace, &f.dir.join("solo")).is_dir());
    f.ok(&["uninstall", solo_id.as_str()]);
    assert!(!workspace.join("repos").exists());
    assert!(!workspace.join("dist/acme").exists());
}

#[test]
fn update_all_reports_kept_back_packages_and_replace_with_original_without_a_replacement() {
    let f = Fixture::new();
    f.release(
        "store",
        "1.0.1",
        r#","dependencies":{"f851f609-bb81-52d6-bfef-33798624782c":"~1.0"}"#,
    );
    f.ok(&[
        "install",
        f.source(),
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    f.ok(&[
        "install",
        f.source(),
        "76a9b591-60f3-5c7f-9e1c-31648fc73bfd",
    ]);
    f.release("theme", "1.1.0", "");
    let out = f.ok(&["update", "all"]);
    assert!(
        out.contains("f851f609-bb81-52d6-bfef-33798624782c kept back at 1.0.1 (latest 1.1.0: dependency constraints)")
            && out.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd updated 1.0.1 → 1.1.0")
            && out.contains("1 updates, 0 installs, 1 kept back."),
        "{out}"
    );
    // Returning without a binding is idempotent and creates no self-binding.
    f.ok(&[
        "replace",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--with",
        "f851f609-bb81-52d6-bfef-33798624782c",
    ]);
    assert!(f.list(&[]).get("replacements").is_none());
    // A consumer whose dependency was removed with force is skipped with the reason from
    // the plan, not a failure, and the dependency is not installed again.
    f.ok(&[
        "uninstall",
        "f851f609-bb81-52d6-bfef-33798624782c",
        "--force",
    ]);
    f.release(
        "store",
        "1.2.0",
        r#","dependencies":{"f851f609-bb81-52d6-bfef-33798624782c":"~1.0"}"#,
    );
    let out = f.ok(&["update", "all"]);
    assert!(
        out.contains("76a9b591-60f3-5c7f-9e1c-31648fc73bfd skipped at 1.1.0 (dependency f851f609-bb81-52d6-bfef-33798624782c is not installed)")
            && out.contains("0 updates, 0 installs, 0 kept back."),
        "{out}"
    );
    let list = f.ok(&["list"]);
    assert!(
        list.contains("\"76a9b591-60f3-5c7f-9e1c-31648fc73bfd\": {")
            && !list.contains("\"f851f609-bb81-52d6-bfef-33798624782c\": {"),
        "{list}"
    );
}
