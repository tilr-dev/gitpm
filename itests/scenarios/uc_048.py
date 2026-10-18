"""UC-048: State transition matrix (P0).

g3_trio_installed (dan/a 0.1.1 with previous 0.1.0, dan/b 0.1.0, other/c) and
g3_vendor_installed (vendor/demo 0.1.0) for the fork transitions.
"""

from gitpmtest import T, case

A, B = "dan/a", "dan/b"


def payload(t: T, id: str):
    return t.snap(t.clone(id), t.dist(id), git=True)


@case(fixture="g3_trio_installed", spec=["dist/inactive-packages-change-state-and-clone-only", "updates/rollback-swaps-current-and-previous"])
def disable_update_rollback(t: T):
    """disable → update → rollback: enabled stays false; current/previous swap; no dist while
    disabled; enable deploys the current (rolled back) revision."""
    t.gitpm_ok("disable", B)
    t.eq(t.plugin(B)["enabled"], False, "disabled")
    old = t.plugin(B)["current"]
    new = t.release(B, "0.1.1")
    t.gitpm_ok("update", B)
    p = t.plugin(B)
    t.eq((p["current"], p["previous"], p["enabled"]), ({"version": "0.1.1", "commit": new}, old, False),
         "after update")
    t.ok(not t.dist(B).exists(), "no dist while disabled")
    t.gitpm_ok("rollback", B)
    p = t.plugin(B)
    t.eq((p["current"], p["previous"], p["enabled"]), (old, {"version": "0.1.1", "commit": new}, False),
         "after rollback")
    t.ok(not t.dist(B).exists(), "no dist while disabled")
    t.eq(t.rev(t.clone(B)), old["commit"], "clone HEAD")
    t.gitpm_ok("enable", B)
    t.has((t.dist(B) / "index.js").read_text(), "dan/b 0.1.0")


@case(fixture="g3_trio_installed", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def forget_then_install_same_id(t: T):
    """forget → install same ID: refused (use restore); the retained clone is never overwritten."""
    t.release(B, "0.1.1")
    t.gitpm_ok("forget", B)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install", t.repo(B), kind="already_installed", contains=("dan/b", "restore"))
    t.same(before, "workspace with the forgotten clone")
    t.ok(B not in t.state()["plugins"], "still forgotten")


@case(fixture="g3_trio_installed", spec=["install-uninstall/restore-preconditions#dirty-retained-clone"])
def dirty_forget_restore(t: T):
    """dirty clone → forget → restore: restore refused, the edit survives, state stays forgotten."""
    t.write(t.clone(B), {"dist/index.js": "export const greeting = 'dirty';\n"})
    t.gitpm_ok("forget", B)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", B, kind="local_changes", contains="modified: dist/index.js")
    t.same(before, "workspace")
    t.eq((t.clone(B) / "dist/index.js").read_text(), "export const greeting = 'dirty';\n", "edit kept")
    t.ok(B not in t.state()["plugins"], "still forgotten")


@case(fixture="g3_trio_installed", spec=["dist/forget-restore-and-uninstall-settle-dist", "install-uninstall/three-removal-levels#forget-then-restore"])
def forgotten_clean_restore_uninstall(t: T):
    """disabled dan/a with previous → forget → restore (enabled=true, previous=None, current =
    clone HEAD, clone bytes retained, dist deployed again) → uninstall removes it."""
    t.gitpm_ok("disable", A)
    current = t.plugin(A)["current"]
    t.gitpm_ok("forget", A)
    t.ok(not t.dist(A).exists(), "no dist while forgotten")
    kept = t.snap(t.clone(A), git=True)
    t.gitpm_ok("restore", A)
    p = t.plugin(A)
    t.eq((p["current"], p["previous"], p["enabled"]), (current, None, True), "after restore")
    t.eq(p["source"], str(t.repo(A)), "source from origin")
    t.same(kept, "clone retained through forget/restore")
    t.has((t.dist(A) / "index.js").read_text(), "dan/a 0.1.1")
    t.gitpm_err("rollback", A, kind="no_previous_version", contains=A)
    t.gitpm_ok("uninstall", A)
    t.not_installed(A)


def fork_installed(t: T):
    t.gitpm_ok("fork", "vendor/demo", "--ns", "mine", "--into", t.root / "authors/mine-demo")
    t.gitpm_ok("install", t.root / "authors/mine-demo", "--channel", "dev")


@case(fixture="g3_vendor_installed", spec=["dist/enable-and-disable-deploy-and-remove", "package-manifest/forkof-is-provenance-replaces-is-an-offer"])
def fork_disabled_original_enabled(t: T):
    fork_installed(t)
    """Fork mine/demo installed beside vendor/demo; disable the fork: flags and provenance
    kept, clones unchanged, only dist/mine/demo removed."""
    forkof = t.plugin("mine/demo")["forkOf"]
    before = t.snap(t.ws / "plugins", t.dist("vendor/demo"), git=True)
    t.gitpm_ok("disable", "mine/demo")
    t.eq((t.plugin("mine/demo")["enabled"], t.plugin("vendor/demo")["enabled"]), (False, True), "enabled flags")
    t.eq(t.plugin("mine/demo")["forkOf"], forkof, "provenance")
    t.ok("forkOf" not in t.plugin("vendor/demo"), "original has no forkOf")
    t.same(before, "clones and dist/vendor/demo")
    t.ok(not t.dist("mine/demo").exists(), "dist/mine/demo removed")


@case(fixture="g3_vendor_installed", spec=["dist/enable-and-disable-deploy-and-remove", "package-manifest/forkof-is-provenance-replaces-is-an-offer#installing-a-fork-changes-no-consumer"])
def original_disabled_fork_enabled(t: T):
    """Disable the original while its fork is installed: no replacement appears, clones and
    dist/mine/demo unchanged, dist/vendor/demo removed until enable."""
    fork_installed(t)
    before = t.snap(t.ws / "plugins", t.dist("mine/demo"), git=True)
    t.gitpm_ok("disable", "vendor/demo")
    t.eq((t.plugin("mine/demo")["enabled"], t.plugin("vendor/demo")["enabled"]), (True, False), "enabled flags")
    t.ok(not t.state().get("replacements"), "no replacement")
    t.same(before, "clones and dist/mine/demo")
    t.ok(not t.dist("vendor/demo").exists(), "dist/vendor/demo removed")
    t.gitpm_ok("enable", "vendor/demo")
    t.eq(t.plugin("vendor/demo")["enabled"], True, "re-enabled")
    t.has((t.dist("vendor/demo") / "index.js").read_text(), "vendor/demo 0.1.0")
