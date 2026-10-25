"""UC-055: Required replacement update rollback and failure (P0, mandatory).

Fixture g6_replaced: c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0 with consumers 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^1.0) and a768d335-0cf7-597a-b1ba-0eb542495aec
from the catalog; db33e43e-e652-5767-8e21-27fb3cef9723 is a `gitpm fork` of c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 (payload edited, 1.0.1,
replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0) installed from /fx/repos/db33e43e-e652-5767-8e21-27fb3cef9723 and bound by
`gitpm replace c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 --with db33e43e-e652-5767-8e21-27fb3cef9723`.

Each gitpm call is a new process, so "reopen the manager and recheck" is the `list` after
every step. The library entry points (Manager::update/rollback on a replaced provider)
are exercised by the library test dependencies.rs (rust: tests of this suite).
"""

import json
import shutil

from gitpmtest import T, case

ORIGINAL, FORK = "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723"
CONSUMERS = ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3"]
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def resolution(t: T) -> dict:
    """consumer → (provider, offered version, satisfied) for c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9, read by a new process."""
    view = resolved(t)
    out = {}
    for consumer in CONSUMERS:
        row = next(r for r in view[consumer]["dependencies"] if r["id"] == ORIGINAL)
        out[consumer] = (row["provider"], row["version"], row["satisfied"])
    return out


def fork_release(t: T, version: str, payload: str | None = None, declares: str | None = "1.0.0",
                 manifest_text: str | None = None, drop_dist: bool = False) -> str:
    """Commit a new release of the fork (keeping forkOf) and tag it vVERSION."""
    repo = t.repo(FORK)
    m = json.loads((repo / "plugin.json").read_text())
    m["version"] = version
    if declares is None:
        m.pop("replaces", None)
    else:
        m["replaces"] = {"id": ORIGINAL, "version": declares}
    files = {"plugin.json": manifest_text if manifest_text is not None else json.dumps(m, indent=2) + "\n"}
    if payload is not None:
        files["dist/index.js"] = payload
    t.write(repo, files)
    if drop_dist:
        shutil.rmtree(repo / "dist")
    sha = t.commit(repo, f"{FORK} {version}")
    t.tag(repo, f"v{version}")
    return sha


def bound(t: T, version: str = "1.0.0"):
    t.eq(t.state().get("replacements"), {ORIGINAL: FORK}, "binding")
    t.eq(resolution(t), {c: (FORK, version, True) for c in CONSUMERS}, "consumers resolve to the fork")


@case(fixture="g6_replaced", spec=["dist/inactive-packages-change-state-and-clone-only#update-of-a-replaced-package", "fork-replacement/active-fork-rules"])
def original_release_does_not_switch_back(t: T):
    """The original's new release is installed independently; consumers stay on the fork."""
    fork_files = t.snap(t.worktree(FORK), t.dist(FORK))
    sha = t.release(ORIGINAL, "1.1.0")
    t.gitpm_ok("update", ORIGINAL)
    t.eq(t.package(ORIGINAL)["current"], {"version": "1.1.0", "commit": sha}, "original updated")
    t.ok(not t.dist(ORIGINAL).exists(), "the replaced original stays undeployed (only active packages)")
    bound(t)
    t.same(fork_files, "fork clone and dist")


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules"])
def fork_update_preserves_binding(t: T):
    """A fork release (new payload, declares 1.1.0) is followed by the provider and its resolution."""
    previous = t.package(FORK)["current"]
    sha = fork_release(t, "1.0.2", payload="fork payload 2\n", declares="1.1.0")
    t.gitpm_ok("update", FORK)
    p = t.package(FORK)
    t.eq(p["current"], {"version": "1.0.2", "commit": sha}, "fork current")
    t.eq(p["previous"], previous, "fork previous")
    t.eq(t.rev(t.worktree(FORK)), sha, "fork clone HEAD")
    t.eq((t.dist(FORK) / "index.js").read_text(), "fork payload 2\n", "provider payload")
    bound(t, "1.1.0")


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules", "updates/rollback-swaps-current-and-previous"])
def fork_rollback_follows_revision(t: T):
    """Rollback of the fork returns provider bytes, version and declared resolution to 1.0.1."""
    before = t.package(FORK)["current"]
    fork_release(t, "1.0.2", payload="fork payload 2\n", declares="1.1.0")
    t.gitpm_ok("update", FORK)
    t.gitpm_ok("rollback", FORK)
    p = t.package(FORK)
    t.eq(p["current"], before, "fork current after rollback")
    t.eq(p["previous"]["version"], "1.0.2", "fork previous after rollback")
    t.eq(t.rev(t.worktree(FORK)), before["commit"], "fork clone HEAD")
    t.eq((t.dist(FORK) / "index.js").read_text(), FORK_PAYLOAD, "provider payload")
    bound(t, "1.0.0")


@case(fixture="g6_replaced", spec=["dependencies/updates-keep-dependents-working", "errors/failing-operations-leave-the-workspace-unchanged"])
def incompatible_fork_release_refused(t: T):
    """A fork release declaring c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 2.0.0 (consumers need ^1.0) is refused; binding, state, dist kept."""
    before = t.snap(t.ws)
    fork_release(t, "1.0.2", payload="incompatible\n", declares="2.0.0")
    r = t.gitpm_err("update", FORK, kind="dependency_conflict")
    t.ok("9692fc07-23c7-5f20-8779-f09374f1a571" in r.err or "f5984d44-92bd-51ac-8e8f-d911dd824ef3" in r.err, "the refusal names a consumer")
    t.same(before, "workspace")
    bound(t)


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules"])
def fork_release_dropping_replaces_refused(t: T):
    """A fork release that no longer declares `replaces` cannot update the active provider."""
    before = t.snap(t.ws)
    fork_release(t, "1.0.2", payload="no replaces\n", declares=None)
    t.gitpm_err("update", FORK, kind="version_conflict", contains="no longer declares that it replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9")
    t.same(before, "workspace")
    bound(t)


@case(fixture="g6_replaced", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def unavailable_origin_preserves_binding(t: T):
    """The fork's origin is gone: update fails, workspace and binding preserved."""
    fork_release(t, "1.0.2", payload="never fetched\n")
    before = t.snap(t.ws)
    t.repo(FORK).rename(t.root / "moved-away")
    r = t.gitpm("update", FORK)
    t.ok(r.code != 0, "update fails")
    t.has(r.err, "Error kind:")
    t.same(before, "workspace")
    bound(t)
    t.eq((t.dist(FORK) / "index.js").read_text(), FORK_PAYLOAD, "provider payload")


@case(fixture="g6_replaced", spec=["errors/failing-operations-leave-the-workspace-unchanged", "dist/a-revision-without-dist-is-refused#release-without-dist"])
def corrupt_fork_release_preserves_binding(t: T):
    """Fork releases without dist/ and with a broken plugin.json are refused (invalid_manifest)."""
    before = t.snap(t.ws)
    fork_release(t, "1.0.2", drop_dist=True)
    t.gitpm_err("update", FORK, kind="invalid_manifest", contains="dist")
    t.same(before, "workspace")
    bound(t)
    fork_release(t, "1.0.3", payload="x\n", manifest_text='{"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "name": "fixture", "version": "1.0.3",')
    t.gitpm_err("update", FORK, "--ref", f"v1.0.3", kind="invalid_manifest")
    t.same(before, "workspace")
    bound(t)
    t.eq((t.dist(FORK) / "index.js").read_text(), FORK_PAYLOAD, "provider payload")
