"""Group g4: catalog trees, a v1 catalog, sources lists and two catalogs (UC-027…037, 057, 064)."""

import hashlib
import json
import subprocess

from gitpmtest import T, fixture

# Binary payload with NUL/high bytes and a nested file, to check hashes beyond plain text.
BINARY = bytes(range(256)) * 3 + b"\x00\xff\x00"


def init_repo(t: T, path) -> None:
    path = t.path(path)
    (path / "dist").mkdir(parents=True)
    t.git(path, "init", "-q", "-b", "main")


def committed_files(repo, rev: str) -> dict:
    """{path under dist/: {sha256, size}} of the committed dist at rev (independent of pm)."""
    names = subprocess.run(["git", "-C", str(repo), "ls-tree", "-r", "-z", "--name-only", rev, "dist/"],
                           capture_output=True, check=True).stdout.decode().split("\0")
    out = {}
    for name in filter(None, names):
        data = subprocess.run(["git", "-C", str(repo), "cat-file", "blob", f"{rev}:{name}"],
                              capture_output=True, check=True).stdout
        out[name[len("dist/"):]] = {"sha256": hashlib.sha256(data).hexdigest(), "size": len(data)}
    return out


# ------------------------------------------------------------------ 3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6


@fixture()
def g4_oldie(t: T):
    """3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6 at /fx/repos/3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6: tags 3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6@0.1.0, 3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6@0.2.0 (nested dist)."""
    init_repo(t, "3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6")
    for v in ["0.1.0", "0.2.0"]:
        t.release("3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6", v, files={"dist/lib/util.js": f"// util {v}\n"})


@g4_oldie.check
def _(t: T):
    t.eq(sorted(t.git("3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6", "tag", "--list").split()), ["v0.1.0", "v0.2.0"], "oldie tags")
    t.has(t.git("3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6", "show", "v0.1.0:plugin.json"), '"id": "3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6"', '"version": "0.1.0"')


# ------------------------------------------------------------------ catalog tree


@fixture(requires=["hello_releases", "g4_oldie"])
def g4_tree(t: T):
    """Adds 41b88ba0-f9f1-5e65-acb6-3dc95231e0bc 1.0.0, 1.1.0 with nested text and binary dist files (next to
    3dce1568-569c-5b53-9321-37185e751947 0.1.0/0.1.1/0.2.0 and 3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6 0.1.0/0.2.0)."""
    init_repo(t, "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc")
    for v in ["1.0.0", "1.1.0"]:
        t.release("41b88ba0-f9f1-5e65-acb6-3dc95231e0bc", v, files={
            "dist/assets/img/logo.bin": BINARY + v.encode(),
            "dist/lib/deep/nested/util.js": f"export const v = '{v}';\n",
            "dist/empty.txt": "",
        })


@g4_tree.check
def _(t: T):
    t.eq(sorted(t.git("41b88ba0-f9f1-5e65-acb6-3dc95231e0bc", "tag", "--list").split()),
         ["v1.0.0", "v1.1.0"], "widget tags")
    files = committed_files(t.repo("41b88ba0-f9f1-5e65-acb6-3dc95231e0bc"), "v1.1.0")
    t.eq(sorted(files), ["assets/img/logo.bin", "empty.txt", "index.js", "lib/deep/nested/util.js"], "widget dist")
    t.eq(files["empty.txt"]["size"], 0, "empty file size")


G4_TREE_RELEASES = [
    ("3dce1568-569c-5b53-9321-37185e751947", "3dce1568-569c-5b53-9321-37185e751947", ["0.1.0", "0.1.1"], "v{}"),
    ("41b88ba0-f9f1-5e65-acb6-3dc95231e0bc", "41b88ba0-f9f1-5e65-acb6-3dc95231e0bc", ["1.0.0", "1.1.0"], "v{}"),
    ("3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6", "3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6", ["0.1.0", "0.2.0"], "v{}"),
]


@fixture(requires=["g4_tree"], gitpm=True)
def g4_tree_catalog(t: T):
    """/fx/catalog (v2): 3dce1568-569c-5b53-9321-37185e751947 0.1.0, 0.1.1; 41b88ba0-f9f1-5e65-acb6-3dc95231e0bc 1.0.0, 1.1.0; 3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6
    0.1.0, 0.2.0. 3dce1568-569c-5b53-9321-37185e751947 0.2.0 is tagged but not registered. Empty workspace."""
    for _, repo, versions, tag in G4_TREE_RELEASES:
        for v in versions:
            t.gitpm_ok("register", t.repo(repo), "--ref", tag.format(v))


@g4_tree_catalog.check
def _(t: T):
    root = json.loads((t.catalog / "catalog.json").read_text())
    t.eq(root, {"schema": 1, "directories": {"packages": "packages/"}}, "root catalog")
    for id, _, versions, _ in G4_TREE_RELEASES:
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(sorted(index["versions"]), sorted(versions), f"{id} versions")
    t.ok(not t.ws.exists(), "workspace untouched")


# ------------------------------------------------------------------ v1 catalog


def v1_release(t: T, id: str, repo: str, version: str, tag: str, with_files: bool) -> dict:
    commit = t.rev(repo, tag)
    release = {
        "manifest": json.loads(t.git(repo, "show", f"{commit}:plugin.json")),
        "source": str(t.repo(repo)),
        "commit": commit,
        "reference": tag,
    }
    if with_files:
        release["files"] = committed_files(t.repo(repo), commit)
    return release


V1_PACKAGES = [
    ("3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6", ["0.1.0", "0.2.0"], "v{}", False),
    ("3dce1568-569c-5b53-9321-37185e751947", ["0.1.0", "0.1.1"], "v{}", True),
]


@fixture(requires=["hello_releases", "g4_oldie"])
def g4_v1(t: T):
    """Monolithic v1 catalog /fx/v1/catalog.json (the format gitpm 0.5 still read; now
    refused): 3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6 0.1.0, 0.2.0 without files/build; 3dce1568-569c-5b53-9321-37185e751947 0.1.0, 0.1.1 with files."""
    packages = {}
    for id, versions, tag, with_files in V1_PACKAGES:
        packages[id] = {"source": str(t.repo(id)),
                        "versions": {v: v1_release(t, id, id, v, tag.format(v), with_files) for v in versions}}
    (t.root / "v1").mkdir()
    # The v1 layout keeps its literal top-level key `packages`.
    (t.root / "v1/catalog.json").write_text(json.dumps({"plugins": packages}, indent=2) + "\n")


@g4_v1.check
def _(t: T):
    value = json.loads((t.root / "v1/catalog.json").read_text())
    t.eq(sorted(value), ["plugins"], "v1 top level")
    t.eq(sorted(value["plugins"]), ["3dce1568-569c-5b53-9321-37185e751947", "3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6"], "v1 packages")
    for v in value["plugins"]["3cfe3093-9e5f-5f64-b4d1-0a169cc3bdf6"]["versions"].values():
        t.ok("files" not in v and "build" not in v, "old release has no files/build")
    for v in value["plugins"]["3dce1568-569c-5b53-9321-37185e751947"]["versions"].values():
        t.ok(v["files"] and "build" not in v, "scoped release has files, no build")


# ------------------------------------------------------------------ sources list

SPACED = "spaced repos/2bf18134-a7da-5c98-841c-4ee91fa6e90e"


@fixture(requires=["hello_releases"])
def g4_sources(t: T):
    """/fx/lists/sources.txt: 3dce1568-569c-5b53-9321-37185e751947 (relative, absolute and trailing-slash duplicates),
    comments, blank lines, and /fx/"spaced repos"/2bf18134-a7da-5c98-841c-4ee91fa6e90e (1.0.0 lightweight tag,
    1.1.0 annotated tag, untagged HEAD). 3dce1568-569c-5b53-9321-37185e751947 has 0.1.0/0.1.1/0.2.0 + untagged HEAD."""
    repo = t.root / SPACED
    init_repo(t, repo)
    t.release("2bf18134-a7da-5c98-841c-4ee91fa6e90e", "1.0.0", repo=repo)
    t.release("2bf18134-a7da-5c98-841c-4ee91fa6e90e", "1.1.0", repo=repo, tag=False)
    t.git(repo, "tag", "-a", "-m", "annotated release", "v1.1.0")
    t.commit(repo, "unreleased", {"dist/index.js": "dev\n"})
    (t.root / "lists").mkdir()
    (t.root / "lists/sources.txt").write_text(
        "# package sources\n"
        "\n"
        "../repos/3dce1568-569c-5b53-9321-37185e751947\n"
        "   \n"
        "# duplicate spellings of the same repository\n"
        "../repos/3dce1568-569c-5b53-9321-37185e751947/\n"
        f"{t.repo('3dce1568-569c-5b53-9321-37185e751947')}\n"
        f"../{SPACED}\n"
    )


@g4_sources.check
def _(t: T):
    repo = t.root / SPACED
    t.eq(t.git(repo, "cat-file", "-t", "v1.1.0"), "tag", "1.1.0 is annotated")
    t.eq(t.git(repo, "cat-file", "-t", "v1.0.0"), "commit", "1.0.0 is lightweight")
    t.ok(t.rev(repo) != t.rev(repo, "v1.1.0"), "HEAD past the last release")
    t.has((t.root / "lists/sources.txt").read_text(), "# package sources", SPACED)


@fixture(requires=["g4_sources"], gitpm=True)
def g4_sources_catalog(t: T):
    """/fx/catalog generated by catalog-update from /fx/lists/sources.txt: 3dce1568-569c-5b53-9321-37185e751947 0.1.0,
    0.1.1, 0.2.0 and 2bf18134-a7da-5c98-841c-4ee91fa6e90e 1.0.0, 1.1.0. Empty workspace."""
    t.gitpm_ok("catalog-update", "--sources", t.root / "lists/sources.txt")


@g4_sources_catalog.check
def _(t: T):
    for id, versions in [("3dce1568-569c-5b53-9321-37185e751947", ["0.1.0", "0.1.1", "0.2.0"]), ("2bf18134-a7da-5c98-841c-4ee91fa6e90e", ["1.0.0", "1.1.0"])]:
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(sorted(index["versions"]), versions, f"{id} versions")
    t.ok(not t.ws.exists(), "workspace untouched")


# ------------------------------------------------------------------ two catalogs (UC-057)


@fixture()
def g4_xp(t: T):
    """c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 1.0.0, 1.1.0; 627a9f5c-10d4-5960-a0ab-4df072d8b52d 1.0.0; 055241a5-ec83-58c8-9b54-93e74bee2e74 1.0.0 at /fx/repos; /fx/ad90c75e-e7aa-5008-a232-a86fb7f0b777 is a clone of c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5."""
    t.new_package("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", "1.0.0")
    t.release("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", "1.1.0")
    t.new_package("627a9f5c-10d4-5960-a0ab-4df072d8b52d", "1.0.0")
    t.new_package("055241a5-ec83-58c8-9b54-93e74bee2e74", "1.0.0")
    t.run("git", "clone", "-q", t.repo("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5"), t.root / "ad90c75e-e7aa-5008-a232-a86fb7f0b777", check=True)


@g4_xp.check
def _(t: T):
    t.eq(t.rev(t.root / "ad90c75e-e7aa-5008-a232-a86fb7f0b777", "v1.1.0"), t.rev("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", "v1.1.0"), "mirror has the same releases")
    for id in ["627a9f5c-10d4-5960-a0ab-4df072d8b52d", "055241a5-ec83-58c8-9b54-93e74bee2e74"]:
        t.eq(t.git(id, "tag", "--list"), f"v1.0.0", f"{id} tags")


@fixture(requires=["g4_xp"], gitpm=True)
def g4_two_catalogs(t: T):
    """Catalog A /fx/catA: c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 1.0.0 (source /fx/repos/c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5), 055241a5-ec83-58c8-9b54-93e74bee2e74 1.0.0. Catalog B /fx/catB:
    c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 1.0.0 + 1.1.0 from /fx/ad90c75e-e7aa-5008-a232-a86fb7f0b777 (a different source), 627a9f5c-10d4-5960-a0ab-4df072d8b52d 1.0.0, 055241a5-ec83-58c8-9b54-93e74bee2e74 1.0.0 (same
    source and commit as in A). Both are verified (`"verified": true`). Empty workspace."""
    a, b = t.root / "catA", t.root / "catB"
    t.gitpm_ok("--catalog", a, "register", t.repo("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5"), "--ref", "v1.0.0")
    t.gitpm_ok("--catalog", a, "register", t.repo("055241a5-ec83-58c8-9b54-93e74bee2e74"), "--ref", "v1.0.0")
    for v in ["1.0.0", "1.1.0"]:
        t.gitpm_ok("--catalog", b, "register", t.root / "ad90c75e-e7aa-5008-a232-a86fb7f0b777", "--ref", f"v{v}")
    t.gitpm_ok("--catalog", b, "register", t.repo("627a9f5c-10d4-5960-a0ab-4df072d8b52d"), "--ref", "v1.0.0")
    t.gitpm_ok("--catalog", b, "register", t.repo("055241a5-ec83-58c8-9b54-93e74bee2e74"), "--ref", "v1.0.0")
    # Both owners mark their catalog verified by hand.
    for catalog in [a, b]:
        t.edit_json(catalog / "catalog.json", lambda c: c.update(verified=True))


@g4_two_catalogs.check
def _(t: T):
    a, b = t.root / "catA", t.root / "catB"
    t.eq(json.loads(t.catalog_index("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", a).read_text())["source"], str(t.repo("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5")), "A source of c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5")
    t.eq(json.loads(t.catalog_index("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", b).read_text())["source"], str(t.root / "ad90c75e-e7aa-5008-a232-a86fb7f0b777"), "B source of c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5")
    t.eq(sorted(json.loads(t.catalog_index("c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5", b).read_text())["versions"]), ["1.0.0", "1.1.0"], "B c65a9f2a-2e3b-56bd-95d5-92a4bdde6fe5 versions")
    t.ok(not t.catalog_index("627a9f5c-10d4-5960-a0ab-4df072d8b52d", a).exists(), "627a9f5c-10d4-5960-a0ab-4df072d8b52d only in B")
    for catalog in [a, b]:
        t.eq(json.loads((catalog / "catalog.json").read_text()).get("verified"), True, f"{catalog} verified")
    t.eq(t.catalog_release("055241a5-ec83-58c8-9b54-93e74bee2e74", "1.0.0", a).read_bytes(), t.catalog_release("055241a5-ec83-58c8-9b54-93e74bee2e74", "1.0.0", b).read_bytes(),
         "055241a5-ec83-58c8-9b54-93e74bee2e74 release identical in A and B")
    t.ok(not t.ws.exists(), "workspace untouched")
