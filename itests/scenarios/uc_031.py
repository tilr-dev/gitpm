"""UC-031: Catalog generation from sources (P0).

Fixture g4_sources: /fx/lists/sources.txt lists 3dce1568-569c-5b53-9321-37185e751947 three times (relative, trailing
slash, absolute) with comments and blank lines, and "../spaced repos/2bf18134-a7da-5c98-841c-4ee91fa6e90e" (a path
with a space; lightweight 1.0.0, annotated 1.1.0, untagged HEAD). 3dce1568-569c-5b53-9321-37185e751947 has 0.1.0,
0.1.1, 0.2.0 and an untagged HEAD. catalog-update runs from an unrelated cwd.
"""

import hashlib
import json
import subprocess

from gitpmtest import T, case

SPACED = "spaced repos/2bf18134-a7da-5c98-841c-4ee91fa6e90e"
EXPECTED = {"3dce1568-569c-5b53-9321-37185e751947": ["0.1.0", "0.1.1", "0.2.0"], "2bf18134-a7da-5c98-841c-4ee91fa6e90e": ["1.0.0", "1.1.0"]}


def repo_of(t: T, id: str):
    return t.repo(id) if id == "3dce1568-569c-5b53-9321-37185e751947" else t.root / SPACED


def committed_files(repo, rev: str) -> dict:
    names = subprocess.run(["git", "-C", str(repo), "ls-tree", "-r", "-z", "--name-only", rev, "dist/"],
                           capture_output=True, check=True).stdout.decode().split("\0")
    out = {}
    for name in filter(None, names):
        data = subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{rev}:{name}"],
                              capture_output=True, check=True).stdout
        out[name[len("dist/"):]] = {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
    return out


def generate(t: T):
    elsewhere = t.root / "elsewhere"
    elsewhere.mkdir(exist_ok=True)
    return t.gitpm_ok("--catalog", t.catalog, "catalog-update", "--sources", t.root / "lists/sources.txt", cwd=elsewhere)


@case(fixture="g4_sources", spec=["catalog/catalog-generated-from-git-release-tags#two-releases",
                                 "catalog/release-record", "catalog/catalog-generated-from-git-release-tags#duplicate-source-lines"])
def all_releases_indexed_once(t: T):
    """Every valid release tag of both sources is indexed exactly once at its tag commit
    (annotated tags peeled), with committed hashes; untagged HEADs are not releases; paths
    resolve relative to sources.txt (not the cwd); author repos are not pushed to/changed."""
    authors = t.snap(t.repo("3dce1568-569c-5b53-9321-37185e751947"), t.root / SPACED, git=True)
    out = generate(t)
    t.has(out.out, "Catalog ready: 5 release(s)")
    t.has(out.err, f"Reading releases: {t.repo('3dce1568-569c-5b53-9321-37185e751947')}", f"Reading releases: {t.root / SPACED}")
    t.eq(out.err.count("Reading releases:"), 2, "each source read once (duplicates merged)")
    for id, versions in EXPECTED.items():
        repo = repo_of(t, id)
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(index["source"], str(repo), f"{id} source: canonical path relative to sources.txt")
        t.eq(index["versions"], {v: f"versions/{v}/release.json" for v in versions}, f"{id} versions")
        for v in versions:
            release = json.loads(t.catalog_release(id, v).read_text())
            commit = t.rev(repo, f"v{v}")
            t.eq(release["commit"], commit, f"{id} {v} commit (peeled)")
            t.eq(release["reference"], f"v{v}", f"{id} {v} reference")
            t.eq(release["files"], committed_files(repo, commit), f"{id} {v} committed hashes")
            t.ok("build" not in release, f"{id} {v} no build")
    t.ok(not (t.root / "elsewhere/catalog.json").exists() and not any((t.root / "elsewhere").iterdir()),
         "nothing written to the cwd")
    t.same(authors, "author repositories (no push, no build, no new refs)")
    t.ok(not t.ws.exists(), "nothing installed")


@case(fixture="g4_sources", spec=["catalog/published-versions-are-immutable#regenerate"])
def repeat_is_safe(t: T):
    """A second run with unchanged sources leaves every catalog byte unchanged."""
    generate(t)
    before = t.snap(t.catalog)
    out = generate(t)
    t.has(out.out, "Catalog ready: 5 release(s)")
    t.same(before, "catalog after repeated catalog-update")


@case(fixture="g4_sources", spec=["catalog/catalog-generated-from-git-release-tags",
                                 "cli/settings-come-from-flags-environment-defaults"])
def relative_paths_use_sources_location_not_cwd(t: T):
    """Run from a cwd where the relative entries would resolve elsewhere: still the same
    repositories (relative to the sources file); the file is given by GITPM_SOURCES."""
    decoy = t.root / "66e90411-37e5-5ec1-85d1-972659d7bcd3"
    decoy.mkdir(parents=True)
    out = t.gitpm_ok("--catalog", t.catalog, "catalog-update", cwd=decoy,
                  env={"GITPM_SOURCES": str(t.root / "lists/sources.txt")})
    t.has(out.out, "Catalog ready: 5 release(s)")
    for id in EXPECTED:
        t.eq(json.loads(t.catalog_index(id).read_text())["source"], str(repo_of(t, id)), f"{id} source")
