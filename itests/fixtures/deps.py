"""Dependency graph with replacement forks, as in the stand's requirements.rs."""

import json

from gitpmtest import T, fixture

GRAPH = {
    "vendor/base": ({}, {}),
    "app/a": ({"vendor/base": "^1.0"}, {}),
    "app/b": ({"vendor/base": ">=0.9.0"}, {}),
    "mine/base": ({}, {"replaces": {"id": "vendor/base", "version": "1.0.0"}}),
    "old/base": ({}, {"replaces": {"id": "vendor/base", "version": "0.9.0"}}),
}


@fixture()
def deps(t: T):
    """vendor/base, its consumers app/a (^1.0) and app/b (>=0.9.0), and forks mine/base
    (replaces vendor/base 1.0.0) and old/base (replaces 0.9.0); each released at 1.0.0."""
    for id, (dependencies, extra) in GRAPH.items():
        t.new_package(id, "1.0.0", deps=dependencies, extra=extra)


@deps.check
def _(t: T):
    for id, (dependencies, extra) in GRAPH.items():
        manifest = json.loads(t.git(id, "show", f"{id}@1.0.0:plugin.json"))
        t.eq(manifest.get("dependencies", {}), dependencies, f"{id} dependencies")
        t.eq(manifest.get("replaces"), extra.get("replaces"), f"{id} replaces")


@fixture(requires=["deps"], gitpm=True)
def deps_catalog(t: T):
    """All five packages of `deps` registered at 1.0.0 in /fx/catalog; empty workspace."""
    for id in GRAPH:
        t.gitpm_ok("register", t.repo(id), "--ref", f"{id}@1.0.0")


@deps_catalog.check
def _(t: T):
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, *GRAPH)
    t.ok(not t.ws.exists(), "workspace untouched")


@fixture(requires=["deps_catalog"], gitpm=True)
def deps_installed(t: T):
    """app/a, app/b, mine/base, old/base installed from the catalog; vendor/base pulled in as a dependency."""
    for id in ["app/a", "app/b", "mine/base", "old/base"]:
        t.gitpm_ok("install-catalog", id)


@deps_installed.check
def _(t: T):
    state = t.gitpm_ok("list").json()
    t.eq(sorted(state["packages"]), sorted(GRAPH), "installed packages")
    t.ok("replacements" not in state or not state["replacements"], "no active replacement")
