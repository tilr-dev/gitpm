"""UC-028: Catalog tree and hashes (P0).

Fixture g4_tree_catalog: namespaces dan, acme and old, two versions each,
nested and binary dist files. Hashes are recomputed independently from Git blobs.
"""

import hashlib
import json
import subprocess
from pathlib import Path

from gitpmtest import T, case

PACKAGES = {
    "dan/hello": (["0.1.0", "0.1.1"], "dan/hello@{}"),
    "acme/widget": (["1.0.0", "1.1.0"], "acme/widget@{}"),
    "old/oldie": (["0.1.0", "0.2.0"], "old/oldie@{}"),
}


def folder(t: T, id: str) -> Path:
    return t.catalog / "namespaces" / id


def load(path: Path) -> dict:
    return json.loads(path.read_text())


def committed_files(repo: Path, rev: str) -> dict:
    names = subprocess.run(["git", "-C", str(repo), "ls-tree", "-r", "-z", "--name-only", rev, "dist/"],
                           capture_output=True, check=True).stdout.decode().split("\0")
    out = {}
    for name in filter(None, names):
        data = subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{rev}:{name}"],
                              capture_output=True, check=True).stdout
        out[name[len("dist/"):]] = {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
    return out


@case(fixture="g4_tree_catalog", spec=["catalog/catalog-is-a-tree-of-static-files"])
def schema1_directories_and_links_resolve(t: T):
    """Every directory catalog.json is schema 1 with only `name/` links that resolve to a
    child catalog.json or plugin.json; package indexes link every version to its release."""
    t.eq(load(t.catalog / "catalog.json"),
         {"schema": 1, "directories": {"namespaces": "namespaces/"}}, "root")
    t.eq(load(t.catalog / "namespaces/catalog.json"),
         {"schema": 1, "directories": {"acme": "acme/", "dan": "dan/", "old": "old/"}}, "namespaces/")
    t.eq(load(t.catalog / "namespaces/dan/catalog.json"), {"schema": 1, "directories": {"hello": "hello/"}}, "dan/")
    t.eq(load(t.catalog / "namespaces/acme/catalog.json"), {"schema": 1, "directories": {"widget": "widget/"}},
         "acme/")
    t.eq(load(t.catalog / "namespaces/old/catalog.json"), {"schema": 1, "directories": {"oldie": "oldie/"}}, "old/")

    seen = set()

    def walk(directory: Path, depth=0):
        value = load(directory / "catalog.json")
        seen.add(directory / "catalog.json")
        t.eq(sorted(value), ["directories", "schema"], f"keys of {directory}/catalog.json")
        for name, link in value["directories"].items():
            t.eq(link, name + "/", f"relative link {name}")
            child = directory / link
            if (child / "plugin.json").is_file():
                index = load(child / "plugin.json")
                seen.add(child / "plugin.json")
                t.eq(index["schema"], 1, f"{child} plugin.json schema")
                for version, rel in index["versions"].items():
                    t.eq(rel, f"versions/{version}/release.json", f"{index['id']} {version} link")
                    t.ok((child / rel).is_file(), f"{index['id']} {version} release resolves")
                    seen.add(child / rel)
            else:
                t.ok((child / "catalog.json").is_file(), f"{child} has catalog.json")
                walk(child, depth + 1)

    walk(t.catalog)
    on_disk = {p for p in t.catalog.rglob("*") if p.is_file()}
    t.eq(sorted(str(p) for p in on_disk - seen), [], "files not reachable from the root")
    for id, (versions, _) in PACKAGES.items():
        index = load(folder(t, id) / "plugin.json")
        t.eq(index, {"schema": 1, "id": id, "source": str(t.repo(id)),
                     "versions": {v: f"versions/{v}/release.json" for v in versions},
                     "latest": {"version": versions[-1], "title": None, "description": None,
                                "provides": [], "tags": []}}, f"{id} index")


@case(fixture="g4_tree_catalog", spec=["catalog/release-record"])
def releases_match_git(t: T):
    """manifest/source/commit/reference and every file digest and size equal Git content."""
    for id, (versions, tag) in PACKAGES.items():
        repo = t.repo(id)
        for v in versions:
            release = load(folder(t, id) / f"versions/{v}/release.json")
            commit = t.rev(repo, tag.format(v))
            t.eq(release["commit"], commit, f"{id} {v} commit")
            t.eq(release["reference"], tag.format(v), f"{id} {v} reference")
            t.eq(release["source"], str(repo), f"{id} {v} source")
            manifest = json.loads(t.git(repo, "show", f"{commit}:plugin.json"))
            for key, value in manifest.items():
                t.eq(release["manifest"].get(key), value, f"{id} {v} manifest.{key}")
            t.eq(release["files"], committed_files(repo, commit), f"{id} {v} files (sha256 + size)")
            t.ok("build" not in release, f"{id} {v} has no build")
    widget = load(folder(t, "acme/widget") / "versions/1.1.0/release.json")["files"]
    t.ok({"assets/img/logo.bin", "lib/deep/nested/util.js", "empty.txt"} <= set(widget), "nested + binary files")
    t.eq(widget["empty.txt"], {"sha256": hashlib.sha256(b"").hexdigest(), "size": 0}, "empty file digest")


@case(fixture="g4_tree_catalog", spec=["catalog/published-versions-are-immutable", "catalog/register-one-version"])
def adding_release_preserves_previous_records(t: T):
    """Registering dan/hello 0.2.0 adds only its release.json and plugin.json entry; previous
    releases, other packages and directory catalogs keep their bytes."""
    before = t.snap(t.catalog)
    t.gitpm_ok("register", t.repo("dan/hello"), "--ref", "dan/hello@0.2.0")
    after = t.snap(t.catalog)
    new = str(folder(t, "dan/hello") / "versions/0.2.0/release.json")
    index = str(folder(t, "dan/hello") / "plugin.json")
    t.eq(sorted(set(after) - set(before)), [new], "added files")
    t.eq(sorted(k for k in before if before[k] != after.get(k)), [index], "changed files")
    t.eq(load(Path(index))["versions"],
         {v: f"versions/{v}/release.json" for v in ["0.1.0", "0.1.1", "0.2.0"]}, "hello versions")
    release = load(Path(new))
    t.eq(release["files"], committed_files(t.repo("dan/hello"), t.rev("dan/hello", "dan/hello@0.2.0")), "new hashes")
