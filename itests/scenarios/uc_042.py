"""UC-042: Author journey (create → release → publish to a local catalog) (P0).

gitpm writes catalogs only to local folders; publishing them is the author's job, so the
journey ends at the local catalog.
"""

import hashlib
import json
import subprocess
from pathlib import Path

from gitpmtest import T, case

REPO = Path("/fx/authors/journey")
ID = "dan/journey"


def blob(repo: Path, rev: str, path: str) -> bytes:
    return subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{rev}:{path}"],
                          capture_output=True, check=True).stdout


def canonical(t: T, text: str, id: str, version: str) -> dict:
    m = json.loads(text)
    t.eq(text, json.dumps(m, indent=2, ensure_ascii=False) + "\n", f"{id} {version} manifest is canonical JSON")
    t.eq(list(m)[:3], ["id", "version", "dependencies"], "canonical key order")
    t.eq((m["id"], m["version"]), (id, version), "manifest identity")
    return m


def released(t: T, repo: Path, id: str, version: str, before: str | None, dist: bytes | None = None):
    """Exactly one new commit on top of `before`, tagged vVERSION, clean tree."""
    head = t.rev(repo)
    tag = f"v{version}"
    t.eq(t.rev(repo, tag), head, f"tag {tag} on HEAD")
    if before is None:
        t.eq(t.git(repo, "rev-list", "--count", "HEAD"), "1", "init made one commit")
    else:
        t.eq(t.git(repo, "rev-list", "--count", f"{before}..HEAD"), "1", "exactly one new commit")
        t.eq(t.rev(repo, "HEAD~1"), before, "parent is the previous HEAD")
    t.eq(t.git(repo, "tag", "--points-at", "HEAD"), tag, "exactly one tag on the release commit")
    t.eq(t.git(repo, "status", "--porcelain", "--ignored"), "", "working tree clean")
    canonical(t, blob(repo, "HEAD", "plugin.json").decode(), id, version)
    if dist is not None:
        t.eq(blob(repo, "HEAD", "dist/index.js"), dist, "edited dist is in the release commit")


def digest(data: bytes) -> dict:
    return {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}


def release_json(t: T, id: str, version: str) -> dict:
    return json.loads(t.catalog_release(id, version).read_text())


def check_release(t: T, repo: Path, id: str, version: str):
    r = release_json(t, id, version)
    tag = f"v{version}"
    t.eq(r["commit"], t.rev(repo, tag), f"{tag} commit")
    t.eq(r["reference"], tag, f"{tag} reference")
    t.eq(r["manifest"], json.loads(blob(repo, tag, "plugin.json")), f"{tag} manifest")
    t.eq(r["files"], {"index.js": digest(blob(repo, tag, "dist/index.js"))}, f"{tag} file hashes")


@case(spec=["authoring/init-creates-a-package-with-its-first-release", "authoring/commit-releases-a-version"])
def each_commit_creates_one_commit_and_tag(t: T):
    """init NS/NAME --into DIR → 0.1.0; edit dist + commit 0.1.1; edit + commit 0.1.2."""
    t.has(t.gitpm_ok("init", ID, "--into", REPO).out, f"ID: {ID}", f"v0.1.0")
    released(t, REPO, ID, "0.1.0", None)
    t.eq(t.git(REPO, "branch", "--show-current"), "main", "branch main")
    for version, before in [("0.1.1", t.rev(REPO)), ("0.1.2", None)]:
        before = before or t.rev(REPO)
        dist = f"export const greeting = 'journey {version}';\n".encode()
        t.write(REPO, {"dist/index.js": dist})
        t.has(t.gitpm_ok("commit", REPO, version).out, f"{ID} committed {version} (local commit and tag v{version})")
        released(t, REPO, ID, version, before, dist)
    t.eq(sorted(t.git(REPO, "tag", "--list").split()), [f"v0.1.0", f"v0.1.1", f"v0.1.2"], "all tags")


@case(fixture="g5_journey", spec=["catalog/published-versions-are-immutable#regenerate",
                                 "catalog/catalog-generated-from-git-release-tags"])
def catalog_update_is_idempotent_and_keeps_old_releases(t: T):
    for v in ["0.1.0", "0.1.1"]:
        check_release(t, REPO, ID, v)
    before = t.snap(t.catalog)
    t.gitpm_ok("catalog-update")
    t.same(before, "catalog after a run without changes")
    old = {v: t.catalog_release(ID, v).read_bytes() for v in ["0.1.0", "0.1.1"]}
    t.write(REPO, {"dist/index.js": "export const greeting = 'journey 0.1.2';\n"})
    t.gitpm_ok("commit", REPO, "0.1.2")
    t.gitpm_ok("catalog-update")
    check_release(t, REPO, ID, "0.1.2")
    for v, data in old.items():
        t.ok(t.catalog_release(ID, v).read_bytes() == data, f"release {v} byte-identical")
    index = json.loads(t.catalog_index(ID).read_text())
    t.eq(index["versions"], {v: f"versions/{v}/release.json" for v in ["0.1.0", "0.1.1", "0.1.2"]}, "index versions")
    after = t.snap(t.catalog)
    changed = sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k))
    t.eq(changed, [str(t.catalog_index(ID)), str(t.catalog_release(ID, "0.1.2"))], "only the index and the new release changed")


@case(fixture="g5_journey", spec=["package-manifest/forkof-is-provenance-replaces-is-an-offer",
                                 "catalog/catalog-generated-from-git-release-tags"])
def fork_release_gets_its_own_catalog_entry(t: T):
    scratch = t.root / "scratch"
    t.gitpm_ok("--root", scratch, "install", REPO)
    base = t.rev(REPO, f"v0.1.1")
    fork = t.root / "authors/fork-journey"
    t.gitpm_ok("--root", scratch, "fork", ID, "--ns", "mine", "--into", fork)
    m = json.loads((fork / "plugin.json").read_text())
    t.eq(m["id"], "mine/journey", "fork ID")
    t.eq(m["forkOf"], {"id": ID, "version": "0.1.1", "commit": base, "source": str(REPO)}, "forkOf")
    t.eq(m["replaces"], {"id": ID, "version": "0.1.1"}, "replaces")
    before_commit = t.rev(fork)
    dist = b"export const greeting = 'mine fork';\n"
    t.write(fork, {"dist/index.js": dist})
    t.gitpm_ok("commit", fork, "0.1.2")
    released(t, fork, "mine/journey", "0.1.2", before_commit, dist)
    fm = json.loads(blob(fork, "HEAD", "plugin.json"))
    t.eq((fm["forkOf"], fm["replaces"]), (m["forkOf"], m["replaces"]), "release keeps forkOf/replaces")
    original = t.snap(t.catalog / "namespaces/dan")
    with open(t.root / "sources.txt", "a") as f:
        f.write("authors/fork-journey\n")
    t.gitpm_ok("catalog-update")
    index = json.loads(t.catalog_index("mine/journey").read_text())
    t.eq(index["source"], str(fork), "fork entry source")
    t.eq(sorted(index["versions"]), ["0.1.2"], "fork has only its own release (old dan/journey tags ignored)")
    check_release(t, fork, "mine/journey", "0.1.2")
    r = release_json(t, "mine/journey", "0.1.2")
    t.eq((r["manifest"]["forkOf"], r["manifest"]["replaces"]), (m["forkOf"], m["replaces"]), "catalog keeps provenance")
    t.has((t.catalog / "namespaces/catalog.json").read_text(), '"mine": "mine/"')
    t.same(original, "original's catalog entry")
