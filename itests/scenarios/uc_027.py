"""UC-027: Register and immutable catalog releases (P0).

Error messages name the package ID and version.
"""

import hashlib
import json
import subprocess

from gitpmtest import T, case

ID = "dan/hello"


def committed(repo, rev: str, path: str) -> dict:
    data = subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{rev}:{path}"],
                          capture_output=True, check=True).stdout
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def release(t: T, version: str) -> dict:
    return json.loads(t.catalog_release(ID, version).read_text())


@case(fixture="hello", spec=["catalog/register-one-version", "catalog/release-record"])
def register_into_new_catalog(t: T):
    """1. A committed scoped tag registered into a new local catalog: exact commit,
    manifest, reference and committed file hashes; nothing installed."""
    t.ok(t.catalog_empty(), "no catalog yet")
    out = t.gitpm_ok("register", t.repo(ID), "--ref", f"{ID}@0.1.0").json()
    commit = t.rev(ID, f"{ID}@0.1.0")
    stored = release(t, "0.1.0")
    t.eq(out, stored, "printed release == stored release.json")
    t.eq(stored["commit"], commit, "commit")
    t.eq(stored["reference"], f"{ID}@0.1.0", "reference")
    t.eq(stored["source"], str(t.repo(ID)), "source (canonical path)")
    t.eq(stored["manifest"]["id"], ID, "manifest id")
    t.eq(stored["manifest"]["version"], "0.1.0", "manifest version")
    t.eq(stored["files"], {"index.js": committed(t.repo(ID), commit, "dist/index.js")}, "file hashes")
    t.ok("build" not in stored, "no build metadata")
    index = json.loads(t.catalog_index(ID).read_text())
    t.eq(index, {"schema": 2, "id": ID, "source": str(t.repo(ID)),
                 "versions": {"0.1.0": "versions/0.1.0/release.json"}}, "package index")
    t.ok(not t.ws.exists(), "register installs nothing")


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable"])
def identical_registration_is_safe(t: T):
    """2. Repeating the same registration (same tag, or the commit SHA of it) succeeds and
    changes no catalog byte."""
    before = t.snap(t.catalog)
    t.gitpm_ok("register", t.repo(ID), "--ref", f"{ID}@0.1.1")
    t.same(before, "catalog after identical re-registration")
    t.gitpm_ok("register", t.repo(ID), "--ref", t.rev(ID, f"{ID}@0.1.1"))
    t.same(before, "catalog after re-registration by commit SHA")
    t.ok(not t.ws.exists(), "nothing installed")


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable", "errors/errors-name-the-package"])
def different_commit_same_version_conflicts(t: T):
    """3. Another commit under the same ID/version (moved tag) is refused; the existing
    release is not overwritten and nothing is installed."""
    before = t.snap(t.catalog)
    old = release(t, "0.1.1")
    t.commit(ID, "rewritten 0.1.1", {"dist/index.js": "rewritten\n"})
    t.tag(ID, f"{ID}@0.1.1", force=True)
    r = t.gitpm_err("register", t.repo(ID), "--ref", f"{ID}@0.1.1", kind="version_conflict", contains="immutable")
    t.same(before, "catalog")
    t.eq(release(t, "0.1.1"), old, "stored release")
    t.ok(not t.ws.exists(), "nothing installed")
    t.has(r.err, ID)  # error messages name the package ID (product decision)


@case(fixture="catalog", spec=["catalog/published-versions-are-immutable", "errors/errors-name-the-package"])
def other_source_same_id_conflicts(t: T):
    """4. The same ID from another source (a mirror clone) is refused, even for a new
    version; the catalog is unchanged."""
    t.release(ID, "0.3.0")
    mirror = t.root / "mirror/hello"
    t.run("git", "clone", "-q", t.repo(ID), mirror, check=True)
    before = t.snap(t.catalog)
    errors = []
    for ref in [f"{ID}@0.3.0", f"{ID}@0.1.0"]:
        errors.append(t.gitpm_err("register", mirror, "--ref", ref, kind="catalog", contains="different source").err)
        t.same(before, f"catalog after refused {ref}")
    t.ok(not t.ws.exists(), "nothing installed")
    t.has(errors[0], ID)  # error messages name the package ID (product decision)


@case(fixture="catalog", spec=["catalog/release-record"])
def uncommitted_dist_does_not_affect_hashes(t: T):
    """5. Dirty (uncommitted) dist in the author source: a new committed version is
    registered with hashes of the committed content only; older releases untouched."""
    commit = t.release(ID, "0.3.0", files={"dist/extra.js": "committed extra\n"})
    t.write(ID, {"dist/index.js": "UNCOMMITTED EDIT\n", "dist/extra.js": "dirty\n", "dist/untracked.js": "x\n"})
    others = t.snap(t.catalog / "namespaces/dan/hello/versions", t.catalog / "namespaces/dan/num")
    t.gitpm_ok("register", t.repo(ID), "--ref", f"{ID}@0.3.0")
    stored = release(t, "0.3.0")
    t.eq(stored["commit"], commit, "commit")
    t.eq(stored["files"], {"index.js": committed(t.repo(ID), commit, "dist/index.js"),
                           "extra.js": committed(t.repo(ID), commit, "dist/extra.js")}, "committed hashes only")
    t.eq(sorted(json.loads(t.catalog_index(ID).read_text())["versions"]), ["0.1.0", "0.1.1", "0.2.0", "0.3.0"],
         "versions")
    t.has(t.git(ID, "status", "--porcelain"), "dist/index.js", "dist/untracked.js")
    after = t.snap(t.catalog / "namespaces/dan/hello/versions", t.catalog / "namespaces/dan/num")
    t.eq({k: v for k, v in after.items() if "/0.3.0/" not in k}, dict(others), "older releases unchanged")
    t.ok(not t.ws.exists(), "nothing installed")
