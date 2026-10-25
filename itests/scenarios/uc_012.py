"""UC-012: Recorded modes: releases, pinned refs and branches (P0).

`--ref TAG|COMMIT` pins, `--ref BRANCH` follows that origin branch, `--dev` follows the
origin's default branch; the mode is recorded per package and `update ID` follows it.

Fixture hello: 3dce1568-569c-5b53-9321-37185e751947 with one release 0.1.0 on main.
"""

from gitpmtest import T, case

ID = "3dce1568-569c-5b53-9321-37185e751947"


def dist(t: T) -> str:
    return (t.dist(ID) / "index.js").read_text()


@case(fixture="hello", spec=["install-uninstall/install-an-explicit-ref-or-a-branch"])
def install_dev_installs_default_branch_head(t: T):
    """install --dev takes the untagged HEAD with its new manifest version and records branch mode main."""
    head = t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--dev")
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.2.0", "commit": head}, "current")
    t.eq(p["mode"], {"branch": "main"}, "mode")
    t.eq(t.rev(t.worktree(ID)), head, "clone HEAD")
    t.has(dist(t), "3dce1568-569c-5b53-9321-37185e751947 0.2.0")
    t.lacks((t.ws / "state.json").read_text(), "channel")


@case(fixture="hello", spec=["updates/branch-mode-follows-the-branch-head-by-commit"])
def update_follows_the_recorded_branch(t: T):
    """Plain update of a --dev install moves to origin main HEAD, ignores other branches, needs no version bump."""
    first = t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--dev")
    second = t.release(ID, "0.3.0", tag=False)
    t.git(ID, "checkout", "-q", "-b", "side")
    t.release(ID, "9.9.9", tag=False)
    t.git(ID, "checkout", "-q", "main")
    t.gitpm_ok("update", ID)
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.3.0", "commit": second}, "current = main HEAD")
    t.eq(p["previous"], {"version": "0.2.0", "commit": first, "mode": {"branch": "main"}}, "previous (with its mode)")
    t.has(dist(t), "3dce1568-569c-5b53-9321-37185e751947 0.3.0")
    same_version = t.commit(ID, "code change, same version", {"dist/index.js": "export const greeting = 'dev';\n"})
    r = t.gitpm_ok("update", ID)
    t.has(r.out, f"{ID} updated 0.3.0 (")
    t.eq(t.package(ID)["current"], {"version": "0.3.0", "commit": same_version}, "branch identity is the commit")
    t.eq(dist(t), "export const greeting = 'dev';\n", "dist")


@case(fixture="hello", spec=["updates/branch-mode-follows-the-branch-head-by-commit"])
def mode_is_recorded_and_switched(t: T):
    """A --dev install keeps following main; --ref pins (update then says pinned); --release returns."""
    t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--dev")
    t.release(ID, "0.3.0")
    dev_head = t.release(ID, "0.4.0", tag=False)
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"], {"version": "0.4.0", "commit": dev_head}, "plain update follows main")
    r = t.gitpm_ok("update", ID, "--release")
    t.has(r.out, f"{ID} updated 0.4.0 → 0.3.0")
    t.ok("mode" not in t.package(ID), "back to releases (down to the newest one)")
    r = t.gitpm_ok("rollback", ID)
    t.eq(t.package(ID)["mode"], {"branch": "main"}, "rollback restores the mode with the revision")
    t.gitpm_err("install", t.repo(ID), "--ref", "HEAD~1", kind="invalid_argument", contains="unknown ref")
    t.gitpm_ok("update", ID, "--ref", f"v0.3.0")
    t.eq(t.package(ID)["mode"], {"pinned": f"v0.3.0"}, "a tag pins")
    t.release(ID, "0.5.0")
    before = t.snap(t.ws, git=True)
    r = t.gitpm_ok("update", ID)
    t.eq(r.out, f"{ID} pinned at 0.3.0 (v0.3.0)\n", "pinned line")
    t.same(before, "workspace (pinned)")
    r = t.gitpm_ok("--catalog", "", "check")
    t.has(r.out, "pinned (v0.3.0)")
    r = t.gitpm_ok("update", ID, "--release")
    t.has(r.out, f"{ID} updated 0.3.0 → 0.5.0")
    t.ok("mode" not in t.package(ID), "release mode records no mode")


@case(fixture="hello", spec=["install-uninstall/install-an-explicit-ref-or-a-branch", "updates/explicit-ref-resolves-against-the-origin#branch-name"])
def explicit_ref_selects_its_revision(t: T):
    """--ref tag / branch on install, --ref tag on update select exactly that revision and record the mode."""
    tagged = t.rev(ID, f"v0.1.0")
    second = t.release(ID, "0.2.0")
    head = t.release(ID, "0.3.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--ref", f"v0.1.0")
    t.eq(t.package(ID)["current"], {"version": "0.1.0", "commit": tagged}, "install --ref tag")
    t.eq(t.package(ID)["mode"], {"pinned": f"v0.1.0"}, "install --ref tag pins")
    t.gitpm_ok("update", ID, "--ref", f"v0.2.0")
    t.eq(t.package(ID)["current"], {"version": "0.2.0", "commit": second}, "update --ref tag")
    t.eq(t.rev(t.worktree(ID)), second, "clone HEAD")
    t.has(dist(t), "3dce1568-569c-5b53-9321-37185e751947 0.2.0")
    t.gitpm_ok("update", ID, "--ref", "main")
    t.eq(t.package(ID)["current"], {"version": "0.3.0", "commit": head}, "update --ref upstream branch")
    t.eq(t.package(ID)["mode"], {"branch": "main"}, "update --ref branch follows it")
    other = t.root / "ws-branch"
    t.gitpm_ok("--root", other, "install", t.repo(ID), "--ref", "main")
    t.has((other / "state.json").read_text(), head, '"branch": "main"')


@case(fixture="hello", spec=["install-uninstall/install-an-explicit-ref-or-a-branch"])
def install_a_branch_only_the_origin_has(t: T):
    """install --ref feature: the fresh clone resolves origin/feature (no local branch), branch mode feature."""
    t.git(ID, "checkout", "-q", "-b", "feature")
    feature = t.release(ID, "0.2.0", tag=False)
    t.git(ID, "checkout", "-q", "main")
    t.gitpm_ok("install", t.repo(ID), "--ref", "feature")
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.2.0", "commit": feature}, "current = origin/feature")
    t.eq(p["mode"], {"branch": "feature"}, "mode")
    t.git(ID, "checkout", "-q", "feature")
    newer = t.release(ID, "0.3.0", tag=False)
    t.git(ID, "checkout", "-q", "main")
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"], {"version": "0.3.0", "commit": newer}, "update follows origin/feature")


@case(fixture="hello", spec=["cli/option-validation#ref-dev-and-release"])
def targets_exclude_each_other(t: T):
    """--ref, --dev and --release exclude each other, --channel is gone: usage errors, nothing created."""
    for args, needle in [(("install", t.repo(ID), "--ref", f"v0.1.0", "--dev"), "--dev conflicts with --ref"),
                         (("install", t.repo(ID), "--channel", "dev"), "unknown option: --channel; use --help"),
                         (("install", t.repo(ID), "--release"), "--release requires update")]:
        r = t.gitpm(*args)
        t.ok(r.code != 0, f"gitpm {' '.join(map(str, args))} fails")
        t.has(r.err, needle)
        t.lacks(r.err, "Error kind")
        t.not_installed(ID)
    t.gitpm_ok("install", t.repo(ID))
    t.release(ID, "0.2.0")
    before = t.snap(t.ws, git=True)
    for args, needle in [(("update", ID, "--ref", f"v0.2.0", "--dev"), "--dev conflicts with --ref"),
                         (("update", ID, "--dev", "--release"), "--release conflicts with --dev"),
                         (("update", ID, "--ref", f"v0.2.0", "--release"), "--release conflicts with --ref"),
                         (("rollback", ID, "--dev"), "--dev requires install/download/update")]:
        r = t.gitpm(*args)
        t.ok(r.code != 0, f"gitpm {' '.join(args)} fails")
        t.has(r.err, needle)
        t.same(before, "workspace")
    t.gitpm_ok("update", ID, "--release")
    t.eq(t.package(ID)["current"]["version"], "0.2.0", "update --release")


@case(fixture="hello", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def unknown_ref_install_fails_without_partial_install(t: T):
    """install --ref with an unknown name or SHA: error naming the ref; no clone, dist, state entry or txn."""
    for ref in ["no-such-ref", "0123456789abcdef0123456789abcdef01234567"]:
        r = t.gitpm("install", t.repo(ID), "--ref", ref)
        t.ok(r.code != 0, f"install --ref {ref} fails")
        t.has(r.err, ref, "Error kind:")
        t.not_installed(ID)


@case(fixture="hello", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def unknown_ref_update_fails_without_change(t: T):
    """update --ref unknown name / SHA / local branch: refused, installed state untouched."""
    t.gitpm_ok("install", t.repo(ID))
    t.release(ID, "0.2.0")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("update", ID, "--ref", "no-such-ref", kind="invalid_argument", contains="unknown ref no-such-ref")
    t.same(before, "workspace")
    r = t.gitpm_err("update", ID, "--ref", "0123456789abcdef0123456789abcdef01234567")
    t.has(r.err, "Error kind:")
    t.same(before, "workspace")
    t.gitpm_err("update", ID, "--ref", "refs/heads/main", kind="invalid_argument", contains="local branches")
    t.same(before, "workspace")
    t.eq(t.package(ID)["current"]["version"], "0.1.0", "still 0.1.0")
