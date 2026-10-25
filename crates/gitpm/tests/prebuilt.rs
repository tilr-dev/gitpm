//! Real loopback downloads through the public library API without a source repository.
mod common;
use common::{clone_path, worktree_path};
use gitpm::{
    Manager, Manifest, Options,
    catalog::{self, Build, FileDigest, Release},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
const ID: &str = "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc";
fn sha(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn archive(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encoder);
    for (name, bytes, mode) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(*mode);
        header.set_cksum();
        tar.append_data(&mut header, name, *bytes).unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}
struct Server {
    url: String,
    files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let files = Arc::new(Mutex::new(BTreeMap::<String, Vec<u8>>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (served, stopping) = (files.clone(), stop.clone());
        let worker = thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") {
                    if stream.read(&mut byte).unwrap() == 0 {
                        break;
                    }
                    request.push(byte[0]);
                }
                let path = String::from_utf8_lossy(&request)
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("")
                    .to_owned();
                let body = served.lock().unwrap().get(&path).cloned();
                let (status, body) = body
                    .map(|b| ("200 OK", b))
                    .unwrap_or(("404 Not Found", Vec::new()));
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        Self {
            url: format!("http://{address}"),
            files,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        self.worker.take().unwrap().join().unwrap();
    }
}
struct Fixture {
    dir: PathBuf,
    catalog: PathBuf,
    root: PathBuf,
    server: Server,
    versions: BTreeMap<String, String>,
    verified: bool,
}
impl Fixture {
    fn new(verified: bool) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "gitpm-prebuilt-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self {
            catalog: dir.join("catalog"),
            root: dir.join("workspace"),
            dir,
            server: Server::new(),
            versions: BTreeMap::new(),
            verified,
        }
    }
    fn publish(&mut self, version: &str, contents: &[u8]) -> Release {
        let bytes = archive(&[("index.js", contents, 0o755)]);
        let url = format!("{}/{}.tar.gz", self.server.url, version);
        self.server
            .files
            .lock()
            .unwrap()
            .insert(format!("/{version}.tar.gz"), bytes.clone());
        let release = Release {
            manifest: Manifest {
                id: ID.into(),
                name: "widget".into(),
                version: version.into(),
                ..Default::default()
            },
            source: self.dir.join("unavailable-source").to_string_lossy().into(),
            commit: sha(version.as_bytes())[..40].into(),
            reference: format!("v{version}"),
            files: BTreeMap::from([(
                "index.js".into(),
                FileDigest {
                    sha256: sha(contents),
                    size: contents.len() as u64,
                },
            )]),
            build: Some(Build {
                url,
                sha256: sha(&bytes),
            }),
            path: None,
        };
        self.write_release(&release);
        release
    }
    fn write_release(&mut self, release: &Release) {
        let version = &release.manifest.version;
        self.versions
            .insert(version.clone(), format!("versions/{version}/release.json"));
        let folder = self
            .catalog
            .join("packages/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc");
        fs::create_dir_all(folder.join(format!("versions/{version}"))).unwrap();
        fs::write(
            self.catalog.join("catalog.json"),
            json!({"schema":1,"verified":self.verified,"directories":{"packages":"packages/"}})
                .to_string(),
        )
        .unwrap();
        fs::write(
            self.catalog.join("packages/catalog.json"),
            json!({"schema":1,"directories":{ID:format!("{ID}/plugin.json")}}).to_string(),
        )
        .unwrap();
        fs::write(
            folder.join("plugin.json"),
            json!({"schema":1,"id":ID,"source":release.source,"versions":self.versions})
                .to_string(),
        )
        .unwrap();
        fs::write(
            folder.join(format!("versions/{version}/release.json")),
            serde_json::to_vec(release).unwrap(),
        )
        .unwrap();
    }
    fn manager(&self) -> Manager {
        Manager::open_with(
            &self.root,
            Options {
                catalog: Some(self.catalog.to_string_lossy().into()),
                ..Default::default()
            },
        )
        .unwrap()
    }
    fn dist(&self) -> PathBuf {
        self.root
            .join("dist/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/index.js")
    }
    fn snapshot(&self) -> BTreeMap<String, FileDigest> {
        catalog::hash_files(&self.root).unwrap()
    }
    fn no_clone(&self) {
        assert!(!self.root.join("packages").exists());
        assert!(!self.root.join("repos").exists());
        assert!(!self.root.join("worktrees").exists());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn install_update_and_offline_rollback_need_no_repository() {
    for verified in [false, true] {
        let mut f = Fixture::new(verified);
        f.publish("1.0.0", b"one");
        f.publish("1.1.0", b"two");
        let m = f.manager();
        m.install_from_catalog(ID, Some("1.0.0")).unwrap();
        f.no_clone();
        assert_eq!(fs::read(f.dist()).unwrap(), b"one");
        assert_eq!(m.state().unwrap().packages[ID].catalog.is_some(), verified);
        assert!(
            m.state().unwrap().packages[ID]
                .build
                .as_ref()
                .unwrap()
                .location
                .is_some()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_ne!(
                fs::metadata(f.dist()).unwrap().permissions().mode() & 0o111,
                0
            );
        }
        drop(m);
        let m = Manager::open(&f.root).unwrap();
        assert!(m.check().unwrap()[0].update_available);
        assert!(m.update(ID).unwrap().changed);
        f.no_clone();
        assert_eq!(fs::read(f.dist()).unwrap(), b"two");
        fs::remove_dir_all(&f.catalog).unwrap();
        f.server.files.lock().unwrap().clear();
        m.rollback(ID).unwrap();
        assert_eq!(fs::read(f.dist()).unwrap(), b"one");
        m.rollback(ID).unwrap();
        assert_eq!(fs::read(f.dist()).unwrap(), b"two");
        m.set_enabled(ID, false).unwrap();
        assert!(!f.dist().exists());
        m.set_enabled(ID, true).unwrap();
        assert_eq!(fs::read(f.dist()).unwrap(), b"two");
        fs::remove_dir_all(f.dist().parent().unwrap()).unwrap();
        m.sync_dist().unwrap();
        assert_eq!(fs::read(f.dist()).unwrap(), b"two");
        f.no_clone();
    }
}
#[test]
fn update_all_and_removal_keep_only_two_cached_revisions() {
    let mut f = Fixture::new(false);
    f.publish("1.0.0", b"one");
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    f.publish("1.1.0", b"two");
    assert_eq!(m.update_all().unwrap().updates.len(), 1);
    f.publish("1.2.0", b"three");
    assert_eq!(m.update_all().unwrap().updates.len(), 1);
    assert_eq!(
        fs::read_dir(f.root.join("builds/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc"))
            .unwrap()
            .count(),
        2
    );
    assert!(m.forget(ID).unwrap().clone_kept);
    m.restore(ID).unwrap();
    assert_eq!(fs::read(f.dist()).unwrap(), b"three");
    assert!(m.uninstall_plan(ID).unwrap().lost_work.is_empty());
    m.uninstall(ID, &[], false).unwrap();
    assert!(
        !f.root.join("builds").exists()
            || catalog::hash_files(&f.root.join("builds"))
                .unwrap()
                .is_empty()
    );
    f.no_clone();
}
#[test]
fn download_verifies_and_retains_output_without_deploying() {
    let mut f = Fixture::new(false);
    f.publish("1.0.0", b"one");
    let m = f.manager();
    m.download_from_catalog(ID, None).unwrap();
    assert!(!f.dist().exists());
    f.no_clone();
    f.server.files.lock().unwrap().clear();
    m.install_from_catalog(ID, None).unwrap();
    assert_eq!(fs::read(f.dist()).unwrap(), b"one");
}
#[test]
fn invalid_or_unavailable_build_leaves_the_workspace_unchanged() {
    let mut f = Fixture::new(false);
    f.publish("1.0.0", b"one");
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    let mut bad = f.publish("1.1.0", b"two");
    let before = f.snapshot();
    bad.build.as_mut().unwrap().sha256 = "0".repeat(64);
    f.write_release(&bad);
    assert_eq!(m.update(ID).unwrap_err().kind(), "catalog");
    assert_eq!(f.snapshot(), before);
    let bytes = archive(&[("index.js", b"wrong", 0o644)]);
    bad.build.as_mut().unwrap().sha256 = sha(&bytes);
    f.server
        .files
        .lock()
        .unwrap()
        .insert("/1.1.0.tar.gz".into(), bytes);
    f.write_release(&bad);
    assert_eq!(m.update_all().unwrap_err().kind(), "catalog");
    assert_eq!(f.snapshot(), before);
    f.server.files.lock().unwrap().clear();
    assert_eq!(m.update(ID).unwrap_err().kind(), "http");
    assert_eq!(f.snapshot(), before);
    f.no_clone();
}
#[test]
fn malformed_archives_and_duplicate_paths_are_refused_atomically() {
    let mut f = Fixture::new(false);
    let mut release = f.publish("1.0.0", b"one");
    let m = f.manager();
    let before = f.snapshot();
    for bytes in [
        b"not gzip".to_vec(),
        archive(&[("index.js", b"one", 0o644), ("index.js", b"one", 0o644)]),
        archive(&[("extra.js", b"one", 0o644)]),
    ] {
        release.build.as_mut().unwrap().sha256 = sha(&bytes);
        f.write_release(&release);
        f.server
            .files
            .lock()
            .unwrap()
            .insert("/1.0.0.tar.gz".into(), bytes);
        assert_eq!(
            m.install_from_catalog(ID, None).unwrap_err().kind(),
            "catalog"
        );
        assert_eq!(f.snapshot(), before);
    }
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
#[test]
fn build_can_switch_to_git_and_rollback_in_both_directions() {
    for target in [
        gitpm::Target::Dev,
        gitpm::Target::Ref("v1.0.0".into()),
        gitpm::Target::Release,
    ] {
        let mut f = Fixture::new(true);
        let repo = common::author(
            &f.dir.join("source"),
            "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
            "widget",
        );
        gitpm::scaffold::commit(&repo, "1.0.0").unwrap();
        let bytes = archive(&[("index.js", b"built", 0o644)]);
        f.server
            .files
            .lock()
            .unwrap()
            .insert("/1.0.0.tar.gz".into(), bytes.clone());
        let mut release = catalog::register_build(
            f.catalog.to_str().unwrap(),
            repo.to_str().unwrap(),
            "v1.0.0",
            Some(Build {
                url: format!(
                    "{}/1.0.0.tar.gz",
                    f.server.url.replace("http://", "https://")
                ),
                sha256: sha(&bytes),
            }),
        )
        .unwrap();
        // The build differs from the committed output; authors publish matching file metadata.
        release.build.as_mut().unwrap().url = format!("{}/1.0.0.tar.gz", f.server.url);
        release.files = BTreeMap::from([(
            "index.js".into(),
            FileDigest {
                sha256: sha(b"built"),
                size: 5,
            },
        )]);
        f.write_release(&release);
        let m = f.manager();
        m.install_from_catalog(ID, None).unwrap();
        f.no_clone();
        m.fork_into(ID, &f.dir.join("fork")).unwrap();
        let detached = target == gitpm::Target::Release;
        m.update_with(ID, target, gitpm::UpdateOptions { detach: detached })
            .unwrap();
        assert!(m.state().unwrap().packages[ID].build.is_none());
        assert!(
            clone_path(
                &f.root,
                std::path::Path::new(&m.state().unwrap().packages[ID].source)
            )
            .join("objects")
            .exists()
        );
        let git_bytes = fs::read(f.dist()).unwrap();
        assert_ne!(git_bytes, b"built");
        m.rollback(ID).unwrap();
        assert_eq!(fs::read(f.dist()).unwrap(), b"built");
        m.rollback(ID).unwrap();
        assert_eq!(fs::read(f.dist()).unwrap(), git_bytes);
    }
}

#[test]
fn git_release_without_build_remains_a_git_install() {
    let mut f = Fixture::new(false);
    let repo = common::author(
        &f.dir.join("source"),
        "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
        "widget",
    );
    gitpm::scaffold::commit(&repo, "1.0.0").unwrap();
    let release = catalog::register(
        f.catalog.to_str().unwrap(),
        repo.to_str().unwrap(),
        "v1.0.0",
    )
    .unwrap();
    f.write_release(&release);
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    assert!(m.state().unwrap().packages[ID].build.is_none());
    assert!(
        clone_path(
            &f.root,
            std::path::Path::new(&m.state().unwrap().packages[ID].source)
        )
        .join("objects")
        .exists()
    );
}

#[test]
fn links_special_entries_and_traversal_cannot_escape_staging() {
    let mut f = Fixture::new(false);
    let mut release = f.publish("1.0.0", b"one");
    let m = f.manager();
    let before = f.snapshot();
    let absolute = f.dir.join("escape-absolute").to_str().unwrap().to_owned();
    let mut cases = vec![
        ("link".to_owned(), tar::EntryType::Symlink),
        ("hardlink".to_owned(), tar::EntryType::Link),
        ("device".to_owned(), tar::EntryType::Char),
    ];
    // Scratch output is three levels below the workspace. Include paths reaching
    // each ancestor through the fixture root, without escaping the fixture itself.
    for path in [3, 4, 1, 2]
        .into_iter()
        .map(|depth| format!("{}escape", "../".repeat(depth)))
        .chain([absolute])
    {
        cases.push((format!("{path}/"), tar::EntryType::Directory));
        cases.push((path, tar::EntryType::Regular));
    }
    for (path, kind) in cases {
        let mut header = tar::Header::new_gnu();
        header.set_size(0);
        header.set_mode(0o644);
        header.set_entry_type(kind);
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut tar = tar::Builder::new(encoder);
        // Write raw names: append_data intentionally refuses unsafe paths.
        // GNU long names keep absolute paths valid even with long temp prefixes.
        if path.len() >= 100 {
            let mut long_name = tar::Header::new_gnu();
            let bytes = format!("{path}\0");
            long_name.set_entry_type(tar::EntryType::GNULongName);
            long_name.set_size(bytes.len() as u64);
            long_name.set_cksum();
            tar.append(&long_name, bytes.as_bytes()).unwrap();
        } else {
            header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
        }
        header.set_cksum();
        tar.append(&header, &[][..]).unwrap();
        let bytes = tar.into_inner().unwrap().finish().unwrap();
        release.build.as_mut().unwrap().sha256 = sha(&bytes);
        f.write_release(&release);
        f.server
            .files
            .lock()
            .unwrap()
            .insert("/1.0.0.tar.gz".into(), bytes);
        let result = m.install_from_catalog(ID, None);
        for base in [&f.root, &f.dir] {
            for name in ["escape", "escape-absolute"] {
                assert!(!base.join(name).exists(), "{path}: wrote outside staging");
            }
        }
        let error = result.unwrap_err();
        if kind.is_file() || kind.is_dir() {
            assert!(
                error.to_string().contains("unsafe build archive path"),
                "{path}: {error}"
            );
        } else {
            assert_eq!(error.kind(), "unsupported", "{path}: {error}");
        }
        assert_eq!(f.snapshot(), before, "{path}");
    }
}

fn publish_repo(f: &Fixture, repo: &std::path::Path, id: &str, version: &str) -> Release {
    gitpm::scaffold::commit(repo, version).unwrap();
    let contents = fs::read(repo.join("dist/index.js")).unwrap();
    let bytes = archive(&[("index.js", &contents, 0o644)]);
    let name = format!("/{}-{version}.tar.gz", id.replace('/', "-"));
    f.server
        .files
        .lock()
        .unwrap()
        .insert(name.clone(), bytes.clone());
    let mut release = catalog::register_build(
        f.catalog.to_str().unwrap(),
        repo.to_str().unwrap(),
        &format!("v{version}"),
        Some(Build {
            url: format!("{}{name}", f.server.url.replace("http://", "https://")),
            sha256: sha(&bytes),
        }),
    )
    .unwrap();
    release.build.as_mut().unwrap().url = format!("{}{name}", f.server.url);
    let path = f
        .catalog
        .join(format!("packages/{id}/versions/{version}/release.json"));
    fs::write(path, serde_json::to_vec(&release).unwrap()).unwrap();
    release
}
#[test]
fn build_dependencies_install_and_update_together_without_clones() {
    for all in [false, true] {
        let f = Fixture::new(false);
        let app = common::author(
            &f.dir.join("source"),
            "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
            "widget",
        );
        let lib = common::author(
            &f.dir.join("lib"),
            "cfbc4d17-8707-5571-853d-c9136a7f9f21",
            "lib",
        );
        publish_repo(&f, &app, ID, "1.0.0");
        let m = f.manager();
        m.install_from_catalog(ID, None).unwrap();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(app.join("plugin.json")).unwrap()).unwrap();
        manifest["dependencies"] = json!({"cfbc4d17-8707-5571-853d-c9136a7f9f21":"^1.0.0"});
        fs::write(
            app.join("plugin.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        publish_repo(&f, &lib, "cfbc4d17-8707-5571-853d-c9136a7f9f21", "1.0.0");
        publish_repo(&f, &app, ID, "1.1.0");
        fs::remove_dir_all(&app).unwrap();
        fs::remove_dir_all(&lib).unwrap();
        if all {
            assert_eq!(m.update_all().unwrap().installs.len(), 1);
        } else {
            m.update(ID).unwrap();
        }
        let state = m.state().unwrap();
        assert_eq!(state.packages[ID].current.version, "1.1.0");
        assert!(
            state.packages["cfbc4d17-8707-5571-853d-c9136a7f9f21"]
                .build
                .is_some()
        );
        assert!(state.packages["cfbc4d17-8707-5571-853d-c9136a7f9f21"].enabled);
        assert!(!f.root.join("packages").exists());
        f.no_clone();
    }
}
#[test]
fn catalog_updates_can_alternate_git_and_build_revisions() {
    let f = Fixture::new(false);
    let app = common::author(
        &f.dir.join("source"),
        "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
        "widget",
    );
    publish_repo(&f, &app, ID, "1.0.0");
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    gitpm::scaffold::commit(&app, "1.1.0").unwrap();
    catalog::register(f.catalog.to_str().unwrap(), app.to_str().unwrap(), "v1.1.0").unwrap();
    m.update(ID).unwrap();
    assert!(m.state().unwrap().packages[ID].build.is_none());
    // Protect the Git install before publishing another build, so updates follow this catalog.
    let root_file = f.catalog.join("catalog.json");
    let mut root: serde_json::Value =
        serde_json::from_slice(&fs::read(&root_file).unwrap()).unwrap();
    root["verified"] = json!(true);
    fs::write(root_file, root.to_string()).unwrap();
    m.install_from_catalog(ID, None).unwrap();
    publish_repo(&f, &app, ID, "1.2.0");
    m.update(ID).unwrap();
    assert!(m.state().unwrap().packages[ID].build.is_some());
    assert!(m.uninstall_plan(ID).unwrap().lost_work.is_empty());
    m.uninstall(ID, &[], false).unwrap();
    m.install_from_catalog(ID, Some("1.1.0")).unwrap();
    m.update(ID).unwrap();
    m.rollback(ID).unwrap();
    assert_eq!(m.state().unwrap().packages[ID].current.version, "1.1.0");
    assert!(m.state().unwrap().packages[ID].build.is_none());
    m.rollback(ID).unwrap();
    assert_eq!(m.state().unwrap().packages[ID].current.version, "1.2.0");
    assert!(!worktree_path(&f.root, ID).exists());
    m.rollback(ID).unwrap();
    assert_eq!(m.state().unwrap().packages[ID].current.version, "1.1.0");
    // Existing local Git work must survive an explicit transition request.
    let clone = worktree_path(&f.root, ID);
    fs::write(clone.join("work.txt"), "keep").unwrap();
    let before = fs::read(f.root.join("state.json")).unwrap();
    let error = m
        .update_with(
            ID,
            gitpm::Target::Dev,
            gitpm::UpdateOptions { detach: true },
        )
        .unwrap_err();
    assert_eq!(error.kind(), "local_changes", "{error:?}");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    assert_eq!(git(&clone, &["status", "--porcelain"]), "?? work.txt");
    gitpm::scaffold::commit(&app, "1.3.0").unwrap();
    catalog::register(f.catalog.to_str().unwrap(), app.to_str().unwrap(), "v1.3.0").unwrap();
    assert_eq!(m.update(ID).unwrap_err().kind(), "local_changes");
    assert_eq!(fs::read(clone.join("work.txt")).unwrap(), b"keep");
    assert!(!m.uninstall_plan(ID).unwrap().lost_work.is_empty());
    assert_eq!(
        m.uninstall(ID, &[], false).unwrap_err().kind(),
        "local_changes"
    );
    assert_eq!(fs::read(clone.join("work.txt")).unwrap(), b"keep");
}

#[test]
fn joint_build_upgrade_uses_the_joint_dependency_solution() {
    let f = Fixture::new(false);
    let app = common::author(
        &f.dir.join("source"),
        "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
        "widget",
    );
    let lib = common::author(
        &f.dir.join("lib"),
        "e4fcd067-4e1e-5766-9cda-34e49dc376a1",
        "zlib",
    );
    for version in ["1.0.0", "2.0.0"] {
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(app.join("plugin.json")).unwrap()).unwrap();
        manifest["dependencies"] =
            json!({"e4fcd067-4e1e-5766-9cda-34e49dc376a1": format!("^{version}")});
        fs::write(
            app.join("plugin.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        publish_repo(&f, &lib, "e4fcd067-4e1e-5766-9cda-34e49dc376a1", version);
        publish_repo(&f, &app, ID, version);
        if version == "1.0.0" {
            let m = f.manager();
            m.install_from_catalog(ID, None).unwrap();
        }
    }
    let m = f.manager();
    let plan = m.update_all().unwrap();
    assert_eq!(plan.updates.len(), 2);
    for p in m.state().unwrap().packages.values() {
        assert_eq!(p.current.version, "2.0.0");
    }
    f.no_clone();
}

#[test]
fn build_update_all_keeps_an_active_replacement_declaration() {
    let f = Fixture::new(false);
    let original = common::author(
        &f.dir.join("original"),
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "base",
    );
    let fork = common::author(
        &f.dir.join("fork"),
        "db33e43e-e652-5767-8e21-27fb3cef9723",
        "base",
    );
    publish_repo(
        &f,
        &original,
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "1.0.0",
    );
    let path = fork.join("plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["replaces"] = json!({"id":"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version":"1.0.0"});
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    publish_repo(&f, &fork, "db33e43e-e652-5767-8e21-27fb3cef9723", "1.0.0");
    let m = f.manager();
    m.install_from_catalog("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", None)
        .unwrap();
    m.install_from_catalog("db33e43e-e652-5767-8e21-27fb3cef9723", None)
        .unwrap();
    m.replace(
        "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9",
        "db33e43e-e652-5767-8e21-27fb3cef9723",
    )
    .unwrap();
    manifest.as_object_mut().unwrap().remove("replaces");
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    publish_repo(&f, &fork, "db33e43e-e652-5767-8e21-27fb3cef9723", "2.0.0");
    fs::remove_dir_all(&fork).unwrap();
    fs::remove_dir_all(&original).unwrap();
    let plan = m.update_all().unwrap();
    assert!(plan.updates.is_empty());
    let held = plan
        .kept_back
        .iter()
        .find(|h| h.id == "db33e43e-e652-5767-8e21-27fb3cef9723")
        .unwrap();
    assert_eq!(held.selected, "1.0.0");
    assert_eq!(held.latest, "2.0.0");
    assert_eq!(
        held.reason,
        gitpm::HeldBackReason::NoLongerReplaces {
            original: "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9".into()
        }
    );
    let state = m.state().unwrap();
    assert!(
        state.packages["db33e43e-e652-5767-8e21-27fb3cef9723"]
            .replaces
            .is_some()
    );
    assert_eq!(
        state.replacements["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"],
        "db33e43e-e652-5767-8e21-27fb3cef9723"
    );
}

fn inactive_build_dependency(operation: &str) {
    let f = Fixture::new(false);
    let app = common::author(
        &f.dir.join("source"),
        "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
        "widget",
    );
    let lib = common::author(
        &f.dir.join("lib"),
        "cfbc4d17-8707-5571-853d-c9136a7f9f21",
        "lib",
    );
    let path = app.join("plugin.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["dependencies"] = json!({"cfbc4d17-8707-5571-853d-c9136a7f9f21":"^1"});
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    publish_repo(&f, &lib, "cfbc4d17-8707-5571-853d-c9136a7f9f21", "1.0.0");
    publish_repo(&f, &app, ID, "1.0.0");
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    if operation == "rollback" {
        manifest.as_object_mut().unwrap().remove("dependencies");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        publish_repo(&f, &app, ID, "2.0.0");
        m.update(ID).unwrap();
    } else {
        m.forget(ID).unwrap();
    }
    if operation == "restore-missing" {
        m.uninstall("cfbc4d17-8707-5571-853d-c9136a7f9f21", &[], false)
            .unwrap();
    } else {
        m.disable("cfbc4d17-8707-5571-853d-c9136a7f9f21").unwrap();
    }
    let before = fs::read(f.root.join("state.json")).unwrap();
    let deployed = fs::read(
        f.root
            .join("dist/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/index.js"),
    )
    .ok();
    let error = if operation == "rollback" {
        m.rollback(ID).unwrap_err()
    } else {
        m.restore(ID).unwrap_err()
    };
    assert_eq!(error.kind(), "dependency_conflict", "{operation}");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    assert_eq!(
        fs::read(
            f.root
                .join("dist/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/index.js")
        )
        .ok(),
        deployed
    );
    if operation == "restore-missing" {
        m.install_from_catalog("cfbc4d17-8707-5571-853d-c9136a7f9f21", None)
            .unwrap();
    } else {
        m.enable("cfbc4d17-8707-5571-853d-c9136a7f9f21").unwrap();
    }
    if operation == "rollback" {
        m.rollback(ID).unwrap();
    } else {
        m.restore(ID).unwrap();
    }
    assert_eq!(m.state().unwrap().packages[ID].current.version, "1.0.0");
    assert!(
        f.root
            .join("dist/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/index.js")
            .is_file()
    );
}

#[test]
fn build_restore_requires_active_dependencies() {
    inactive_build_dependency("restore");
    inactive_build_dependency("restore-missing");
}
#[test]
fn build_rollback_requires_active_dependencies() {
    inactive_build_dependency("rollback");
}

#[test]
fn clone_free_build_transition_preserves_a_siblings_local_work() {
    check_sibling_local_work(false);
}

#[test]
fn clone_free_build_transition_preserves_a_forgotten_siblings_local_work() {
    check_sibling_local_work(true);
}

fn check_sibling_local_work(forgotten: bool) {
    let f = Fixture::new(false);
    let repo = common::author(
        &f.dir.join("source"),
        "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
        "widget",
    );
    publish_repo(&f, &repo, ID, "1.0.0");
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    assert!(m.state().unwrap().packages[ID].clone.is_none());
    fs::create_dir_all(repo.join("apps/widget")).unwrap();
    fs::rename(
        repo.join("plugin.json"),
        repo.join("apps/widget/plugin.json"),
    )
    .unwrap();
    fs::rename(repo.join("dist"), repo.join("apps/widget/dist")).unwrap();
    fs::create_dir_all(repo.join("apps/lib/dist")).unwrap();
    fs::write(
        repo.join("apps/lib/plugin.json"),
        r#"{"id":"cfbc4d17-8707-5571-853d-c9136a7f9f21","name": "fixture", "version":"1.0.0"}"#,
    )
    .unwrap();
    fs::write(repo.join("apps/lib/dist/index.js"), "sibling").unwrap();
    fs::write(repo.join("gitpm.json"), r#"{"packages":["apps/*"]}"#).unwrap();
    gitpm::scaffold::commit_package(&repo, "2.0.0", Some(ID)).unwrap();
    m.install_package(
        repo.to_str().unwrap(),
        Some("cfbc4d17-8707-5571-853d-c9136a7f9f21"),
        gitpm::InstallTarget::Release,
    )
    .unwrap();
    if forgotten {
        m.forget("cfbc4d17-8707-5571-853d-c9136a7f9f21").unwrap();
    }
    let clone = worktree_path(&f.root, "cfbc4d17-8707-5571-853d-c9136a7f9f21");
    let installed_head = git(&clone, &["rev-parse", "HEAD"]);
    fs::write(clone.join("local.txt"), "keep").unwrap();
    git(&clone, &["add", "local.txt"]);
    git(&clone, &["commit", "-m", "local sibling work"]);
    let local_head = git(&clone, &["rev-parse", "HEAD"]);
    git(&clone, &["tag", "local-work", &local_head]);
    let before = fs::read(f.root.join("state.json")).unwrap();
    assert_eq!(
        m.update_with(
            ID,
            gitpm::Target::Ref("v1.0.0".into()),
            gitpm::UpdateOptions { detach: true },
        )
        .unwrap_err()
        .kind(),
        "local_changes"
    );
    assert_eq!(
        git(&clone, &["rev-parse", "refs/tags/local-work"]),
        local_head
    );
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
    git(&clone, &["tag", "-d", "local-work"]);
    m.update_with(
        ID,
        gitpm::Target::Ref("v1.0.0".into()),
        gitpm::UpdateOptions { detach: true },
    )
    .unwrap();
    assert_eq!(fs::read(clone.join("local.txt")).unwrap(), b"keep");
    assert_eq!(git(&clone, &["rev-parse", "HEAD"]), local_head);
    assert_eq!(
        git(&worktree_path(&f.root, ID), &["rev-parse", "HEAD"]),
        m.state().unwrap().packages[ID].current.commit
    );
    git(&clone, &["checkout", "--detach", &installed_head]);
    if forgotten {
        m.restore("cfbc4d17-8707-5571-853d-c9136a7f9f21").unwrap();
    }
    let state = m.state().unwrap();
    assert_eq!(
        state.packages[ID].clone,
        state.packages["cfbc4d17-8707-5571-853d-c9136a7f9f21"].clone
    );
    assert_eq!(
        fs::read(
            f.root
                .join("dist/cfbc4d17-8707-5571-853d-c9136a7f9f21/index.js")
        )
        .unwrap(),
        b"sibling"
    );
    assert_eq!(fs::read_dir(f.root.join("repos")).unwrap().count(), 1);
}

#[test]
fn retained_clone_can_leave_a_build_whose_commit_is_gone_upstream() {
    let f = Fixture::new(true);
    let repo = common::author(
        &f.dir.join("source"),
        "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc",
        "widget",
    );
    gitpm::scaffold::commit(&repo, "1.0.0").unwrap();
    let first = git(&repo, &["rev-parse", "HEAD"]);
    catalog::register(
        f.catalog.to_str().unwrap(),
        repo.to_str().unwrap(),
        "v1.0.0",
    )
    .unwrap();
    let root_file = f.catalog.join("catalog.json");
    let mut catalog_root: serde_json::Value =
        serde_json::from_slice(&fs::read(&root_file).unwrap()).unwrap();
    catalog_root["verified"] = json!(true);
    fs::write(root_file, catalog_root.to_string()).unwrap();
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    publish_repo(&f, &repo, ID, "2.0.0");
    m.update(ID).unwrap();
    assert!(m.state().unwrap().packages[ID].build.is_some());
    let clone = clone_path(&f.root, &repo);
    assert!(!worktree_path(&f.root, ID).exists());
    assert_eq!(
        git(
            &clone,
            &[
                "rev-parse",
                "refs/gitpm/41b88ba0-f9f1-5e65-acb6-3dc95231e0bc/previous"
            ]
        ),
        first
    );
    // A retained Git revision remains safe even when no tag names its commit.
    git(&clone, &["tag", "-d", "v1.0.0"]);
    let plan = m.uninstall_plan(ID).unwrap();
    assert!(plan.lost_work.is_empty(), "{:?}", plan.lost_work);
    // Replace the origin history after A with C, removing build commit B entirely.
    git(&repo, &["reset", "--hard", &first]);
    gitpm::scaffold::commit(&repo, "3.0.0").unwrap();
    git(&repo, &["tag", "-d", "v2.0.0"]);
    git(&repo, &["reflog", "expire", "--expire=now", "--all"]);
    git(&repo, &["gc", "--prune=now"]);
    m.update_with(
        ID,
        gitpm::Target::Ref("v3.0.0".into()),
        gitpm::UpdateOptions::default(),
    )
    .unwrap();
    let state = m.state().unwrap();
    assert_eq!(state.packages[ID].current.version, "3.0.0");
    assert!(state.packages[ID].build.is_none());
    assert_eq!(
        state.packages[ID]
            .previous_build
            .as_ref()
            .unwrap()
            .release
            .manifest
            .version,
        "2.0.0"
    );
    assert!(clone.join("objects").is_dir());
    m.rollback(ID).unwrap();
    assert!(m.state().unwrap().packages[ID].build.is_some());
}

#[test]
fn releasing_last_retained_clone_refuses_unpublished_work() {
    let f = Fixture::new(true);
    let repo = common::author(&f.dir.join("source"), ID, "widget");
    gitpm::scaffold::commit(&repo, "1.0.0").unwrap();
    catalog::register(
        f.catalog.to_str().unwrap(),
        repo.to_str().unwrap(),
        "v1.0.0",
    )
    .unwrap();
    let root_file = f.catalog.join("catalog.json");
    let mut catalog_root: serde_json::Value =
        serde_json::from_slice(&fs::read(&root_file).unwrap()).unwrap();
    catalog_root["verified"] = json!(true);
    fs::write(root_file, catalog_root.to_string()).unwrap();
    let m = f.manager();
    m.install_from_catalog(ID, None).unwrap();
    publish_repo(&f, &repo, ID, "2.0.0");
    m.update(ID).unwrap();

    let bare = clone_path(&f.root, &repo);
    let tree = git(&bare, &["rev-parse", "HEAD^{tree}"]);
    let head = git(&bare, &["rev-parse", "HEAD"]);
    let local = git(
        &bare,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit-tree",
            &tree,
            "-p",
            &head,
            "-m",
            "local work",
        ],
    );
    git(&bare, &["tag", "local-work", &local]);
    let state = fs::read(f.root.join("state.json")).unwrap();
    publish_repo(&f, &repo, ID, "3.0.0");
    assert_eq!(m.update(ID).unwrap_err().kind(), "local_changes");
    assert_eq!(fs::read(f.root.join("state.json")).unwrap(), state);
    assert_eq!(git(&bare, &["rev-parse", "refs/tags/local-work"]), local);
}

#[test]
fn retained_build_transition_refuses_unpublished_tags() {
    for catalog_update in [false, true] {
        let f = Fixture::new(true);
        let repo = common::author(&f.dir.join("source"), ID, "widget");
        gitpm::scaffold::commit(&repo, "1.0.0").unwrap();
        catalog::register(
            f.catalog.to_str().unwrap(),
            repo.to_str().unwrap(),
            "v1.0.0",
        )
        .unwrap();
        let root_file = f.catalog.join("catalog.json");
        let mut root: serde_json::Value =
            serde_json::from_slice(&fs::read(&root_file).unwrap()).unwrap();
        root["verified"] = json!(true);
        fs::write(root_file, root.to_string()).unwrap();
        let m = f.manager();
        m.install_from_catalog(ID, None).unwrap();
        publish_repo(&f, &repo, ID, "2.0.0");
        m.update(ID).unwrap();
        let bare = clone_path(&f.root, &repo);
        let tree = git(&bare, &["rev-parse", "HEAD^{tree}"]);
        let local = git(
            &bare,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit-tree",
                &tree,
                "-m",
                "local work",
            ],
        );
        git(&bare, &["tag", "local-work", &local]);
        gitpm::scaffold::commit(&repo, "3.0.0").unwrap();
        catalog::register(
            f.catalog.to_str().unwrap(),
            repo.to_str().unwrap(),
            "v3.0.0",
        )
        .unwrap();
        let before = fs::read(f.root.join("state.json")).unwrap();
        let update = || {
            if catalog_update {
                m.update(ID)
            } else {
                m.update_with(
                    ID,
                    gitpm::Target::Ref("v3.0.0".to_owned()),
                    gitpm::UpdateOptions { detach: true },
                )
            }
        };
        assert_eq!(update().unwrap_err().kind(), "local_changes");
        assert_eq!(git(&bare, &["rev-parse", "refs/tags/local-work"]), local);
        assert_eq!(fs::read(f.root.join("state.json")).unwrap(), before);
        assert!(!worktree_path(&f.root, ID).exists());
        git(&bare, &["tag", "-d", "local-work"]);
        update().unwrap();
        assert!(m.state().unwrap().packages[ID].build.is_none());
    }
}
