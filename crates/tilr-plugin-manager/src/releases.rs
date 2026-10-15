//! Release tags are the shared source of truth for installs and catalog generation.
use crate::catalog::version_key;
use crate::error::{Error, Result, ensure};
use crate::{Manifest, exec::git, model::resolve_ref, valid_id};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Debug)]
pub struct Tag {
    pub name: String,
    pub commit: String,
    pub manifest: Manifest,
}

pub fn discover(repo: &Path, expected_id: Option<&str>) -> Result<Vec<Tag>> {
    let head: Manifest =
        serde_json::from_str(&git(repo, &["show", "HEAD:plugin.json"])?).map_err(|e| {
            Error::InvalidManifest(format!("invalid plugin.json at repository HEAD: {e}"))
        })?;
    let id = expected_id.unwrap_or(&head.id);
    valid_id(id)?;
    let mut releases = BTreeMap::new();
    for name in git(repo, &["tag", "--list"])?.lines() {
        let version = name
            .strip_prefix(&format!("{id}@"))
            .or_else(|| name.strip_prefix('v'));
        let Some(version) = version else { continue };
        let Ok(key) = version_key(version) else {
            continue;
        };
        let reference = format!("refs/tags/{name}");
        let commit = resolve_ref(repo, &reference)?;
        let manifest: Manifest =
            serde_json::from_str(&git(repo, &["show", &format!("{commit}:plugin.json")])?)
                .map_err(|e| {
                    Error::InvalidManifest(format!("invalid manifest in release tag {name}: {e}"))
                })?;
        // Forks retain upstream tags. They are not releases of the fork.
        if manifest.id != id {
            continue;
        }
        ensure!(
            manifest.version == version,
            VersionConflict,
            "tag {name} disagrees with manifest version {}",
            manifest.version
        );
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
                },
            );
        }
    }
    ensure!(
        !releases.is_empty(),
        NoReleases,
        "no release tags for {id}; create vVERSION or {id}@VERSION, or use --channel dev"
    );
    Ok(releases.into_values().collect())
}

pub fn latest(repo: &Path, expected_id: Option<&str>) -> Result<Tag> {
    discover(repo, expected_id)?
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
        let error = discover(&author.path, None).unwrap_err();
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
    #[ignore = "known defect: one bad historical tag breaks release discovery"]
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
    }
}
