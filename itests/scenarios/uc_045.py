"""UC-045: Corrupt state and missing managed files (P0).

Workspace g3_trio_installed: A = dan/a (0.1.1, previous 0.1.0), B = dan/b (0.1.0), plus
other/c. Subcases 1–2 are split per command (one case each) so each command's verdict is
visible; a catalog with dan/a is registered first so `check` has a real catalog to read and
can only fail because of the state.
"""

import json
import shutil

from gitpmtest import T, case

A, B = "dan/a", "dan/b"


def everything(t: T):
    return t.snap(t.ws, t.root / "repos", git=True)


# ---------------------------------------------------------------- 1–2 broken state.json

def truncate(t: T):
    f = t.ws / "state.json"
    data = f.read_bytes()
    f.write_bytes(data[: len(data) // 2])


def bad_id_key(t: T):
    f = t.ws / "state.json"
    state = json.loads(f.read_text())
    state["packages"]["../x"] = dict(state["packages"][B])
    f.write_text(json.dumps(state, indent=2))


def wrong_types(t: T):
    f = t.ws / "state.json"
    state = json.loads(f.read_text())
    state["packages"][A]["enabled"] = "yes"
    state["packages"][A]["current"] = 5
    f.write_text(json.dumps(state, indent=2))


BREAKS = {"truncated": (truncate, "invalid_data"), "bad_id_key": (bad_id_key, "invalid_id"),
          "wrong_types": (wrong_types, "invalid_data")}


def command(t: T, name: str) -> list:
    if name == "install":
        t.new_package("dan/new", "0.1.0")
        return ["install", t.repo("dan/new")]
    if name == "update":
        t.release(A, "0.1.2")
        return ["update", A]
    return {"list": ["list"], "check": ["check"], "uninstall": ["uninstall", A]}[name]


def broken_case(brk: str, name: str):
    breaker, kind = BREAKS[brk]

    def fn(t: T):
        t.gitpm_ok("register", t.repo(A), "--ref", "v0.1.1")
        argv = command(t, name)
        breaker(t)
        raw = (t.ws / "state.json").read_bytes()
        before = everything(t)
        t.gitpm_err(*argv, kind=kind)
        t.eq((t.ws / "state.json").read_bytes(), raw, "state.json bytes (never rewritten or reset)")
        t.same(before, "workspace and author repos")

    fn.__name__ = f"state_{brk}_{name}"
    fn.__doc__ = f"{'1' if brk == 'truncated' else '2'}. state.json {brk}: `{name}` fails with {kind}; nothing rewritten."
    return case(fixture="g3_trio_installed", spec=["workspace-layout/foreign-directories-are-refused#corrupt-state",
                                                   "errors/conversions-choose-documented-kinds"])(fn)


for _brk in BREAKS:
    for _name in ["list", "check", "install", "update", "uninstall"]:
        broken_case(_brk, _name)


# ---------------------------------------------------------------- 3. clone of A removed

def missing_clone_case(name: str, argv: list):
    def fn(t: T):
        t.release(B, "0.1.1")
        shutil.rmtree(t.clone(A))
        b_before = t.snap(t.clone(B), t.dist(B), git=True)
        state = t.state()
        # restore works on forgotten records only: a recorded package is already_installed.
        kind = "already_installed" if name == "restore" else "repository_not_found"
        t.gitpm_err(*argv, kind=kind, contains=A)
        t.eq(t.state(), state, "state unchanged")
        t.same(b_before, "B untouched")
        t.gitpm_ok("update", B)
        t.eq(t.package(B)["current"], {"version": "0.1.1", "commit": t.rev(B, "v0.1.1")}, "B updated")
        t.has((t.dist(B) / "index.js").read_text(), "dan/b 0.1.1")
        t.eq(t.package(A), state["packages"][A], "A state entry kept")

    fn.__name__ = f"missing_clone_{name}"
    fn.__doc__ = f"3. packages/dan/a removed: `{' '.join(argv)}` fails ({'already_installed' if name == 'restore' else 'repository_not_found'}); B untouched and updatable."
    return case(fixture="g3_trio_installed", spec=["dist/undeployable-packages-do-not-block-others",
                                                   "errors/failing-operations-leave-the-workspace-unchanged"])(fn)


for _name, _argv in [("update", ["update", A]), ("rollback", ["rollback", A]),
                     ("restore", ["restore", A])]:
    missing_clone_case(_name, _argv)


@case(fixture="g3_trio_installed", spec=["dist/sync-dist-repairs-dist-explicitly",
                                         "dist/undeployable-packages-do-not-block-others"])
def missing_clone_uninstall_drops_the_record(t: T):
    """3. packages/dan/a removed: `uninstall dan/a` drops its record and dist (what sync-dist's hint
    names); B untouched."""
    shutil.rmtree(t.clone(A))
    b_before = t.snap(t.clone(B), t.dist(B), git=True)
    t.gitpm_ok("uninstall", A)
    t.not_installed(A)
    t.same(b_before, "B untouched")
    t.gitpm_ok("sync-dist")


# ---------------------------------------------------------------- 4. dist of A removed

@case(fixture="g3_trio_installed", spec=["dist/every-mutating-command-reconciles-all-of-dist"])
def missing_dist_update_redeploys(t: T):
    """4. dist/dan/a removed: update to a new release recreates it from the clone."""
    commit = t.release(A, "0.1.2")
    shutil.rmtree(t.dist(A))
    t.gitpm_ok("update", A)
    t.eq(t.package(A)["current"], {"version": "0.1.2", "commit": commit}, "current")
    t.eq((t.dist(A) / "index.js").read_text(), (t.clone(A) / "dist/index.js").read_text(), "dist from clone")
    t.has((t.dist(A) / "index.js").read_text(), "dan/a 0.1.2")


@case(fixture="g3_trio_installed", spec=["dist/forget-restore-and-uninstall-settle-dist"])
def missing_dist_restore_redeploys(t: T):
    """4. dist/dan/a removed: forget + restore recreates it from the clone."""
    shutil.rmtree(t.dist(A))
    t.gitpm_ok("forget", A)
    t.gitpm_ok("restore", A)
    t.has((t.dist(A) / "index.js").read_text(), "dan/a 0.1.1")
    t.eq(t.package(A)["current"]["version"], "0.1.1", "restored version")


# ---------------------------------------------------------------- 5. HEAD moved

def move_head(t: T) -> str:
    old = t.rev(A, "v0.1.0")
    t.git(t.clone(A), "checkout", "-q", "--detach", old)
    return old


@case(fixture="g3_trio_installed", spec=["updates/local-work-blocks-update-and-rollback"])
def moved_head_update_refused(t: T):
    """5. clone HEAD moved to another commit: update refused (local_changes, HEAD moved)."""
    t.release(A, "0.1.2")
    old = move_head(t)
    before = everything(t)
    t.gitpm_err("update", A, kind="local_changes", contains=f"HEAD moved: {old}")
    t.same(before, "workspace and author repos")


@case(fixture="g3_trio_installed", spec=["install-uninstall/uninstall-is-planned-and-confirmed"])
def moved_head_uninstall_refused(t: T):
    """5. uninstall without a TTY refused, listing the moved HEAD."""
    old = move_head(t)
    before = everything(t)
    t.gitpm_err("uninstall", A, kind="local_changes", contains=f"HEAD moved: {old}")
    t.same(before, "workspace and author repos")


# ---------------------------------------------------------------- 6. leftover transaction

@case(fixture="g3_trio_installed", spec=["transactions-locking/transactions-stage-only-touched-paths#failure-before-commit"])
def uncommitted_transaction_discarded(t: T):
    """6. <ws>/.pm-txn/new/... without a journal: list reads the live state and leaves it; the next command
    that opens the workspace discards it; state and packages identical."""
    before = everything(t)
    t.write(t.ws / ".pm-txn/new/packages/dan/a", {"junk.txt": "half-staged\n"})
    t.write(t.ws / ".pm-txn/new", {"state.json": "{}"})
    out = t.gitpm_ok("list").json()
    t.eq(sorted(out["packages"]), [A, B, "other/c"], "listed packages")
    t.ok((t.ws / ".pm-txn").exists(), "list does not recover")
    t.gitpm_ok("sync-dist")
    t.ok(not (t.ws / ".pm-txn").exists(), "leftover discarded")
    t.same(before, "workspace and author repos")


# ---------------------------------------------------------------- final: restore snapshot

@case(fixture="g3_trio_installed", spec=["updates/stable-update-moves-to-the-newest-release"])
def restored_snapshot_updates(t: T):
    """Corrupt state, failed commands, then the original bytes restored: a valid update works."""
    backup = t.root / "ws-backup"
    shutil.copytree(t.ws, backup, symlinks=True)
    truncate(t)
    t.gitpm_err("update", A, kind="invalid_data")
    shutil.rmtree(t.ws)
    shutil.copytree(backup, t.ws, symlinks=True)
    commit = t.release(A, "0.1.2")
    t.gitpm_ok("update", A)
    t.eq(t.package(A)["current"], {"version": "0.1.2", "commit": commit}, "updated")
    t.eq(t.package(A)["previous"]["version"], "0.1.1", "previous")
