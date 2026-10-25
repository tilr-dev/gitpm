"""UC-043: Ignored files, unpublished refs and uninstall confirmation (P0, mandatory).

Workspace g3_trio_installed: 807b9715-06b4-55c2-add2-59b487bfa42e (0.1.1, previous 0.1.0) gets one kind of local work per
case; 03c25ec4-f583-513d-adfb-092fb98d8dba and 48cd1267-06f0-5741-820c-7d167ecfd3f8 are the unrelated packages. A new release 807b9715-06b4-55c2-add2-59b487bfa42e@0.1.2 exists
upstream so a non-refusing update would really change the clone.

Per kind of local work two cases: `<kind>_refused` (update, rollback, uninstall without
TTY, uninstall on a TTY answered N and EOF — the prompt must list the work —, then fork
leaves it in the installed clone; every refusal leaves every byte of the workspace and author repos intact)
and `<kind>_confirmed` (TTY answer y removes only 807b9715-06b4-55c2-add2-59b487bfa42e). `forced_uninstall` covers
--force with all kinds at once.
"""

import shlex

from gitpmtest import T, case

A = "807b9715-06b4-55c2-add2-59b487bfa42e"
OTHERS = ["03c25ec4-f583-513d-adfb-092fb98d8dba", "48cd1267-06f0-5741-820c-7d167ecfd3f8"]


def tty(t: T, args: list, answer: str = ""):
    """gitpm with stdin on a pseudo-terminal (util-linux script); `answer` is typed in."""
    return t.run("script", "-qec", shlex.join(["gitpm", *map(str, args)]), "/dev/null", input=answer)


def installed(t: T) -> str:
    return t.package(A)["current"]["commit"]


# ------------------------------------------------------------------ kinds of local work
# each: (setup(t, clone), needle in the lost-work list, verify(t, fork, clone_facts))

def ignored_setup(t, c):
    t.write(c, {"node_modules/private.txt": "secret\n"})


def ignored_verify(t, f):
    t.ok(not (f / "node_modules/private.txt").exists(), "ignored file excluded")
    t.eq(t.git(f, "check-ignore", "node_modules/private.txt"), "node_modules/private.txt", "still ignored")


def tracked_setup(t, c):
    t.write(c, {"dist/index.js": "export const greeting = 'edited';\n"})


def tracked_verify(t, f):
    expected = t.git(t.clone(A), "show", f"{installed(t)}:dist/index.js")
    t.eq((f / "dist/index.js").read_text().strip(), expected, "committed source bytes")


def untracked_setup(t, c):
    t.write(c, {"notes/todo.txt": "todo\n"})


def untracked_verify(t, f):
    t.ok(not (f / "notes/todo.txt").exists(), "untracked file excluded")


def staged_setup(t, c):
    t.write(c, {"staged.txt": "staged\n"})
    t.git(c, "add", "staged.txt")


def staged_verify(t, f):
    t.eq(t.git(f, "diff", "--cached", "--name-only"), "", "clean fork index")
    t.ok(not (f / "staged.txt").exists(), "staged file excluded")


def stash_setup(t, c):
    t.write(c, {"dist/index.js": "export const greeting = 'stashed';\n"})
    t.git(c, "stash", "push", "-q", "-m", "keep-this")


def stash_verify(t, f):
    t.eq(t.git(f, "stash", "list"), "", "local stash excluded")


def branch_setup(t, c):
    head = installed(t)
    t.git(c, "checkout", "-q", "-b", "local-work")
    t.commit(c, "local branch work", {"local.txt": "branch\n"})
    t.git(c, "checkout", "-q", "--detach", head)


def branch_verify(t, f):
    t.eq(t.run("git", "-C", f, "show-ref", "--quiet", "--verify", "refs/heads/local-work").code, 1, "local branch excluded")


def tag_setup(t, c):
    head = installed(t)
    t.commit(c, "local tag work", {"tagged.txt": "tag\n"})
    t.git(c, "tag", "local-tag")
    t.git(c, "checkout", "-q", "--detach", head)


def tag_verify(t, f):
    t.eq(t.run("git", "-C", f, "show-ref", "--quiet", "--verify", "refs/tags/local-tag").code, 1, "local tag excluded")


KINDS = {
    "ignored": (ignored_setup, "ignored: node_modules/private.txt", ignored_verify),
    "tracked_edit": (tracked_setup, "modified: dist/index.js", tracked_verify),
    "untracked": (untracked_setup, "untracked: notes/todo.txt", untracked_verify),
    "staged": (staged_setup, "modified: staged.txt", staged_verify),
    "stash": (stash_setup, "stash: stash@{0}", stash_verify),
    "unpublished_branch": (branch_setup, "unpublished branch: local-work (1 commit(s))", branch_verify),
    "unpublished_tag": (tag_setup, "unpublished tag: local-tag", tag_verify),
}


def everything(t: T):
    return t.snap(t.ws, t.root / "repos", git=True)


def refused_case(kind: str):
    setup, needle, verify = KINDS[kind]

    def fn(t: T):
        t.release(A, "0.1.2")
        setup(t, t.clone(A))
        before = everything(t)
        entry = t.package(A)

        r = t.gitpm_err("update", A, kind="local_changes", contains=needle)
        t.same(before, "everything after update")
        r = t.gitpm_err("rollback", A, kind="local_changes", contains=needle)
        t.same(before, "everything after rollback")
        r = t.gitpm_err("uninstall", A, kind="local_changes", contains=needle)
        t.same(before, "everything after uninstall without TTY")

        for answer, what in [("n\n", "N"), ("\n", "empty"), ("", "EOF")]:
            r = tty(t, ["uninstall", A], answer)
            t.ok(r.code != 0, f"TTY answer {what}: non-zero exit (got {r.code})")
            t.has(r.out, f"Uninstall {A} would delete:", str(t.clone(A)), str(t.dist(A)),
                  f"state.json entry {A}", "Local work that would be lost:", needle, "Delete anyway? [y/N]")
            t.same(before, f"everything after TTY answer {what}")
        t.eq(t.package(A), entry, "state entry")

        fork = t.root / "773d50d9-444b-5bd1-b190-ad4a1d9ff982"
        t.gitpm_ok("fork", A, "--into", fork)
        verify(t, fork)
        t.eq(t.rev(fork, "HEAD~1"), installed(t), "fork uses recorded installed revision")
        t.eq(t.git(fork, "status", "--porcelain", "--untracked-files=all"), "", "clean fork")
        t.same(before, "everything after fork")

    fn.__name__ = f"{kind}_refused"
    fn.__doc__ = (f"{kind}: update/rollback/uninstall (no TTY, TTY N/empty/EOF) refuse without "
                  f"changes, the prompt lists {needle!r}; fork leaves it in the installation.")
    return case(fixture="g3_trio_installed", spec=["install-uninstall/uninstall-is-planned-and-confirmed",
                                                   "updates/local-work-blocks-update-and-rollback",
                                                   "install-uninstall/cli-confirmation-flow-for-uninstall",
                                                   "fork-replacement/fork-copies-an-installed-package"])(fn)


def confirmed_case(kind: str):
    setup, needle, _ = KINDS[kind]

    def fn(t: T):
        setup(t, t.clone(A))
        keep = t.snap(*[t.clone(i) for i in OTHERS], *[t.dist(i) for i in OTHERS], t.root / "repos", git=True)
        state = t.state()
        r = tty(t, ["uninstall", A], "y\n")
        t.eq(r.code, 0, "exit code")
        t.has(r.out, needle, "Delete anyway? [y/N]")
        t.not_installed(A)
        del state["packages"][A]
        t.eq(t.state(), state, "state minus 807b9715-06b4-55c2-add2-59b487bfa42e")
        t.same(keep, "unrelated packages and author repositories")

    fn.__name__ = f"{kind}_confirmed"
    fn.__doc__ = f"{kind}: uninstall on a TTY answered y removes only 807b9715-06b4-55c2-add2-59b487bfa42e."
    return case(fixture="g3_trio_installed",
                spec=["install-uninstall/uninstall-is-planned-and-confirmed#confirmed-uninstall"])(fn)


for _kind in KINDS:
    refused_case(_kind)
    confirmed_case(_kind)


@case(fixture="g3_trio_installed", spec=["install-uninstall/uninstall-is-planned-and-confirmed"])
def forced_uninstall(t: T):
    """All kinds of local work at once: --force deletes without a prompt (also on a TTY)."""
    clone = t.clone(A)
    for kind in ["unpublished_branch", "unpublished_tag", "stash", "tracked_edit", "staged", "untracked", "ignored"]:
        KINDS[kind][0](t, clone)
    keep = t.snap(*[t.clone(i) for i in OTHERS], *[t.dist(i) for i in OTHERS], t.root / "repos", git=True)
    r = tty(t, ["uninstall", A, "--force"])
    t.eq(r.code, 0, "exit code")
    t.lacks(r.out, "Delete anyway")
    t.not_installed(A)
    t.same(keep, "unrelated packages and author repositories")


@case(fixture="g3_trio_installed", spec=["install-uninstall/cli-confirmation-flow-for-uninstall#no-tty"])
def forced_uninstall_without_tty(t: T):
    clone = t.clone(A)
    for kind in ["stash", "ignored", "untracked"]:
        KINDS[kind][0](t, clone)
    keep = t.snap(*[t.clone(i) for i in OTHERS], *[t.dist(i) for i in OTHERS], t.root / "repos", git=True)
    t.gitpm_ok("uninstall", A, "--force")
    t.not_installed(A)
    t.same(keep, "unrelated packages and author repositories")
