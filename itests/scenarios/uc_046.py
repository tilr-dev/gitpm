"""UC-046: Write failure and interrupted transactions (P0).

Injection is real, at the OS boundary, never inside gitpm:
* stage copy / state write: gitpm runs with RLIMIT_FSIZE (SIGXFSZ ignored), so writing a
  file above the limit fails with EFBIG exactly like a full disk;
* activation and catalog writes: gitpm runs as the unprivileged user `nobody` (setpriv)
  over a tree it owns, with one managed directory made read-only (root ignores modes);
* interruption after staging: a `git` wrapper first on PATH SIGKILLs its parent gitpm at
  `git checkout --detach`, i.e. after the stage copy/clone and before the commit.
"""

import json
import os
import resource
import shutil
import signal
import subprocess
from pathlib import Path

from gitpmtest import R, T, case, gitpm_argv

ID = "3dce1568-569c-5b53-9321-37185e751947"
NOBODY = "65534"


def show(r: R) -> R:
    print(f"$ {' '.join(r.argv)}")
    for name, text in [("", r.out), ("stderr:\n", r.err)]:
        if text:
            print(("  " + name if name else "") + "".join(f"    {l}\n" for l in text.rstrip().splitlines()), end="")
    print(f"  -> exit {r.code}")
    return r


def gitpm_fsize(t: T, *args, limit: int) -> R:
    """gitpm with a file-size limit: larger writes fail with EFBIG (SIGXFSZ ignored)."""
    def limit_writes():
        signal.signal(signal.SIGXFSZ, signal.SIG_IGN)
        resource.setrlimit(resource.RLIMIT_FSIZE, (limit, limit))
    argv = ["gitpm", *map(str, args)]
    p = subprocess.run(argv, cwd=t.root, capture_output=True, text=True, timeout=120,
                       stdin=subprocess.DEVNULL, preexec_fn=limit_writes)
    return show(R(argv + [f"(RLIMIT_FSIZE={limit})"], p.returncode, p.stdout, p.stderr))


def give_to_nobody(t: T):
    # Root's own git calls on nobody's repositories need safe.directory, which Git reads
    # only from protected (system/global) configuration.
    config = Path("/tmp/pmt-gitconfig-safe")
    config.write_text(Path(os.environ["GIT_CONFIG_GLOBAL"]).read_text() + "[safe]\n\tdirectory = *\n")
    config.chmod(0o644)
    os.environ["GIT_CONFIG_GLOBAL"] = str(config)
    t.run("chown", "-R", f"{NOBODY}:{NOBODY}", t.root, check=True)


def gitpm_nobody(t: T, *args) -> R:
    return t.run("setpriv", f"--reuid={NOBODY}", f"--regid={NOBODY}", "--clear-groups", "--", "gitpm", *gitpm_argv(args),
                 env={"HOME": "/tmp"})


def frozen(t: T) -> dict:
    """What the user sees as installed: state bytes, dist bytes, clone HEAD."""
    return {"state": (t.ws / "state.json").read_bytes(),
            "dist": (t.dist(ID) / "index.js").read_bytes(),
            "head": t.rev(t.clone(ID))}


def journal(t: T) -> dict | None:
    f = t.ws / ".pm-txn/journal.json"
    return json.loads(f.read_text()) if f.exists() else None


def updated(t: T):
    p = t.package(ID)
    t.eq(p["current"]["version"], "0.1.1", "current after recovery")
    t.eq(p["previous"]["version"], "0.1.0", "previous after recovery")
    t.has((t.dist(ID) / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.1")
    t.eq(t.rev(t.clone(ID)), t.rev(ID, f"v0.1.1"), "clone HEAD after recovery")
    t.ok(not (t.ws / ".pm-txn").exists(), "no transaction left")


@case(fixture="hello_installed", spec=["transactions-locking/transactions-stage-only-touched-paths#failure-before-commit"])
def stage_copy_failure_changes_nothing(t: T):
    """update stages a copy of the clone; the copy hits EFBIG: refused, workspace intact."""
    t.release(ID, "0.1.1")
    before = t.snap(t.ws)
    r = gitpm_fsize(t, "update", ID, limit=2048)
    t.ok(r.code == 1, "update failed cleanly (exit 1, not killed)")
    t.has(r.err, "File too large", "Error kind: io")
    t.same(before, "workspace")
    t.ok(not (t.ws / ".pm-txn").exists(), "stage discarded")
    t.gitpm_ok("update", ID)
    updated(t)


@case(fixture="hello_installed", spec=["transactions-locking/transactions-stage-only-touched-paths#failure-before-commit"])
def state_write_failure_changes_nothing(t: T):
    """disable writes only the next state.json into the transaction: EFBIG there."""
    before = t.snap(t.ws)
    r = gitpm_fsize(t, "disable", ID, limit=64)
    t.ok(r.code == 1, "disable failed cleanly")
    t.has(r.err, "File too large", "Error kind: io")
    t.same(before, "workspace")
    t.eq(t.package(ID)["enabled"], True, "still enabled")
    t.gitpm_ok("disable", ID)
    t.eq(t.package(ID)["enabled"], False, "retry applies")


def activation_failure(t: T, readonly: str):
    t.release(ID, "0.1.1")
    give_to_nobody(t)
    before = frozen(t)
    (t.ws / readonly).chmod(0o555)
    r = gitpm_nobody(t, "update", ID)
    t.ok(r.code == 1, "update failed")
    t.has(r.err, "Permission denied", "Error kind: io")
    t.eq(journal(t), {"paths": {f"dist/{ID}": "replace", str(t.clone(ID).relative_to(t.ws)): "replace"}}, "committed journal kept")
    t.eq(json.loads((t.ws / "state.json").read_text()), json.loads(before["state"]), "state.json still the previous one")
    t.ok((t.ws / "state.json").read_bytes() == before["state"], "state.json complete and unchanged")
    (t.ws / readonly).chmod(0o755)
    return before


@case(fixture="hello_installed", spec=["transactions-locking/crash-recovery#crash-during-commit"])
def activation_failure_at_first_move_rolls_forward(t: T):
    """dist/dan is read-only: the first move of the commit fails. The previous dist and
    clone stay active; the next open finishes the decided commit."""
    before = activation_failure(t, "dist/dan")
    t.eq((t.dist(ID) / "index.js").read_bytes(), before["dist"], "previous dist still active")
    t.eq(t.rev(t.clone(ID)), before["head"], "previous clone still active")
    # list only reads; the next command that opens the workspace finishes the commit.
    t.eq(gitpm_nobody(t, "sync-dist").code, 0, "next open succeeds")
    updated(t)


@case(fixture="hello_installed", spec=["transactions-locking/crash-recovery#crash-during-commit"])
def activation_failure_midway_rolls_forward(t: T):
    """repos is read-only: dist moved, the clone did not. The next open rolls the
    commit forward instead of leaving a mixed installation."""
    before = activation_failure(t, "repos")
    t.has((t.dist(ID) / "index.js").read_text(), "0.1.1")
    t.eq(t.rev(t.clone(ID)), before["head"], "clone not moved yet")
    r = gitpm_nobody(t, "disable", ID)
    t.eq(r.code, 0, "next mutation succeeds")
    t.eq(t.package(ID)["enabled"], False, "the new mutation applied after recovery")
    t.ok(not t.dist(ID).exists(), "disabled: no dist/ID")
    t.eq(gitpm_nobody(t, "enable", ID).code, 0, "enable succeeds")
    updated(t)


KILLER = """#!/bin/sh
# Test wrapper: SIGKILL the calling gitpm at `git checkout --detach` (after staging).
for a in "$@"; do
  if [ "$a" = "--detach" ] && [ -n "$GITPMT_KILL" ]; then
    kill -9 "$PPID"
    sleep 2
    exit 1
  fi
done
exec /usr/bin/git "$@"
"""


def killer(t: T) -> dict:
    bin = t.root / "killbin"
    bin.mkdir(exist_ok=True)
    (bin / "git").write_text(KILLER)
    (bin / "git").chmod(0o755)
    return {"PATH": f"{bin}:{os.environ['PATH']}", "GITPMT_KILL": "1"}


@case(fixture="hello_installed", spec=["transactions-locking/crash-recovery"])
def update_killed_after_staging_is_discarded(t: T):
    t.release(ID, "0.1.1")
    before = t.snap(t.ws)
    r = t.gitpm("update", ID, env=killer(t))
    t.eq(r.code, -9, "gitpm was killed")
    staged = t.ws / ".pm-txn/new" / t.clone(ID).relative_to(t.ws)
    t.ok(staged.is_dir(), "the staged copy was left by the killed process")
    t.eq(journal(t), None, "no journal: the commit was never decided")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "previous version still installed")
    t.has((t.dist(ID) / "index.js").read_text(), "3dce1568-569c-5b53-9321-37185e751947 0.1.0")
    t.gitpm_ok("sync-dist")  # the next open (list only reads)
    t.same(before, "workspace after the next open")
    t.gitpm_ok("update", ID)
    updated(t)


@case(fixture="hello", spec=["transactions-locking/crash-recovery"])
def install_killed_after_clone_is_discarded(t: T):
    t.gitpm_ok("sync-dist")  # opens (creates) the empty workspace; list would not
    before = t.snap(t.ws)
    r = t.gitpm("install", t.repo(ID), env=killer(t))
    t.eq(r.code, -9, "gitpm was killed")
    t.ok((t.ws / ".pm-txn/scratch/incoming/.git").is_dir(), "the scratch clone was left behind")
    t.eq(journal(t), None, "no journal")
    t.eq(t.state().get("packages"), {}, "nothing registered")
    # list is read-only: it leaves the scratch area to the next command that opens.
    t.ok((t.ws / ".pm-txn/scratch/incoming/.git").is_dir(), "list left the scratch clone alone")
    t.gitpm_ok("sync-dist")
    t.same(before, "workspace after the next open")
    t.not_installed(ID)
    t.gitpm_ok("install", t.repo(ID))
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "retry installs")


def catalog_write_failure(t: T, *command):
    t.release("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "1.11.0")
    give_to_nobody(t)
    folder = t.catalog_index("2190eee8-4be5-5a91-81d2-6ca95c1e61d0").parent
    before = t.snap(t.catalog)
    old = {p: p.read_bytes() for p in t.catalog.rglob("*.json")}
    folder.chmod(0o555)
    r = gitpm_nobody(t, *command)
    t.ok(r.code == 1, "catalog write failed")
    t.has(r.err, "Permission denied", "Error kind: io")
    for path, data in old.items():
        t.ok(path.read_bytes() == data, f"existing record {path.relative_to(t.catalog)} unchanged")
    added = sorted(set(t.snap(t.catalog)) - set(before))
    print(f"  files left by the failed write: {added}")
    t.ok(set(added) <= {str(t.catalog_release("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "1.11.0"))}, "at most the new release file was written")
    # A consumer still sees a consistent catalog: the index does not link 1.11.0.
    t.gitpm_ok("--root", t.root / "consumer", "install-catalog", "2190eee8-4be5-5a91-81d2-6ca95c1e61d0")
    t.eq(json.loads((t.root / "consumer/state.json").read_text())["packages"]["2190eee8-4be5-5a91-81d2-6ca95c1e61d0"]["current"]["version"],
         "1.10.0", "consumer gets the last complete release")
    folder.chmod(0o755)
    r = gitpm_nobody(t, *command)
    t.eq(r.code, 0, "retry succeeds")
    for path, data in old.items():
        t.ok(path == t.catalog_index("2190eee8-4be5-5a91-81d2-6ca95c1e61d0") or path.read_bytes() == data,
             f"immutable record {path.relative_to(t.catalog)} preserved")
    index = json.loads(t.catalog_index("2190eee8-4be5-5a91-81d2-6ca95c1e61d0").read_text())
    t.eq(sorted(index["versions"]), sorted(["1.10.0", "1.11.0", "1.9.0"]), "index after retry")
    release = json.loads(t.catalog_release("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "1.11.0").read_text())
    t.eq(release["commit"], t.rev("2190eee8-4be5-5a91-81d2-6ca95c1e61d0", "v1.11.0"), "new release recorded")
    t.eq(list(t.catalog.rglob("*.tmp")), [], "no temporary files")


@case(fixture="catalog", spec=["catalog/writers-are-serialized-and-crash-safe"])
def register_write_failure_then_retry(t: T):
    catalog_write_failure(t, "register", t.repo("2190eee8-4be5-5a91-81d2-6ca95c1e61d0"), "--ref", "v1.11.0")


@case(fixture="catalog", spec=["catalog/writers-are-serialized-and-crash-safe"])
def catalog_update_write_failure_then_retry(t: T):
    (t.root / "sources.txt").write_text("repos/2190eee8-4be5-5a91-81d2-6ca95c1e61d0\nrepos/3dce1568-569c-5b53-9321-37185e751947\n")
    catalog_write_failure(t, "catalog-update")
