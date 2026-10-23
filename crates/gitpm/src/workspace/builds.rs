//! Build revisions share workspace transactions and dependency resolution with Git.
use super::*;
use crate::{artifact, model::BuildInstall};

pub(super) fn stage_build(txn: &mut Txn, id: &str, build: &BuildInstall) -> Result<()> {
    let relative = artifact::relative(id, build);
    let output = txn.scratch(&format!(
        "build-{}",
        build.release.build.as_ref().unwrap().sha256
    ));
    artifact::download(&build.release, &output)?;
    txn.add_repo(&relative, &output)?;
    Ok(())
}

pub(super) fn deploy_build(txn: &mut Txn, id: &str, build: &BuildInstall) -> Result<()> {
    let relative = artifact::relative(id, build);
    let source = txn
        .staged_repo(&relative)
        .unwrap_or_else(|| txn.root().join(&relative));
    ensure!(
        catalog::hash_files(&source)? == build.release.files,
        Catalog,
        "cached build output differs from catalog"
    );
    let target = txn.stage_dist(id)?;
    copy_tree(&source, &target, false)
}

pub(super) fn build_result(id: &str, p: &Installed, already: bool) -> InstallResult {
    InstallResult {
        id: id.into(),
        source: p.source.clone(),
        current: p.current.clone(),
        fork_of: p.fork_of.clone(),
        skipped: Vec::new(),
        warnings: Vec::new(),
        already_installed: already,
        available: None,
        mode: p.mode.clone(),
        requested_mode: None,
        enabled: p.enabled,
        dependencies: Vec::new(),
        dist: DistSync::default(),
    }
}

pub(super) fn adopt_build(
    txn: &mut Txn,
    state: &mut State,
    release: &catalog::Release,
    location: Option<String>,
    protection: Option<String>,
    enabled: bool,
) -> Result<InstallResult> {
    let id = &release.manifest.id;
    check_new_id(txn.root(), state, &release.manifest)?;
    let build = BuildInstall {
        location,
        release: release.clone(),
    };
    stage_build(txn, id, &build)?;
    let mut p = Installed {
        source: release.source.clone(),
        current: Revision {
            version: release.manifest.version.clone(),
            commit: release.commit.clone(),
            ..Default::default()
        },
        enabled,
        catalog: protection,
        path: release.path.clone(),
        build: Some(build),
        ..Default::default()
    };
    p.adopt(&release.manifest);
    let result = build_result(id, &p, false);
    state.packages.insert(id.clone(), p.clone());
    if state.is_active(id) {
        deploy_build(txn, id, p.build.as_ref().unwrap())?;
    }
    Ok(result)
}

impl Manager {
    pub(super) fn install_build_release(
        &self,
        release: &catalog::Release,
        protection: Option<String>,
        activation: Activation,
    ) -> Result<InstallResult> {
        let enabled = matches!(activation, Activation::Install { .. });
        let force = matches!(activation, Activation::Install { force: true });
        let location = self
            .options
            .catalog
            .as_deref()
            .map(catalog::canonical_location)
            .transpose()?;
        let (result, dist) = self.transaction(|txn, state| {
            let id = &release.manifest.id;
            if let Some(p) = state.packages.get(id).cloned() {
                ensure!(
                    p.source == release.source,
                    VersionConflict,
                    "package {id} is installed from another source"
                );
                let protecting = protection.as_ref().filter(|_| p.catalog.is_none());
                if let Some(location) = protecting {
                    if let Some(build) = &p.build {
                        let expected = catalog::lookup(location, id, Some(&p.current.version))?;
                        ensure!(
                            expected.source == p.source
                                && expected.commit == p.current.commit
                                && expected.path == p.path,
                            Catalog,
                            "installed revision cannot be protected by that catalog"
                        );
                        verify_catalog_manifest(&build.release.manifest, &expected)?;
                        let cached = txn.root().join(artifact::relative(id, build));
                        ensure!(
                            catalog::hash_files(&cached)? == expected.files,
                            Catalog,
                            "cached build differs from catalog"
                        );
                    } else {
                        let live = installed_repo(txn.root(), id, &p)?;
                        let repo = txn.scratch("protect-existing");
                        copy_tree(&live, &repo, true)?;
                        verify_installed_release(
                            txn,
                            &repo,
                            id,
                            &p,
                            location,
                            self.options.custom_ssh,
                        )?;
                    }
                    state.packages.get_mut(id).unwrap().catalog = Some(location.clone());
                }
                if enabled && !p.enabled {
                    state.packages.get_mut(id).unwrap().enabled = true;
                    if !force {
                        deps::refuse_inactive_dependencies(state, id, "install", true)?;
                    }
                }
                let mut result = build_result(id, &state.packages[id], p.enabled || !enabled);
                if let Some(location) = protecting {
                    result
                        .warnings
                        .push(format!("{id} is now protected by catalog {location}"));
                }
                if release.commit != p.current.commit {
                    result.available = Some(Revision {
                        version: release.manifest.version.clone(),
                        commit: release.commit.clone(),
                        ..Default::default()
                    });
                }
                return Ok(result);
            }
            let m = &release.manifest;
            let mut next = state.clone();
            let mut p = Installed::default();
            p.current.version = m.version.clone();
            p.adopt(m);
            next.packages.insert(id.clone(), p);
            let mut roots = deps::healthy_roots(state)?;
            if !state.replacements.contains_key(id) {
                roots.insert(id.clone(), deps::version(&m.version)?);
            }
            let missing =
                deps::solve(&deps::fixed_from_state(&next)?, &roots, location.as_deref())?;
            let dependencies =
                self.install_missing(txn, state, &missing, enabled, location.as_deref())?;
            let mut result = adopt_build(
                txn,
                state,
                release,
                location.clone(),
                protection.clone(),
                enabled,
            )?;
            if enabled && !force && state.is_active(id) {
                deps::refuse_inactive_dependencies(state, id, "install", true)?;
            }
            result.dependencies = dependencies;
            Ok(result)
        })?;
        Ok(InstallResult { dist, ..result })
    }

    pub(super) fn apply_catalog_revision(
        &self,
        txn: &mut Txn,
        state: &mut State,
        id: &str,
        release: &catalog::Release,
        location: Option<String>,
    ) -> Result<RevisionChange> {
        let p = state.packages[id].clone();
        let next = Revision {
            version: release.manifest.version.clone(),
            commit: release.commit.clone(),
            ..Default::default()
        };
        let mut entry = p.clone();
        entry.previous = Some(p.current.with_mode(&p.mode));
        entry.previous_build = p.build.clone();
        entry.current = next.clone();
        entry.mode = Mode::Release;
        entry.path = release.path.clone();
        entry.adopt(&release.manifest);
        if release.build.is_some() {
            let build = BuildInstall {
                location,
                release: release.clone(),
            };
            stage_build(txn, id, &build)?;
            entry.build = Some(build);
        } else {
            let ssh = self.options.custom_ssh;
            if let Ok(live) = installed_repo(txn.root(), id, &p) {
                guard_retained(&live, id, ssh)?;
                let relative = p.clone_folder(id);
                let repo = txn.repo(&relative, &live)?;
                clones::fetch_origin(&repo, true, ssh)?;
                ensure_release_commit(&repo, release, ssh)?;
                let found = updated_package(&repo, &release.commit, id, ssh)?;
                ensure!(
                    found.subdir() == release.path.as_deref(),
                    Catalog,
                    "catalog package folder does not match Git revision"
                );
                verify_catalog_manifest(&found.manifest, release)?;
                let output = txn.scratch("verify-build-to-git");
                export_dist(
                    &repo,
                    &release.commit,
                    release.path.as_deref(),
                    &output,
                    ssh,
                )?;
                verify_catalog_release(&found.manifest, release, &output)?;
                SystemTransport.checkout_detached(&repo, &release.commit, false, ssh)?;
            } else {
                let cloned = clone_revision(txn, &Fetch::release(release), "build-to-git", ssh)?;
                let (_, _, key) = place_clone(txn, state, &cloned, &release.source, ssh)?;
                entry.clone = key;
            }
            entry.build = None;
        }
        state.packages.insert(id.into(), entry.clone());
        if state.is_active(id) {
            deploy_committed(txn, id, &entry, self.options.custom_ssh)?;
        }
        Ok(RevisionChange {
            id: id.into(),
            from: p.current.clone(),
            to: next.clone(),
            changed: true,
            major: deps::is_major_text(&p.current.version, &next.version),
            warnings: metadata_warnings(&release.manifest),
            skipped: Vec::new(),
            unchanged_reason: None,
            mode: Mode::Release,
            detached: None,
            dist: DistSync::default(),
        })
    }
}

pub(super) fn candidates(p: &Installed, id: &str, location: &str) -> Result<Vec<catalog::Release>> {
    if p.catalog.is_some() {
        ensure!(
            catalog::is_verified(location)?,
            Catalog,
            "catalog {location} that protects {id} is no longer verified; detach {id} to update it from its origin"
        );
    }
    let current = deps::version(&p.current.version)?;
    let mut releases = Vec::new();
    for version in catalog::versions(location, id)? {
        let key = deps::version(&version)?;
        if key <= current || !deps::follows(Some(&current), &key) {
            continue;
        }
        let release = catalog::lookup(location, id, Some(&version))?;
        catalog::validate_release(&release, id, &version)?;
        ensure!(
            release.source == p.source,
            Catalog,
            "catalog source differs from installed source"
        );
        releases.push(release);
    }
    releases.sort_by_key(|r| deps::version(&r.manifest.version).expect("validated version"));
    Ok(releases)
}

/// A build may retain a clone from an older Git revision; never overwrite user work.
pub(super) fn guard_retained(repo: &Path, id: &str, ssh: bool) -> Result<()> {
    clean(repo, id, ssh)?;
    let items = lost_work::unpublished_head_commits(repo, ssh)?;
    if !items.is_empty() {
        return Err(Error::LocalChanges {
            id: id.into(),
            message: "unpublished commits block transition to Git".into(),
            items,
        });
    }
    lost_work::require_no_hidden_for_update(
        repo,
        id,
        "unpublished work blocks transition to Git",
        ssh,
    )
}
