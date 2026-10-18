"""Group g6: dependencies, global fork replacement and the consumer journey.

Families:
  * g6_core*   — lib/core 1.0.0/1.1.0/2.0.0 with consumers, a cycle and a missing dependency (UC-058/059);
  * g6_base*   — vendor/base 1.0.0, consumers app/a, app/b (^1.0), an unrelated other/x, a later
                 consumer app/c, and a real `gitpm fork` mine/base made the active provider (UC-053/055/056/066);
  * g6_compat* — vendor/base 1.4.0, forks declaring 1.4.0 / 1.2.0, consumers ^1.0, >=1.3 and a transitive
                 chain app/c → lib/mid → vendor/base (UC-054);
  * g6_journey — dan/hello 0.1.0 registered and installed from the catalog (UC-068).
"""

import json

from gitpmtest import T, fixture

# ------------------------------------------------------------------------- g6_core

CORE = {
    "lib/core": [("1.0.0", {}), ("1.1.0", {}), ("2.0.0", {})],
    "app/a": [("1.0.0", {"lib/core": "^1.0"})],
    "app/b": [("1.0.0", {"lib/core": "^2.0"})],
    "app/c": [("1.0.0", {"lib/core": "^1"})],
    "app/lonely": [("1.0.0", {"lib/missing": "^1"})],
    "x/a": [("1.0.0", {"x/b": "^1"})],
    "x/b": [("1.0.0", {"x/a": "^1"})],
    "other/x": [("1.0.0", {})],
}


@fixture()
def g6_core(t: T):
    """Author repos: lib/core 1.0.0, 1.1.0, 2.0.0; app/a (lib/core ^1.0), app/b (^2.0), app/c (^1),
    app/lonely (lib/missing ^1), the cycle x/a ↔ x/b, and other/x without dependencies."""
    for id, releases in CORE.items():
        first, deps = releases[0]
        t.new_package(id, first, deps=deps)
        for version, deps in releases[1:]:
            t.release(id, version, deps=deps)


@g6_core.check
def _(t: T):
    for id, releases in CORE.items():
        for version, deps in releases:
            m = json.loads(t.git(id, "show", f"{id}@{version}:plugin.json"))
            t.eq(m.get("dependencies", {}), deps, f"{id}@{version} dependencies")


@fixture(requires=["g6_core"], gitpm=True)
def g6_core_catalog(t: T):
    """Every g6_core release registered in /fx/catalog; empty workspace."""
    for id, releases in CORE.items():
        for version, _ in releases:
            t.gitpm_ok("register", t.repo(id), "--ref", f"{id}@{version}")


@g6_core_catalog.check
def _(t: T):
    for id, releases in CORE.items():
        for version, _ in releases:
            t.ok(t.catalog_release(id, version).is_file(), f"{id} {version} registered")
    t.ok(not t.ws.exists(), "workspace untouched")


@fixture(requires=["g6_core_catalog"], gitpm=True)
def g6_core_deps_installed(t: T):
    """From the catalog: app/a and app/c (both on lib/core, pulled in at 1.1.0) and other/x."""
    for id in ["app/a", "app/c", "other/x"]:
        t.gitpm_ok("install-catalog", id)


@g6_core_deps_installed.check
def _(t: T):
    state = t.state()
    t.eq(sorted(state["packages"]), ["app/a", "app/c", "lib/core", "other/x"], "installed")
    t.eq(state["packages"]["lib/core"]["current"]["version"], "1.1.0", "lib/core version")
    resolved = {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}
    t.eq(resolved["lib/core"].get("dependents"), ["app/a", "app/c"], "lib/core dependents")


# ------------------------------------------------------------------------- g6_base

BASE = {
    "vendor/base": {},
    "app/a": {"vendor/base": "^1.0"},
    "app/b": {"vendor/base": "^1.0"},
    "app/c": {"vendor/base": "^1.0"},
    "other/x": {},
}
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


@fixture()
def g6_base(t: T):
    """Author repos at 1.0.0: vendor/base; app/a, app/b, app/c (vendor/base ^1.0); other/x."""
    for id, deps in BASE.items():
        t.new_package(id, "1.0.0", deps=deps)


@g6_base.check
def _(t: T):
    for id, deps in BASE.items():
        m = json.loads(t.git(id, "show", f"{id}@1.0.0:plugin.json"))
        t.eq(m.get("dependencies", {}), deps, f"{id} dependencies")


@fixture(requires=["g6_base"], gitpm=True)
def g6_base_catalog(t: T):
    """All g6_base packages registered at 1.0.0 in /fx/catalog; empty workspace."""
    for id in BASE:
        t.gitpm_ok("register", t.repo(id), "--ref", f"{id}@1.0.0")


@g6_base_catalog.check
def _(t: T):
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, *BASE)
    t.ok(not t.ws.exists(), "workspace untouched")


@fixture(requires=["g6_base_catalog"], gitpm=True)
def g6_base_installed(t: T):
    """app/a, app/b, other/x installed from the catalog; vendor/base pulled in as their dependency."""
    for id in ["app/a", "app/b", "other/x"]:
        t.gitpm_ok("install-catalog", id)


@g6_base_installed.check
def _(t: T):
    t.eq(sorted(t.state()["packages"]), ["app/a", "app/b", "other/x", "vendor/base"], "installed")
    t.ok(not t.state().get("replacements"), "no replacement")


@fixture(requires=["g6_base_installed"], gitpm=True)
def g6_forked(t: T):
    """g6_base_installed + `gitpm fork vendor/base --ns mine --into /fx/repos/mine/base`, payload edited,
    `gitpm commit … 1.0.1` (still replaces vendor/base 1.0.0), mine/base installed from Git. No replacement."""
    fork = t.repo("mine/base")
    t.gitpm_ok("fork", "vendor/base", "--ns", "mine", "--into", fork)
    t.write(fork, {"dist/index.js": FORK_PAYLOAD})
    t.gitpm_ok("commit", fork, "1.0.1")
    t.gitpm_ok("install", fork)


@g6_forked.check
def _(t: T):
    m = t.package("mine/base")
    t.eq(m["current"]["version"], "1.0.1", "fork version")
    t.eq(m["forkOf"]["id"], "vendor/base", "forkOf")
    t.eq(m["replaces"], {"id": "vendor/base", "version": "1.0.0"}, "replaces")
    t.eq((t.dist("mine/base") / "index.js").read_text(), FORK_PAYLOAD, "fork payload")
    t.ok(not t.state().get("replacements"), "installing the fork redirects nothing")


@fixture(requires=["g6_forked"], gitpm=True)
def g6_replaced(t: T):
    """g6_forked + `gitpm replace vendor/base --with mine/base` (no prompt: every consumer satisfied)."""
    t.gitpm_ok("replace", "vendor/base", "--with", "mine/base")


@g6_replaced.check
def _(t: T):
    t.eq(t.state().get("replacements"), {"vendor/base": "mine/base"}, "binding")
    resolved = {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}
    for consumer in ["app/a", "app/b"]:
        t.eq(resolved[consumer]["dependencies"][0]["provider"], "mine/base", f"{consumer} provider")


# ----------------------------------------------------------------------- g6_compat

COMPAT = {
    "vendor/base": ({}, {}),
    "app/a": ({"vendor/base": "^1.0"}, {}),
    "app/b": ({"vendor/base": ">=1.3"}, {}),
    "lib/mid": ({"vendor/base": "^1"}, {}),
    "app/c": ({"lib/mid": "^1"}, {}),
}
COMPAT_FORKS = {
    "mine/base": "1.4.0",
    "old/base": "1.2.0",
}


@fixture()
def g6_compat(t: T):
    """vendor/base 1.4.0 (1.0.0 first); app/a (^1.0), app/b (>=1.3), lib/mid (^1), app/c (lib/mid ^1);
    forks mine/base 0.1.0 (replaces vendor/base 1.4.0) and old/base 0.1.0 (replaces 1.2.0)."""
    t.new_package("vendor/base", "1.0.0")
    t.release("vendor/base", "1.4.0")
    for id, (deps, _) in COMPAT.items():
        if id != "vendor/base":
            t.new_package(id, "1.0.0", deps=deps)
    for id, declares in COMPAT_FORKS.items():
        t.new_package(id, "0.1.0", extra={"replaces": {"id": "vendor/base", "version": declares}})


@g6_compat.check
def _(t: T):
    for id, declares in COMPAT_FORKS.items():
        m = json.loads(t.git(id, "show", f"{id}@0.1.0:plugin.json"))
        t.eq(m["replaces"], {"id": "vendor/base", "version": declares}, f"{id} replaces")
    t.eq(json.loads(t.git("app/c", "show", "HEAD:plugin.json"))["dependencies"], {"lib/mid": "^1"}, "app/c deps")


@fixture(requires=["g6_compat"], gitpm=True)
def g6_compat_installed(t: T):
    """vendor/base 1.4.0, app/a, app/b, lib/mid, app/c registered and app/a, app/b, app/c installed from
    the catalog (vendor/base and lib/mid pulled in); forks mine/base and old/base installed from Git."""
    t.gitpm_ok("register", t.repo("vendor/base"), "--ref", "vendor/base@1.4.0")
    for id in ["app/a", "app/b", "lib/mid", "app/c"]:
        t.gitpm_ok("register", t.repo(id), "--ref", f"{id}@1.0.0")
    for id in ["app/a", "app/b", "app/c"]:
        t.gitpm_ok("install-catalog", id)
    for id in COMPAT_FORKS:
        t.gitpm_ok("install", t.repo(id))


@g6_compat_installed.check
def _(t: T):
    state = t.state()
    t.eq(sorted(state["packages"]),
         ["app/a", "app/b", "app/c", "lib/mid", "mine/base", "old/base", "vendor/base"], "installed")
    t.eq(state["packages"]["vendor/base"]["current"]["version"], "1.4.0", "vendor/base version")
    t.ok(not state.get("replacements"), "no replacement")


# ---------------------------------------------------------------------- g6_journey


@fixture(requires=["hello"], gitpm=True)
def g6_journey(t: T):
    """dan/hello 0.1.0 registered in /fx/catalog (verified) and installed from it with install-catalog."""
    t.gitpm_ok("register", t.repo("dan/hello"), "--ref", "dan/hello@0.1.0")
    # A verified catalog: the installed package records it (protected).
    t.edit_json(t.catalog / "catalog.json", lambda c: c.update(verified=True))
    t.gitpm_ok("install-catalog", "dan/hello")


@g6_journey.check
def _(t: T):
    p = t.package("dan/hello")
    t.eq(p["current"], {"version": "0.1.0", "commit": t.rev("dan/hello", "dan/hello@0.1.0")}, "current")
    t.eq(p["previous"], None, "previous")
    t.ok(p.get("catalog"), "catalog recorded")
    t.eq(t.rev(t.clone("dan/hello")), p["current"]["commit"], "clone HEAD")
