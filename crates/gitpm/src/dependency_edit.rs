//! Working-tree package inspection and explicit dependency edits.
use crate::error::ensure;
use crate::{Error, ForkOf, Manifest, Result, State, model, packages, transport, valid_id};
use serde::Serialize;
use std::{collections::BTreeMap, fs, path::Path};

/// A package as declared in a working-tree manifest, rather than its installed snapshot.
#[derive(Clone, Debug, Serialize)]
pub struct PackageDetails {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Repository-relative package folder; empty for a root package.
    pub path: String,
    #[serde(rename = "forkOf")]
    pub fork_of: Option<ForkOf>,
    pub dependencies: BTreeMap<String, String>,
    pub installed: bool,
}
impl PackageDetails {
    fn new(manifest: Manifest, path: String, installed: bool) -> Self {
        Self {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            path,
            fork_of: manifest.fork_of,
            dependencies: manifest.dependencies,
            installed,
        }
    }
}

/// No inference or version adjustment is performed by an edit.
#[derive(Clone, Debug)]
pub enum DependencyEdit {
    Add {
        dependency: String,
        requirement: String,
    },
    Update {
        dependency: String,
        new_dependency: String,
        requirement: String,
    },
    Remove {
        dependency: String,
    },
}

fn repository(directory: &Path) -> Result<std::path::PathBuf> {
    let directory = fs::canonicalize(directory)?;
    ensure!(
        Path::new(&transport::repository_root(&directory, false)?) == directory,
        InvalidArgument,
        "pass the repository root"
    );
    Ok(directory)
}

/// Inspect every working-tree package, optionally checking installation membership.
/// Missing workspaces are empty; no workspace is created or locked.
pub fn repository_packages(directory: &Path, root: Option<&Path>) -> Result<Vec<PackageDetails>> {
    let directory = repository(directory)?;
    let state = match root {
        Some(root) => crate::check::read_state(root)?,
        None => State::default(),
    };
    let found = match packages::packages_in(&directory)? {
        Some(found) => found,
        None => vec![packages::Package {
            path: String::new(),
            manifest: read_manifest(&directory.join("plugin.json"))?,
        }],
    };
    let mut ids = std::collections::BTreeSet::new();
    found
        .into_iter()
        .map(|p| {
            model::validate_manifest(&p.manifest)?;
            ensure!(
                ids.insert(p.manifest.id.clone()),
                InvalidManifest,
                "duplicate package UUID: {}",
                p.manifest.id
            );
            let installed = state.packages.contains_key(&p.manifest.id);
            Ok(PackageDetails::new(p.manifest, p.path, installed))
        })
        .collect()
}

fn read_manifest(path: &Path) -> Result<Manifest> {
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        Unsupported,
        "symlinks are not supported: {}",
        path.display()
    );
    let manifest = serde_json::from_slice(&fs::read(path)?)
        .map_err(|e| Error::invalid_manifest(format!("invalid plugin.json: {e}")))?;
    model::validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Edit one package in an author repository; no commit or installation is performed.
pub fn edit_dependency(
    directory: &Path,
    package: &str,
    edit: &DependencyEdit,
) -> Result<PackageDetails> {
    valid_id(package)?;
    let directory = repository(directory)?;
    let found = repository_packages(&directory, None)?;
    let selected = found
        .into_iter()
        .find(|p| p.id == package)
        .ok_or_else(|| Error::InvalidArgument(format!("repository has no package {package}")))?;
    edit_at(&directory, package, &selected.path, edit, false)
}

pub(crate) fn edit_at(
    directory: &Path,
    package: &str,
    folder: &str,
    edit: &DependencyEdit,
    installed: bool,
) -> Result<PackageDetails> {
    // Validate every parent before following a path from state or package discovery.
    let mut current = directory.to_path_buf();
    if !folder.is_empty() {
        packages::validate_path(folder)?;
        for part in folder.split('/') {
            current.push(part);
            ensure!(
                !fs::symlink_metadata(&current)?.file_type().is_symlink(),
                Unsupported,
                "symlinks are not supported: {}",
                current.display()
            );
        }
    }
    let path = current.join("plugin.json");
    let mut manifest = read_manifest(&path)?;
    ensure!(
        manifest.id == package,
        InvalidManifest,
        "package identity differs from {package}"
    );
    match edit {
        DependencyEdit::Add {
            dependency,
            requirement,
        } => {
            ensure!(
                !manifest.dependencies.contains_key(dependency),
                InvalidArgument,
                "dependency {dependency} already exists"
            );
            manifest
                .dependencies
                .insert(dependency.clone(), requirement.clone());
        }
        DependencyEdit::Update {
            dependency,
            new_dependency,
            requirement,
        } => {
            ensure!(
                manifest.dependencies.contains_key(dependency),
                InvalidArgument,
                "dependency {dependency} does not exist"
            );
            ensure!(
                dependency == new_dependency || !manifest.dependencies.contains_key(new_dependency),
                InvalidArgument,
                "dependency {new_dependency} already exists"
            );
            manifest.dependencies.remove(dependency);
            manifest
                .dependencies
                .insert(new_dependency.clone(), requirement.clone());
        }
        DependencyEdit::Remove { dependency } => {
            ensure!(
                manifest.dependencies.remove(dependency).is_some(),
                InvalidArgument,
                "dependency {dependency} does not exist"
            );
        }
    }
    model::validate_manifest(&manifest)?;
    let original = fs::read(&path)?;
    let updated = model::manifest_json(&manifest)?;
    if original != updated.as_bytes()
        && let Err(error) = fs::write(&path, updated)
    {
        let _ = fs::write(&path, original);
        return Err(error.into());
    }
    Ok(PackageDetails::new(manifest, folder.into(), installed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TempDir, git, manifest, write_manifest};
    const PACKAGE: &str = "11111111-1111-4111-8111-111111111111";
    const DEP: &str = "22222222-2222-4222-8222-222222222222";
    const NEW: &str = "33333333-3333-4333-8333-333333333333";

    #[test]
    fn dependency_edits_validate_and_preserve_metadata() {
        for nested in [false, true] {
            let temp = TempDir::new("dependency-edits");
            git(temp.path(), &["init", "-q"]);
            let folder = if nested {
                fs::write(temp.join("gitpm.json"), r#"{"packages":["packages/*"]}"#).unwrap();
                temp.join("packages/app")
            } else {
                temp.path().to_path_buf()
            };
            fs::create_dir_all(&folder).unwrap();
            let mut m = manifest(PACKAGE, "1.0.0");
            m.extra
                .insert("custom".into(), serde_json::json!({"nested": [1,2]}));
            m.extra.insert("title".into(), "Keep me".into());
            write_manifest(&folder, &m);
            // Author manifests remain inspectable/editable before producing output.
            let root = temp.join("missing-workspace");
            let view = repository_packages(temp.path(), Some(&root)).unwrap();
            assert_eq!(view.len(), 1);
            assert_eq!(view[0].name, "fixture");
            assert!(view[0].fork_of.is_none());
            assert!(!view[0].installed);
            assert!(!root.exists() && !temp.join(".missing-workspace.lock").exists());
            let original = fs::read(folder.join("plugin.json")).unwrap();
            for (edit, kind) in [
                (
                    DependencyEdit::Add {
                        dependency: PACKAGE.into(),
                        requirement: "*".into(),
                    },
                    "invalid_manifest",
                ),
                (
                    DependencyEdit::Add {
                        dependency: DEP.into(),
                        requirement: "not a version".into(),
                    },
                    "invalid_manifest",
                ),
                (
                    DependencyEdit::Add {
                        dependency: "invalid".into(),
                        requirement: "*".into(),
                    },
                    "invalid_id",
                ),
                (
                    DependencyEdit::Remove {
                        dependency: DEP.into(),
                    },
                    "invalid_argument",
                ),
                (
                    DependencyEdit::Update {
                        dependency: DEP.into(),
                        new_dependency: NEW.into(),
                        requirement: "*".into(),
                    },
                    "invalid_argument",
                ),
            ] {
                assert_eq!(
                    edit_dependency(temp.path(), PACKAGE, &edit)
                        .unwrap_err()
                        .kind(),
                    kind
                );
                assert_eq!(fs::read(folder.join("plugin.json")).unwrap(), original);
            }
            assert_eq!(
                edit_dependency(
                    temp.path(),
                    NEW,
                    &DependencyEdit::Remove {
                        dependency: DEP.into()
                    }
                )
                .unwrap_err()
                .kind(),
                "invalid_argument"
            );
            for dependency in [DEP, NEW] {
                edit_dependency(
                    temp.path(),
                    PACKAGE,
                    &DependencyEdit::Add {
                        dependency: dependency.into(),
                        requirement: "^1".into(),
                    },
                )
                .unwrap();
            }
            let before = fs::read(folder.join("plugin.json")).unwrap();
            for edit in [
                DependencyEdit::Add {
                    dependency: DEP.into(),
                    requirement: "*".into(),
                },
                DependencyEdit::Update {
                    dependency: DEP.into(),
                    new_dependency: NEW.into(),
                    requirement: "*".into(),
                },
            ] {
                assert_eq!(
                    edit_dependency(temp.path(), PACKAGE, &edit)
                        .unwrap_err()
                        .kind(),
                    "invalid_argument"
                );
                assert_eq!(fs::read(folder.join("plugin.json")).unwrap(), before);
            }
            edit_dependency(
                temp.path(),
                PACKAGE,
                &DependencyEdit::Remove {
                    dependency: NEW.into(),
                },
            )
            .unwrap();
            let changed = edit_dependency(
                temp.path(),
                PACKAGE,
                &DependencyEdit::Update {
                    dependency: DEP.into(),
                    new_dependency: DEP.into(),
                    requirement: "~2.1".into(),
                },
            )
            .unwrap();
            assert_eq!(changed.dependencies[DEP], "~2.1");
            let changed = edit_dependency(
                temp.path(),
                PACKAGE,
                &DependencyEdit::Update {
                    dependency: DEP.into(),
                    new_dependency: NEW.into(),
                    requirement: "^3".into(),
                },
            )
            .unwrap();
            assert_eq!(changed.dependencies[NEW], "^3");
            assert!(!changed.dependencies.contains_key(DEP));
            let changed = edit_dependency(
                temp.path(),
                PACKAGE,
                &DependencyEdit::Remove {
                    dependency: NEW.into(),
                },
            )
            .unwrap();
            assert!(changed.dependencies.is_empty());
            let final_manifest = read_manifest(&folder.join("plugin.json")).unwrap();
            assert_eq!(final_manifest.extra, m.extra);
            assert_eq!(final_manifest.id, m.id);
            assert_eq!(final_manifest.version, m.version);
        }
    }

    #[test]
    fn installed_fork_dependency_edits_leave_installation_snapshot_intact() {
        let temp = TempDir::new("installed-dependency-edits");
        let author = crate::test_support::Author::new(temp.join("author"), PACKAGE);
        let root = temp.join("workspace");
        let manager = crate::Manager::open(&root).unwrap();
        manager.install(author.source()).unwrap();
        let fork = manager
            .fork_packages(&[PACKAGE], &temp.join("fork"))
            .unwrap();
        let id = &fork.packages[0].id;
        crate::scaffold::commit(&fork.path, "0.2.0").unwrap();
        manager.install(fork.path.to_str().unwrap()).unwrap();
        let state = fs::read(root.join("state.json")).unwrap();
        let dist = crate::test_support::tree_hash(&root.join("dist"));
        let list = manager.list().unwrap();
        let installed = list.iter().find(|p| &p.id == id).unwrap();
        assert_eq!(installed.name, "fixture");
        assert!(installed.installed);
        assert_eq!(installed.fork_of.as_ref().unwrap().id, PACKAGE);
        assert!(repository_packages(&fork.path, Some(&root)).unwrap()[0].installed);
        let edited = manager
            .edit_dependency(
                id,
                &DependencyEdit::Add {
                    dependency: DEP.into(),
                    requirement: "^1".into(),
                },
            )
            .unwrap();
        assert!(edited.installed);
        assert_eq!(edited.dependencies[DEP], "^1");
        let manifest_path = root.join("worktrees").join(id).join("plugin.json");
        let before = fs::read(&manifest_path).unwrap();
        assert_eq!(
            manager
                .edit_dependency(
                    id,
                    &DependencyEdit::Update {
                        dependency: DEP.into(),
                        new_dependency: id.clone(),
                        requirement: "*".into()
                    }
                )
                .unwrap_err()
                .kind(),
            "invalid_manifest"
        );
        assert_eq!(fs::read(&manifest_path).unwrap(), before);
        manager
            .edit_dependency(
                id,
                &DependencyEdit::Update {
                    dependency: DEP.into(),
                    new_dependency: NEW.into(),
                    requirement: "~2".into(),
                },
            )
            .unwrap();
        manager
            .edit_dependency(
                id,
                &DependencyEdit::Remove {
                    dependency: NEW.into(),
                },
            )
            .unwrap();
        assert_eq!(fs::read(root.join("state.json")).unwrap(), state);
        assert_eq!(crate::test_support::tree_hash(&root.join("dist")), dist);
        assert!(
            manager
                .list()
                .unwrap()
                .iter()
                .find(|p| &p.id == id)
                .unwrap()
                .dependencies
                .is_empty()
        );
    }
}
