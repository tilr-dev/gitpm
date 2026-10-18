"""UC-017: Rollback and one-version history (P0).

Fixture g2_hello_history: dan/hello installed at 0.1.0, updated to 0.1.1, then 0.1.2
(current 0.1.2, previous 0.1.1). hello_installed for the no-history case.
"""

from gitpmtest import T, case

ID = "dan/hello"


def tagged(t: T, version: str) -> str:
    return t.rev(ID, f"{ID}@{version}")


def at(t: T, current: str, previous: str | None):
    p = t.plugin(ID)
    t.eq(p["current"], {"version": current, "commit": tagged(t, current)}, "current")
    t.eq(p["previous"], previous and {"version": previous, "commit": tagged(t, previous)}, "previous")
    t.eq(t.rev(t.clone(ID)), tagged(t, current), "clone HEAD")
    if p["enabled"]:
        t.eq((t.dist(ID) / "index.js").read_text(), f'export const greeting = "{ID} {current}";\n', "dist")
    else:
        t.ok(not t.dist(ID).exists(), "no dist/ID while disabled")


@case(fixture="hello_installed", spec=["updates/rollback-swaps-current-and-previous"])
def rollback_without_history_fails_safely(t: T):
    """No update yet: rollback refuses with no_previous_version; nothing changes."""
    before = t.snap(t.ws, git=True)
    t.gitpm_err("rollback", ID, kind="no_previous_version", contains=f"no previous version of {ID}")
    t.same(before, "workspace")
    at(t, "0.1.0", None)


@case(fixture="g2_hello_history", spec=["workspace-layout/installed-package-record#update-keeps-one-previous-revision"])
def only_one_previous_version_is_kept(t: T):
    """After two updates state keeps exactly one previous revision (0.1.1); 0.1.0 is gone."""
    at(t, "0.1.2", "0.1.1")
    text = (t.ws / "state.json").read_text()
    t.lacks(text, "0.1.0", tagged(t, "0.1.0"))
    t.eq(sorted(t.plugin(ID)), sorted(["source", "current", "previous", "enabled", "metadata"]),
         "entry fields (no history list)")


@case(fixture="g2_hello_history", spec=["updates/rollback-swaps-current-and-previous#rollback-after-update"])
def rollback_twice_swaps_current_and_previous(t: T):
    """rollback → 0.1.1 (previous 0.1.2); rollback again → 0.1.2 (previous 0.1.1), never 0.1.0."""
    t.gitpm_ok("rollback", ID)
    at(t, "0.1.1", "0.1.2")
    t.gitpm_ok("rollback", ID)
    at(t, "0.1.2", "0.1.1")


@case(fixture="g2_hello_history", spec=["updates/rollback-swaps-current-and-previous", "dist/inactive-packages-change-state-and-clone-only"])
def rollback_keeps_enabled_flag(t: T):
    """A disabled plugin stays disabled across rollbacks; an enabled one stays enabled."""
    t.gitpm_ok("rollback", ID)
    t.eq(t.plugin(ID)["enabled"], True, "enabled kept")
    t.gitpm_ok("disable", ID)
    t.gitpm_ok("rollback", ID)
    at(t, "0.1.2", "0.1.1")
    t.eq(t.plugin(ID)["enabled"], False, "disabled kept")
    t.gitpm_ok("enable", ID)
    at(t, "0.1.2", "0.1.1")


def corrupt_previous(t: T, change):
    t.edit_json(t.ws / "state.json", lambda s: change(s["plugins"][ID]["previous"]))
    before = t.snap(t.ws, git=True)
    r = t.gitpm_err("rollback", ID)
    t.has(r.err, "Error kind:")
    t.same(before, "workspace (active installation intact)")
    p = t.plugin(ID)
    t.eq(p["current"], {"version": "0.1.2", "commit": tagged(t, "0.1.2")}, "current")
    t.eq(t.rev(t.clone(ID)), tagged(t, "0.1.2"), "clone HEAD")
    t.has((t.dist(ID) / "index.js").read_text(), f"{ID} 0.1.2")
    t.eq([str(p) for p in t.ws.rglob("*.pm-txn*")], [], "no leftover transaction")
    return r


@case(fixture="g2_hello_history", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def missing_previous_snapshot_fails_safely(t: T):
    """previous.commit names a commit that does not exist in the clone: rollback fails, nothing changes."""
    corrupt_previous(t, lambda p: p.update(commit="0123456789abcdef0123456789abcdef01234567"))


@case(fixture="g2_hello_history", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def corrupt_previous_snapshot_fails_safely(t: T):
    """previous says 0.1.1 but points to the 0.1.0 commit: manifest mismatch refuses."""
    r = corrupt_previous(t, lambda p: p.update(commit=tagged(t, "0.1.0")))
    t.has(r.err, "Error kind: invalid_manifest", "mismatch")


@case(fixture="g2_hello_history", spec=["dist/a-revision-without-dist-is-refused"])
def previous_with_missing_payload_fails_safely(t: T):
    """previous points to a commit of the clone without dist/: refused, active install intact."""
    clone = t.clone(ID)
    t.git(clone, "checkout", "-q", "--detach", tagged(t, "0.1.1"))
    t.git(clone, "rm", "-q", "-r", "dist")
    t.git(clone, "commit", "-q", "-m", "drop dist")
    broken = t.rev(clone)
    t.git(clone, "checkout", "-q", "--detach", tagged(t, "0.1.2"))
    t.git(clone, "update-ref", "refs/remotes/origin/keep", broken)  # published: not local work
    corrupt_previous(t, lambda p: p.update(commit=broken))
