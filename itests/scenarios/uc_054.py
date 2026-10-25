"""UC-054: Required replacement compatibility and conflicts (P0, mandatory).

Fixture g6_compat_installed: vendor/base 1.4.0; app/a (^1.0), app/b (>=1.3), the chain
app/c → lib/mid (^1) → vendor/base; forks mine/base (replaces vendor/base 1.4.0) and
old/base (replaces 1.2.0), both installed and not active.

The terminal is a real pseudo-terminal on gitpm's stdin (os.openpty, local helper `gitpm_tty`);
EOF is Ctrl-D at the start of a line in canonical mode.
"""

import os
import shlex
import subprocess

from gitpmtest import R, T, case

ORIGINAL = "vendor/base"


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
    """1. mine/base declares 1.4.0: no prompt even on a TTY; all consumers incl. the transitive lib/mid move."""
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "mine/base", answer=b"n\n")
    t.eq(r.code, 0, "exit code")
    t.lacks(r.err + r.out, "Warning", "[y/N]")
    t.eq(t.state().get("replacements"), {ORIGINAL: "mine/base"}, "binding")
    view = resolved(t)
    for consumer in ["app/a", "app/b", "lib/mid"]:
        t.eq(provider(view, consumer), ("mine/base", True), f"{consumer} provider")
    t.eq(provider(view, "app/c", "lib/mid"), ("lib/mid", True), "app/c still on lib/mid (transitive chain)")
    t.eq(view["mine/base"].get("dependents"), ["app/a", "app/b", "lib/mid"], "fork dependents")


def warned(t: T, r: R):
    t.has(r.err, "app/b needs vendor/base >=1.3", "Error kind: incompatible_replacement")
    t.lacks(r.err, "app/a needs", "lib/mid needs", "Replace anyway?", "[y/N]")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty#replacement-prompt-declined"])
def incompatible_fork_answer_no(t: T):
    """old/base on a TTY: strict refusal names app/b only and changes nothing."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "old/base", answer=b"n\n")
    warned(t, r)
    t.lacks(r.err, "Replace anyway? [y/N]")
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty#replacement-prompt-declined"])
def incompatible_fork_empty_answer(t: T):
    """An empty terminal input cannot override the strict refusal."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "old/base", answer=b"\n")
    warned(t, r)
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty"])
def incompatible_fork_eof(t: T):
    """EOF on terminal input still returns a strict refusal without changes."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "old/base", answer=b"\x04")
    warned(t, r)
    t.ok(r.code != 0, "non-zero exit")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only", "cli/prompts-only-on-a-tty"])
def incompatible_fork_answer_yes(t: T):
    """An affirmative input cannot override incompatibility; there is no prompt."""
    before = unchanged(t)
    r = gitpm_tty(t, "replace", ORIGINAL, "--with", "old/base", answer=b"y\n")
    warned(t, r)
    t.ok(r.code != 0, "strict refusal")
    t.same(before, "workspace")

@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only#incompatible-consumer-without-a-tty"])
def incompatible_fork_without_tty(t: T):
    """No terminal is needed to report a strict refusal."""
    before = unchanged(t)
    r = t.gitpm_err("replace", ORIGINAL, "--with", "old/base", kind="incompatible_replacement")
    warned(t, r)
    t.same(before, "workspace")

@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only"])
def incompatible_fork_force_is_rejected(t: T):
    """Force is no longer a replacement option."""
    before = unchanged(t)
    r = t.gitpm("replace", ORIGINAL, "--with", "old/base", "--force")
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
    """4b. With vendor/base → mine/base active, cyc/base (declares it replaces mine/base) cannot provide
    mine/base, and dep/base (replaces vendor/base but depends on it) cannot provide vendor/base; terminal input cannot override cycles."""
    t.gitpm_ok("replace", ORIGINAL, "--with", "mine/base")
    t.new_package("cyc/base", "0.1.0", extra={"replaces": {"id": "mine/base", "version": "0.1.0"}})
    t.new_package("dep/base", "0.1.0", deps={ORIGINAL: "^1"},
                 extra={"replaces": {"id": ORIGINAL, "version": "1.4.0"}})
    t.gitpm_ok("install", t.repo("cyc/base"))
    t.gitpm_ok("install", t.repo("dep/base"))
    before = unchanged(t)
    for extra in ([],):
        t.gitpm_err("replace", "mine/base", "--with", "cyc/base", *extra, kind="invalid_argument",
                 contains=("cyclic replacement", "mine/base provides vendor/base"))
        t.gitpm_err("replace", ORIGINAL, "--with", "dep/base", *extra, kind="invalid_argument",
                 contains=("cyclic replacement", "dep/base depends on vendor/base"))
    r = gitpm_tty(t, "replace", "mine/base", "--with", "cyc/base", answer=b"y\n")
    t.ok(r.code != 0, "refused on a TTY with y")
    t.has(r.err, "cyclic replacement")
    t.same(before, "workspace")
    t.eq(t.state().get("replacements"), {ORIGINAL: "mine/base"}, "binding kept")


@case(fixture="g6_compat_installed", spec=["fork-replacement/replacement-is-explicit-and-global#output-variants", "fork-replacement/switching-back"])
def two_compatible_forks_single_binding(t: T):
    """5. Two compatible forks: none is chosen automatically; each explicit replace activates exactly one,
    the second switches the single global binding."""
    t.new_package("alt/base", "0.1.0", extra={"replaces": {"id": ORIGINAL, "version": "1.4.0"}})
    t.gitpm_ok("install", t.repo("alt/base"))
    t.ok(not t.state().get("replacements"), "no fork chosen automatically")
    t.eq(provider(resolved(t), "app/a"), (ORIGINAL, True), "consumers on the original")

    t.gitpm_ok("replace", ORIGINAL, "--with", "mine/base")
    t.eq(t.state()["replacements"], {ORIGINAL: "mine/base"}, "first binding")
    r = t.gitpm_ok("replace", ORIGINAL, "--with", "alt/base")
    t.has(r.out, "vendor/base replaced by alt/base 0.1.0 (as vendor/base 1.4.0) for: app/a, app/b, lib/mid")
    t.eq(t.state()["replacements"], {ORIGINAL: "alt/base"}, "switched binding")
    view = resolved(t)
    for consumer in ["app/a", "app/b", "lib/mid"]:
        t.eq(provider(view, consumer), ("alt/base", True), f"{consumer} provider")
    t.ok(not view["mine/base"].get("replacing"), "mine/base no longer active")
    t.eq([p for p, v in view.items() if v.get("replacing")], ["alt/base"], "exactly one active provider")


@case(fixture="g6_compat_installed", spec=["fork-replacement/replacement-is-explicit-and-global", "fork-replacement/fork-metadata"])
def provenance_never_triggers_replacement(t: T):
    """6. A real `gitpm fork` (forkOf + replaces) of vendor/base, released and installed: no binding."""
    fork = t.root / "forks/base"
    t.gitpm_ok("fork", ORIGINAL, "--ns", "prov", "--into", fork)
    t.gitpm_ok("commit", fork, "1.4.1")
    t.gitpm_ok("install", fork)
    p = t.package("prov/base")
    t.eq(p["forkOf"]["id"], ORIGINAL, "forkOf")
    t.eq(p["replaces"]["id"], ORIGINAL, "replaces")
    t.ok(not t.state().get("replacements"), "no binding")
    view = resolved(t)
    for consumer in ["app/a", "app/b", "lib/mid"]:
        t.eq(provider(view, consumer), (ORIGINAL, True), f"{consumer} provider")
