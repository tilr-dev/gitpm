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


# ------------------------------------------------------------------ old/oldie


@fixture()
def g4_oldie(t: T):
    """old/oldie at /fx/repos/old/oldie: tags old/oldie@0.1.0, old/oldie@0.2.0 (nested dist)."""
    init_repo(t, "old/oldie")
    for v in ["0.1.0", "0.2.0"]:
        t.release("old/oldie", v, files={"dist/lib/util.js": f"// util {v}\n"})


@g4_oldie.check
def _(t: T):
    t.eq(sorted(t.git("old/oldie", "tag", "--list").split()), ["v0.1.0", "v0.2.0"], "oldie tags")
    t.has(t.git("old/oldie", "show", "v0.1.0:plugin.json"), '"id": "old/oldie"', '"version": "0.1.0"')


# ------------------------------------------------------------------ catalog tree


@fixture(requires=["hello_releases", "g4_oldie"])
def g4_tree(t: T):
    """Adds acme/widget 1.0.0, 1.1.0 with nested text and binary dist files (next to
    dan/hello 0.1.0/0.1.1/0.2.0 and old/oldie 0.1.0/0.2.0)."""
    init_repo(t, "acme/widget")
    for v in ["1.0.0", "1.1.0"]:
        t.release("acme/widget", v, files={
            "dist/assets/img/logo.bin": BINARY + v.encode(),
            "dist/lib/deep/nested/util.js": f"export const v = '{v}';\n",
            "dist/empty.txt": "",
        })


@g4_tree.check
def _(t: T):
    t.eq(sorted(t.git("acme/widget", "tag", "--list").split()),
         ["v1.0.0", "v1.1.0"], "widget tags")
    files = committed_files(t.repo("acme/widget"), "v1.1.0")
    t.eq(sorted(files), ["assets/img/logo.bin", "empty.txt", "index.js", "lib/deep/nested/util.js"], "widget dist")
    t.eq(files["empty.txt"]["size"], 0, "empty file size")


G4_TREE_RELEASES = [
    ("dan/hello", "dan/hello", ["0.1.0", "0.1.1"], "v{}"),
    ("acme/widget", "acme/widget", ["1.0.0", "1.1.0"], "v{}"),
    ("old/oldie", "old/oldie", ["0.1.0", "0.2.0"], "v{}"),
]


@fixture(requires=["g4_tree"], gitpm=True)
def g4_tree_catalog(t: T):
    """/fx/catalog (v2): dan/hello 0.1.0, 0.1.1; acme/widget 1.0.0, 1.1.0; old/oldie
    0.1.0, 0.2.0. dan/hello 0.2.0 is tagged but not registered. Empty workspace."""
    for _, repo, versions, tag in G4_TREE_RELEASES:
        for v in versions:
            t.gitpm_ok("register", t.repo(repo), "--ref", tag.format(v))


@g4_tree_catalog.check
def _(t: T):
    root = json.loads((t.catalog / "catalog.json").read_text())
    t.eq(root, {"schema": 1, "directories": {"namespaces": "namespaces/"}}, "root catalog")
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
    ("old/oldie", ["0.1.0", "0.2.0"], "v{}", False),
    ("dan/hello", ["0.1.0", "0.1.1"], "v{}", True),
]


@fixture(requires=["hello_releases", "g4_oldie"])
def g4_v1(t: T):
    """Monolithic v1 catalog /fx/v1/catalog.json (the format gitpm 0.5 still read; now
    refused): old/oldie 0.1.0, 0.2.0 without files/build; dan/hello 0.1.0, 0.1.1 with files."""
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
    t.eq(sorted(value["plugins"]), ["dan/hello", "old/oldie"], "v1 packages")
    for v in value["plugins"]["old/oldie"]["versions"].values():
        t.ok("files" not in v and "build" not in v, "old release has no files/build")
    for v in value["plugins"]["dan/hello"]["versions"].values():
        t.ok(v["files"] and "build" not in v, "scoped release has files, no build")


# ------------------------------------------------------------------ sources list

SPACED = "spaced repos/dan/spaced"


@fixture(requires=["hello_releases"])
def g4_sources(t: T):
    """/fx/lists/sources.txt: dan/hello (relative, absolute and trailing-slash duplicates),
    comments, blank lines, and /fx/"spaced repos"/dan/spaced (1.0.0 lightweight tag,
    1.1.0 annotated tag, untagged HEAD). dan/hello has 0.1.0/0.1.1/0.2.0 + untagged HEAD."""
    repo = t.root / SPACED
    init_repo(t, repo)
    t.release("dan/spaced", "1.0.0", repo=repo)
    t.release("dan/spaced", "1.1.0", repo=repo, tag=False)
    t.git(repo, "tag", "-a", "-m", "annotated release", "v1.1.0")
    t.commit(repo, "unreleased", {"dist/index.js": "dev\n"})
    (t.root / "lists").mkdir()
    (t.root / "lists/sources.txt").write_text(
        "# package sources\n"
        "\n"
        "../repos/dan/hello\n"
        "   \n"
        "# duplicate spellings of the same repository\n"
        "../repos/dan/hello/\n"
        f"{t.repo('dan/hello')}\n"
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
    """/fx/catalog generated by catalog-update from /fx/lists/sources.txt: dan/hello 0.1.0,
    0.1.1, 0.2.0 and dan/spaced 1.0.0, 1.1.0. Empty workspace."""
    t.gitpm_ok("catalog-update", "--sources", t.root / "lists/sources.txt")


@g4_sources_catalog.check
def _(t: T):
    for id, versions in [("dan/hello", ["0.1.0", "0.1.1", "0.2.0"]), ("dan/spaced", ["1.0.0", "1.1.0"])]:
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(sorted(index["versions"]), versions, f"{id} versions")
    t.ok(not t.ws.exists(), "workspace untouched")


# ------------------------------------------------------------------ two catalogs (UC-057)


@fixture()
def g4_xp(t: T):
    """x/p 1.0.0, 1.1.0; y/q 1.0.0; z/r 1.0.0 at /fx/repos; /fx/mirror/x/p is a clone of x/p."""
    t.new_package("x/p", "1.0.0")
    t.release("x/p", "1.1.0")
    t.new_package("y/q", "1.0.0")
    t.new_package("z/r", "1.0.0")
    t.run("git", "clone", "-q", t.repo("x/p"), t.root / "mirror/x/p", check=True)


@g4_xp.check
def _(t: T):
    t.eq(t.rev(t.root / "mirror/x/p", "v1.1.0"), t.rev("x/p", "v1.1.0"), "mirror has the same releases")
    for id in ["y/q", "z/r"]:
        t.eq(t.git(id, "tag", "--list"), f"v1.0.0", f"{id} tags")


@fixture(requires=["g4_xp"], gitpm=True)
def g4_two_catalogs(t: T):
    """Catalog A /fx/catA: x/p 1.0.0 (source /fx/repos/x/p), z/r 1.0.0. Catalog B /fx/catB:
    x/p 1.0.0 + 1.1.0 from /fx/mirror/x/p (a different source), y/q 1.0.0, z/r 1.0.0 (same
    source and commit as in A). Both are verified (`"verified": true`). Empty workspace."""
    a, b = t.root / "catA", t.root / "catB"
    t.gitpm_ok("--catalog", a, "register", t.repo("x/p"), "--ref", "v1.0.0")
    t.gitpm_ok("--catalog", a, "register", t.repo("z/r"), "--ref", "v1.0.0")
    for v in ["1.0.0", "1.1.0"]:
        t.gitpm_ok("--catalog", b, "register", t.root / "mirror/x/p", "--ref", f"v{v}")
    t.gitpm_ok("--catalog", b, "register", t.repo("y/q"), "--ref", "v1.0.0")
    t.gitpm_ok("--catalog", b, "register", t.repo("z/r"), "--ref", "v1.0.0")
    # Both owners mark their catalog verified by hand.
    for catalog in [a, b]:
        t.edit_json(catalog / "catalog.json", lambda c: c.update(verified=True))


@g4_two_catalogs.check
def _(t: T):
    a, b = t.root / "catA", t.root / "catB"
    t.eq(json.loads(t.catalog_index("x/p", a).read_text())["source"], str(t.repo("x/p")), "A source of x/p")
    t.eq(json.loads(t.catalog_index("x/p", b).read_text())["source"], str(t.root / "mirror/x/p"), "B source of x/p")
    t.eq(sorted(json.loads(t.catalog_index("x/p", b).read_text())["versions"]), ["1.0.0", "1.1.0"], "B x/p versions")
    t.ok(not t.catalog_index("y/q", a).exists(), "y/q only in B")
    for catalog in [a, b]:
        t.eq(json.loads((catalog / "catalog.json").read_text()).get("verified"), True, f"{catalog} verified")
    t.eq(t.catalog_release("z/r", "1.0.0", a).read_bytes(), t.catalog_release("z/r", "1.0.0", b).read_bytes(),
         "z/r release identical in A and B")
    t.ok(not t.ws.exists(), "workspace untouched")
