"""UC-056: Required switch back and dependent deletion guards (P0, mandatory).

Fixture g6_replaced: c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0, consumers 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^1.0), a768d335-0cf7-597a-b1ba-0eb542495aec;
db33e43e-e652-5767-8e21-27fb3cef9723 (a `gitpm fork` with an edited payload, 1.0.1, replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0) is the
active provider of c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 for every consumer.

TTY prompts use a real pseudo-terminal on stdin (local helper `gitpm_tty`).
"""

import os
import shlex
import subprocess
import time

from gitpmtest import R, T, case

ORIGINAL, FORK = "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9", "db33e43e-e652-5767-8e21-27fb3cef9723"
CONSUMERS = ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3"]
FORK_PAYLOAD = "export const greeting = 'fork payload';\n"


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def resolution(t: T) -> dict:
    view = resolved(t)
    out = {}
    for consumer in CONSUMERS:
        row = next(r for r in view[consumer]["dependencies"] if r["id"] == ORIGINAL)
        out[consumer] = (row["provider"], row["satisfied"])
    return out


def on_fork(t: T):
    t.eq(t.state().get("replacements"), {ORIGINAL: FORK}, "fork binding preserved")
    t.eq(resolution(t), {c: (FORK, True) for c in CONSUMERS}, "every consumer on the fork")


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


# ------------------------------------------------------------------ provider guards


@case(fixture="g6_replaced", spec=["dependencies/dependents-guard-removal", "fork-replacement/active-fork-rules"])
def active_provider_guarded(t: T):
    """With c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 disabled (it cannot take the consumers over), disable / forget /
    uninstall of the active fork refuse naming both consumers; nothing changes."""
    t.gitpm_ok("disable", ORIGINAL)
    before = t.snap(t.ws)
    for action in ["disable", "forget", "uninstall"]:
        t.gitpm_err(action, FORK, kind="has_dependents",
                 contains=f"cannot {action} {FORK}: required by 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3")
    t.same(before, "workspace")
    on_fork(t)


@case(fixture="g6_replaced", spec=["fork-replacement/active-fork-rules", "fork-replacement/active-fork-rules#uninstall-the-fork"])
def forced_provider_removal_drops_binding(t: T):
    """uninstall --force / forget --force of the active fork never leave a dangling binding: consumers
    fall back to the installed original."""
    t.gitpm_ok("forget", FORK, "--force")
    t.ok(not t.state().get("replacements"), "binding dropped with forget --force")
    t.eq(resolution(t), {c: (ORIGINAL, True) for c in CONSUMERS}, "consumers back on the original")
    t.gitpm_ok("restore", FORK)
    t.gitpm_ok("replace", ORIGINAL, "--with", FORK)
    t.gitpm_ok("uninstall", FORK, "--force")
    t.not_installed(FORK)
    t.ok(not t.state().get("replacements"), "binding dropped with uninstall --force")
    t.eq(resolution(t), {c: (ORIGINAL, True) for c in CONSUMERS}, "consumers back on the original")


# --------------------------------------------------------------------- switch back


@case(fixture="g6_replaced", spec=["fork-replacement/switching-back", "dist/switching-back-redeploys-the-original#back-to-the-original"])
def switch_back_to_compatible_original(t: T):
    """replace ORIGINAL --with ORIGINAL: no prompt, every consumer on c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 again; fork keeps history and edits."""
    fork_entry = t.package(FORK)
    fork_files = t.snap(t.clone(FORK), t.dist(FORK), git=True)
    fork_author = t.snap(t.repo(FORK), git=True)
    r = t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    t.lacks(r.err, "Warning", "[y/N]")
    t.has(r.out, f"{ORIGINAL} switched back from {FORK} ({ORIGINAL} 1.0.0 for: 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3); "
                 f"{FORK} stays installed and enabled (disable or uninstall it if not wanted)")
    t.ok("replacements" not in t.state(), "no binding in state.json")
    t.eq(resolution(t), {c: (ORIGINAL, True) for c in CONSUMERS}, "consumers on the original")
    view = resolved(t)
    t.eq(view[ORIGINAL].get("dependents"), CONSUMERS, "original dependents")
    t.ok(not view[FORK].get("dependents") and not view[FORK].get("replacing"), "fork is unused")
    t.has((t.dist(ORIGINAL) / "index.js").read_text(), "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.0.0")
    t.eq(t.package(FORK), fork_entry, "fork state entry")
    t.same(fork_files, "fork clone (with history) and dist")
    t.same(fork_author, "fork author repository")
    t.eq((t.dist(FORK) / "index.js").read_text(), FORK_PAYLOAD, "fork edits kept")
    t.ok("replacements" not in t.state(), "still switched back in a new process")


def original_breaks_consumers(t: T):
    """While replaced, c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 moves to 2.0.0, which 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^1.0) do not accept."""
    t.release(ORIGINAL, "2.0.0")
    t.gitpm_ok("update", ORIGINAL)
    t.eq(t.package(ORIGINAL)["current"]["version"], "2.0.0", "original version")
    on_fork(t)


def warned(t: T, r: R):
    t.has(r.err, "9692fc07-23c7-5f20-8779-f09374f1a571 needs c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 ^1.0", "f5984d44-92bd-51ac-8e8f-d911dd824ef3 needs c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 ^1.0", "Error kind: incompatible_replacement")
    t.lacks(r.err, "Replace anyway?", "[y/N]")


@case(fixture="g6_replaced", spec=["fork-replacement/switching-back", "cli/prompts-only-on-a-tty"])
def switch_back_incompatible_answer_no(t: T):
    """Returning to c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 2.0.0 on a TTY refuses without a prompt, preserving the binding."""
    original_breaks_consumers(t)
    before = t.snap(t.ws)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", ORIGINAL, answer=b"n\n")
    warned(t, r)
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")
    on_fork(t)


@case(fixture="g6_replaced", spec=["fork-replacement/switching-back", "cli/prompts-only-on-a-tty"])
def switch_back_incompatible_eof(t: T):
    """EOF does not override the strict refusal."""
    original_breaks_consumers(t)
    before = t.snap(t.ws)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", ORIGINAL, answer=b"\x04")
    warned(t, r)
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")
    on_fork(t)


@case(fixture="g6_replaced", spec=["fork-replacement/switching-back", "fork-replacement/compatibility-is-checked-by-versions-only"])
def switch_back_incompatible_without_tty(t: T):
    """Without a TTY: typed refusal, binding kept, force rejected."""
    original_breaks_consumers(t)
    before = t.snap(t.ws)
    r = t.gitpm_err("replace", ORIGINAL, "--with", ORIGINAL, kind="incompatible_replacement",
                 contains="9692fc07-23c7-5f20-8779-f09374f1a571 needs c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 ^1.0")
    warned(t, r)
    t.same(before, "workspace")
    on_fork(t)
    r = t.gitpm("replace", ORIGINAL, "--with", ORIGINAL, "--force")
    t.ok(r.code != 0, "force rejected")
    t.has(r.err, "--force is only valid")
    t.same(before, "workspace")

@case(fixture="g6_replaced", spec=["fork-replacement/switching-back", "cli/prompts-only-on-a-tty"])
def switch_back_incompatible_answer_yes(t: T):
    """Affirmative terminal input cannot override version incompatibility."""
    original_breaks_consumers(t)
    before = t.snap(t.ws)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", ORIGINAL, answer=b"y\n")
    warned(t, r)
    t.ok(r.code != 0, "strict refusal")
    t.same(before, "workspace")
    on_fork(t)

@case(fixture="g6_replaced", spec=["fork-replacement/switching-back#back-with-the-original-absent", "fork-replacement/active-fork-rules#reinstalled-original-stays-replaced"])
def switch_back_to_unavailable_original(t: T):
    """An absent original must be installed before returning, without force."""
    t.gitpm_ok("uninstall", ORIGINAL)
    t.not_installed(ORIGINAL)
    t.repo(ORIGINAL).rename(t.root / "unreachable")
    before = t.snap(t.ws)
    t.gitpm_err("replace", ORIGINAL, "--with", ORIGINAL, kind="not_installed", contains=ORIGINAL)
    t.same(before, "workspace")
    on_fork(t)


# ------------------------------------------------------------- removing the fork


@case(fixture="g6_replaced", spec=["install-uninstall/only-manager-owned-files-are-deleted#uninstalling-a-fork"])
def remove_unused_clean_fork(t: T):
    """After switching back the clean fork uninstalls without a guard or a question."""
    author = t.snap(t.repo(FORK), git=True)
    t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    t.gitpm_ok("uninstall", FORK)
    t.not_installed(FORK)
    t.eq(resolution(t), {c: (ORIGINAL, True) for c in CONSUMERS}, "consumers on the original")
    t.same(author, "fork author repository")


def dirty_unused_fork(t: T):
    t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    (t.clone(FORK) / "dist/index.js").write_text("local edit\n")
    (t.clone(FORK) / "notes.txt").write_text("unpublished\n")


@case(fixture="g6_replaced", spec=["install-uninstall/cli-confirmation-flow-for-uninstall#no-tty"])
def remove_dirty_fork_without_tty_refused(t: T):
    """A dirty unused fork without a TTY: typed refusal listing the lost work; nothing deleted."""
    dirty_unused_fork(t)
    before = t.snap(t.ws)
    t.gitpm_err("uninstall", FORK, kind="local_changes", contains=("modified: dist/index.js", "untracked: notes.txt"))
    t.same(before, "workspace")


@case(fixture="g6_replaced", spec=["install-uninstall/uninstall-is-planned-and-confirmed#confirmed-uninstall", "cli/prompts-only-on-a-tty"])
def remove_dirty_fork_tty_confirmation(t: T):
    """On a TTY the plan lists the lost work and asks; N keeps everything, y deletes."""
    dirty_unused_fork(t)
    before = t.snap(t.ws)
    r = gitpm_tty(t, "uninstall", FORK, answer=b"n\n")
    t.ok(r.code != 0, "N: non-zero exit")
    # The prompt and what it lists go to stderr; stdout carries only result lines.
    t.has(r.err, "modified: dist/index.js", "untracked: notes.txt", "Delete anyway? [y/N]",
          "uninstall cancelled; nothing was changed")
    t.lacks(r.out, "Delete anyway")
    t.same(before, "workspace")
    r = gitpm_tty(t, "uninstall", FORK, answer=b"y\n")
    t.eq(r.code, 0, "y: exit code")
    t.not_installed(FORK)
    t.eq(resolution(t), {c: (ORIGINAL, True) for c in CONSUMERS}, "consumers on the original")


@case(fixture="g6_replaced", spec=["install-uninstall/uninstall-is-planned-and-confirmed"])
def remove_dirty_fork_with_force(t: T):
    """--force removes the dirty unused fork without asking."""
    dirty_unused_fork(t)
    t.gitpm_ok("uninstall", FORK, "--force")
    t.not_installed(FORK)
