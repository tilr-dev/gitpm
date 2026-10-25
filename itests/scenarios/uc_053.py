"""UC-053: Required fork replacement of a dependency (P0, mandatory).

c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0 with consumers 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^1.0) and the unrelated a768d335-0cf7-597a-b1ba-0eb542495aec are
installed from the catalog; db33e43e-e652-5767-8e21-27fb3cef9723 is a real `gitpm fork` (forkOf + replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9
1.0.0) with an edited payload, released as 1.0.1 and installed (fixture g6_forked).

The library half is exercised by the library's own test
dependencies.rs::fork_replacement_main_scenario (rust: tests of this suite); here the CLI
is checked. Since 0.5 dist/ holds only active packages: the replaced original keeps its
clone and state entry but loses dist/c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9.
Every gitpm invocation is a new process, i.e. a reopened manager.
"""

import json

from gitpmtest import T, case

ORIGINAL, FORK = "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723"
CONSUMERS = ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3"]
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
    """Installing the fork (forkOf + replaces in its manifest) leaves every consumer on c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9."""
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
    """replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723: persisted binding, both consumers on the fork payload,
    original installed and addressable, unrelated package untouched, binding survives a new process."""
    other = t.snap(t.worktree("a768d335-0cf7-597a-b1ba-0eb542495aec"), t.dist("a768d335-0cf7-597a-b1ba-0eb542495aec"))
    other_entry = t.package("a768d335-0cf7-597a-b1ba-0eb542495aec")
    original_files = t.snap(t.worktree(ORIGINAL))
    original_entry = t.package(ORIGINAL)

    r = t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    t.has(r.out, f"{ORIGINAL} replaced by {FORK} 1.0.1 (as {ORIGINAL} 1.0.0) for: 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3")
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
    t.same(other, "a768d335-0cf7-597a-b1ba-0eb542495aec clone and dist")
    t.eq(t.package("a768d335-0cf7-597a-b1ba-0eb542495aec"), other_entry, "a768d335-0cf7-597a-b1ba-0eb542495aec state entry")
    t.ok(not view["a768d335-0cf7-597a-b1ba-0eb542495aec"].get("dependencies"), "a768d335-0cf7-597a-b1ba-0eb542495aec has no dependencies")

    # Reopen once more: unchanged.
    t.eq(t.state().get("replacements"), {ORIGINAL: FORK}, "binding after reopen")
    t.eq(dependency(resolved(t), "f5984d44-92bd-51ac-8e8f-d911dd824ef3", ORIGINAL)["provider"], FORK, "f5984d44-92bd-51ac-8e8f-d911dd824ef3 after reopen")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global"])
def new_consumer_after_replacement_resolves_to_fork(t: T):
    """1a620c0a-aa3c-533c-9960-3d49016fa231 (c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 ^1.0) installed after the replacement resolves to db33e43e-e652-5767-8e21-27fb3cef9723; nothing new fetched."""
    t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    before = sorted(t.state()["packages"])
    original = t.snap(t.worktree(ORIGINAL), t.dist(ORIGINAL))  # dist absent: replaced
    t.gitpm_ok("install-catalog", "1a620c0a-aa3c-533c-9960-3d49016fa231")
    t.eq(sorted(t.state()["packages"]), sorted(before + ["1a620c0a-aa3c-533c-9960-3d49016fa231"]), "only 1a620c0a-aa3c-533c-9960-3d49016fa231 added")
    t.same(original, "original clone and dist")
    view = resolved(t)
    row = dependency(view, "1a620c0a-aa3c-533c-9960-3d49016fa231", ORIGINAL)
    t.eq((row["provider"], row["satisfied"]), (FORK, True), "1a620c0a-aa3c-533c-9960-3d49016fa231 resolution")
    t.eq(view[FORK].get("dependents"), ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "1a620c0a-aa3c-533c-9960-3d49016fa231"], "fork dependents")
    t.eq(t.state()["replacements"], {ORIGINAL: FORK}, "single binding")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global", "workspace-layout/replacements-and-forgotten-records"])
def binding_persists_after_reopen(t: T):
    """The binding is in state.json and every later process (list, check, unrelated mutations) keeps it."""
    t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    snapshot = (t.ws / "state.json").read_bytes()
    for args in [("list",), ("check",), ("disable", "a768d335-0cf7-597a-b1ba-0eb542495aec"), ("enable", "a768d335-0cf7-597a-b1ba-0eb542495aec")]:
        t.gitpm_ok(*args)
        t.eq(t.state().get("replacements"), {ORIGINAL: FORK}, f"binding after gitpm {' '.join(args)}")
    t.eq(json.loads((t.ws / "state.json").read_bytes()), json.loads(snapshot), "state after round trip")
    for consumer in CONSUMERS:
        t.eq(dependency(resolved(t), consumer, ORIGINAL)["provider"], FORK, f"{consumer} provider")
