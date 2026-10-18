"""UC-053: Required fork replacement of a dependency (P0, mandatory).

vendor/base 1.0.0 with consumers app/a and app/b (^1.0) and the unrelated other/x are
installed from the catalog; mine/base is a real `gitpm fork` (forkOf + replaces vendor/base
1.0.0) with an edited payload, released as 1.0.1 and installed (fixture g6_forked).

The library half is exercised by the library's own test
dependencies.rs::fork_replacement_main_scenario (rust: tests of this suite); here the CLI
is checked. Since 0.5 dist/ holds only active packages: the replaced original keeps its
clone and state entry but loses dist/vendor/base.
Every gitpm invocation is a new process, i.e. a reopened manager.
"""

import json

from gitpmtest import T, case

ORIGINAL, FORK = "vendor/base", "mine/base"
CONSUMERS = ["app/a", "app/b"]
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def dependency(view: dict, consumer: str, dep: str) -> dict:
    for row in view[consumer].get("dependencies", []):
        if row["id"] == dep:
            return row
    raise AssertionError(f"{consumer} has no dependency {dep}: {view[consumer]}")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global", "fork-replacement/fork-metadata"])
def provenance_alone_does_not_redirect(t: T):
    """Installing the fork (forkOf + replaces in its manifest) leaves every consumer on vendor/base."""
    fork = t.package(FORK)
    t.eq(fork["forkOf"]["id"], ORIGINAL, "forkOf id")
    t.eq(fork["forkOf"]["commit"], t.package(ORIGINAL)["current"]["commit"], "forkOf commit")
    t.eq(fork["replaces"], {"id": ORIGINAL, "version": "1.0.0"}, "declared replacement")
    t.ok(not t.state().get("replacements"), "no binding in state.json")
    view = resolved(t)
    for consumer in CONSUMERS:
        row = dependency(view, consumer, ORIGINAL)
        t.eq((row["provider"], row["satisfied"]), (ORIGINAL, True), f"{consumer} provider")
    t.ok(not view[FORK].get("replacing"), "fork replaces nothing yet")
    t.ok(not view[FORK].get("dependents"), "fork has no dependents")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global#output-variants", "fork-replacement/replacement-is-explicit-and-global#replace-for-all-consumers", "dist/replace-removes-the-original-s-dist", "dist/dist-holds-exactly-the-active-packages#enabled-but-replaced"])
def explicit_replacement_is_global_and_inspectable(t: T):
    """replace vendor/base --with mine/base: persisted binding, both consumers on the fork payload,
    original installed and addressable, unrelated package untouched, binding survives a new process."""
    other = t.snap(t.clone("other/x"), t.dist("other/x"))
    other_entry = t.package("other/x")
    original_files = t.snap(t.clone(ORIGINAL))
    original_entry = t.package(ORIGINAL)

    r = t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    t.has(r.out, f"{ORIGINAL} replaced by {FORK} 1.0.1 (as {ORIGINAL} 1.0.0) for: app/a, app/b")
    t.lacks(r.err, "Warning", "[y/N]")

    # Persisted, inspectable binding.
    raw = json.loads((t.ws / "state.json").read_text())
    t.eq(raw.get("replacements"), {ORIGINAL: FORK}, "state.json replacements")
    view = resolved(t)  # a new process: the manager is reopened
    for consumer in CONSUMERS:
        row = dependency(view, consumer, ORIGINAL)
        t.eq((row["provider"], row["version"], row["satisfied"]), (FORK, "1.0.0", True), f"{consumer} resolution")
    t.eq(view[FORK].get("replacing"), [ORIGINAL], "fork replacing")
    t.eq(view[FORK].get("dependents"), CONSUMERS, "fork dependents")
    t.ok(not view[ORIGINAL].get("dependents"), "original has no dependents while replaced")
    check = t.gitpm_ok("check").out
    t.has(check, f"Replacement: {ORIGINAL} → {FORK} (for every consumer)")

    # The selected provider's payload is the fork's.
    t.eq((t.dist(FORK) / "index.js").read_text(), FORK_PAYLOAD, "provider payload")
    t.ok(not t.dist(ORIGINAL).exists(), "the replaced original is not deployed")

    # The original stays installed and independently addressable.
    t.same(original_files, "original clone")
    t.eq(t.package(ORIGINAL), original_entry, "original state entry")
    t.gitpm_ok("disable", ORIGINAL)
    t.eq(t.package(ORIGINAL)["enabled"], False, "original disabled independently")
    t.gitpm_ok("enable", ORIGINAL)
    t.gitpm_ok("update", ORIGINAL)  # its own update path still works (no-op)
    t.eq(t.package(ORIGINAL), original_entry, "original state entry after addressing it")
    t.ok(not t.dist(ORIGINAL).exists(), "still not deployed while replaced")

    # The unrelated package is unchanged.
    t.same(other, "other/x clone and dist")
    t.eq(t.package("other/x"), other_entry, "other/x state entry")
    t.ok(not view["other/x"].get("dependencies"), "other/x has no dependencies")

    # Reopen once more: unchanged.
    t.eq(t.state().get("replacements"), {ORIGINAL: FORK}, "binding after reopen")
    t.eq(dependency(resolved(t), "app/b", ORIGINAL)["provider"], FORK, "app/b after reopen")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global"])
def new_consumer_after_replacement_resolves_to_fork(t: T):
    """app/c (vendor/base ^1.0) installed after the replacement resolves to mine/base; nothing new fetched."""
    t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    before = sorted(t.state()["packages"])
    original = t.snap(t.clone(ORIGINAL), t.dist(ORIGINAL))  # dist absent: replaced
    t.gitpm_ok("install-catalog", "app/c")
    t.eq(sorted(t.state()["packages"]), sorted(before + ["app/c"]), "only app/c added")
    t.same(original, "original clone and dist")
    view = resolved(t)
    row = dependency(view, "app/c", ORIGINAL)
    t.eq((row["provider"], row["satisfied"]), (FORK, True), "app/c resolution")
    t.eq(view[FORK].get("dependents"), ["app/a", "app/b", "app/c"], "fork dependents")
    t.eq(t.state()["replacements"], {ORIGINAL: FORK}, "single binding")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global", "workspace-layout/replacements-and-forgotten-records"])
def binding_persists_after_reopen(t: T):
    """The binding is in state.json and every later process (list, check, unrelated mutations) keeps it."""
    t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    snapshot = (t.ws / "state.json").read_bytes()
    for args in [("list",), ("check",), ("disable", "other/x"), ("enable", "other/x")]:
        t.gitpm_ok(*args)
        t.eq(t.state().get("replacements"), {ORIGINAL: FORK}, f"binding after gitpm {' '.join(args)}")
    t.eq(json.loads((t.ws / "state.json").read_bytes()), json.loads(snapshot), "state after round trip")
    for consumer in CONSUMERS:
        t.eq(dependency(resolved(t), consumer, ORIGINAL)["provider"], FORK, f"{consumer} provider")
