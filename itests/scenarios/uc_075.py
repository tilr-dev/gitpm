"""UC-075: Unified output of mutating commands (P1, gitpm 0.5).

Every mutating command prints one line per affected package, `ID <verb> …` (detail lines
such as `Fork created:` aside), and never an `OK:` line (README "Changed in 0.5");
`--version`/`-V` print `gitpm VERSION` (plus `-dev+HASH` for a Git checkout build).
The packages are created with `gitpm init` inside the case: vendor/base 0.1.0 and app/a
0.1.1 depending on vendor/base ^0.1.
"""

import json
import re

from gitpmtest import T, case

VERBS = "cli/one-output-line-per-affected-package"
BASE, APP, FORK = "vendor/base", "app/a", "mine/base"


class Out:
    """Collects every stdout/stderr of the case to check for OK: lines at the end."""

    def __init__(self, t: T):
        self.t, self.texts = t, []

    def ok(self, *args) -> list[str]:
        r = self.t.gitpm_ok(*args)
        self.texts.append(r.out + r.err)
        return r.out.splitlines()

    def lines(self, args, expected: list[str]):
        self.t.eq(self.ok(*args), expected, f"stdout of gitpm {' '.join(map(str, args))}")

    def no_ok_lines(self):
        bad = [l for text in self.texts for l in text.splitlines() if l.startswith("OK")]
        self.t.eq(bad, [], "OK: lines")


def setup(t: T):
    base, app = t.repo(BASE), t.repo(APP)
    t.gitpm_ok("init", BASE, "--into", base)
    t.gitpm_ok("init", APP, "--into", app)
    t.edit_json(app / "plugin.json", lambda m: m.update(dependencies={BASE: "^0.1"}))
    t.gitpm_ok("commit", app, "0.1.1")
    return base, app


@case(spec=[f"{VERBS}#single-package-verbs", "install-uninstall/install-is-idempotent-for-the-same-source"])
def single_package_verbs(t: T):
    """install, repeated install, update, rollback, enable/disable, forget/restore, uninstall: one line each."""
    base, app = setup(t)
    o = Out(t)
    o.lines(["install", base], [f"installed {BASE}@0.1.0 from {base}"])
    o.lines(["install", base], [f"{BASE} already installed (0.1.0)"])
    o.lines(["install", app], [f"installed {APP}@0.1.1 from {app}"])
    o.lines(["commit", base, "0.1.2"], [f"{BASE} committed 0.1.2 (local commit and tag v0.1.2)"])
    o.lines(["install", base], [f"{BASE} already installed (0.1.0); run update"])
    o.lines(["update", BASE], [f"{BASE} updated 0.1.0 → 0.1.2"])
    o.lines(["update", BASE], [f"{BASE} up to date (0.1.2)"])
    o.lines(["rollback", BASE], [f"{BASE} rolled back 0.1.2 → 0.1.0"])
    o.lines(["disable", BASE, "--force"], [f"{BASE} disabled"])
    o.lines(["enable", BASE], [f"{BASE} enabled"])
    o.lines(["forget", APP], [f"{APP} forgotten (clone kept; restore re-registers it)"])
    o.lines(["restore", APP], [f"{APP} restored 0.1.1"])
    o.lines(["uninstall", APP], [f"{APP} uninstalled"])
    o.no_ok_lines()


@case(spec=[f"{VERBS}#fork-replace-and-commit-verbs"])
def fork_replace_and_commit_verbs(t: T):
    """fork, commit, replace --with, replace --back and fork --replace print their verb lines."""
    base, app = setup(t)
    o = Out(t)
    o.ok("install", base)
    o.ok("install", app)
    fork = t.repo(FORK)
    out = o.ok("fork", BASE, "--ns", "mine", "--into", fork)
    t.ok(f"{FORK} forked from {BASE} (repository root)" in out, "fork line")
    t.ok(f"Fork created: {fork}" in out, "Fork created: detail line")
    o.lines(["commit", fork, "0.1.3"], [f"{FORK} committed 0.1.3 (local commit and tag v0.1.3)"])
    o.lines(["install", fork], [f"installed {FORK}@0.1.3 from {fork}"])
    o.lines(["replace", BASE, "--with", FORK], [f"{BASE} replaced by {FORK} 0.1.3 (as {BASE} 0.1.0) for: {APP}"])
    out = o.ok("replace", BASE, "--back")
    t.eq(len(out), 1, "one line for replace --back")
    t.ok(out[0].startswith(f"{BASE} switched back from {FORK} ({BASE} 0.1.0 for: {APP})"), "switch-back line")
    out = o.ok("fork", BASE, "--ns", "two", "--into", t.repo("two/base"), "--replace")
    t.ok(f"two/base forked from {BASE} (repository root)" in out, "fork --replace: fork line")
    t.ok(any(l.startswith(f"{BASE} replaced by two/base 0.1.0") and l.endswith(f"for: {APP}") for l in out),
         "fork --replace: replacement line")
    o.no_ok_lines()


@case(spec=[f"{VERBS}#update-all-lines", "updates/update-all-dry-run"])
def update_all_lines(t: T):
    """update all: one line per package and a totals line; --dry-run says nothing was changed."""
    base, app = setup(t)
    o = Out(t)
    o.ok("install", base)
    o.ok("install", app)
    t.gitpm_ok("commit", base, "0.1.2")
    o.lines(["update", "all", "--dry-run"], [f"{APP} up to date (0.1.1)", f"{BASE} would be updated 0.1.0 → 0.1.2",
                                           "1 updates, 0 installs, 0 kept back. Nothing was changed."])
    t.eq(t.package(BASE)["current"]["version"], "0.1.0", "dry run changed nothing")
    o.lines(["update", "all"], [f"{APP} up to date (0.1.1)", f"{BASE} updated 0.1.0 → 0.1.2",
                                "1 updates, 0 installs, 0 kept back."])
    o.lines(["sync-dist"], ["dist/ already holds exactly the active packages."])
    o.no_ok_lines()


@case(spec=["cli/version-string"])
def version_string(t: T):
    """--version and -V print `gitpm VERSION` (or VERSION-dev+HASH), exit 0, nothing on stderr."""
    outs = []
    for flag in ("--version", "-V"):
        r = t.gitpm_ok(flag)
        t.eq(r.err, "", f"{flag} stderr")
        outs.append(r.out)
    t.eq(outs[0], outs[1], "--version == -V")
    t.ok(re.fullmatch(r"gitpm \d+\.\d+\.\d+(-dev\+[0-9a-f]{7,})?\n", outs[0]) is not None,
         f"version line {outs[0]!r}")
    t.ok(not t.ws.exists(), "no workspace created")
