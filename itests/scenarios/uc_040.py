"""UC-040: Workspace/catalog locking and recovery (P0).

Locks are held by a separate helper process with the same POSIX record lock gitpm uses
(fcntl F_SETLK on the lock file beside the folder: /fx/.ws.lock, /fx/.catalog.catalog.lock).
Interrupted commits are reproduced with the exact `.pm-txn` layout of txn.rs (journal.json,
new/, old/, state.json); the real-process interruptions are in UC-046. No power-loss
durability or atomic visibility to external readers is claimed.
"""

import hashlib
import json
import shutil
import signal
import subprocess
import sys
from pathlib import Path

from gitpmtest import T, case

ID = "3dce1568-569c-5b53-9321-37185e751947"
WS_LOCK = Path("/fx/.ws.lock")
CATALOG_LOCK = Path("/fx/.catalog.catalog.lock")

HOLDER = """import fcntl, sys, time
f = open(sys.argv[1], 'a+')
fcntl.lockf(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
print('HELD', flush=True)
time.sleep(600)
"""


def hold(path: Path) -> subprocess.Popen:
    p = subprocess.Popen([sys.executable, "-c", HOLDER, str(path)], stdout=subprocess.PIPE, text=True)
    line = p.stdout.readline().strip()
    assert line == "HELD", f"lock helper did not take {path}: {line!r}"
    print(f"  helper pid {p.pid} holds {path}")
    return p


def release(p: subprocess.Popen, sig=signal.SIGTERM):
    p.send_signal(sig)
    p.wait(10)
    print(f"  helper pid {p.pid} stopped ({sig.name})")


def rel_snap(path: Path) -> dict:
    out = {}
    for p in sorted(path.rglob("*")):
        if ".git" in p.relative_to(path).parts and p.name != "HEAD":
            continue
        if p.is_file() and not p.is_symlink():
            out[str(p.relative_to(path))] = p.read_bytes()
    return out


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock#two-cli-processes"])
def held_workspace_lock_refuses_every_mutation(t: T):
    t.release(ID, "0.1.1")
    t.new_package("95d14dc8-6dfc-5022-9fd1-7146ee6aee58", "0.1.0")
    holder = hold(WS_LOCK)
    before = t.snap(t.ws)
    for args in [("update", ID), ("disable", ID), ("rollback", ID), ("forget", ID), ("uninstall", ID, "--force"),
                 ("install", t.repo("95d14dc8-6dfc-5022-9fd1-7146ee6aee58")), ("sync-dist",)]:
        t.gitpm_err(*args, kind="locked", contains="workspace is already in use")
    t.same(before, "workspace while locked")
    release(holder)
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"]["version"], "0.1.1", "retry after release")
    t.gitpm_ok("install", t.repo("95d14dc8-6dfc-5022-9fd1-7146ee6aee58"))
    t.ok(WS_LOCK.exists(), "lock file kept")


@case(fixture="hello_installed", spec=["transactions-locking/workspace-lock#two-cli-processes",
                                       "dist/read-only-commands-never-change-dist"])
def list_reads_during_a_held_lock(t: T):
    """While another process holds the workspace lock, `list` reports the last committed state; it takes
    no lock and creates or recovers nothing."""
    holder = hold(WS_LOCK)
    before = t.snap(t.ws)
    r = t.gitpm_ok("list")
    value = json.loads(r.out)
    t.eq(value["packages"][ID]["current"]["version"], "0.1.0", "committed state")
    t.eq([p["id"] for p in value["resolved"]], [ID], "resolved")
    t.eq(json.loads(t.gitpm_ok("list", "--provides", "nothing").out)["resolved"], [], "filter applied")
    t.gitpm_err("enable", ID, kind="locked")
    t.same(before, "workspace while listed")
    release(holder)
    # A new root is not created by list.
    fresh = t.root / "fresh-ws"
    t.eq(json.loads(t.gitpm_ok("--root", fresh, "list").out)["packages"], {}, "empty list")
    t.ok(not fresh.exists(), "list created no workspace")


@case(fixture="hello_installed", spec=["transactions-locking/locks-are-process-owned-and-survive-crashes#killed-owner"])
def killed_lock_owner_releases_the_workspace(t: T):
    holder = hold(WS_LOCK)
    t.gitpm_err("disable", ID, kind="locked")
    release(holder, signal.SIGKILL)
    t.ok(WS_LOCK.exists(), "leftover lock file of the dead owner")
    t.gitpm_ok("disable", ID)
    t.eq(t.package(ID)["enabled"], False, "mutation after the owner died")


@case(fixture="catalog", spec=["catalog/writers-are-serialized-and-crash-safe"])
def held_catalog_lock_refuses_writers_not_readers(t: T):
    t.release("3dce1568-569c-5b53-9321-37185e751947", "0.3.0")
    (t.root / "sources.txt").write_text("repos/3dce1568-569c-5b53-9321-37185e751947\n")
    holder = hold(CATALOG_LOCK)
    before = t.snap(t.catalog)
    t.gitpm_err("register", t.repo("3dce1568-569c-5b53-9321-37185e751947"), "--ref", "v0.3.0", kind="locked", contains="catalog locked")
    t.gitpm_err("catalog-update", kind="locked", contains="catalog locked")
    t.same(before, "catalog while locked")
    # Readers do not take the writer lock.
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, "3dce1568-569c-5b53-9321-37185e751947")
    t.gitpm_ok("install-catalog", "2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "--version", "1.9.0")
    t.gitpm_ok("check")
    release(holder)
    t.gitpm_ok("register", t.repo("3dce1568-569c-5b53-9321-37185e751947"), "--ref", "v0.3.0")
    t.ok(t.catalog_release("3dce1568-569c-5b53-9321-37185e751947", "0.3.0").is_file(), "retry after release")
    t.gitpm_ok("catalog-update")


@case(fixture="catalog", spec=["transactions-locking/workspace-lock", "catalog/writers-are-serialized-and-crash-safe"])
def workspace_and_catalog_locks_are_independent(t: T):
    t.release("3dce1568-569c-5b53-9321-37185e751947", "0.3.0")
    ws = hold(WS_LOCK)
    t.gitpm_ok("register", t.repo("3dce1568-569c-5b53-9321-37185e751947"), "--ref", "v0.3.0")
    release(ws)
    cat = hold(CATALOG_LOCK)
    t.gitpm_ok("install-catalog", "3dce1568-569c-5b53-9321-37185e751947", "--version", "0.3.0")
    t.gitpm_ok("install", t.repo("2190eee8-4be5-5a91-81d2-6ca95c1e61d0"))
    release(cat)
    t.eq(sorted(t.state()["packages"]), sorted(["3dce1568-569c-5b53-9321-37185e751947", "2190eee8-4be5-5a91-81d2-6ca95c1e61d0"]), "installed while the catalog was locked")


@case(fixture="catalog", spec=["catalog/writers-are-serialized-and-crash-safe"])
def dead_writers_temporary_file_is_cleaned(t: T):
    stale = t.catalog / "packages/3dce1568-569c-5b53-9321-37185e751947/plugin.json.99999-0.tmp"
    stale.write_text("{ half")
    index = t.catalog_index("3dce1568-569c-5b53-9321-37185e751947").read_bytes()
    t.release("3dce1568-569c-5b53-9321-37185e751947", "0.3.0")
    t.gitpm_ok("register", t.repo("3dce1568-569c-5b53-9321-37185e751947"), "--ref", "v0.3.0")
    t.ok(not stale.exists(), "stale temporary removed under the lock")
    t.ok(json.loads(t.catalog_index("3dce1568-569c-5b53-9321-37185e751947").read_text())["versions"].keys() >= {"0.1.0", "0.3.0"}, "index valid")
    t.ok(index != t.catalog_index("3dce1568-569c-5b53-9321-37185e751947").read_bytes(), "index rewritten atomically")


@case(fixture="hello_installed", spec=["transactions-locking/transactions-stage-only-touched-paths#failure-before-commit"])
def uncommitted_transaction_is_discarded_on_next_open(t: T):
    """Stage without journal (crash before commit decided): the read-only `list` shows the old state and
    leaves .pm-txn alone; the next command that opens the workspace removes it, and the live workspace
    stays exactly as it was."""
    before = t.snap(t.ws)
    txn = t.ws / ".pm-txn"
    shutil.copytree(t.worktree(ID), txn / "new/worktrees" / ID, symlinks=True)
    t.write(txn, {f"new/dist/{ID}/index.js": "half-staged\n", "state.json": "{ partial",
                  "scratch/incoming/plugin.json": "{}"})
    out = t.gitpm_ok("list").json()
    t.eq(out["packages"][ID]["current"]["version"], "0.1.0", "list shows the old state")
    t.ok(txn.exists(), "list does not recover")
    t.gitpm_ok("sync-dist")
    t.ok(not txn.exists(), ".pm-txn discarded")
    t.same(before, "live workspace")


def committed_txn(t: T, applied: list[str]) -> Path:
    """A real update computed in a copy of the workspace, laid out as a committed
    transaction of the live one; `applied` journal paths were already moved in place."""
    t.release(ID, "0.1.1")
    after = t.root / "ws-after"
    shutil.copytree(t.ws, after, symlinks=True)
    t.rebind_worktrees(after)
    t.gitpm_ok("--root", after, "update", ID)
    txn = t.ws / ".pm-txn"
    paths = [f"dist/{ID}", str(t.worktree(ID).relative_to(t.ws)), str(t.clone(ID).relative_to(t.ws))]
    for rel in paths:
        shutil.copytree(after / rel, txn / "new" / rel, symlinks=True)
    shutil.copyfile(after / "state.json", txn / "state.json")
    admin = str(t.clone(ID).relative_to(t.ws) / "worktrees" / hashlib.sha256(ID.encode()).hexdigest())
    (txn / "journal.json").write_text(json.dumps({"paths": {p: "replace" for p in paths},
                                                  "worktrees": {f"worktrees/{ID}": admin}}, indent=2))
    for rel in applied:
        (txn / "old" / rel).parent.mkdir(parents=True, exist_ok=True)
        (t.ws / rel).rename(txn / "old" / rel)
        (txn / "new" / rel).rename(t.ws / rel)
    return after


def rolled_forward(t: T, after: Path):
    t.ok(not (t.ws / ".pm-txn").exists(), "transaction finished")
    t.eq(json.loads((t.ws / "state.json").read_text())["packages"][ID]["current"],
         json.loads((after / "state.json").read_text())["packages"][ID]["current"], "state rolled forward")
    t.eq(rel_snap(t.dist(ID)), rel_snap(after / "dist" / ID), "dist rolled forward")
    t.eq(t.rev(t.worktree(ID)), t.rev(ID, f"v0.1.1"), "clone HEAD rolled forward")


@case(fixture="hello_installed", spec=["transactions-locking/crash-recovery#crash-during-commit"])
def committed_transaction_rolls_forward_on_next_open(t: T):
    """Crash after the journal with dist already moved: `list` reports state.json as it is and leaves the
    transaction alone; the next command that opens the workspace (sync-dist) finishes the commit."""
    after = committed_txn(t, [f"dist/{ID}"])
    t.has((t.dist(ID) / "index.js").read_text(), "0.1.1")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "state not yet switched")
    t.ok((t.ws / ".pm-txn/journal.json").exists(), "list left the journal")
    t.gitpm_ok("sync-dist")
    out = t.gitpm_ok("list").json()
    t.eq(out["packages"][ID]["current"]["version"], "0.1.1", "list sees the recovered state")
    rolled_forward(t, after)


@case(fixture="hello_installed", spec=["transactions-locking/crash-recovery#crash-during-commit"])
def committed_transaction_rolls_forward_before_next_mutation(t: T):
    """Crash right after the journal (nothing moved): the next mutation (disable) first
    completes the update, then applies itself (removing dist/ID); enable redeploys 0.1.1."""
    after = committed_txn(t, [])
    t.gitpm_ok("disable", ID)
    t.ok(not (t.ws / ".pm-txn").exists(), "transaction finished")
    t.eq(t.package(ID)["enabled"], False, "the new mutation applied on top")
    t.eq(t.package(ID)["current"]["version"], "0.1.1", "state rolled forward")
    t.eq(t.package(ID)["previous"]["version"], "0.1.0", "previous kept from the recovered update")
    t.ok(not t.dist(ID).exists(), "disabled: no dist/ID")
    t.gitpm_ok("enable", ID)
    rolled_forward(t, after)
