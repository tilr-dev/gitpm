"""UC-018: Local edit and unpublished commit protection (P0, mandatory acceptance).

Fixture g2_guarded: 3dce1568-569c-5b53-9321-37185e751947 current 0.1.2 / previous 0.1.1 with an unapplied upstream
0.1.3 (so update and rollback would both change something), and an unrelated 95d14dc8-6dfc-5022-9fd1-7146ee6aee58.
Each dirtiness type (tracked edit, untracked file, local commit, stash entry) gets its own
cases for: update+rollback refusal, uninstall without a TTY, the TTY prompt answered
N / empty / EOF, the prompt answered y, and --force.

The TTY is a real pseudo-terminal (python pty); the answer is written after the
`[y/N]` prompt appears, EOF is Ctrl-D on an empty line.
"""

import os
import pty
import select
import shlex
import time

from gitpmtest import R, T, Failure, case

ID, OTHER = "3dce1568-569c-5b53-9321-37185e751947", "95d14dc8-6dfc-5022-9fd1-7146ee6aee58"


# ----------------------------------------------------------------------------- helpers

def tty_gitpm(t: T, *args, answer: str | None, timeout: float = 60) -> tuple[R, bool]:
    """Run gitpm with stdin/stdout/stderr on a pty. Once `[y/N]` appears, send `answer` + newline
    (None: Ctrl-D, i.e. EOF). Returns the result and whether the prompt was seen."""
    argv = ["gitpm", *map(str, args)]
    pid, fd = pty.fork()
    if pid == 0:  # child
        try:
            os.chdir(t.root)
            os.execvp("gitpm", argv)
        finally:
            os._exit(127)
    out, prompted, deadline = b"", False, time.monotonic() + timeout
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            os.kill(pid, 9)
            os.waitpid(pid, 0)
            raise Failure(f"{shlex.join(argv)} timed out on the pty; output so far:\n{out.decode(errors='replace')}")
        ready, _, _ = select.select([fd], [], [], remaining)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 4096)
        except OSError:  # EIO: the child closed the pty
            break
        if not chunk:
            break
        out += chunk
        if not prompted and b"[y/N]" in out:
            prompted = True
            time.sleep(0.1)
            os.write(fd, b"\x04" if answer is None else answer.encode() + b"\r")
    _, status = os.waitpid(pid, 0)
    os.close(fd)
    r = R(argv, os.waitstatus_to_exitcode(status), out.decode(errors="replace").replace("\r\n", "\n"), "")
    print(f"$ {shlex.join(argv)}   (pty, answer {'EOF' if answer is None else repr(answer)})")
    print("".join(f"    {line}\n" for line in r.out.rstrip("\n").splitlines()), end="")
    print(f"  -> exit {r.code}, prompt seen: {prompted}")
    return r, prompted


def make_tracked(t: T) -> list[str]:
    (t.worktree(ID) / "dist/index.js").write_text("export const greeting = 'local edit';\n")
    return ["modified: dist/index.js"]


def make_untracked(t: T) -> list[str]:
    (t.worktree(ID) / "notes.txt").write_text("my local notes\n")
    return ["untracked: notes.txt"]


def make_commit(t: T) -> list[str]:
    clone = t.worktree(ID)
    (clone / "dist/index.js").write_text("export const greeting = 'committed locally';\n")
    t.git(clone, "commit", "-q", "-am", "local work")
    sha = t.rev(clone)
    return [f"unpublished commit: {sha} local work", f"HEAD moved: {sha}"]


def make_stash(t: T) -> list[str]:
    clone = t.worktree(ID)
    (clone / "dist/index.js").write_text("export const greeting = 'stashed';\n")
    t.git(clone, "stash", "push", "-q", "-m", "wip greeting")
    t.eq(t.git(clone, "status", "--porcelain"), "", "worktree clean after stash")
    return ["stash: stash@{0}", "wip greeting"]


KINDS = {"tracked_edit": make_tracked, "untracked_file": make_untracked,
         "local_commit": make_commit, "stash_entry": make_stash}


class Guard:
    """Snapshots of everything that must survive: the dirty installation, the author repo and 95d14dc8-6dfc-5022-9fd1-7146ee6aee58."""

    def __init__(self, t: T):
        self.t = t
        self.ws = t.snap(t.ws, git=True)
        self.author = t.snap(t.repo(ID), git=True)
        self.other = t.snap(t.worktree(OTHER), t.dist(OTHER), git=True)
        self.other_entry = t.package(OTHER)

    def unchanged(self, what: str):
        self.t.same(self.ws, f"workspace after {what}")
        self.t.same(self.author, "author repository")

    def removed_only_selected(self):
        t = self.t
        t.not_installed(ID)
        t.same(self.author, "author repository")
        t.same(self.other, f"{OTHER} clone and dist")
        t.eq(t.package(OTHER), self.other_entry, f"{OTHER} state entry")


# ----------------------------------------------------------------------------- cases

def update_and_rollback_refused(t: T, make):
    """update (stable, --ref, dev) and rollback refuse with local_changes; nothing changes."""
    make(t)
    g = Guard(t)
    for args in [("update", ID), ("update", ID, "--ref", f"v0.1.3"), ("update", ID, "--dev"),
                 ("rollback", ID)]:
        t.gitpm_err(*args, kind="local_changes")
        g.unchanged(" ".join(args))


def uninstall_without_tty_refused(t: T, make):
    """uninstall with stdin not a TTY refuses (local_changes), lists the local work, changes nothing."""
    expected = make(t)
    g = Guard(t)
    r = t.gitpm_err("uninstall", ID, kind="local_changes")
    t.has(r.err, *expected)
    g.unchanged("uninstall without a TTY")


def prompt_declined(t: T, make):
    """TTY prompt lists the local work and what is deleted; N, empty line and EOF change nothing."""
    expected = make(t)
    g = Guard(t)
    for answer in ["N", "", None]:
        r, prompted = tty_gitpm(t, "uninstall", ID, answer=answer)
        t.ok(prompted, "confirmation prompt shown")
        t.has(r.out, *expected)
        t.has(r.out, str(t.worktree(ID).relative_to(t.ws)), f"dist/{ID}", f"state.json entry {ID}", "Delete anyway? [y/N]")
        t.ok(r.code != 0, f"answer {answer!r}: non-zero exit")
        g.unchanged(f"answer {answer!r}")


def prompt_confirmed(t: T, make):
    """TTY prompt answered y removes clone, dist and state entry of the selected package only."""
    expected = make(t)
    g = Guard(t)
    r, prompted = tty_gitpm(t, "uninstall", ID, answer="y")
    t.ok(prompted, "confirmation prompt shown")
    t.has(r.out, *expected)
    t.eq(r.code, 0, "exit code")
    g.removed_only_selected()


def force_removes(t: T, make):
    """uninstall --force removes the dirty installation without a prompt (also on a TTY)."""
    make(t)
    g = Guard(t)
    r, prompted = tty_gitpm(t, "uninstall", ID, "--force", answer="N")
    t.ok(not prompted, "no prompt with --force")
    t.eq(r.code, 0, "exit code")
    g.removed_only_selected()


STEPS = {
    update_and_rollback_refused: ["updates/local-work-blocks-update-and-rollback"],
    uninstall_without_tty_refused: ["install-uninstall/cli-confirmation-flow-for-uninstall#no-tty"],
    prompt_declined: ["install-uninstall/cli-confirmation-flow-for-uninstall", "cli/prompts-only-on-a-tty"],
    prompt_confirmed: ["install-uninstall/uninstall-is-planned-and-confirmed#confirmed-uninstall"],
    force_removes: ["install-uninstall/uninstall-is-planned-and-confirmed"],
}
STASH = ["install-uninstall/uninstall-is-planned-and-confirmed#stash-alone-blocks",
         "updates/local-work-blocks-update-and-rollback#stash-blocks-update"]

for _kind, _make in KINDS.items():
    for _step, _spec in STEPS.items():
        def _fn(t: T, step=_step, make=_make):
            step(t, make)
        _fn.__name__ = f"{_kind}_{_step.__name__}"
        _fn.__doc__ = f"{_kind.replace('_', ' ')}: {_step.__doc__}"
        _fn.__module__ = __name__
        _tags = _spec + [x for x in STASH if _kind == "stash_entry" and x.split("#")[0] in " ".join(_spec)]
        case(fixture="g2_guarded", spec=_tags)(_fn)


@case(fixture="g2_guarded", spec=["install-uninstall/uninstall-is-planned-and-confirmed"])
def clean_clone_uninstalls_without_prompt(t: T):
    """Control: a clean clone is removed on a TTY without asking; 95d14dc8-6dfc-5022-9fd1-7146ee6aee58 untouched."""
    g = Guard(t)
    r, prompted = tty_gitpm(t, "uninstall", ID, answer="N")
    t.ok(not prompted, "no prompt for a clean clone")
    t.eq(r.code, 0, "exit code")
    g.removed_only_selected()
