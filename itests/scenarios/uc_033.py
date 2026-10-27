"""UC-033: Catalog tag identity and version rules (P0).

Fixture numeric: 2190eee8-4be5-5a91-81d2-6ca95c1e61d0 with releases 1.9.0 and 1.10.0 (tags vVERSION).
"""

import json

from gitpmtest import T, case

ID = "2190eee8-4be5-5a91-81d2-6ca95c1e61d0"


def sources(t: T, *repos) -> str:
    path = t.root / "sources.txt"
    path.write_text("".join(f"{r}\n" for r in repos))
    return str(path)


def versions(t: T, id: str = ID) -> list[str]:
    return sorted(json.loads(t.catalog_index(id).read_text())["versions"])


def side_commit(t: T, manifest_version: str, tag: str, start: str = f"v1.9.0") -> str:
    """A commit on a side branch whose manifest says manifest_version, tagged `tag`."""
    t.git(ID, "checkout", "-q", "-b", f"side-{tag.replace('/', '-').replace('@', '-')}", start)
    sha = t.release(ID, manifest_version, tag=False)
    t.tag(ID, tag)
    t.git(ID, "checkout", "-q", "main")
    return sha


@case(fixture="numeric", spec=["package-ids/version-format#numeric-ordering", "catalog/catalog-generated-from-git-release-tags"])
def numeric_latest(t: T):
    """1. Stable install and catalog-update pick 1.10.0 (numeric, not lexical)."""
    t.gitpm_ok("install", t.repo(ID))
    t.eq(t.package(ID)["current"], {"version": "1.10.0", "commit": t.rev(ID, f"v1.10.0")}, "installed")
    t.gitpm_ok("catalog-update", "--sources", sources(t, t.repo(ID)))
    t.eq(versions(t), ["1.10.0", "1.9.0"], "catalog versions")
    info = t.gitpm_ok("--catalog", t.catalog, "info").out
    t.has(info, f"{ID} — latest: 1.10.0, versions: 2")


@case(fixture="numeric", spec=["package-ids/broken-historical-tags-are-skipped#one-bad-old-tag", "package-ids/release-tag-names#tag-disagrees-with-the-manifest"])
def old_mismatched_tag_is_skipped(t: T):
    """2. Old tag v0.0.9 on a commit whose manifest says 0.1.0: install and
    catalog-update succeed, warn naming the tag, and publish only valid releases."""
    side_commit(t, "0.1.0", f"v0.0.9")
    r = t.gitpm_ok("install", t.repo(ID))
    t.has(r.err, f"v0.0.9", "disagrees with manifest version 0.1.0")
    t.eq(t.package(ID)["current"]["version"], "1.10.0", "installed latest valid")
    r = t.gitpm_ok("catalog-update", "--sources", sources(t, t.repo(ID)))
    t.has(r.err, f"Warning: {t.repo(ID)}: tag v0.0.9 disagrees with manifest version 0.1.0")
    t.has(r.out, "Catalog ready: 2 release(s)")
    t.eq(versions(t), ["1.10.0", "1.9.0"], "catalog versions")
    t.ok(not t.catalog_release(ID, "0.0.9").exists() and not t.catalog_release(ID, "0.1.0").exists(),
         "no record for the broken tag")


@case(fixture="numeric", spec=["package-ids/release-tag-names#tag-disagrees-with-the-manifest", "package-ids/broken-historical-tags-are-skipped"])
def broken_tag_newer_than_valid(t: T):
    """3. Tag v9.0.0 on the 1.10.0 commit: stable install and catalog-update into a
    fresh catalog fail (version_conflict, "latest release tag is broken"); nothing written."""
    t.tag(ID, f"v9.0.0", f"v1.10.0")
    t.gitpm_err("install", t.repo(ID), kind="version_conflict", contains="latest release tag is broken")
    t.not_installed(ID)
    t.gitpm_err("catalog-update", "--sources", sources(t, t.repo(ID)),
             kind="version_conflict", contains="latest release tag is broken")
    t.ok(t.catalog_empty(), "fresh catalog not created")


@case(fixture="numeric", spec=["catalog/catalog-generated-from-git-release-tags", "package-ids/release-tag-names#vversion-tag-of-a-scoped-single-package-repository"])
def fork_ignores_original_tags(t: T):
    """4. A fork into namespace `me` keeps upstream repository tags; catalog-update of the fork source
    publishes only 0295f5b0-82f5-5067-b4d8-f2726a3c401d releases."""
    t.gitpm_ok("install", t.repo(ID))
    fork = t.root / "71d98915-a8ea-558c-9234-5ff81b3dd5a7"
    t.gitpm_ok("fork", ID, "--into", fork)
    fork_id = t.manifest_id(fork)
    t.has(t.git(fork, "tag", "--list"), f"v1.9.0", f"v1.10.0")
    t.gitpm_ok("commit", fork, "2.0.0")
    r = t.gitpm_ok("catalog-update", "--sources", sources(t, fork))
    t.has(r.out, "Catalog ready: 1 release(s)")
    t.eq(versions(t, fork_id), ["2.0.0"], "fork releases")
    t.eq(json.loads(t.catalog_release(fork_id, "2.0.0").read_text())["commit"], t.rev(fork, "v2.0.0"),
         "fork release commit")
    t.ok(not t.catalog_index(ID).exists(), "original ID not published from the fork source")
    t.lacks(r.err, f"v1.9.0", f"v1.10.0")


@case(fixture="numeric", spec=["package-ids/release-tag-names#pre-release-tag", "package-ids/release-tag-names#tags-that-are-not-release-versions"])
def prerelease_build_and_named_tags_ignored(t: T):
    """5. v2.0.0-beta is a release (a pre-release); v2.0.0+b1 and vlatest
    are not: install picks 1.10.0 (never a pre-release), the catalog has 1.9.0, 1.10.0 and
    2.0.0-beta."""
    side_commit(t, "2.0.0-beta", f"v2.0.0-beta", f"v1.10.0")
    side_commit(t, "2.0.0+b1", f"v2.0.0+b1", f"v1.10.0")
    side_commit(t, "3.0.0", "vlatest", f"v1.10.0")
    r = t.gitpm_ok("install", t.repo(ID))
    t.eq(t.package(ID)["current"]["version"], "1.10.0", "installed")
    r2 = t.gitpm_ok("catalog-update", "--sources", sources(t, t.repo(ID)))
    t.has(r2.out, "Catalog ready: 3 release(s)")
    t.eq(versions(t), ["1.10.0", "1.9.0", "2.0.0-beta"], "catalog versions")
    for text in [r.err, r2.err]:
        t.lacks(text, "2.0.0+b1", "vlatest")
    # The pre-release is installed only by name.
    t.gitpm_ok("--root", t.root / "ws-beta", "install-catalog", ID, "--version", "2.0.0-beta")
    t.eq(t.gitpm_ok("--root", t.root / "ws-beta", "list").json()["packages"][ID]["current"]["version"],
         "2.0.0-beta", "explicit pre-release")


@case(fixture="numeric", spec=["package-ids/version-format"])
def two_part_version_refused(t: T):
    """6. A manifest with "version": "1.0" installed via --ref is refused (invalid_version)."""
    t.new_package("8553ec2d-a4e8-576e-b0a5-74081dc68ec6", "1.0")
    t.gitpm_err("install", t.repo("8553ec2d-a4e8-576e-b0a5-74081dc68ec6"), "--ref", "v1.0", kind="invalid_version")
    t.not_installed("8553ec2d-a4e8-576e-b0a5-74081dc68ec6")
