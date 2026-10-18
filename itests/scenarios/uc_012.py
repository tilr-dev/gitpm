"""UC-012: Dev channel and explicit references (P0).

Fixture hello: dan/hello with one release 0.1.0 on main.
"""

from gitpmtest import T, case

ID = "dan/hello"


def dist(t: T) -> str:
    return (t.dist(ID) / "index.js").read_text()


@case(fixture="hello", spec=["cli/option-validation#channel-and-ref"])
def install_dev_installs_default_branch_head(t: T):
    """install --channel dev takes the untagged HEAD with its new manifest version; no channel stored."""
    head = t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--channel", "dev")
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.2.0", "commit": head}, "current")
    t.eq(t.rev(t.clone(ID)), head, "clone HEAD")
    t.has(dist(t), "dan/hello 0.2.0")
    text = (t.ws / "state.json").read_text()
    t.lacks(text, "channel", '"dev"')


@case(fixture="hello", spec=["updates/dev-channel-follows-the-origin-head-by-commit"])
def update_dev_follows_origin_default_branch(t: T):
    """update --channel dev moves to origin HEAD (main), ignores other branches, needs no version bump."""
    first = t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--channel", "dev")
    second = t.release(ID, "0.3.0", tag=False)
    t.git(ID, "checkout", "-q", "-b", "side")
    t.release(ID, "9.9.9", tag=False)
    t.git(ID, "checkout", "-q", "main")
    t.gitpm_ok("update", ID, "--channel", "dev")
    p = t.package(ID)
    t.eq(p["current"], {"version": "0.3.0", "commit": second}, "current = main HEAD")
    t.eq(p["previous"], {"version": "0.2.0", "commit": first}, "previous")
    t.has(dist(t), "dan/hello 0.3.0")
    same_version = t.commit(ID, "code change, same version", {"dist/index.js": "export const greeting = 'dev';\n"})
    t.gitpm_ok("update", ID, "--channel", "dev")
    t.eq(t.package(ID)["current"], {"version": "0.3.0", "commit": same_version}, "dev identity is the commit")
    t.eq(dist(t), "export const greeting = 'dev';\n", "dist")


@case(fixture="hello", spec=["updates/dev-channel-follows-the-origin-head-by-commit"])
def channel_is_chosen_per_invocation(t: T):
    """After a dev install, plain update uses stable; dev again only when asked."""
    t.release(ID, "0.2.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--channel", "dev")
    stable = t.release(ID, "0.3.0")
    dev_head = t.release(ID, "0.4.0", tag=False)
    t.gitpm_ok("update", ID)
    t.eq(t.package(ID)["current"], {"version": "0.3.0", "commit": stable}, "plain update = latest release, not HEAD")
    t.gitpm_ok("update", ID, "--channel", "dev")
    t.eq(t.package(ID)["current"], {"version": "0.4.0", "commit": dev_head}, "dev on request")
    before = t.snap(t.ws, git=True)
    t.gitpm_err("update", ID, kind="version_conflict", contains="latest release is older")
    t.same(before, "workspace (stable update does not follow dev)")


@case(fixture="hello", spec=["install-uninstall/install-an-explicit-ref-or-the-dev-channel", "updates/explicit-ref-resolves-against-the-origin#branch-name"])
def explicit_ref_selects_its_revision(t: T):
    """--ref tag / branch on install, --ref tag on update select exactly that revision."""
    tagged = t.rev(ID, f"{ID}@0.1.0")
    second = t.release(ID, "0.2.0")
    head = t.release(ID, "0.3.0", tag=False)
    t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.1.0")
    t.eq(t.package(ID)["current"], {"version": "0.1.0", "commit": tagged}, "install --ref tag")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.2.0")
    t.eq(t.package(ID)["current"], {"version": "0.2.0", "commit": second}, "update --ref tag")
    t.eq(t.rev(t.clone(ID)), second, "clone HEAD")
    t.has(dist(t), "dan/hello 0.2.0")
    t.gitpm_ok("update", ID, "--ref", "main")
    t.eq(t.package(ID)["current"], {"version": "0.3.0", "commit": head}, "update --ref upstream branch")
    other = t.root / "ws-branch"
    t.gitpm_ok("--root", other, "install", t.repo(ID), "--ref", "main")
    t.has((other / "state.json").read_text(), head)


@case(fixture="hello", spec=["cli/option-validation#channel-and-ref"])
def ref_with_channel_is_rejected(t: T):
    """--ref with --channel dev: the library's invalid_argument, nothing installed; --channel stable --ref
    installs the ref (stable is the default); an unknown channel or --channel elsewhere: usage error."""
    r = t.gitpm("install", t.repo(ID), "--ref", f"{ID}@0.1.0", "--channel", "dev")
    t.ok(r.code != 0, "install --ref --channel dev fails")
    t.has(r.err, "the dev channel conflicts with an explicit ref", "Error kind: invalid_argument")
    t.not_installed(ID)
    r = t.gitpm("install", t.repo(ID), "--channel", "beta")
    t.ok(r.code != 0, "unknown channel fails")
    t.has(r.err, "channel must be stable or dev")
    t.not_installed(ID)
    r = t.gitpm_ok("install", t.repo(ID), "--ref", f"{ID}@0.1.0", "--channel", "stable")
    t.has(r.out, f"installed {ID}@0.1.0 from ")
    t.release(ID, "0.2.0")
    before = t.snap(t.ws, git=True)
    for args, needle in [(("update", ID, "--ref", f"{ID}@0.2.0", "--channel", "dev"),
                          "the dev channel conflicts with an explicit ref"),
                         (("update", ID, "--channel", "nightly"), "channel must be stable or dev"),
                         (("rollback", ID, "--channel", "dev"), "--channel requires install/update")]:
        r = t.gitpm(*args)
        t.ok(r.code != 0, f"gitpm {' '.join(args)} fails")
        t.has(r.err, needle)
        t.same(before, "workspace")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.2.0", "--channel", "stable")
    t.eq(t.package(ID)["current"]["version"], "0.2.0", "update --channel stable --ref")


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
