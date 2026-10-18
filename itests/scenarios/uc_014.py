"""UC-014: Invalid manifests and missing payload (P0).

Direct cases use fixture hello_installed: dan/hello 0.1.0 is the healthy sentinel and the
whole workspace (with full .git of the clone) must stay byte-identical. Catalog cases use
fixture catalog with dan/num installed from it as the sentinel.
"""

import json
import os

from gitpmtest import T, case

SENTINEL = "dan/hello"
DIST = {"dist/index.js": "export const greeting = 'bad';\n"}


def bad_repo(t: T, name: str, files: dict, symlinks: dict | None = None) -> tuple:
    """A committed repository /fx/repos/bad/NAME with exactly these files; returns (path, sha)."""
    repo = t.root / "repos/bad" / name
    repo.mkdir(parents=True)
    t.git(repo, "init", "-q", "-b", "main")
    t.write(repo, files)
    for link, target in (symlinks or {}).items():
        (repo / link).parent.mkdir(parents=True, exist_ok=True)
        os.symlink(target, repo / link)
    t.commit(repo, f"bad fixture {name}")
    return repo, t.rev(repo)


def manifest(**fields) -> str:
    return json.dumps(fields) + "\n"


def refused(t: T, repo, *extra, kind=None, contains=()):
    before = t.snap(t.ws, git=True)
    r = t.gitpm_err("install", repo, *extra, kind=kind, contains=contains)
    t.same(before, "workspace with the sentinel")
    t.eq(t.package(SENTINEL)["current"]["version"], "0.1.0", "sentinel still installed")
    return r


@case(fixture="hello_installed", spec=["package-manifest/known-manifest-keys", "errors/failing-operations-leave-the-workspace-unchanged"])
def malformed_manifest(t: T):
    """Broken JSON in plugin.json: rejected by --ref and by latest-release discovery."""
    repo, sha = bad_repo(t, "malformed", {"plugin.json": "{ not json", **DIST})
    t.tag(repo, "bad/malformed@0.1.0")
    refused(t, repo, "--ref", sha, kind="invalid_manifest", contains="invalid plugin.json")
    refused(t, repo, kind="invalid_manifest", contains="plugin.json")


@case(fixture="hello_installed", spec=["package-manifest/package-folder-contents#missing-plugin-json"])
def missing_manifest(t: T):
    """No plugin.json at all."""
    repo, sha = bad_repo(t, "nomanifest", DIST)
    refused(t, repo, "--ref", sha, kind="invalid_manifest", contains="missing plugin.json")
    r = refused(t, repo)
    t.has(r.err, "plugin.json", "Error kind:")


@case(fixture="hello_installed", spec=["package-manifest/unknown-keys-are-preserved", "package-manifest/known-manifest-keys"])
def unknown_manifest_fields_and_missing_version(t: T):
    """Missing / empty version are invalid manifests; unknown fields are kept, not refused."""
    for name, text in [("noversion", manifest(id="bad/noversion")),
                       ("emptyversion", manifest(id="bad/emptyversion", version=" "))]:
        repo, sha = bad_repo(t, name, {"plugin.json": text, **DIST})
        refused(t, repo, "--ref", sha, kind="invalid_manifest")
        t.not_installed(f"bad/{name}")
    repo, sha = bad_repo(t, "extra", {"plugin.json": manifest(id="bad/extra", version="0.1.0", pinned=True), **DIST})
    t.gitpm_ok("install", repo, "--ref", sha)
    t.eq(t.package("bad/extra")["current"]["version"], "0.1.0", "installed version")
    t.eq(json.loads((t.clone("bad/extra") / "plugin.json").read_text())["pinned"], True, "unknown field kept")


@case(fixture="hello_installed", spec=["package-ids/package-id-syntax#unsafe-components-are-refused", "package-ids/package-id-syntax#empty-namespace"])
def invalid_id(t: T):
    """Traversal, three components and empty IDs: invalid_id; nothing created outside packages/."""
    for name, id in [("dotdot", "../evil"), ("deep", "a/b/c"), ("empty", ""), ("space", "bad/demo 1"),
                     ("abs", "/etc")]:
        repo, sha = bad_repo(t, name, {"plugin.json": manifest(id=id, version="0.1.0"), **DIST})
        refused(t, repo, "--ref", sha, kind="invalid_id")
    for escaped in [t.ws / "evil", t.root / "evil", t.ws / "packages/a", t.ws / "dist/a"]:
        t.ok(not escaped.exists(), f"{escaped} not created")


@case(fixture="hello_installed", spec=["package-ids/version-format#malformed-version-is-refused"])
def invalid_version(t: T):
    """Versions that are not MAJOR.MINOR.PATCH[-PRE]: invalid_version (build metadata included)."""
    for name, version in [("short", "1.0"), ("prefixed", "v1.0.0"), ("zero", "01.0.0"), ("pre", "1.0.0-01"),
                          ("build", "1.0.0+build.5")]:
        repo, sha = bad_repo(t, name, {"plugin.json": manifest(id=f"bad/{name}", version=version), **DIST})
        refused(t, repo, "--ref", sha, kind="invalid_version", contains="MAJOR.MINOR.PATCH")
        t.not_installed(f"bad/{name}")


@case(fixture="hello_installed", spec=["dist/a-revision-without-dist-is-refused#deploy-from-git-objects"])
def missing_dist(t: T):
    """No dist/ in the commit: rejected before anything is installed."""
    repo, sha = bad_repo(t, "nodist", {"plugin.json": manifest(id="bad/nodist", version="0.1.0"),
                                       "README.md": "no payload\n"})
    t.tag(repo, "bad/nodist@0.1.0")
    refused(t, repo, "--ref", sha, kind="invalid_manifest", contains="dist/")
    refused(t, repo, kind="invalid_manifest", contains="dist/")
    t.not_installed("bad/nodist")


@case(fixture="hello_installed", spec=["dist/a-revision-without-dist-is-refused"])
def empty_dist_folder_is_missing_payload(t: T):
    """An empty dist/ folder cannot be committed: the author's empty folder is never installed."""
    repo, sha = bad_repo(t, "emptydist", {"plugin.json": manifest(id="bad/emptydist", version="0.1.0"),
                                          "README.md": "x\n"})
    (repo / "dist").mkdir()
    refused(t, repo, "--ref", sha, kind="invalid_manifest", contains="dist/")
    t.not_installed("bad/emptydist")


@case(fixture="hello_installed", spec=["dist/deploys-copy-the-committed-tree"])
def dist_with_only_placeholder_recorded(t: T):
    """Contract unspecified: dist/ holding only an empty .gitkeep. Recorded; only safety is asserted."""
    id = "bad/keep"
    repo, sha = bad_repo(t, "keep", {"plugin.json": manifest(id=id, version="0.1.0"), "dist/.gitkeep": ""})
    sentinel = t.snap(t.clone(SENTINEL), t.dist(SENTINEL), git=True)
    r = t.gitpm("install", repo, "--ref", sha)
    if r.code == 0:
        print("  RECORDED: a dist/ with only an empty placeholder file installs")
        t.eq(sorted(p.name for p in t.dist(id).rglob("*")), [".gitkeep"], "deployed exactly the committed files")
        t.eq(t.package(id)["current"], {"version": "0.1.0", "commit": sha}, "current")
    else:
        print("  RECORDED: a dist/ with only an empty placeholder file is refused")
        t.not_installed(id)
    t.same(sentinel, "sentinel clone and dist")


@case(fixture="hello_installed", spec=["install-uninstall/same-id-from-another-source-is-refused", "package-ids/case-colliding-ids-are-refused"])
def conflicting_ids(t: T):
    """Another source claiming the sentinel's ID, a case variant, or an unscoped ID equal to its namespace."""
    repo, sha = bad_repo(t, "sameid", {"plugin.json": manifest(id=SENTINEL, version="0.2.0"), **DIST})
    refused(t, repo, "--ref", sha, kind="already_installed", contains=SENTINEL)
    repo, sha = bad_repo(t, "case", {"plugin.json": manifest(id="Dan/hello", version="0.1.0"), **DIST})
    refused(t, repo, "--ref", sha, kind="workspace", contains="letter case")
    repo, sha = bad_repo(t, "nsclash", {"plugin.json": manifest(id="dan", version="0.1.0"), **DIST})
    refused(t, repo, "--ref", sha, kind="invalid_id", contains="namespace/name")
    t.not_installed("Dan/hello")


@case(fixture="hello_installed", spec=["dist/deploys-copy-the-committed-tree"])
def symlinks_in_dist_cannot_escape(t: T):
    """dist/ entries that are symlinks (to a file outside, to a parent dir) are refused before deploy."""
    for name, link, target in [("filelink", "dist/index.js", "/etc/passwd"),
                               ("dirlink", "dist/up", "../.."),
                               ("manifestlink", "dist/plugin.json", "../plugin.json")]:
        id = f"bad/{name}"
        files = {"plugin.json": manifest(id=id, version="0.1.0")}
        if link != "dist/index.js":
            files.update(DIST)
        repo, sha = bad_repo(t, name, files, symlinks={link: target})
        refused(t, repo, "--ref", sha, kind="unsupported", contains="symlink")
        t.not_installed(id)


# ------------------------------------------------------------------ catalog installs

def catalog_refused(t: T, id: str, kind: str | None, contains=()):
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install-catalog", id, kind=kind, contains=contains)
    t.same(before, "workspace with the sentinel")
    t.not_installed(id)


@case(fixture="catalog", spec=["catalog/register-one-version"])
def register_rejects_invalid_manifests(t: T):
    """The catalog never accepts a release with a broken manifest, bad ID/version or no dist."""
    catalog = t.snap(t.catalog)
    for name, files, kind in [
        ("malformed", {"plugin.json": "{", **DIST}, "invalid_manifest"),
        ("traversal", {"plugin.json": manifest(id="../evil", version="0.1.0"), **DIST}, "invalid_id"),
        ("version", {"plugin.json": manifest(id="bad/version", version="1.0"), **DIST}, "invalid_version"),
        ("nodist", {"plugin.json": manifest(id="bad/nodist", version="0.1.0"), "x.txt": "x\n"}, "invalid_manifest"),
    ]:
        repo, sha = bad_repo(t, name, files)
        t.gitpm_err("register", repo, "--ref", sha, kind=kind)
        t.same(catalog, "catalog")
    t.ok(not (t.catalog / "namespaces/bad").exists(), "no bad namespace in the catalog")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog"])
def catalog_release_pointing_to_broken_manifest(t: T):
    """A release.json whose commit carries a malformed manifest is refused; sentinel intact."""
    t.gitpm_ok("install-catalog", "dan/num", "--version", "1.9.0")
    broken = t.commit("dan/hello", "broken manifest", {"plugin.json": "{ broken"})
    t.edit_json(t.catalog_release("dan/hello", "0.2.0"), lambda r: r.update(commit=broken))
    catalog_refused(t, "dan/hello", None, contains="Error kind:")
    t.eq(t.package("dan/num")["current"]["version"], "1.9.0", "sentinel")


@case(fixture="catalog", spec=["install-uninstall/install-from-the-catalog#tampered-dist"])
def catalog_release_with_escaping_file_path(t: T):
    """files keys with '..' or absolute paths are refused before any clone is adopted."""
    t.gitpm_ok("install-catalog", "dan/num", "--version", "1.9.0")
    path = t.catalog_release("dan/hello", "0.2.0")
    original = json.loads(path.read_text())
    for bad in ["../../escape.js", "/etc/passwd", "a/../../b.js"]:
        def change(r, bad=bad):
            r["files"] = {bad: original["files"]["index.js"]}
        t.edit_json(path, change)
        catalog_refused(t, "dan/hello", "catalog", contains="invalid file path")
    t.ok(not (t.root / "escape.js").exists() and not (t.ws / "escape.js").exists(), "nothing written outside")
