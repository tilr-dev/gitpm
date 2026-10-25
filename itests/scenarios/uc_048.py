"""UC-048: State transition matrix (P0).

g3_trio_installed (807b9715-06b4-55c2-add2-59b487bfa42e 0.1.1 with previous 0.1.0, 03c25ec4-f583-513d-adfb-092fb98d8dba 0.1.0, 48cd1267-06f0-5741-820c-7d167ecfd3f8) and
g3_vendor_installed (dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0) for the fork transitions.
"""

from gitpmtest import T, case

A, B = "807b9715-06b4-55c2-add2-59b487bfa42e", "03c25ec4-f583-513d-adfb-092fb98d8dba"


def payload(t: T, id: str):
    return t.snap(t.clone(id), t.dist(id), git=True)


@case(fixture="g3_trio_installed", spec=["dist/inactive-packages-change-state-and-clone-only", "updates/rollback-swaps-current-and-previous"])
def disable_update_rollback(t: T):
    """disable → update → rollback: enabled stays false; current/previous swap; no dist while
    disabled; enable deploys the current (rolled back) revision."""
    t.gitpm_ok("disable", B)
    t.eq(t.package(B)["enabled"], False, "disabled")
    old = t.package(B)["current"]
    new = t.release(B, "0.1.1")
    t.gitpm_ok("update", B)
    p = t.package(B)
    t.eq((p["current"], p["previous"], p["enabled"]), ({"version": "0.1.1", "commit": new}, old, False),
         "after update")
    t.ok(not t.dist(B).exists(), "no dist while disabled")
    t.gitpm_ok("rollback", B)
    p = t.package(B)
    t.eq((p["current"], p["previous"], p["enabled"]), (old, {"version": "0.1.1", "commit": new}, False),
         "after rollback")
    t.ok(not t.dist(B).exists(), "no dist while disabled")
    t.eq(t.rev(t.clone(B)), old["commit"], "clone HEAD")
    t.gitpm_ok("enable", B)
    t.has((t.dist(B) / "index.js").read_text(), "03c25ec4-f583-513d-adfb-092fb98d8dba 0.1.0")


@case(fixture="g3_trio_installed", spec=["install-uninstall/three-removal-levels#forget-then-restore"])
def forget_then_install_same_id(t: T):
    """forget → install same ID: refused (use restore); the retained clone is never overwritten."""
    t.release(B, "0.1.1")
    t.gitpm_ok("forget", B)
    before = t.snap(t.ws, git=True)
    r = t.gitpm_err("install", t.repo(B), kind="already_installed",
                    contains="Error: 03c25ec4-f583-513d-adfb-092fb98d8dba was forgotten and its clone is kept\n")
    # The library message is neutral; the CLI names the commands that help.
    t.has(r.err, "Hint: ", "`gitpm restore 03c25ec4-f583-513d-adfb-092fb98d8dba`", "`gitpm uninstall 03c25ec4-f583-513d-adfb-092fb98d8dba`")
    t.same(before, "workspace with the forgotten clone")
    t.ok(B not in t.state()["packages"], "still forgotten")


@case(fixture="g3_trio_installed", spec=["install-uninstall/restore-preconditions#dirty-retained-clone"])
def dirty_forget_restore(t: T):
    """dirty clone → forget → restore: restore refused, the edit survives, state stays forgotten."""
    t.write(t.clone(B), {"dist/index.js": "export const greeting = 'dirty';\n"})
    t.gitpm_ok("forget", B)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("restore", B, kind="local_changes", contains="modified: dist/index.js")
    t.same(before, "workspace")
    t.eq((t.clone(B) / "dist/index.js").read_text(), "export const greeting = 'dirty';\n", "edit kept")
    t.ok(B not in t.state()["packages"], "still forgotten")


@case(fixture="g3_trio_installed", spec=["dist/forget-restore-and-uninstall-settle-dist", "install-uninstall/three-removal-levels#forget-then-restore"])
def forgotten_clean_restore_uninstall(t: T):
    """disabled 807b9715-06b4-55c2-add2-59b487bfa42e with previous → forget → restore (as recorded: disabled, previous kept,
    clone bytes retained, no dist) → enable deploys → uninstall removes it."""
    t.gitpm_ok("disable", A)
    current, previous = t.package(A)["current"], t.package(A)["previous"]
    t.gitpm_ok("forget", A)
    t.ok(not t.dist(A).exists(), "no dist while forgotten")
    kept = t.snap(t.clone(A), git=True)
    t.gitpm_ok("restore", A)
    p = t.package(A)
    t.eq((p["current"], p["previous"], p["enabled"]), (current, previous, False), "after restore")
    t.eq(p["source"], str(t.repo(A)), "source as recorded")
    t.same(kept, "clone retained through forget/restore")
    t.ok(not t.dist(A).exists(), "still disabled: no dist")
    t.gitpm_ok("enable", A)
    t.has((t.dist(A) / "index.js").read_text(), "807b9715-06b4-55c2-add2-59b487bfa42e 0.1.1")
    t.gitpm_ok("uninstall", A)
    t.not_installed(A)


def fork_installed(t: T):
    t.gitpm_ok("fork", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2", "--into", t.root / "authors/mine-demo")
    t.gitpm_ok("install", t.root / "authors/mine-demo", "--dev")


@case(fixture="g3_vendor_installed", spec=["dist/enable-and-disable-deploy-and-remove", "package-manifest/forkof-is-provenance-replaces-is-an-offer"])
def fork_disabled_original_enabled(t: T):
    fork_installed(t)
    fork_id = t.manifest_id(t.root / "authors/mine-demo")
    """Fork fe3ac959-50ab-5e6c-849f-6e69253d8676 installed beside dd3387ed-a5a3-5059-81ad-b1f6987a28f2; disable the fork: flags and provenance
    kept, clones unchanged, only dist/fe3ac959-50ab-5e6c-849f-6e69253d8676 removed."""
    forkof = t.package(fork_id)["forkOf"]
    before = t.snap(t.ws / "repos", t.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), git=True)
    t.gitpm_ok("disable", fork_id)
    t.eq((t.package(fork_id)["enabled"], t.package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")["enabled"]), (False, True), "enabled flags")
    t.eq(t.package(fork_id)["forkOf"], forkof, "provenance")
    t.ok("forkOf" not in t.package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2"), "original has no forkOf")
    t.same(before, "clones and dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
    t.ok(not t.dist(fork_id).exists(), "dist/fe3ac959-50ab-5e6c-849f-6e69253d8676 removed")


@case(fixture="g3_vendor_installed", spec=["dist/enable-and-disable-deploy-and-remove", "package-manifest/forkof-is-provenance-replaces-is-an-offer#installing-a-fork-changes-no-consumer"])
def original_disabled_fork_enabled(t: T):
    """Disable the original while its fork is installed: no replacement appears, clones and
    dist/fe3ac959-50ab-5e6c-849f-6e69253d8676 unchanged, dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2 removed until enable."""
    fork_installed(t)
    fork_id = t.manifest_id(t.root / "authors/mine-demo")
    before = t.snap(t.ws / "repos", t.dist(fork_id), git=True)
    t.gitpm_ok("disable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
    t.eq((t.package(fork_id)["enabled"], t.package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")["enabled"]), (True, False), "enabled flags")
    t.ok(not t.state().get("replacements"), "no replacement")
    t.same(before, "clones and dist/fe3ac959-50ab-5e6c-849f-6e69253d8676")
    t.ok(not t.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2").exists(), "dist/dd3387ed-a5a3-5059-81ad-b1f6987a28f2 removed")
    t.gitpm_ok("enable", "dd3387ed-a5a3-5059-81ad-b1f6987a28f2")
    t.eq(t.package("dd3387ed-a5a3-5059-81ad-b1f6987a28f2")["enabled"], True, "re-enabled")
    t.has((t.dist("dd3387ed-a5a3-5059-81ad-b1f6987a28f2") / "index.js").read_text(), "dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0")
