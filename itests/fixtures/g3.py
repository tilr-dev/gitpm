"""Group g3 fixtures: a three-package workspace, an installed vendor package, an
unscoped (refused) repository and a catalog fork of vendor/p."""

import json

from gitpmtest import T, fixture

GITIGNORE = {".gitignore": "node_modules/\n"}


@fixture()
def g3_trio(t: T):
    """dan/a (0.1.0, 0.1.1), dan/b (0.1.0) and other/c (0.1.0); each repo ignores node_modules/."""
    t.new_package("dan/a", "0.1.0", files=GITIGNORE)
    t.release("dan/a", "0.1.1")
    t.new_package("dan/b", "0.1.0", files=GITIGNORE)
    t.new_package("other/c", "0.1.0", files=GITIGNORE)


@g3_trio.check
def _(t: T):
    t.eq(t.git("dan/a", "tag", "--list").split(), ["dan/a@0.1.0", "dan/a@0.1.1"], "dan/a tags")
    t.eq(t.git("dan/b", "tag", "--list"), "dan/b@0.1.0", "dan/b tags")
    t.eq(t.git("other/c", "tag", "--list"), "other/c@0.1.0", "other/c tags")
    for id in ["dan/a", "dan/b", "other/c"]:
        t.eq(t.git(id, "status", "--porcelain"), "", f"{id} clean")
        t.eq((t.repo(id) / ".gitignore").read_text(), "node_modules/\n", f"{id} .gitignore")


@fixture(requires=["g3_trio"], gitpm=True)
def g3_trio_installed(t: T):
    """dan/a installed at 0.1.0 and updated to 0.1.1 (previous 0.1.0); dan/b 0.1.0 and
    other/c 0.1.0 installed directly from Git. No catalog."""
    t.gitpm_ok("install", t.repo("dan/a"), "--ref", "dan/a@0.1.0")
    t.gitpm_ok("update", "dan/a")
    t.gitpm_ok("install", t.repo("dan/b"))
    t.gitpm_ok("install", t.repo("other/c"))


@g3_trio_installed.check
def _(t: T):
    a = t.package("dan/a")
    t.eq(a["current"], {"version": "0.1.1", "commit": t.rev("dan/a", "dan/a@0.1.1")}, "dan/a current")
    t.eq(a["previous"], {"version": "0.1.0", "commit": t.rev("dan/a", "dan/a@0.1.0")}, "dan/a previous")
    for id in ["dan/b", "other/c"]:
        p = t.package(id)
        t.eq(p["current"]["version"], "0.1.0", f"{id} version")
        t.eq(p["previous"], None, f"{id} previous")
    for id in ["dan/a", "dan/b", "other/c"]:
        p = t.package(id)
        t.eq(p["enabled"], True, f"{id} enabled")
        t.eq(t.rev(t.clone(id)), p["current"]["commit"], f"{id} clone HEAD")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} {p['current']['version']}")
        t.eq(t.git(t.clone(id), "status", "--porcelain", "--ignored"), "", f"{id} clone clean")
    t.ok(t.catalog_empty(), "no catalog")


@fixture()
def g3_vendor(t: T):
    """vendor/demo: one release 0.1.0 (ignores node_modules/)."""
    t.new_package("vendor/demo", "0.1.0", files=GITIGNORE)


@g3_vendor.check
def _(t: T):
    t.eq(t.git("vendor/demo", "tag", "--list"), "vendor/demo@0.1.0", "tags")
    t.eq(t.git("vendor/demo", "status", "--porcelain"), "", "clean")


@fixture(requires=["g3_vendor"], gitpm=True)
def g3_vendor_installed(t: T):
    """vendor/demo 0.1.0 installed directly from Git; no catalog."""
    t.gitpm_ok("install", t.repo("vendor/demo"), "--ref", "vendor/demo@0.1.0")


@g3_vendor_installed.check
def _(t: T):
    p = t.package("vendor/demo")
    t.eq(p["current"], {"version": "0.1.0", "commit": t.rev("vendor/demo", "vendor/demo@0.1.0")}, "current")
    t.eq(p["source"], str(t.repo("vendor/demo")), "source")
    t.ok("forkOf" not in p, "no forkOf")
    t.eq(t.rev(t.clone("vendor/demo")), p["current"]["commit"], "clone HEAD")
    t.eq(t.git(t.clone("vendor/demo"), "status", "--porcelain", "--ignored"), "", "clone clean")


@fixture()
def g3_legacy(t: T):
    """Unscoped repository `demo` at /fx/repos/demo with tags v0.1.0 and v0.1.1: the
    legacy shape gitpm refuses (no package ID namespace, no ID@VERSION tags)."""
    repo = t.repo("demo")
    (repo / "dist").mkdir(parents=True)
    t.git(repo, "init", "-q", "-b", "main")
    t.release("demo", "0.1.0", tag=False)
    t.tag(repo, "v0.1.0")
    t.release("demo", "0.1.1", tag=False)
    t.tag(repo, "v0.1.1")


@g3_legacy.check
def _(t: T):
    t.eq(t.git("demo", "tag", "--list").split(), ["v0.1.0", "v0.1.1"], "tags")
    t.eq(json.loads(t.git("demo", "show", "v0.1.0:plugin.json"))["id"], "demo", "manifest id")


@fixture()
def g3_vendor_p(t: T):
    """vendor/p: one release 1.0.0."""
    t.new_package("vendor/p", "1.0.0")


@g3_vendor_p.check
def _(t: T):
    t.eq(t.git("vendor/p", "tag", "--list"), "vendor/p@1.0.0", "tags")


@fixture(requires=["g3_vendor_p"], gitpm=True)
def g3_fork_catalog(t: T):
    """vendor/p 1.0.0 registered in /fx/catalog and installed from it; forked as mine/p
    into /fx/repos/mine/p; fork release mine/p@1.0.1 made with `gitpm commit` and installed."""
    t.gitpm_ok("register", t.repo("vendor/p"), "--ref", "vendor/p@1.0.0")
    t.gitpm_ok("install-catalog", "vendor/p", "--version", "1.0.0")
    t.gitpm_ok("fork", "vendor/p", "--ns", "mine", "--into", t.repo("mine/p"))
    t.gitpm_ok("commit", t.repo("mine/p"), "1.0.1")
    t.gitpm_ok("install", t.repo("mine/p"), "--ref", "mine/p@1.0.1")


@g3_fork_catalog.check
def _(t: T):
    vendor = t.package("vendor/p")
    base = t.rev("vendor/p", "vendor/p@1.0.0")
    t.eq(vendor["current"], {"version": "1.0.0", "commit": base}, "vendor/p current")
    t.ok("forkOf" not in vendor and "replaces" not in vendor, "vendor/p has no fork fields")
    mine = t.package("mine/p")
    t.eq(mine["current"], {"version": "1.0.1", "commit": t.rev("mine/p", "mine/p@1.0.1")}, "mine/p current")
    t.eq(mine["forkOf"], {"id": "vendor/p", "version": "1.0.0", "commit": base,
                          "source": str(t.repo("vendor/p"))}, "mine/p forkOf")
    t.eq(mine["replaces"], {"id": "vendor/p", "version": "1.0.0"}, "mine/p replaces")
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, "vendor/p")
