"""Group g3 fixtures: a three-package workspace, an installed vendor package, an
unscoped (refused) repository and a catalog fork of dbe02e84-996e-560c-91fc-cce0f29e320e."""

import json

from gitpmtest import T, fixture

GITIGNORE = {".gitignore": "node_modules/\n"}


@fixture()
def g3_trio(t: T):
    """807b9715-06b4-55c2-add2-59b487bfa42e (0.1.0, 0.1.1), 03c25ec4-f583-513d-adfb-092fb98d8dba (0.1.0) and 48cd1267-06f0-5741-820c-7d167ecfd3f8 (0.1.0); each repo ignores node_modules/."""
    t.new_package("807b9715-06b4-55c2-add2-59b487bfa42e", "0.1.0", files=GITIGNORE)
    t.release("807b9715-06b4-55c2-add2-59b487bfa42e", "0.1.1")
    t.new_package("03c25ec4-f583-513d-adfb-092fb98d8dba", "0.1.0", files=GITIGNORE)
    t.new_package("48cd1267-06f0-5741-820c-7d167ecfd3f8", "0.1.0", files=GITIGNORE)


@g3_trio.check
def _(t: T):
    t.eq(t.git("807b9715-06b4-55c2-add2-59b487bfa42e", "tag", "--list").split(), ["v0.1.0", "v0.1.1"], "807b9715-06b4-55c2-add2-59b487bfa42e tags")
    t.eq(t.git("03c25ec4-f583-513d-adfb-092fb98d8dba", "tag", "--list"), "v0.1.0", "03c25ec4-f583-513d-adfb-092fb98d8dba tags")
    t.eq(t.git("48cd1267-06f0-5741-820c-7d167ecfd3f8", "tag", "--list"), "v0.1.0", "48cd1267-06f0-5741-820c-7d167ecfd3f8 tags")
    for id in ["807b9715-06b4-55c2-add2-59b487bfa42e", "03c25ec4-f583-513d-adfb-092fb98d8dba", "48cd1267-06f0-5741-820c-7d167ecfd3f8"]:
        t.eq(t.git(id, "status", "--porcelain"), "", f"{id} clean")
        t.eq((t.repo(id) / ".gitignore").read_text(), "node_modules/\n", f"{id} .gitignore")


@fixture(requires=["g3_trio"], gitpm=True)
def g3_trio_installed(t: T):
    """807b9715-06b4-55c2-add2-59b487bfa42e installed at 0.1.0 and updated to 0.1.1 (previous 0.1.0); 03c25ec4-f583-513d-adfb-092fb98d8dba 0.1.0 and
    48cd1267-06f0-5741-820c-7d167ecfd3f8 0.1.0 installed directly from Git. No catalog."""
    # The ref pins 807b9715-06b4-55c2-add2-59b487bfa42e; --release returns it to release mode at 0.1.1.
    t.gitpm_ok("install", t.repo("807b9715-06b4-55c2-add2-59b487bfa42e"), "--ref", "v0.1.0")
    t.gitpm_ok("update", "807b9715-06b4-55c2-add2-59b487bfa42e", "--release")
    t.gitpm_ok("install", t.repo("03c25ec4-f583-513d-adfb-092fb98d8dba"))
    t.gitpm_ok("install", t.repo("48cd1267-06f0-5741-820c-7d167ecfd3f8"))


@g3_trio_installed.check
def _(t: T):
    a = t.package("807b9715-06b4-55c2-add2-59b487bfa42e")
    t.eq(a["current"], {"version": "0.1.1", "commit": t.rev("807b9715-06b4-55c2-add2-59b487bfa42e", "v0.1.1")}, "807b9715-06b4-55c2-add2-59b487bfa42e current")
    t.eq(a["previous"], {"version": "0.1.0", "commit": t.rev("807b9715-06b4-55c2-add2-59b487bfa42e", "v0.1.0"),
                         "mode": {"pinned": "v0.1.0"}}, "807b9715-06b4-55c2-add2-59b487bfa42e previous (with the mode it had)")
    for id in ["03c25ec4-f583-513d-adfb-092fb98d8dba", "48cd1267-06f0-5741-820c-7d167ecfd3f8"]:
        p = t.package(id)
        t.eq(p["current"]["version"], "0.1.0", f"{id} version")
        t.eq(p["previous"], None, f"{id} previous")
    for id in ["807b9715-06b4-55c2-add2-59b487bfa42e", "03c25ec4-f583-513d-adfb-092fb98d8dba", "48cd1267-06f0-5741-820c-7d167ecfd3f8"]:
        p = t.package(id)
        t.eq(p["enabled"], True, f"{id} enabled")
        t.eq(t.rev(t.worktree(id)), p["current"]["commit"], f"{id} clone HEAD")
        t.has((t.dist(id) / "index.js").read_text(), f"{id} {p['current']['version']}")
        t.eq(t.git(t.worktree(id), "status", "--porcelain", "--ignored"), "", f"{id} clone clean")
    t.ok(t.catalog_empty(), "no catalog")


@fixture()
def g3_vendor(t: T):
    """dd3387ed-a5a3-5059-81ad-b1f6987a28f2: one release 0.1.0 (ignores node_modules/)."""
    t.new_package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "0.1.0", files=GITIGNORE)


@g3_vendor.check
def _(t: T):
    t.eq(t.git("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "tag", "--list"), "v0.1.0", "tags")
    t.eq(t.git("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "status", "--porcelain"), "", "clean")


@fixture(requires=["g3_vendor"], gitpm=True)
def g3_vendor_installed(t: T):
    """dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0 installed directly from Git; no catalog."""
    t.gitpm_ok("install", t.repo("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"))


@g3_vendor_installed.check
def _(t: T):
    p = t.package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
    t.eq(p["current"], {"version": "0.1.0", "commit": t.rev("dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "v0.1.0")}, "current")
    t.eq(p["source"], str(t.repo("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")), "source")
    t.ok("forkOf" not in p, "no forkOf")
    t.eq(t.rev(t.worktree("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")), p["current"]["commit"], "clone HEAD")
    t.eq(t.git(t.worktree("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), "status", "--porcelain", "--ignored"), "", "clone clean")


@fixture()
def g3_legacy(t: T):
    """Unscoped repository `demo` at /fx/repos/demo with tags v0.1.0 and v0.1.1: the
    legacy shape gitpm refuses (no package ID namespace)."""
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
    """dbe02e84-996e-560c-91fc-cce0f29e320e: one release 1.0.0."""
    t.new_package("dbe02e84-996e-560c-91fc-cce0f29e320e", "1.0.0")


@g3_vendor_p.check
def _(t: T):
    t.eq(t.git("dbe02e84-996e-560c-91fc-cce0f29e320e", "tag", "--list"), "v1.0.0", "tags")


@fixture(requires=["g3_vendor_p"], gitpm=True)
def g3_fork_catalog(t: T):
    """dbe02e84-996e-560c-91fc-cce0f29e320e 1.0.0 registered in /fx/catalog and installed from it; forked as ced9e78d-f1f1-5c12-a495-5f882e50b2d6
    into /fx/repos/ced9e78d-f1f1-5c12-a495-5f882e50b2d6; fork release ced9e78d-f1f1-5c12-a495-5f882e50b2d6@1.0.1 made with `gitpm commit` and installed."""
    t.gitpm_ok("register", t.repo("dbe02e84-996e-560c-91fc-cce0f29e320e"), "--ref", "v1.0.0")
    t.gitpm_ok("install-catalog", "dbe02e84-996e-560c-91fc-cce0f29e320e", "--version", "1.0.0")
    original = "dbe02e84-996e-560c-91fc-cce0f29e320e"
    fork_id = "ced9e78d-f1f1-5c12-a495-5f882e50b2d6"
    fork = t.repo(fork_id)
    current = t.package(original)["current"]
    t.run("git", "clone", "-q", t.repo(original), fork, check=True)
    t.git(fork, "checkout", "-q", "-b", "fork", current["commit"])
    t.git(fork, "remote", "rename", "origin", "upstream")
    def identify(m):
        m["id"] = fork_id
        m["forkOf"] = {"id": original, **current, "source": t.package(original)["source"]}
        m["replaces"] = {"id": original, "version": current["version"]}
    t.edit_json(fork / "plugin.json", identify)
    t.commit(fork, "Create fork fixture")
    t.gitpm_ok("commit", t.repo("ced9e78d-f1f1-5c12-a495-5f882e50b2d6"), "1.0.1")
    t.gitpm_ok("install", t.repo("ced9e78d-f1f1-5c12-a495-5f882e50b2d6"))


@g3_fork_catalog.check
def _(t: T):
    vendor = t.package("dbe02e84-996e-560c-91fc-cce0f29e320e")
    base = t.rev("dbe02e84-996e-560c-91fc-cce0f29e320e", "v1.0.0")
    t.eq(vendor["current"], {"version": "1.0.0", "commit": base}, "dbe02e84-996e-560c-91fc-cce0f29e320e current")
    t.ok("forkOf" not in vendor and "replaces" not in vendor, "dbe02e84-996e-560c-91fc-cce0f29e320e has no fork fields")
    mine = t.package("ced9e78d-f1f1-5c12-a495-5f882e50b2d6")
    t.eq(mine["current"], {"version": "1.0.1", "commit": t.rev("ced9e78d-f1f1-5c12-a495-5f882e50b2d6", "v1.0.1")}, "ced9e78d-f1f1-5c12-a495-5f882e50b2d6 current")
    t.eq(mine["forkOf"], {"id": "dbe02e84-996e-560c-91fc-cce0f29e320e", "version": "1.0.0", "commit": base,
                          "source": str(t.repo("dbe02e84-996e-560c-91fc-cce0f29e320e"))}, "ced9e78d-f1f1-5c12-a495-5f882e50b2d6 forkOf")
    t.eq(mine["replaces"], {"id": "dbe02e84-996e-560c-91fc-cce0f29e320e", "version": "1.0.0"}, "ced9e78d-f1f1-5c12-a495-5f882e50b2d6 replaces")
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, "dbe02e84-996e-560c-91fc-cce0f29e320e")
