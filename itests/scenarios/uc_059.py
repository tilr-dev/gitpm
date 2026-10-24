"""UC-059: Required dependent guards on disable, forget and uninstall (P0, mandatory).

Fixture g6_core_deps_installed: app/a (lib/core ^1.0) and app/c (lib/core ^1) installed
from the catalog with lib/core 1.1.0 pulled in, plus the unrelated other/x. app/c plays
the role of the spec's second dependent "app/b" (in the shared g6_core catalog app/b is
the ^2.0 conflict consumer of UC-058).

Owner decision: `disable --force` of a package with dependents is valid (disables it).
"""

import os
import shlex
import subprocess
import time

from gitpmtest import R, T, case

CORE = "lib/core"
DEPENDENTS = "app/a, app/c"


def gitpm_tty(t: T, *args: str, answer: bytes, timeout: int = 60) -> R:
    """Run gitpm with a pseudo-terminal as stdin, typing `answer`; stdout/stderr piped."""
    master, slave = os.openpty()
    argv = ["gitpm", *map(str, args)]
    print(f"$ {shlex.join(argv)}   (stdin: pty, typed {answer!r})")
    p = subprocess.Popen(argv, cwd=t.root, stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         text=True)
    os.close(slave)
    time.sleep(0.3)
    os.write(master, answer)
    try:
        out, err = p.communicate(timeout=timeout)
    finally:
        os.close(master)
    r = R(argv, p.returncode, out, err)
    for name, text in (("stdout", out), ("stderr", err)):
        if text:
            print(f"  {name}:\n" + "".join(f"    {line}\n" for line in text.rstrip("\n").splitlines()))
    print(f"  -> exit {r.code}")
    return r


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def core_status(t: T, consumer: str) -> dict:
    return next(r for r in resolved(t)[consumer]["dependencies"] if r["id"] == CORE)


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal#uninstall-a-dependency", "errors/display-is-the-complete-message#has-dependents"])
def uninstall_refused_naming_dependents(t: T):
    """1. uninstall lib/core is refused naming app/a and app/c; nothing changes."""
    before = t.snap(t.ws)
    t.gitpm_err("uninstall", CORE, kind="has_dependents",
             contains=f"cannot uninstall {CORE}: required by {DEPENDENTS}")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal"])
def forget_refused_naming_dependents(t: T):
    """2a. forget lib/core: the same guard."""
    before = t.snap(t.ws)
    t.gitpm_err("forget", CORE, kind="has_dependents", contains=f"cannot forget {CORE}: required by {DEPENDENTS}")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal"])
def disable_refused_naming_dependents(t: T):
    """2b. disable lib/core: the same guard."""
    before = t.snap(t.ws)
    t.gitpm_err("disable", CORE, kind="has_dependents", contains=f"cannot disable {CORE}: required by {DEPENDENTS}")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["dependencies/broken-consumers-are-reported-not-enforced", "dist/enable-and-disable-deploy-and-remove"])
def disable_force_reports_broken_dependents(t: T):
    """2c. disable --force disables lib/core (dist removed); dependents are reported, never silently broken."""
    t.gitpm_ok("disable", CORE, "--force")
    t.eq(t.package(CORE)["enabled"], False, "lib/core disabled")
    t.ok(not t.dist(CORE).exists(), "dist/lib/core removed")
    for consumer in ["app/a", "app/c"]:
        row = core_status(t, consumer)
        t.eq((row["provider"], row["satisfied"], row.get("problem")), (CORE, False, f"{CORE} is disabled"),
             f"{consumer} dependency")
    check = t.gitpm_ok("--catalog", t.catalog, "check").out
    t.has(check, f"Unsatisfied dependency: app/a → {CORE} ^1.0: {CORE} is disabled",
          f"Unsatisfied dependency: app/c → {CORE} ^1: {CORE} is disabled")


@case(fixture="g6_core_deps_installed", spec=["dependencies/broken-consumers-are-reported-not-enforced#unrelated-install-after-forced-removal"])
def uninstall_force_reports_unsatisfied(t: T):
    """3. uninstall --force removes lib/core; list and check report app/a and app/c unsatisfied."""
    t.gitpm_ok("uninstall", CORE, "--force")
    t.not_installed(CORE)
    for consumer in ["app/a", "app/c"]:
        row = core_status(t, consumer)
        t.eq((row["provider"], row["satisfied"], row.get("problem")), (None, False, "not installed"),
             f"{consumer} dependency")
    check = t.gitpm_ok("check").out
    t.has(check, f"Unsatisfied dependency: app/a → {CORE} ^1.0: not installed",
          f"Unsatisfied dependency: app/c → {CORE} ^1: not installed")
    # A broken consumer does not block unrelated operations.
    t.gitpm_ok("disable", "other/x")
    t.gitpm_ok("enable", "other/x")


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal", "dependencies/state-keeps-dependency-edges"])
def unused_dependency_uninstalls_without_guard(t: T):
    """4. After uninstalling app/a and app/c, lib/core is unused and uninstalls without a guard."""
    t.gitpm_ok("uninstall", "app/a")
    t.eq(resolved(t)[CORE].get("dependents"), ["app/c"], "dependents after app/a")
    t.gitpm_err("uninstall", CORE, kind="has_dependents", contains="required by app/c")
    t.gitpm_ok("uninstall", "app/c")
    t.ok(not resolved(t)[CORE].get("dependents"), "lib/core unused")
    r = t.gitpm_ok("uninstall", CORE)
    t.lacks(r.err, "required by")
    t.not_installed(CORE)
    t.eq(sorted(t.state()["packages"]), ["other/x"], "only the unrelated package is left")


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal", "install-uninstall/uninstall-is-planned-and-confirmed"])
def dirty_dependency_reports_guard_and_lost_work(t: T):
    """5. Dirty lib/core clone: one refusal reports the dependents and the local work; nothing changes."""
    (t.worktree(CORE) / "dist/index.js").write_text("local edit\n")
    (t.worktree(CORE) / "scratch.txt").write_text("untracked\n")
    before = t.snap(t.ws)
    r = t.gitpm_err("uninstall", CORE, kind="has_dependents", contains=f"required by {DEPENDENTS}")
    t.has(r.err, "modified: dist/index.js", "untracked: scratch.txt")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal", "install-uninstall/cli-confirmation-flow-for-uninstall"])
def dirty_dependency_on_tty_still_guarded(t: T):
    """5'. On a TTY, even answering y, the dependent guard refuses and the local work is listed."""
    (t.worktree(CORE) / "dist/index.js").write_text("local edit\n")
    before = t.snap(t.ws)
    r = gitpm_tty(t, "uninstall", CORE, answer=b"y\n")
    t.ok(r.code != 0, "refused")
    t.has(r.err, f"required by {DEPENDENTS}", "modified: dist/index.js", "Error kind: has_dependents")
    t.same(before, "workspace")
