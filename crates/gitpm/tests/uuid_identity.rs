use gitpm::{Manager, Manifest, scaffold};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "gitpm-uuid-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn manifest(path: &Path) -> Manifest {
    serde_json::from_slice(&fs::read(path.join("plugin.json")).unwrap()).unwrap()
}

#[test]
fn init_generates_persistent_ids_and_equal_aliases_install_side_by_side() {
    let f = Fixture::new();
    let first = scaffold::init(&f.0.join("first"), "same").unwrap();
    let second = scaffold::init(&f.0.join("second"), "same").unwrap();
    gitpm::valid_id(&first.id).unwrap();
    gitpm::valid_id(&second.id).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(manifest(&first.repo).name, "same");
    assert_eq!(manifest(&second.repo).name, "same");
    let root = f.0.join("workspace");
    let manager = Manager::open(&root).unwrap();
    manager.install(first.repo.to_str().unwrap()).unwrap();
    manager.install(second.repo.to_str().unwrap()).unwrap();
    let before = manager.state().unwrap();
    for id in [&first.id, &second.id] {
        assert!(before.packages.contains_key(id));
        assert!(root.join("dist").join(id).join("index.js").is_file());
    }
    // Changing the human alias and releasing does not change identity or paths.
    let mut changed = manifest(&first.repo);
    changed.name = "renamed".into();
    fs::write(
        first.repo.join("plugin.json"),
        serde_json::to_vec(&changed).unwrap(),
    )
    .unwrap();
    scaffold::commit(&first.repo, "0.2.0").unwrap();
    assert_eq!(manifest(&first.repo).id, first.id);
    manager.update(&first.id).unwrap();
    assert_eq!(manager.state().unwrap().packages.len(), 2);
    assert!(root.join("dist").join(&first.id).is_dir());
    for invalid in ["same", "vendor/same"] {
        assert_eq!(manager.disable(invalid).unwrap_err().kind(), "invalid_id");
    }
}

#[test]
fn replacement_uses_original_uuid_without_rewriting_consumer_edges() {
    let f = Fixture::new();
    let original = scaffold::init(&f.0.join("original"), "base").unwrap();
    let consumer = scaffold::init(&f.0.join("consumer"), "app").unwrap();
    let mut app = manifest(&consumer.repo);
    app.dependencies.insert(original.id.clone(), "^0.1".into());
    fs::write(
        consumer.repo.join("plugin.json"),
        serde_json::to_vec(&app).unwrap(),
    )
    .unwrap();
    scaffold::commit(&consumer.repo, "0.1.1").unwrap();
    let manager = Manager::open(f.0.join("workspace")).unwrap();
    manager.install(original.repo.to_str().unwrap()).unwrap();
    manager.install(consumer.repo.to_str().unwrap()).unwrap();
    let copy = manager
        .fork_packages(&[&original.id], &f.0.join("fork"))
        .unwrap();
    let fork = manifest(&copy.path);
    assert_ne!(fork.id, original.id);
    assert_eq!(fork.name, "base");
    assert_eq!(fork.fork_of.as_ref().unwrap().id, original.id);
    assert_eq!(fork.replaces.as_ref().unwrap().id, original.id);
    scaffold::commit(&copy.path, "0.1.2").unwrap();
    manager.install(copy.path.to_str().unwrap()).unwrap();
    manager.replace(&original.id, &fork.id).unwrap();
    let state = manager.state().unwrap();
    assert_eq!(state.replacements[&original.id], fork.id);
    assert!(
        state.packages[&consumer.id]
            .dependencies
            .contains_key(&original.id)
    );
    assert!(
        !state.packages[&consumer.id]
            .dependencies
            .contains_key(&fork.id)
    );
    assert_eq!(manifest(&consumer.repo).dependencies, app.dependencies);
}
