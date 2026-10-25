"""UC-054: Required replacement compatibility and conflicts (P0, mandatory).

Fixture g6_compat_installed: c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0; 9692fc07-23c7-5f20-8779-f09374f1a571 (^1.0), f5984d44-92bd-51ac-8e8f-d911dd824ef3 (>=1.3), the chain
1a620c0a-aa3c-533c-9960-3d49016fa231 → c4304791-586c-54cb-a7c6-27bd09f28dbd (^1) → c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9; forks db33e43e-e652-5767-8e21-27fb3cef9723 (replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0) and
4e02af40-ea75-510c-b524-5f21e6de2d55 (replaces 1.2.0), both installed and not active.

The terminal is a real pseudo-terminal on gitpm's stdin (os.openpty, local helper `gitpm_tty`);
EOF is Ctrl-D at the start of a line in canonical mode.
"""

import os
import shlex
import subprocess

from gitpmtest import R, T, case

ORIGINAL = "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def provider(view: dict, consumer: str, dep: str = ORIGINAL) -> tuple:
    row = next(r for r in view[consumer]["dependencies"] if r["id"] == dep)
    return row["provider"], row["satisfied"]


def gitpm_tty(t: T, *args: str, answer: bytes, timeout: int = 60) -> R:
    """Run gitpm with a pseudo-terminal as stdin, typing `answer` into it; stdout/stderr piped."""
    master, slave = os.openpty()
    argv = ["gitpm", *map(str, args)]
    print(f"$ {shlex.join(argv)}   (stdin: pty, typed {answer!r})")
    os.write(master, answer)
    p = subprocess.Popen(argv, cwd=t.root, stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         text=True)
    os.close(slave)
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


def unchanged(t: T):
    return t.snap(t.ws)


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "fork-replacement/replacement-is-explicit-and-global#replace-for-all-consumers"])
def compatible_fork_replaces_without_prompt(t: T):
    """1. db33e43e-e652-5767-8e21-27fb3cef9723 declares 1.4.0: no prompt even on a TTY; all consumers incl. the transitive c4304791-586c-54cb-a7c6-27bd09f28dbd move."""
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "db33e43e-e652-5767-8e21-27fb3cef9723", answer=b"n\n")
    t.eq(r.code, 0, "exit code")
    t.lacks(r.err + r.out, "Warning", "[y/N]")
    t.eq(t.state().get("replacements"), {ORIGINAL: "db33e43e-e652-5767-8e21-27fb3cef9723"}, "binding")
    view = resolved(t)
    for consumer in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "c4304791-586c-54cb-a7c6-27bd09f28dbd"]:
        t.eq(provider(view, consumer), ("db33e43e-e652-5767-8e21-27fb3cef9723", True), f"{consumer} provider")
    t.eq(provider(view, "1a620c0a-aa3c-533c-9960-3d49016fa231", "c4304791-586c-54cb-a7c6-27bd09f28dbd"), ("c4304791-586c-54cb-a7c6-27bd09f28dbd", True), "1a620c0a-aa3c-533c-9960-3d49016fa231 still on c4304791-586c-54cb-a7c6-27bd09f28dbd (transitive chain)")
    t.eq(view["db33e43e-e652-5767-8e21-27fb3cef9723"].get("dependents"), ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "c4304791-586c-54cb-a7c6-27bd09f28dbd"], "fork dependents")


def warned(t: T, r: R):
    t.has(r.err, "f5984d44-92bd-51ac-8e8f-d911dd824ef3 needs c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 >=1.3", "Error kind: incompatible_replacement")
    t.lacks(r.err, "9692fc07-23c7-5f20-8779-f09374f1a571 needs", "c4304791-586c-54cb-a7c6-27bd09f28dbd needs", "Replace anyway?", "[y/N]")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty#replacement-prompt-declined"])
def incompatible_fork_answer_no(t: T):
    """4e02af40-ea75-510c-b524-5f21e6de2d55 on a TTY: strict refusal names f5984d44-92bd-51ac-8e8f-d911dd824ef3 only and changes nothing."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "4e02af40-ea75-510c-b524-5f21e6de2d55", answer=b"n\n")
    warned(t, r)
    t.lacks(r.err, "Replace anyway? [y/N]")
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty#replacement-prompt-declined"])
def incompatible_fork_empty_answer(t: T):
    """An empty terminal input cannot override the strict refusal."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "4e02af40-ea75-510c-b524-5f21e6de2d55", answer=b"\n")
    warned(t, r)
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty"])
def incompatible_fork_eof(t: T):
    """EOF on terminal input still returns a strict refusal without changes."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "4e02af40-ea75-510c-b524-5f21e6de2d55", answer=b"\x04")
    warned(t, r)
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty"])
def incompatible_fork_answer_yes(t: T):
    """An affirmative input cannot override incompatibility; there is no prompt."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "4e02af40-ea75-510c-b524-5f21e6de2d55", answer=b"y\n")
    warned(t, r)
    t.ok(r.code != 0, "strict refusal")
    t.same(before, "workspace")

@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only#incompatible-consumer-without-a-tty"])
def incompatible_fork_without_tty(t: T):
    """No terminal is needed to report a strict refusal."""
    before = unchanged(t)
    r = t.gitpm_err("replace", ORIGINAL, "--with", "4e02af40-ea75-510c-b524-5f21e6de2d55", kind="incompatible_replacement")
    warned(t, r)
    t.same(before, "workspace")

@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only"])
def incompatible_fork_force_is_rejected(t: T):
    """Force is no longer a replacement option."""
    before = unchanged(t)
    r = t.gitpm("replace", ORIGINAL, "--with", "4e02af40-ea75-510c-b524-5f21e6de2d55", "--force")
    t.ok(r.code != 0, "force rejected")
    t.has(r.err, "--force is only valid")
    t.same(before, "workspace")

@case(fixture="g6_compat_installed", spec=["fork-replacement/self-replacement-and-cycles-are-refused"])
def returning_to_unreplaced_original(t: T):
    """Using the original as provider is an idempotent checked operation."""
    t.gitpm_ok("replace", ORIGINAL, "--with", ORIGINAL)
    t.ok(not t.state().get("replacements"), "no self-binding")

@case(fixture="g6_compat_installed", spec=["fork-replacement/self-replacement-and-cycles-are-refused#cycle"])
def cyclic_replacement_refused(t: T):
    """4b. With c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 → db33e43e-e652-5767-8e21-27fb3cef9723 active, e24e20e3-fe33-5dc1-8571-e8545201e81f (declares it replaces db33e43e-e652-5767-8e21-27fb3cef9723) cannot provide
    db33e43e-e652-5767-8e21-27fb3cef9723, and 0b95e0fe-8e86-5560-a398-c376f9ca9a8b (replaces c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 but depends on it) cannot provide c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9; terminal input cannot override cycles."""
    t.gitpm_ok("replace", ORIGINAL, "--with", "db33e43e-e652-5767-8e21-27fb3cef9723")
    t.new_package("e24e20e3-fe33-5dc1-8571-e8545201e81f", "0.1.0", extra={"replaces": {"id": "db33e43e-e652-5767-8e21-27fb3cef9723", "version": "0.1.0"}})
    t.new_package("0b95e0fe-8e86-5560-a398-c376f9ca9a8b", "0.1.0", deps={ORIGINAL: "^1"},
                 extra={"replaces": {"id": ORIGINAL, "version": "1.4.0"}})
    t.gitpm_ok("install", t.repo("e24e20e3-fe33-5dc1-8571-e8545201e81f"))
    t.gitpm_ok("install", t.repo("0b95e0fe-8e86-5560-a398-c376f9ca9a8b"))
    before = unchanged(t)
    for extra in ([],):
        t.gitpm_err("replace", "db33e43e-e652-5767-8e21-27fb3cef9723", "--with", "e24e20e3-fe33-5dc1-8571-e8545201e81f", *extra, kind="invalid_argument",
                 contains=("cyclic replacement", "db33e43e-e652-5767-8e21-27fb3cef9723 provides c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"))
        t.gitpm_err("replace", ORIGINAL, "--with", "0b95e0fe-8e86-5560-a398-c376f9ca9a8b", *extra, kind="invalid_argument",
                 contains=("cyclic replacement", "0b95e0fe-8e86-5560-a398-c376f9ca9a8b depends on c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9"))
    r = gitpm_tty(t, "replace", "db33e43e-e652-5767-8e21-27fb3cef9723", "--with", "e24e20e3-fe33-5dc1-8571-e8545201e81f", answer=b"y\n")
    t.ok(r.code != 0, "refused on a TTY with y")
    t.has(r.err, "cyclic replacement")
    t.same(before, "workspace")
    t.eq(t.state().get("replacements"), {ORIGINAL: "db33e43e-e652-5767-8e21-27fb3cef9723"}, "binding kept")


@case(fixture="g6_compat_installed", spec=["fork-replacement/replacement-is-explicit-and-global#output-variants", "fork-replacement/switching-back"])
def two_compatible_forks_single_binding(t: T):
    """5. Two compatible forks: none is chosen automatically; each explicit replace activates exactly one,
    the second switches the single global binding."""
    t.new_package("05a9afb3-6043-51d2-ba6f-e0a71038b7f4", "0.1.0", extra={"replaces": {"id": ORIGINAL, "version": "1.4.0"}})
    t.gitpm_ok("install", t.repo("05a9afb3-6043-51d2-ba6f-e0a71038b7f4"))
    t.ok(not t.state().get("replacements"), "no fork chosen automatically")
    t.eq(provider(resolved(t), "9692fc07-23c7-5f20-8779-f09374f1a571"), (ORIGINAL, True), "consumers on the original")

    t.gitpm_ok("replace", ORIGINAL, "--with", "db33e43e-e652-5767-8e21-27fb3cef9723")
    t.eq(t.state()["replacements"], {ORIGINAL: "db33e43e-e652-5767-8e21-27fb3cef9723"}, "first binding")
    r = t.gitpm_ok("replace", ORIGINAL, "--with", "05a9afb3-6043-51d2-ba6f-e0a71038b7f4")
    t.has(r.out, "c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 replaced by 05a9afb3-6043-51d2-ba6f-e0a71038b7f4 0.1.0 (as c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9 1.4.0) for: 9692fc07-23c7-5f20-8779-f09374f1a571, f5984d44-92bd-51ac-8e8f-d911dd824ef3, c4304791-586c-54cb-a7c6-27bd09f28dbd")
    t.eq(t.state()["replacements"], {ORIGINAL: "05a9afb3-6043-51d2-ba6f-e0a71038b7f4"}, "switched binding")
    view = resolved(t)
    for consumer in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "c4304791-586c-54cb-a7c6-27bd09f28dbd"]:
        t.eq(provider(view, consumer), ("05a9afb3-6043-51d2-ba6f-e0a71038b7f4", True), f"{consumer} provider")
    t.ok(not view["db33e43e-e652-5767-8e21-27fb3cef9723"].get("replacing"), "db33e43e-e652-5767-8e21-27fb3cef9723 no longer active")
    t.eq([p for p, v in view.items() if v.get("replacing")], ["05a9afb3-6043-51d2-ba6f-e0a71038b7f4"], "exactly one active provider")


@case(fixture="g6_compat_installed", spec=["fork-replacement/replacement-is-explicit-and-global", "fork-replacement/fork-metadata"])
def provenance_never_triggers_replacement(t: T):
    """6. A real `gitpm fork` (forkOf + replaces) of c19e4ca3-3f37-5b79-8e3c-b87c6ec12cb9, released and installed: no binding."""
    fork = t.root / "5c0dbec5-2209-5ad7-b092-7ad16acbd4a5"
    t.gitpm_ok("fork", ORIGINAL, "--into", fork)
    fork_id = t.manifest_id(fork)
    t.gitpm_ok("commit", fork, "1.4.1")
    t.gitpm_ok("install", fork)
    p = t.package(fork_id)
    t.eq(p["forkOf"]["id"], ORIGINAL, "forkOf")
    t.eq(p["replaces"]["id"], ORIGINAL, "replaces")
    t.ok(not t.state().get("replacements"), "no binding")
    view = resolved(t)
    for consumer in ["9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", "c4304791-586c-54cb-a7c6-27bd09f28dbd"]:
        t.eq(provider(view, consumer), (ORIGINAL, True), f"{consumer} provider")
