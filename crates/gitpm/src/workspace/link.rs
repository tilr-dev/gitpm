//! Explicit live installation of an author-owned package folder.
use super::*;
use crate::{LinkDescriptor, LinkDiscovery};

#[derive(Clone, Debug, Serialize)]
pub struct LinkResult {
    pub id: String,
    pub version: String,
    pub mode: Mode,
    pub refreshed: bool,
    pub dist: DistSync,
}

fn recorded_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .require(Error::Workspace, "link paths must be valid UTF-8")
}

impl Manager {
    /// Link a local package's validated output. Relinking refreshes its manifest
    /// snapshot; switching an existing managed installation requires uninstall first.
    pub fn link(&self, package: impl AsRef<Path>) -> Result<LinkResult> {
        let package = fs::canonicalize(package.as_ref())
            .map_err(|e| Error::Workspace(format!("cannot resolve link package: {e}")))?;
        ensure!(
            package.is_dir(),
            Workspace,
            "link package is not a directory"
        );
        // Read syntax separately so malformed JSON keeps the public invalid_data kind.
        let bytes = fs::read(package.join("plugin.json"))
            .map_err(|e| Error::invalid_manifest(format!("cannot read plugin.json: {e}")))?;
        let json: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| Error::InvalidData(format!("invalid plugin.json: {e}")))?;
        let draft: Manifest = serde_json::from_value(json)
            .map_err(|e| Error::invalid_manifest(format!("invalid plugin.json: {e}")))?;
        crate::model::validate_manifest(&draft)
            .map_err(|e| Error::invalid_manifest(e.to_string()))?;
        draft.validate_dist()?;
        let target = fs::canonicalize(package.join(draft.dist_folder()))
            .map_err(|e| Error::Workspace(format!("link output is unavailable: {e}")))?;
        ensure!(target.is_dir(), Workspace, "link output is not a directory");
        let workspace_root = fs::canonicalize(&self.root)?;
        for path in [&package, &target] {
            ensure!(
                !path.starts_with(&workspace_root) && !workspace_root.starts_with(path),
                Workspace,
                "link target would reference workspace managed storage or create a cycle"
            );
        }
        let m = draft;
        let source = transport::config_get(&package, "remote.origin.url", self.options.custom_ssh)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| crate::auth::validate_recorded(s).is_ok())
            .unwrap_or_default();
        let descriptor = LinkDescriptor {
            package_root: recorded_path(&package)?,
            target: recorded_path(&target)?,
            discovery: m.fork_of.clone().map(|fork_of| LinkDiscovery { fork_of }),
        };
        let mode = Mode::Link(descriptor);
        let (refreshed, dist) = self.transaction(|txn, state| {
            let previous = state.packages.get(&m.id);
            let refreshed = previous.is_some();
            if let Some(previous) = previous {
                if let Some(link) = previous.link() {
                    let leaf = txn.root().join("dist").join(&m.id);
                    if fs::symlink_metadata(&leaf).is_ok() {
                        ensure!(
                            fs::read_link(&leaf).ok().as_deref() == Some(Path::new(&link.target)),
                            Workspace,
                            "unexpected link entry: {}",
                            leaf.display()
                        );
                    }
                }
                ensure!(
                    previous.link().is_some(),
                    Workspace,
                    "{} is managed; uninstall it explicitly before linking",
                    m.id
                );
            } else {
                check_new_id(txn.root(), state, &m)?;
                ensure_no_case_collision(&txn.root().join("dist"), &m.id)?;
                let leaf = txn.root().join("dist").join(&m.id);
                match fs::symlink_metadata(&leaf) {
                    Ok(_) => {
                        return Err(Error::Workspace(format!(
                            "unexpected deployment entry: {}",
                            leaf.display()
                        )));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
            if let Some(original) = no_longer_replaces(state, &m.id, &m) {
                return Err(Error::DependencyConflict(format!(
                    "{} no longer replaces {original}",
                    m.id
                )));
            }
            let before = state.clone();
            let mut record = Installed {
                name: m.name.clone(),
                source: source.clone(),
                // Internal solver snapshot only: link serialization has a declared
                // version and no revision, commit, history or managed-clone fields.
                current: Revision {
                    version: m.version.clone(),
                    ..Default::default()
                },
                enabled: previous.is_none_or(|p| p.enabled),
                mode: mode.clone(),
                dependencies: m.dependencies.clone(),
                metadata: m.metadata(),
                replaces: m.replaces.clone(),
                ..Default::default()
            };
            record.fork_of = None;
            state.packages.insert(m.id.clone(), record);
            let broken = deps::breaking_dependents(&before, &m.id, state);
            ensure!(
                broken.is_empty(),
                DependencyConflict,
                "relink of {} breaks dependents: {}",
                m.id,
                broken.join(", ")
            );
            deps::refuse_inactive_dependencies(state, &m.id, "link", false)?;
            if state.is_active(&m.id) {
                txn.deploy_link(&m.id, &target)?;
            }
            Ok(refreshed)
        })?;
        Ok(LinkResult {
            id: m.id,
            version: m.version,
            mode,
            refreshed,
            dist,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Author, TempDir};

    #[cfg(unix)]
    #[test]
    fn non_utf8_link_paths_cannot_be_recorded_lossily() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = PathBuf::from(OsString::from_vec(b"/author-\xff/dist".to_vec()));
        assert_eq!(recorded_path(&path).unwrap_err().kind(), "workspace");
    }

    #[test]
    fn link_entries_recover_at_every_commit_rename_without_restoring_author_bytes() {
        for renames in 0..3 {
            let dir = TempDir::new("link-recovery");
            let author = Author::new(dir.join("author"), "d23a35dc-fbc9-5f63-89f2-6a133fde3cd1");
            let root = dir.join("workspace");
            let manager = Manager::open(&root).unwrap();
            manager.link(author.source()).unwrap();
            // Relink moves the old entry, publishes the new one, then state.
            txn::CRASH_AFTER.with(|c| c.set(Some(renames)));
            let result = manager.link(author.source());
            txn::CRASH_AFTER.with(|c| c.set(None));
            assert!(result.unwrap_err().to_string().contains("simulated crash"));
            fs::write(
                Path::new(author.source()).join("dist/live.txt"),
                "external edit after journal",
            )
            .unwrap();
            drop(manager);
            let manager = Manager::open(&root).unwrap();
            assert!(
                manager.state().unwrap().packages["d23a35dc-fbc9-5f63-89f2-6a133fde3cd1"]
                    .link()
                    .is_some()
            );
            assert_eq!(
                fs::read_to_string(root.join("dist/d23a35dc-fbc9-5f63-89f2-6a133fde3cd1/live.txt"))
                    .unwrap(),
                "external edit after journal"
            );
            assert!(!root.join(txn::DIR).exists());
            drop(manager);
            let _again = Manager::open(&root).unwrap();
        }
    }

    #[test]
    fn recovery_of_link_install_and_removal_never_follows_a_dangling_target() {
        for removing in [false, true] {
            for renames in 0..2 {
                let dir = TempDir::new("link-dangling-recovery");
                let author =
                    Author::new(dir.join("author"), "d23a35dc-fbc9-5f63-89f2-6a133fde3cd1");
                let root = dir.join("workspace");
                let manager = Manager::open(&root).unwrap();
                if removing {
                    manager.link(author.source()).unwrap();
                }
                txn::CRASH_AFTER.with(|c| c.set(Some(renames)));
                let failed = if removing {
                    manager
                        .uninstall("d23a35dc-fbc9-5f63-89f2-6a133fde3cd1", &[], true)
                        .map(|_| ())
                } else {
                    manager.link(author.source()).map(|_| ())
                };
                txn::CRASH_AFTER.with(|c| c.set(None));
                assert!(failed.is_err());
                let author = Path::new(author.source());
                fs::rename(author.join("dist"), author.join("moved")).unwrap();
                fs::write(author.join("moved/precious.txt"), "keep").unwrap();
                drop(manager);
                let manager = Manager::open(&root).unwrap();
                assert_eq!(
                    manager
                        .state()
                        .unwrap()
                        .packages
                        .contains_key("d23a35dc-fbc9-5f63-89f2-6a133fde3cd1"),
                    !removing
                );
                assert_eq!(
                    fs::symlink_metadata(root.join("dist/d23a35dc-fbc9-5f63-89f2-6a133fde3cd1"))
                        .is_ok(),
                    !removing
                );
                assert_eq!(
                    fs::read_to_string(author.join("moved/precious.txt")).unwrap(),
                    "keep"
                );
                assert!(!author.join("dist").exists());
            }
        }
    }

    #[test]
    fn uncommitted_staged_links_are_discarded_without_following_them() {
        let dir = TempDir::new("link-uncommitted");
        let author = Author::new(dir.join("author"), "d23a35dc-fbc9-5f63-89f2-6a133fde3cd1");
        let root = dir.join("workspace");
        let manager = Manager::open(&root).unwrap();
        let state = fs::read(root.join("state.json")).unwrap();
        {
            let mut transaction = Txn::begin(&root).unwrap();
            transaction
                .deploy_link(
                    "d23a35dc-fbc9-5f63-89f2-6a133fde3cd1",
                    &Path::new(author.source()).join("dist"),
                )
                .unwrap();
            // Failure before the journal: dropping it cleans only staged entries.
        }
        assert_eq!(fs::read(root.join("state.json")).unwrap(), state);
        assert!(
            !root
                .join("dist/d23a35dc-fbc9-5f63-89f2-6a133fde3cd1")
                .exists()
        );
        assert!(Path::new(author.source()).join("dist").is_dir());
        drop(manager);
        let _again = Manager::open(&root).unwrap();
    }
}
