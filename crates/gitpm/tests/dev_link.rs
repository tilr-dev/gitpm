//! Live author folders through the public API; all sources are local fixtures.
mod common;
use gitpm::{Manager, Mode, Target, UnchangedReason, check};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    dir: PathBuf,
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-link-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        Self {
            root: dir.join("workspace"),
            dir,
        }
    }
    fn author(&self, name: &str) -> PathBuf {
        common::author(
            &self.dir.join(name),
            match name {
                "app" => "b879842f-16c2-558f-9d03-a271dc901f57",
                "beta" => "b4db3752-788f-590f-815a-60e10be71ae5",
                "greeter" => "d135a7cf-1c63-549a-93cb-8f89656b2ced",
                "hello" => "7e6e8392-7be8-55ae-bd03-2b430b1c5d59",
                "lib" => "cfbc4d17-8707-5571-853d-c9136a7f9f21",
                "none" => "918a06b8-4971-5358-8ae7-82d0218459ec",
                "other" => "cde71d60-4de0-5f8d-a3c9-26b40d4af02a",
                "one" => "c9ba156e-d929-5e1e-89c7-1d2c88b82d6e",
                "two" => "5f3efa49-5bd5-51cd-8645-c639938fc615",
                "three" => "b2ad1672-c6bc-57c4-a43a-056a37e29457",
                "single" => "ef39b8c6-2737-5a84-93fc-78ce726f370f",
                "draft" => "858a0fe7-31ef-58be-bd1c-1935fb0198d3",
                "a" => "39ffa43f-fef4-5346-a145-a9c92a049c9b",
                "solo" => "5aa0b257-9aa8-5927-9bfc-a255436a64b0",
                "root" => "365ade82-57cd-53a7-a61f-c790d46cba87",
                "work" => "721516e0-5494-5f92-9ef5-15054fe232a6",
                "bad" => "8a507792-598e-55ea-8db6-acdad3ee50f4",
                "releases" => "d987ea8b-800e-5e48-842c-21b8c3a59025",
                "dependency" => "4fe156cf-bb53-542b-ad94-caea3e691fbd",
                "healthy" => "e914c724-686f-5c52-a507-7cb1111fda8d",
                "consumer" => "bc03d01c-5dee-5431-ba23-809d7e584479",
                "base" => "c928de13-88cc-53d2-86f0-e669dcd5df6a",
                "fork_consumer" => "f25aca89-9525-56cc-8da1-0e761b66a2c4",
                "other_base" => "aad44be7-9a0e-5d84-ab1c-5e07db6842a0",
                "missing" => "b48ad341-d02e-5b25-9a8f-966f4faf12ab",
                "blocked" => "09a39ff1-85b9-5187-a570-b026ead5b81f",
                "widget" => "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
                "zlib" => "e4fcd067-4e1e-5766-9cda-34e49dc376a1",
                "sibling" => "33832cb1-5888-5a42-83e7-843d3b4b7224",
                "released" => "2454b358-69da-5cd4-aed8-7a2632f29b76",
                "pinned" => "96eae968-7cde-52da-bf95-bfc5c0e039d1",
                "branch" => "1eb889ae-7e4a-5823-b61d-24ca75c94bda",
                "newer" => "769ccb53-e0de-5311-9e8b-815d9ad1cbcd",
                "untagged" => "d98860c4-754e-51e3-b473-cff9f2429ea3",
                "listed" => "52c9669b-1ee0-5d3d-b549-45e7885e1ff6",
                "direct" => "2e6e67e1-7565-5c2f-811d-cad1a7f945c0",
                "gone" => "0d33ddaf-c197-5882-8059-fee90f2f22f9",
                "ext" => "527fc2ea-c1ee-5b23-a265-b3516c3ed655",
                "fd" => "eba8ed95-5887-5bb9-8294-c455a2fedc7c",
                "relative" => "e628c4ae-44fd-51b4-982c-c89a575e9f6c",
                "original" => "32485fb1-a32d-53d8-b3d0-42cc220c1c2b",
                "demo2" => "80b84611-1d3c-5cd6-be71-ad33787c0b53",
                "x" => "8384861b-4de7-53fb-a770-1fc6c8441244",
                "recovery" => "d23a35dc-fbc9-5f63-89f2-6a133fde3cd1",
                "ffi" => "4839e6dd-69db-53e5-bfb8-fccf30739e34",
                "demo1" => "645eb52e-b765-5211-a9b5-d9e7e23dbd85",
                "theme" => "0e4950ff-9cb6-54f3-bf22-f5a9f71129c1",
                "default" => "00000000-0000-4000-8000-000000000010",
                "custom" => "00000000-0000-4000-8000-000000000011",
                "independent" => "19d80777-777c-5594-80cb-00fa7577ef74",
                "unowned" => "00000000-0000-4000-8000-000000000012",
                "multi" => "00000000-0000-4000-8000-000000000013",
                "encoding" => "00000000-0000-4000-8000-000000000014",
                _ => panic!("unknown fixture alias"),
            },
            name,
        )
    }
    fn manager(&self) -> Manager {
        Manager::open(&self.root).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn manifest(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path.join("plugin.json")).unwrap()).unwrap()
}
fn write_manifest(path: &Path, value: &Value) {
    fs::write(path.join("plugin.json"), serde_json::to_vec(value).unwrap()).unwrap();
}
fn release(path: &Path, version: &str) {
    let mut m = manifest(path);
    m["version"] = version.into();
    write_manifest(path, &m);
    fs::write(path.join("dist/index.js"), version).unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-m", version]);
    git(path, &["tag", &format!("v{version}")]);
}
fn linked(reason: &Option<UnchangedReason>) -> &check::CheckRow {
    match reason {
        Some(UnchangedReason::DevLinked { availability, .. }) => availability,
        other => panic!("expected dev_linked: {other:?}"),
    }
}

#[test]
fn live_outputs_and_manifest_snapshots_have_no_managed_revision() {
    let f = Fixture::new();
    let m = f.manager();
    for (name, folder) in [("default", "dist"), ("root", "."), ("custom", "output/vtf")] {
        let package = f.author(name);
        let mut data = manifest(&package);
        if folder != "dist" {
            data["dist"] = folder.into();
        }
        data["title"] = "recorded title".into();
        write_manifest(&package, &data);
        fs::create_dir_all(package.join(folder)).unwrap();
        fs::write(package.join(folder).join("live.txt"), "before").unwrap();
        let result = m.link(&package).unwrap();
        assert!(!result.refreshed);
        let id = result.id.clone();
        let leaf = f.root.join("dist").join(&id);
        assert_eq!(
            fs::read_link(&leaf).unwrap(),
            fs::canonicalize(package.join(folder)).unwrap()
        );
        fs::write(
            package.join(folder).join("live.txt"),
            "after without a commit",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(leaf.join("live.txt")).unwrap(),
            "after without a commit"
        );
        let state = serde_json::to_value(m.state().unwrap()).unwrap();
        let record = &state["packages"][&id];
        for field in ["current", "previous", "clone", "path", "catalog", "forkOf"] {
            assert!(record.get(field).is_none(), "{field}");
        }
        assert_eq!(record["version"], data["version"]);
        assert!(record["dependencies"].is_object());
        assert_eq!(
            record["mode"]["link"]["packageRoot"],
            fs::canonicalize(&package).unwrap().to_str().unwrap()
        );
        data["title"] = "edited title".into();
        write_manifest(&package, &data);
        assert_eq!(
            m.list()
                .unwrap()
                .iter()
                .find(|p| p.id == id)
                .unwrap()
                .title
                .as_deref(),
            Some("recorded title")
        );
        assert!(m.link(&package).unwrap().refreshed);
        let listed = m.list().unwrap().into_iter().find(|p| p.id == id).unwrap();
        assert!(listed.current.is_none());
        assert!(listed.version.is_some());
        assert_eq!(listed.title.as_deref(), Some("edited title"));
        assert!(!f.root.join("packages").join(id).exists());
    }
}

#[test]
fn lifecycle_preserves_dirty_stashed_and_unpublished_external_work() {
    let f = Fixture::new();
    let package = f.author("work");
    let m = f.manager();
    let id = "721516e0-5494-5f92-9ef5-15054fe232a6";
    m.link(&package).unwrap();
    fs::write(package.join("stash.txt"), "stash").unwrap();
    git(&package, &["add", "."]);
    git(&package, &["stash", "push", "-m", "keep me"]);
    fs::write(package.join("commit.txt"), "unpublished").unwrap();
    git(&package, &["add", "."]);
    git(&package, &["commit", "-m", "unpublished"]);
    fs::write(package.join("staged.txt"), "staged").unwrap();
    git(&package, &["add", "staged.txt"]);
    fs::write(package.join("untracked.txt"), "untracked").unwrap();
    fs::write(package.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(package.join("ignored.txt"), "ignored").unwrap();
    let refs = git(&package, &["show-ref"]);
    let diff = git(&package, &["diff", "--cached"]);
    let state = fs::read(f.root.join("state.json")).unwrap();
    assert_eq!(m.rollback(id).unwrap_err().kind(), "no_previous_version");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    m.disable(id).unwrap();
    assert!(fs::symlink_metadata(f.root.join("dist").join(id)).is_err());
    m.enable(id).unwrap();
    assert!(!m.forget(id).unwrap().clone_kept);
    assert!(m.state().unwrap().forgotten[id].link().is_some());
    m.restore(id).unwrap();
    let plan = m.uninstall_plan(id).unwrap();
    assert!(plan.lost_work.is_empty());
    assert_eq!(
        plan.deletes,
        [f.root.join("dist").join(id).to_string_lossy()]
    );
    m.uninstall(id, &[], true).unwrap();
    assert_eq!(git(&package, &["show-ref"]), refs);
    assert_eq!(git(&package, &["diff", "--cached"]), diff);
    for name in ["staged.txt", "ignored.txt", "untracked.txt", "commit.txt"] {
        assert!(package.join(name).is_file());
    }
    assert!(!git(&package, &["stash", "list"]).is_empty());
}

#[test]
fn invalid_targets_are_refused_without_state_or_author_changes() {
    let f = Fixture::new();
    let m = f.manager();
    let package = f.author("bad");
    let original = manifest(&package);
    let state = fs::read(f.root.join("state.json")).unwrap();
    for (data, kind) in [
        (
            json!({"id":"invalid", "name": "fixture", "version":"1.0.0"}),
            "invalid_manifest",
        ),
        (json!({"version":"1.0.0"}), "invalid_manifest"),
        (
            json!({"id":"8a507792-598e-55ea-8db6-acdad3ee50f4", "name": "fixture", "version":"1.0.0", "dist":"missing"}),
            "workspace",
        ),
    ] {
        write_manifest(&package, &data);
        let before = fs::read(package.join("plugin.json")).unwrap();
        assert_eq!(m.link(&package).unwrap_err().kind(), kind);
        assert_eq!(fs::read(package.join("plugin.json")).unwrap(), before);
        assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    }
    fs::write(package.join("plugin.json"), "{").unwrap();
    assert_eq!(m.link(&package).unwrap_err().kind(), "invalid_data");
    write_manifest(&package, &original);
    let owned = f.root.join("packages/author");
    fs::create_dir_all(&owned).unwrap();
    fs::write(
        owned.join("plugin.json"),
        serde_json::to_vec(&original).unwrap(),
    )
    .unwrap();
    fs::create_dir(owned.join("dist")).unwrap();
    assert_eq!(m.link(&owned).unwrap_err().kind(), "workspace");
    m.install(package.to_str().unwrap()).unwrap();
    assert_eq!(m.link(&package).unwrap_err().kind(), "workspace");
}

#[test]
fn release_discovery_and_explicit_targets_leave_links_unchanged() {
    let f = Fixture::new();
    let origin = f.author("releases");
    let package = f.dir.join("checkout");
    git(
        &f.dir,
        &[
            "clone",
            "--quiet",
            origin.to_str().unwrap(),
            package.to_str().unwrap(),
        ],
    );
    let m = f.manager();
    m.link(&package).unwrap();
    release(&origin, "0.2.0");
    let head = git(&package, &["rev-parse", "HEAD"]);
    let state = fs::read(f.root.join("state.json")).unwrap();
    let row = &m.check().unwrap()[0];
    assert!(matches!(row.mode, Mode::Link(_)));
    assert!(row.update_available);
    assert_eq!(row.latest.as_deref(), Some("0.2.0"));
    for target in [
        Target::Recorded,
        Target::Dev,
        Target::Release,
        Target::Ref("anything".into()),
    ] {
        let result = m
            .update_to("d987ea8b-800e-5e48-842c-21b8c3a59025", target)
            .unwrap();
        assert!(!result.changed);
        assert_eq!(
            linked(&result.unchanged_reason).latest.as_deref(),
            Some("0.2.0")
        );
    }
    let plan = m.update_all_plan().unwrap();
    assert!(plan.updates.is_empty());
    assert!(linked(&plan.unchanged[0].reason).update_available);
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    assert!(linked(&m.update_all().unwrap().unchanged[0].reason).update_available);
    assert_eq!(git(&package, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    fs::rename(&origin, f.dir.join("gone")).unwrap();
    let result = m.update("d987ea8b-800e-5e48-842c-21b8c3a59025").unwrap();
    assert!(linked(&result.unchanged_reason).problem.is_some());
    assert!(!result.changed);
}

#[test]
fn joint_updates_keep_link_constraints_while_updating_healthy_packages() {
    let f = Fixture::new();
    let dependency = f.author("dependency");
    let healthy = f.author("healthy");
    let package = f.author("consumer");
    let m = f.manager();
    m.install(dependency.to_str().unwrap()).unwrap();
    m.install(healthy.to_str().unwrap()).unwrap();
    let mut data = manifest(&package);
    data["dependencies"] = json!({"4fe156cf-bb53-542b-ad94-caea3e691fbd":"^0.1.0"});
    write_manifest(&package, &data);
    m.link(&package).unwrap();
    release(&dependency, "0.2.0");
    release(&healthy, "0.2.0");
    let plan = m.update_all().unwrap();
    assert!(
        plan.updates
            .iter()
            .any(|u| u.id == "e914c724-686f-5c52-a507-7cb1111fda8d")
    );
    assert_eq!(
        m.state().unwrap().packages["4fe156cf-bb53-542b-ad94-caea3e691fbd"]
            .current
            .version,
        "0.1.0"
    );
    assert!(
        plan.unchanged
            .iter()
            .any(|u| u.id == "bc03d01c-5dee-5431-ba23-809d7e584479"
                && matches!(u.reason, Some(UnchangedReason::DevLinked { .. })))
    );
    data["dependencies"] = json!({"4fe156cf-bb53-542b-ad94-caea3e691fbd":">=0.2.0"});
    write_manifest(&package, &data);
    assert_eq!(m.link(&package).unwrap_err().kind(), "dependency_conflict");
    // A forced removal must not let the link reinstall its missing dependency.
    m.uninstall("4fe156cf-bb53-542b-ad94-caea3e691fbd", &[], true)
        .unwrap();
    let preview = m.update_all_plan().unwrap();
    assert!(preview.installs.is_empty());
    let unchanged = preview
        .unchanged
        .iter()
        .find(|u| u.id == "bc03d01c-5dee-5431-ba23-809d7e584479")
        .unwrap();
    let Some(UnchangedReason::DevLinked {
        availability,
        dependency_problems,
    }) = &unchanged.reason
    else {
        panic!("link disposition lost")
    };
    assert!(matches!(availability.mode, Mode::Link(_)));
    assert!(!dependency_problems.is_empty());
}

#[test]
fn explicit_fork_link_replace_and_relink_preserve_validated_provenance() {
    let f = Fixture::new();
    let origin = f.author("base");
    let m = f.manager();
    m.install(origin.to_str().unwrap()).unwrap();
    let before = fs::read(f.root.join("state.json")).unwrap();
    let tags = git(&origin, &["tag"]);
    let fork = m
        .fork_into("c928de13-88cc-53d2-86f0-e669dcd5df6a", &f.dir.join("fork"))
        .unwrap();
    let generated_0: String =
        serde_json::from_slice::<serde_json::Value>(&fs::read(fork.join("plugin.json")).unwrap())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    assert_eq!(git(&origin, &["tag"]), tags);
    m.link(&fork).unwrap();
    assert!(m.state().unwrap().replacements.is_empty());
    let record = serde_json::to_value(&m.state().unwrap().packages[generated_0.as_str()]).unwrap();
    assert!(record.get("forkOf").is_none());
    assert_eq!(
        record["mode"]["link"]["discovery"]["forkOf"]["id"],
        "c928de13-88cc-53d2-86f0-e669dcd5df6a"
    );
    assert_eq!(
        record["replaces"]["id"],
        "c928de13-88cc-53d2-86f0-e669dcd5df6a"
    );
    let consumer = f.author("fork_consumer");
    let mut consumer_manifest = manifest(&consumer);
    consumer_manifest["dependencies"] = json!({"c928de13-88cc-53d2-86f0-e669dcd5df6a":"^0.1.0"});
    write_manifest(&consumer, &consumer_manifest);
    m.link(&consumer).unwrap();
    m.disable(generated_0.as_str()).unwrap();
    assert!(
        m.replace("c928de13-88cc-53d2-86f0-e669dcd5df6a", generated_0.as_str())
            .is_err()
    );
    m.enable(generated_0.as_str()).unwrap();
    m.replace("c928de13-88cc-53d2-86f0-e669dcd5df6a", generated_0.as_str())
        .unwrap();
    let view = m.list().unwrap();
    let consumer_view = view
        .iter()
        .find(|p| p.id == "f25aca89-9525-56cc-8da1-0e761b66a2c4")
        .unwrap();
    assert!(consumer_view.dependencies[0].satisfied);
    assert_eq!(
        consumer_view.dependencies[0].provider.as_deref(),
        Some(generated_0.as_str())
    );
    fs::write(fork.join("dist/live.txt"), "live fork").unwrap();
    assert_eq!(
        fs::read_to_string(f.root.join(format!("dist/{generated_0}/live.txt").as_str())).unwrap(),
        "live fork"
    );
    assert!(
        !f.root
            .join("dist/c928de13-88cc-53d2-86f0-e669dcd5df6a")
            .exists()
    );
    let mut data = manifest(&fork);
    data.as_object_mut().unwrap().remove("forkOf");
    data.as_object_mut().unwrap().remove("replaces");
    write_manifest(&fork, &data);
    release(&origin, "0.2.0");
    let rows = m.check().unwrap();
    assert!(
        rows.iter()
            .find(|r| r.id == generated_0.as_str())
            .unwrap()
            .upstream
            .as_ref()
            .unwrap()
            .newer
    );
    assert_eq!(m.link(&fork).unwrap_err().kind(), "dependency_conflict");
    m.replace(
        "c928de13-88cc-53d2-86f0-e669dcd5df6a",
        "c928de13-88cc-53d2-86f0-e669dcd5df6a",
    )
    .unwrap();
    m.link(&fork).unwrap();
    assert!(
        m.check()
            .unwrap()
            .iter()
            .find(|r| r.id == generated_0.as_str())
            .unwrap()
            .upstream
            .is_none()
    );
    let other = f.author("other_base");
    let base_commit = git(&other, &["rev-parse", "HEAD"]);
    release(&other, "0.2.0");
    data["forkOf"] = json!({"id":"aad44be7-9a0e-5d84-ab1c-5e07db6842a0", "version":"0.1.0", "commit":base_commit, "source":other.to_str().unwrap()});
    write_manifest(&fork, &data);
    assert!(
        m.check()
            .unwrap()
            .iter()
            .find(|r| r.id == generated_0.as_str())
            .unwrap()
            .upstream
            .is_none()
    );
    m.link(&fork).unwrap();
    let rows = m.check().unwrap();
    let upstream = rows
        .iter()
        .find(|r| r.id == generated_0.as_str())
        .unwrap()
        .upstream
        .as_ref()
        .unwrap();
    assert_eq!(upstream.id, "aad44be7-9a0e-5d84-ab1c-5e07db6842a0");
    assert!(upstream.newer);
}

#[cfg(unix)]
#[test]
fn dangling_targets_and_symlinked_parents_never_own_external_files() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let package = f.author("missing");
    let other = f.author("other");
    let m = f.manager();
    m.link(&package).unwrap();
    fs::rename(package.join("dist"), package.join("moved")).unwrap();
    let report = m.link(&other).unwrap().dist;
    assert_eq!(report.skipped[0].id, "b48ad341-d02e-5b25-9a8f-966f4faf12ab");
    assert!(report.skipped[0].link.is_some());
    assert!(
        !m.uninstall_plan("b48ad341-d02e-5b25-9a8f-966f4faf12ab")
            .unwrap()
            .deletes
            .is_empty()
    );
    m.uninstall("b48ad341-d02e-5b25-9a8f-966f4faf12ab", &[], false)
        .unwrap();
    assert!(package.join("moved").is_dir());
    m.uninstall("cde71d60-4de0-5f8d-a3c9-26b40d4af02a", &[], false)
        .unwrap();
    fs::create_dir(f.dir.join("outside")).unwrap();
    fs::remove_dir(f.root.join("dist")).unwrap();
    symlink(f.dir.join("outside"), f.root.join("dist")).unwrap();
    assert_eq!(m.link(&other).unwrap_err().kind(), "workspace");
    assert!(
        fs::read_dir(f.dir.join("outside"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[test]
fn selected_multi_package_folder_installs_no_siblings_and_relink_can_change_output() {
    let f = Fixture::new();
    let repo = f.author("multi");
    fs::write(repo.join("gitpm.json"), r#"{"packages":["packages/*"]}"#).unwrap();
    for name in ["selected", "sibling"] {
        let path = repo.join("packages").join(name);
        fs::create_dir_all(path.join("dist")).unwrap();
        fs::write(
            path.join("plugin.json"),
            json!({"id": if name == "selected" {"00000000-0000-4000-8000-000000000002"} else {"00000000-0000-4000-8000-000000000003"}, "name":name,"version":"1.0.0"}).to_string(),
        )
        .unwrap();
        fs::write(path.join("dist/live.txt"), name).unwrap();
    }
    let m = f.manager();
    let path = repo.join("packages/selected");
    m.link(&path).unwrap();
    assert_eq!(m.state().unwrap().packages.len(), 1);
    assert_eq!(
        fs::read_to_string(
            f.root
                .join("dist/00000000-0000-4000-8000-000000000002/live.txt")
        )
        .unwrap(),
        "selected"
    );
    fs::create_dir(path.join("output")).unwrap();
    fs::write(path.join("output/live.txt"), "new output").unwrap();
    let mut data = manifest(&path);
    data["dist"] = "output".into();
    write_manifest(&path, &data);
    m.link(&path).unwrap();
    assert_eq!(
        fs::read_to_string(
            f.root
                .join("dist/00000000-0000-4000-8000-000000000002/live.txt")
        )
        .unwrap(),
        "new output"
    );
}

#[cfg(unix)]
#[test]
fn linking_refuses_unowned_dist_entries_including_dangling_symlinks() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let package = f.author("unowned");
    let m = f.manager();
    let leaf = f.root.join("dist/00000000-0000-4000-8000-000000000012");
    symlink(f.dir.join("missing-author-output"), &leaf).unwrap();
    let state = fs::read(f.root.join("state.json")).unwrap();
    assert_eq!(m.link(&package).unwrap_err().kind(), "workspace");
    assert_eq!(
        fs::read_link(&leaf).unwrap(),
        f.dir.join("missing-author-output")
    );
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
}

#[cfg(unix)]
#[test]
fn unknown_dist_entry_does_not_block_unrelated_packages() {
    use std::os::unix::fs::symlink;
    for symlink_parent in [false, true] {
        let f = Fixture::new();
        let package = f.author("blocked");
        let healthy = f.author("independent");
        let mut data = manifest(&healthy);
        data["id"] = "19d80777-777c-5594-80cb-00fa7577ef74".into();
        write_manifest(&healthy, &data);
        let m = f.manager();
        m.link(&package).unwrap();
        fs::remove_file(f.root.join("dist/09a39ff1-85b9-5187-a570-b026ead5b81f")).unwrap();
        let outside = f.dir.join("outside");
        fs::create_dir(&outside).unwrap();
        if symlink_parent {
            symlink(&outside, f.root.join("dist/acme")).unwrap();
        } else {
            fs::write(f.root.join("dist/acme"), "obstacle").unwrap();
        }
        let report = m.link(&healthy).unwrap().dist;
        assert!(report.skipped.is_empty());
        assert_eq!(report.unmanaged, ["dist/acme"]);
        assert!(
            f.root
                .join("dist/09a39ff1-85b9-5187-a570-b026ead5b81f")
                .is_dir()
        );
        assert!(
            m.state().unwrap().packages["09a39ff1-85b9-5187-a570-b026ead5b81f"]
                .link()
                .is_some()
        );
        assert!(
            f.root
                .join("dist/19d80777-777c-5594-80cb-00fa7577ef74")
                .is_dir()
        );
        assert!(fs::read_dir(&outside).unwrap().next().is_none());
        assert!(package.join("dist").is_dir());
    }
}

#[cfg(unix)]
fn foreign_link_leaf_does_not_block_an_unrelated_package(symlink_leaf: bool) {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let package = f.author("blocked");
    let healthy = f.author("independent");
    let mut data = manifest(&healthy);
    data["id"] = "19d80777-777c-5594-80cb-00fa7577ef74".into();
    write_manifest(&healthy, &data);
    let m = f.manager();
    m.link(&package).unwrap();
    let descriptor = m.state().unwrap().packages["09a39ff1-85b9-5187-a570-b026ead5b81f"]
        .link()
        .cloned();
    let leaf = f.root.join("dist/09a39ff1-85b9-5187-a570-b026ead5b81f");
    fs::remove_file(&leaf).unwrap();
    let outside = f.dir.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), "external content").unwrap();
    if symlink_leaf {
        symlink(&outside, &leaf).unwrap();
    } else {
        fs::create_dir(&leaf).unwrap();
        fs::write(leaf.join("sentinel"), "foreign content").unwrap();
    }
    let report = m.link(&healthy).unwrap().dist;
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(report.skipped[0].id, "09a39ff1-85b9-5187-a570-b026ead5b81f");
    assert_eq!(report.skipped[0].link, descriptor);
    assert!(
        !report
            .deployed
            .iter()
            .any(|id| id == "09a39ff1-85b9-5187-a570-b026ead5b81f")
    );
    assert_eq!(
        m.state().unwrap().packages["09a39ff1-85b9-5187-a570-b026ead5b81f"].link(),
        descriptor.as_ref()
    );
    assert!(
        f.root
            .join("dist/19d80777-777c-5594-80cb-00fa7577ef74")
            .is_dir()
    );
    assert_eq!(
        fs::read_to_string(outside.join("sentinel")).unwrap(),
        "external content"
    );
    if symlink_leaf {
        assert_eq!(fs::read_link(&leaf).unwrap(), outside);
    } else {
        assert!(
            !fs::symlink_metadata(&leaf)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(leaf.join("sentinel")).unwrap(),
            "foreign content"
        );
    }
    assert!(package.join("dist/index.js").is_file());
    assert_eq!(m.link(&package).unwrap_err().kind(), "workspace");
}

#[cfg(unix)]
#[test]
fn retargeted_link_leaf_does_not_block_an_unrelated_package() {
    foreign_link_leaf_does_not_block_an_unrelated_package(true);
}

#[cfg(unix)]
#[test]
fn foreign_folder_link_leaf_does_not_block_an_unrelated_package() {
    foreign_link_leaf_does_not_block_an_unrelated_package(false);
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_package_or_output_paths_are_refused_before_state_changes() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    for invalid_target_only in [false, true] {
        let f = Fixture::new();
        let mut package = f.author("encoding");
        let m = f.manager();
        let invalid = f.dir.join(OsString::from_vec(b"author-\xff".to_vec()));
        if invalid_target_only {
            fs::rename(package.join("dist"), &invalid).unwrap();
            symlink(&invalid, package.join("dist")).unwrap();
        } else {
            fs::rename(&package, &invalid).unwrap();
            package = invalid;
        }
        let state = fs::read(f.root.join("state.json")).unwrap();
        let manifest = fs::read(package.join("plugin.json")).unwrap();
        assert_eq!(m.link(&package).unwrap_err().kind(), "workspace");
        assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
        assert_eq!(fs::read(package.join("plugin.json")).unwrap(), manifest);
        assert!(
            !f.root
                .join("dist/00000000-0000-4000-8000-000000000014")
                .exists()
        );
        assert!(package.join("dist/index.js").is_file());
    }
}
