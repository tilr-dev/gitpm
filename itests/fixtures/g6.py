"""Group g6: dependencies, global fork replacement and the consumer journey.

Families:
  * g6_core*   — 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.0.0/1.1.0/2.0.0 with consumers, a cycle and a missing dependency (UC-058/059);
  * g6_base*   — c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0, consumers 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^1.0), an unrelated a768d335-0cf7-597a-b1ba-0eb542495aec, a later
                 consumer 1a620c0a-aa3c-533c-9960-3d49016fa231, and a real `gitpm fork` db33e43e-e652-5767-8e21-27fb3cef9723 made the active provider (UC-053/055/056/066);
  * g6_compat* — c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0, forks declaring 1.4.0 / 1.2.0, consumers ^1.0, >=1.3 and a transitive
                 chain 1a620c0a-aa3c-533c-9960-3d49016fa231 → c4304791-586c-54cb-a7c6-27bd09f28dbd → c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 (UC-054);
  * g6_journey — 3dce1568-569c-5b53-9321-37185e751947 0.1.0 registered and installed from the catalog (UC-068).
"""

import json

from gitpmtest import T, fixture

# ------------------------------------------------------------------------- g6_core

CORE = {
    "757fd5ca-2b6a-5b2f-8a5d-60980b028665": [("1.0.0", {}), ("1.1.0", {}), ("2.0.0", {})],
    "9692fc07-23c7-5f20-8779-f09374f1a571": [("1.0.0", {"757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1.0"})],
    "f5984d44-92bd-51ac-8e8f-d911dd824ef3": [("1.0.0", {"757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^2.0"})],
    "1a620c0a-aa3c-533c-9960-3d49016fa231": [("1.0.0", {"757fd5ca-2b6a-5b2f-8a5d-60980b028665": "^1"})],
    "2a8af310-7783-5cbb-8218-c875e4f8a3e6": [("1.0.0", {"0276fe29-bef5-5d0b-aae9-f9920c809729": "^1"})],
    "d35b3151-28fd-557a-a363-b5f18ae491c8": [("1.0.0", {"3a53cdea-2cb1-5653-bf22-898b679440ee": "^1"})],
    "3a53cdea-2cb1-5653-bf22-898b679440ee": [("1.0.0", {"d35b3151-28fd-557a-a363-b5f18ae491c8": "^1"})],
    "a768d335-0cf7-597a-b1ba-0eb542495aec": [("1.0.0", {})],
}


@fixture()
def g6_core(t: T):
    """Author repos: 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.0.0, 1.1.0, 2.0.0; 9692fc07-23c7-5f20-8779-f09374f1a571 (757fd5ca-2b6a-5b2f-8a5d-60980b028665 ^1.0), f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^2.0), 1a620c0a-aa3c-533c-9960-3d49016fa231 (^1),
    2a8af310-7783-5cbb-8218-c875e4f8a3e6 (0276fe29-bef5-5d0b-aae9-f9920c809729 ^1), the cycle d35b3151-28fd-557a-a363-b5f18ae491c8 ↔ 3a53cdea-2cb1-5653-bf22-898b679440ee, and a768d335-0cf7-597a-b1ba-0eb542495aec without dependencies."""
    for id, releases in CORE.items():
        first, deps = releases[0]
        t.new_package(id, first, deps=deps)
        for version, deps in releases[1:]:
            t.release(id, version, deps=deps)


@g6_core.check
def _(t: T):
    for id, releases in CORE.items():
        for version, deps in releases:
            m = json.loads(t.git(id, "show", f"v{version}:plugin.json"))
            t.eq(m.get("dependencies", {}), deps, f"{id}@{version} dependencies")


@fixture(requires=["g6_core"], gitpm=True)
def g6_core_catalog(t: T):
    """Every g6_core release registered in /fx/catalog; empty workspace."""
    for id, releases in CORE.items():
        for version, _ in releases:
            t.gitpm_ok("register", t.repo(id), "--ref", f"v{version}")


@g6_core_catalog.check
def _(t: T):
    for id, releases in CORE.items():
        for version, _ in releases:
            t.ok(t.catalog_release(id, version).is_file(), f"{id} {version} registered")
    t.ok(not t.ws.exists(), "workspace untouched")


@fixture(requires=["g6_core_catalog"], gitpm=True)
def g6_core_deps_installed(t: T):
    """From the catalog: 9692fc07-23c7-5f20-8779-f09374f1a571 and 1a620c0a-aa3c-533c-9960-3d49016fa231 (both on 757fd5ca-2b6a-5b2f-8a5d-60980b028665, pulled in at 1.1.0) and a768d335-0cf7-597a-b1ba-0eb542495aec."""
    for id in ["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231", "a768d335-0cf7-597a-b1ba-0eb542495aec"]:
        t.gitpm_ok("install-catalog", id)


@g6_core_deps_installed.check
def _(t: T):
    state = t.state()
    t.eq(sorted(state["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231", "757fd5ca-2b6a-5b2f-8a5d-60980b028665", "a768d335-0cf7-597a-b1ba-0eb542495aec"]), "installed")
    t.eq(state["packages"]["757fd5ca-2b6a-5b2f-8a5d-60980b028665"]["current"]["version"], "1.1.0", "757fd5ca-2b6a-5b2f-8a5d-60980b028665 version")
    resolved = {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}
    t.eq(resolved["757fd5ca-2b6a-5b2f-8a5d-60980b028665"].get("dependents"), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231"]), "757fd5ca-2b6a-5b2f-8a5d-60980b028665 dependents")


# ------------------------------------------------------------------------- g6_base

BASE = {
    "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": {},
    "9692fc07-23c7-5f20-8779-f09374f1a571": {"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "^1.0"},
    "f5984d44-92bd-51ac-8e8f-d911dd824ef3": {"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "^1.0"},
    "1a620c0a-aa3c-533c-9960-3d49016fa231": {"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "^1.0"},
    "a768d335-0cf7-597a-b1ba-0eb542495aec": {},
}
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


@fixture()
def g6_base(t: T):
    """Author repos at 1.0.0: c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9; 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3, 1a620c0a-aa3c-533c-9960-3d49016fa231 (c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 ^1.0); a768d335-0cf7-597a-b1ba-0eb542495aec."""
    for id, deps in BASE.items():
        t.new_package(id, "1.0.0", deps=deps)


@g6_base.check
def _(t: T):
    for id, deps in BASE.items():
        m = json.loads(t.git(id, "show", f"v1.0.0:plugin.json"))
        t.eq(m.get("dependencies", {}), deps, f"{id} dependencies")


@fixture(requires=["g6_base"], gitpm=True)
def g6_base_catalog(t: T):
    """All g6_base packages registered at 1.0.0 in /fx/catalog; empty workspace."""
    for id in BASE:
        t.gitpm_ok("register", t.repo(id), "--ref", f"v1.0.0")


@g6_base_catalog.check
def _(t: T):
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, *BASE)
    t.ok(not t.ws.exists(), "workspace untouched")


@fixture(requires=["g6_base_catalog"], gitpm=True)
def g6_base_installed(t: T):
    """9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3, a768d335-0cf7-597a-b1ba-0eb542495aec installed from the catalog; c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 pulled in as their dependency."""
    for id in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "a768d335-0cf7-597a-b1ba-0eb542495aec"]:
        t.gitpm_ok("install-catalog", id)


@g6_base_installed.check
def _(t: T):
    t.eq(sorted(t.state()["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "a768d335-0cf7-597a-b1ba-0eb542495aec", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]), "installed")
    t.ok(not t.state().get("replacements"), "no replacement")


@fixture(requires=["g6_base_installed"], gitpm=True)
def g6_forked(t: T):
    """A committed fork-shaped author fixture with a fixed initial UUID.
    Actual fork generation is exercised by the consumer journey scenarios."""
    original = "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"
    fork = t.repo("db33e43e-e652-5767-8e21-27fb3cef9723")
    current = t.package(original)["current"]
    t.run("git", "clone", "-q", t.repo(original), fork, check=True)
    t.git(fork, "checkout", "-q", "-b", "fork", current["commit"])
    t.git(fork, "remote", "rename", "origin", "upstream")
    def identify(m):
        m["id"] = "db33e43e-e652-5767-8e21-27fb3cef9723"
        m["forkOf"] = {"id": original, **current, "source": t.package(original)["source"]}
        m["replaces"] = {"id": original, "version": current["version"]}
    t.edit_json(fork / "plugin.json", identify)
    t.commit(fork, "Create fork fixture")
    t.write(fork, {"dist/index.js": FORK_PAYLOAD})
    t.gitpm_ok("commit", fork, "1.0.1")
    t.gitpm_ok("install", fork)


@g6_forked.check
def _(t: T):
    m = t.package("db33e43e-e652-5767-8e21-27fb3cef9723")
    t.eq(m["current"]["version"], "1.0.1", "fork version")
    t.eq(m["forkOf"]["id"], "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "forkOf")
    t.eq(m["replaces"], {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": "1.0.0"}, "replaces")
    t.eq((t.dist("db33e43e-e652-5767-8e21-27fb3cef9723") / "index.js").read_text(), FORK_PAYLOAD, "fork payload")
    t.ok(not t.state().get("replacements"), "installing the fork redirects nothing")


@fixture(requires=["g6_forked"], gitpm=True)
def g6_replaced(t: T):
    """g6_forked + `gitpm replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723` (no prompt: every consumer satisfied)."""
    t.gitpm_ok("replace", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "--with", "db33e43e-e652-5767-8e21-27fb3cef9723")


@g6_replaced.check
def _(t: T):
    t.eq(t.state().get("replacements"), {"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "db33e43e-e652-5767-8e21-27fb3cef9723"}, "binding")
    resolved = {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}
    for consumer in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3"]:
        t.eq(resolved[consumer]["dependencies"][0]["provider"], "db33e43e-e652-5767-8e21-27fb3cef9723", f"{consumer} provider")


# ----------------------------------------------------------------------- g6_compat

COMPAT = {
    "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": ({}, {}),
    "9692fc07-23c7-5f20-8779-f09374f1a571": ({"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "^1.0"}, {}),
    "f5984d44-92bd-51ac-8e8f-d911dd824ef3": ({"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": ">=1.3"}, {}),
    "c4304791-586c-54cb-a7c6-27bd09f28dbd": ({"c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9": "^1"}, {}),
    "1a620c0a-aa3c-533c-9960-3d49016fa231": ({"c4304791-586c-54cb-a7c6-27bd09f28dbd": "^1"}, {}),
}
COMPAT_FORKS = {
    "db33e43e-e652-5767-8e21-27fb3cef9723": "1.4.0",
    "4e02af40-ea75-510c-b524-5f21e6de2d55": "1.2.0",
}


@fixture()
def g6_compat(t: T):
    """c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0 (1.0.0 first); 9692fc07-23c7-5f20-8779-f09374f1a571 (^1.0), f5984d44-92bd-51ac-8e8f-d911dd824ef3 (>=1.3), c4304791-586c-54cb-a7c6-27bd09f28dbd (^1), 1a620c0a-aa3c-533c-9960-3d49016fa231 (c4304791-586c-54cb-a7c6-27bd09f28dbd ^1);
    forks db33e43e-e652-5767-8e21-27fb3cef9723 0.1.0 (replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0) and 4e02af40-ea75-510c-b524-5f21e6de2d55 0.1.0 (replaces 1.2.0)."""
    t.new_package("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.0.0")
    t.release("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "1.4.0")
    for id, (deps, _) in COMPAT.items():
        if id != "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9":
            t.new_package(id, "1.0.0", deps=deps)
    for id, declares in COMPAT_FORKS.items():
        t.new_package(id, "0.1.0", extra={"replaces": {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": declares}})


@g6_compat.check
def _(t: T):
    for id, declares in COMPAT_FORKS.items():
        m = json.loads(t.git(id, "show", f"v0.1.0:plugin.json"))
        t.eq(m["replaces"], {"id": "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "version": declares}, f"{id} replaces")
    t.eq(json.loads(t.git("1a620c0a-aa3c-533c-9960-3d49016fa231", "show", "HEAD:plugin.json"))["dependencies"], {"c4304791-586c-54cb-a7c6-27bd09f28dbd": "^1"}, "1a620c0a-aa3c-533c-9960-3d49016fa231 deps")


@fixture(requires=["g6_compat"], gitpm=True)
def g6_compat_installed(t: T):
    """c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0, 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3, c4304791-586c-54cb-a7c6-27bd09f28dbd, 1a620c0a-aa3c-533c-9960-3d49016fa231 registered and 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3, 1a620c0a-aa3c-533c-9960-3d49016fa231 installed from
    the catalog (c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 and c4304791-586c-54cb-a7c6-27bd09f28dbd pulled in); forks db33e43e-e652-5767-8e21-27fb3cef9723 and 4e02af40-ea75-510c-b524-5f21e6de2d55 installed from Git."""
    t.gitpm_ok("register", t.repo("c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"), "--ref", "v1.4.0")
    for id in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "c4304791-586c-54cb-a7c6-27bd09f28dbd", "1a620c0a-aa3c-533c-9960-3d49016fa231"]:
        t.gitpm_ok("register", t.repo(id), "--ref", f"v1.0.0")
    for id in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "1a620c0a-aa3c-533c-9960-3d49016fa231"]:
        t.gitpm_ok("install-catalog", id)
    for id in COMPAT_FORKS:
        t.gitpm_ok("install", t.repo(id))


@g6_compat_installed.check
def _(t: T):
    state = t.state()
    t.eq(sorted(state["packages"]),
         sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "1a620c0a-aa3c-533c-9960-3d49016fa231", "c4304791-586c-54cb-a7c6-27bd09f28dbd", "db33e43e-e652-5767-8e21-27fb3cef9723", "4e02af40-ea75-510c-b524-5f21e6de2d55", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]), "installed")
    t.eq(state["packages"]["c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"]["current"]["version"], "1.4.0", "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 version")
    t.ok(not state.get("replacements"), "no replacement")


# ---------------------------------------------------------------------- g6_journey


@fixture(requires=["hello"], gitpm=True)
def g6_journey(t: T):
    """3dce1568-569c-5b53-9321-37185e751947 0.1.0 registered in /fx/catalog (verified) and installed from it with install-catalog."""
    t.gitpm_ok("register", t.repo("3dce1568-569c-5b53-9321-37185e751947"), "--ref", "v0.1.0")
    # A verified catalog: the installed package records it (protected).
    t.edit_json(t.catalog / "catalog.json", lambda c: c.update(verified=True))
    t.gitpm_ok("install-catalog", "3dce1568-569c-5b53-9321-37185e751947")


@g6_journey.check
def _(t: T):
    p = t.package("3dce1568-569c-5b53-9321-37185e751947")
    t.eq(p["current"], {"version": "0.1.0", "commit": t.rev("3dce1568-569c-5b53-9321-37185e751947", "v0.1.0")}, "current")
    t.eq(p["previous"], None, "previous")
    t.ok(p.get("catalog"), "catalog recorded")
    t.eq(t.rev(t.worktree("3dce1568-569c-5b53-9321-37185e751947")), p["current"]["commit"], "clone HEAD")
