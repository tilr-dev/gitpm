//! Release tags are the shared source of truth for installs and catalog generation.
use crate::catalog::version_key;
use crate::error::{Error, Result, ensure};
use crate::{Manifest, exec::git, model::resolve_ref, packages, valid_id};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug)]
pub struct Tag {
    pub name: String,
    pub commit: String,
    pub manifest: Manifest,
    /// Package folder at the tagged commit; empty for the repository root.
    pub path: String,
}

/// A release-looking tag that could not be used; reported as data, never fatal by itself.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SkippedTag {
    pub name: String,
    pub version: String,
    pub reason: String,
}

/// Valid releases in version order plus tags that were skipped.
#[derive(Clone, Debug, Default)]
pub struct Discovery {
    pub tags: Vec<Tag>,
    pub skipped: Vec<SkippedTag>,
}

pub fn discover(repo: &Path, expected_id: Option<&str>) -> Result<Vec<Tag>> {
    Ok(discover_all(repo, expected_id)?.tags)
}

/// Release discovery that tolerates broken historical tags: a tag whose manifest is
/// unreadable or disagrees with the tag version is skipped and reported. Two different
/// commits for one version stay a hard conflict.
pub fn discover_all(repo: &Path, expected_id: Option<&str>) -> Result<Discovery> {
    let default_id;
    let id = match expected_id {
        Some(id) => id,
        None if packages::config_at(repo, "HEAD")?.is_some() => {
            let head = git(repo, &["rev-parse", "HEAD"])?;
            default_id = packages::find_at(repo, &head, None)?.manifest.id;
            &default_id
        }
        None => {
            let head: Manifest = serde_json::from_str(&git(repo, &["show", "HEAD:plugin.json"])?)
                .map_err(|e| {
                Error::InvalidManifest(format!("invalid plugin.json at repository HEAD: {e}"))
            })?;
            default_id = head.id;
            &default_id
        }
    };
    valid_id(id)?;
    // Package sets per commit: several tags (of several packages) can share a commit.
    let mut layouts: BTreeMap<String, Result<Option<packages::Layout>>> = BTreeMap::new();
    let mut releases = BTreeMap::new();
    let mut skipped = Vec::new();
    for name in git(repo, &["tag", "--list"])?.lines() {
        let version = name
            .strip_prefix(&format!("{id}@"))
            .or_else(|| name.strip_prefix('v'));
        let Some(version) = version else { continue };
        let Ok(key) = version_key(version) else {
            continue;
        };
        let mut skip = |reason: String| {
            skipped.push(SkippedTag {
                name: name.into(),
                version: version.into(),
                reason,
            })
        };
        let reference = format!("refs/tags/{name}");
        let commit = resolve_ref(repo, &reference)?;
        let layout = layouts
            .entry(commit.clone())
            .or_insert_with(|| packages::layout_at(repo, &commit))
            .clone();
        let (manifest, path) = match layout {
            Ok(None) => {
                match git(repo, &["show", &format!("{commit}:plugin.json")]).and_then(|text| {
                    serde_json::from_str(&text).map_err(|e| Error::InvalidManifest(e.to_string()))
                }) {
                    Ok(manifest) => (manifest, String::new()),
                    Err(e) => {
                        skip(format!("invalid manifest in release tag {name}: {e}"));
                        continue;
                    }
                }
            }
            Ok(Some(layout)) => {
                let scoped = name.starts_with(&format!("{id}@"));
                let claimed = layout.packages.iter().any(|p| p.manifest.id == id)
                    || layout.problems.iter().any(|p| p.id.as_deref() == Some(id));
                if !scoped {
                    // `vVERSION` cannot say which package of the repository it releases.
                    if claimed {
                        skip(format!(
                            "legacy tag {name} is ambiguous in a repository with several packages; tag {id}@{version} instead"
                        ));
                    }
                    continue;
                }
                // Only this package's own folder matters; defective siblings do not.
                if let Some(problem) = layout.problems.iter().find(|p| {
                    p.id.as_deref()
                        .is_some_and(|other| other.eq_ignore_ascii_case(id))
                }) {
                    skip(format!(
                        "package {id} at tag {name} is not usable: {}",
                        problem.error
                    ));
                    continue;
                }
                match layout.packages.into_iter().find(|p| p.manifest.id == id) {
                    Some(package) => (package.manifest, package.path),
                    None => {
                        skip(format!(
                            "tag {name} points to commit {} without package {id}",
                            &commit[..commit.len().min(12)]
                        ));
                        continue;
                    }
                }
            }
            Err(e) => {
                skip(format!("invalid packages in release tag {name}: {e}"));
                continue;
            }
        };
        // Forks retain upstream tags. They are not releases of the fork.
        if manifest.id != id {
            if name.starts_with(&format!("{id}@")) {
                skip(format!(
                    "tag {name} disagrees with manifest ID {}",
                    manifest.id
                ));
            }
            continue;
        }
        if manifest.version != version {
            skip(format!(
                "tag {name} disagrees with manifest version {}",
                manifest.version
            ));
            continue;
        }
        if let Some(old) = releases.get(&key) {
            let old: &Tag = old;
            ensure!(
                old.commit == commit,
                VersionConflict,
                "conflicting release tags for {id}@{version}"
            );
        } else {
            releases.insert(
                key,
                Tag {
                    name: name.into(),
                    commit,
                    manifest,
                    path,
                },
            );
        }
    }
    ensure!(
        !releases.is_empty(),
        NoReleases,
        "no release tags for {id}; create vVERSION or {id}@VERSION, or use the dev channel{}",
        skipped
            .iter()
            .map(|s| format!("; skipped {}", s.reason))
            .collect::<String>()
    );
    Ok(Discovery {
        tags: releases.into_values().collect(),
        skipped,
    })
}

impl Discovery {
    /// Fails if a skipped tag is newer than every valid release: that broken tag is the
    /// one a caller would select, so it must not silently fall back to an older version.
    pub fn ensure_latest_is_valid(&self) -> Result<()> {
        let latest = match self.tags.last() {
            Some(tag) => version_key(&tag.manifest.version)?,
            None => return Err(Error::NoReleases("no releases".into())),
        };
        if let Some(newer) = self
            .skipped
            .iter()
            .filter(|s| version_key(&s.version).is_ok_and(|key| key > latest))
            .max_by_key(|s| version_key(&s.version).ok())
        {
            return Err(Error::VersionConflict(format!(
                "latest release tag is broken: {}",
                newer.reason
            )));
        }
        Ok(())
    }
}

/// The newest release; see [`Discovery::ensure_latest_is_valid`].
pub fn latest(repo: &Path, expected_id: Option<&str>) -> Result<Tag> {
    let mut found = discover_all(repo, expected_id)?;
    found.ensure_latest_is_valid()?;
    found
        .tags
        .pop()
        .ok_or_else(|| Error::NoReleases("no releases".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{Author, TempDir, git, manifest, write_manifest};
    use std::fs;

    fn names(tags: &[Tag]) -> Vec<&str> {
        tags.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn scoped_tags_are_discovered_in_numeric_order() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        let c19 = author.release("1.9.0");
        let c110 = author.release("1.10.0");
        author.release("1.2.0");
        let tags = discover(&author.path, None).unwrap();
        assert_eq!(
            names(&tags),
            [
                "dan/demo1@0.1.0",
                "dan/demo1@1.2.0",
                "dan/demo1@1.9.0",
                "dan/demo1@1.10.0"
            ]
        );
        assert_eq!(tags[2].commit, c19);
        assert_eq!(tags[3].manifest.version, "1.10.0");
        let latest = latest(&author.path, None).unwrap();
        assert_eq!(
            (latest.name.as_str(), latest.commit),
            ("dan/demo1@1.10.0", c110)
        );
    }

    #[test]
    fn legacy_v_tags_and_scoped_tags_of_the_same_release_are_merged() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "demo1");
        assert_eq!(names(&discover(&author.path, None).unwrap()), ["v0.1.0"]);
        // A scoped plugin may carry both spellings for one commit.
        let scoped = Author::new(dir.join("scoped"), "dan/demo1");
        git(&scoped.path, &["tag", "v0.1.0"]);
        let tags = discover(&scoped.path, None).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].commit, scoped.head());
    }

    #[test]
    fn non_release_and_foreign_tags_are_ignored() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "me/demo1");
        for tag in [
            "v1.0",
            "v1.0.0-beta",
            "v1.0.0+build",
            "v01.0.0",
            "release-2.0.0",
            "me/demo1@2.0",
            "me/demo1@latest",
            "vendor/demo1@9.0.0",
            "me/demo10@9.0.0",
        ] {
            git(&author.path, &["tag", tag]);
        }
        assert_eq!(
            names(&discover(&author.path, None).unwrap()),
            ["me/demo1@0.1.0"]
        );
    }

    #[test]
    fn upstream_tags_retained_by_a_fork_are_not_fork_releases() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "vendor/demo1");
        author.release("0.2.0");
        // The fork changes the ID; the upstream `v`-style tag on the same history must be skipped.
        git(&author.path, &["tag", "v0.2.0"]);
        write_manifest(&author.path, &manifest("me/demo1", "0.3.0"));
        git(&author.path, &["commit", "-q", "-am", "fork"]);
        git(&author.path, &["tag", "me/demo1@0.3.0"]);
        let tags = discover(&author.path, None).unwrap();
        assert_eq!(names(&tags), ["me/demo1@0.3.0"]);
        // The upstream ID can still be asked for explicitly.
        let upstream = discover(&author.path, Some("vendor/demo1")).unwrap();
        assert_eq!(names(&upstream), ["vendor/demo1@0.1.0", "v0.2.0"]);
    }

    #[test]
    fn tag_disagreeing_with_manifest_version_is_a_conflict() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        git(&author.path, &["tag", "dan/demo1@0.2.0"]);
        // Discovery skips the broken tag and reports it ...
        let found = discover_all(&author.path, None).unwrap();
        assert_eq!(names(&found.tags), ["dan/demo1@0.1.0"]);
        assert_eq!(found.skipped.len(), 1);
        assert_eq!(found.skipped[0].name, "dan/demo1@0.2.0");
        assert!(
            found.skipped[0]
                .reason
                .contains("disagrees with manifest version")
        );
        // ... but it would be the latest release, so selecting latest fails.
        let error = latest(&author.path, None).unwrap_err();
        assert_eq!(error.kind(), "version_conflict");
        assert!(error.to_string().contains("dan/demo1@0.2.0"));
    }

    #[test]
    fn same_version_on_two_commits_is_a_conflict() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        // Second commit with the same manifest version, tagged with the legacy spelling.
        fs::write(author.path.join("dist/index.js"), "changed").unwrap();
        git(&author.path, &["commit", "-q", "-am", "same version"]);
        git(&author.path, &["tag", "v0.1.0"]);
        let error = discover(&author.path, None).unwrap_err();
        assert_eq!(error.kind(), "version_conflict");
        assert!(error.to_string().contains("conflicting release tags"));
    }

    #[test]
    fn no_release_tags_and_broken_head_manifest() {
        let dir = TempDir::new("releases");
        let author = Author::untagged(dir.join("repo"), "dan/demo1", "0.1.0");
        let error = discover(&author.path, None).unwrap_err();
        assert_eq!(error.kind(), "no_releases");
        assert!(error.to_string().contains("dan/demo1@VERSION"));
        assert_eq!(
            latest(&author.path, None).unwrap_err().kind(),
            "no_releases"
        );
        assert_eq!(
            discover(&author.path, Some("../x")).unwrap_err().kind(),
            "invalid_id"
        );

        fs::write(author.path.join("plugin.json"), "{not json").unwrap();
        git(&author.path, &["commit", "-q", "-am", "broken"]);
        assert_eq!(
            discover(&author.path, None).unwrap_err().kind(),
            "invalid_manifest"
        );
        // Not a repository at all.
        assert_eq!(
            discover(&dir.join("missing"), None).unwrap_err().kind(),
            "git"
        );
    }

    #[test]
    fn bad_historical_tag_does_not_hide_valid_releases() {
        let dir = TempDir::new("releases");
        let author = Author::new(dir.join("repo"), "dan/demo1");
        // An old commit with a broken manifest got a release-looking tag.
        fs::write(author.path.join("plugin.json"), "{broken").unwrap();
        git(&author.path, &["commit", "-q", "-am", "broken"]);
        git(&author.path, &["tag", "v0.0.1"]);
        let good = author.release("0.2.0");
        let tags = discover(&author.path, None).expect("valid releases stay discoverable");
        assert_eq!(names(&tags), ["dan/demo1@0.1.0", "dan/demo1@0.2.0"]);
        assert_eq!(latest(&author.path, None).unwrap().commit, good);
        let found = discover_all(&author.path, None).unwrap();
        assert_eq!(found.skipped.len(), 1);
        assert_eq!(found.skipped[0].name, "v0.0.1");
    }
}
