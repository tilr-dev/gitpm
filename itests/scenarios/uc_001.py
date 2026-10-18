"""UC-001: CLI and isolated environment (P0).

Help in every spelling, required tools, `list` on an empty explicit --root, and invalid
invocations that must fail without creating a plugin, a catalog release or changing
any fixture data (the whole /fx tree, Git internals included, is compared).
"""

import json
import shutil

from gitpmtest import T, case

ID = "dan/hello"
OPT = "cli/option-validation"
# Every command of `gitpm --help` (main.rs COMMANDS).
COMMANDS = ["install", "install-catalog", "list", "check", "update", "rollback", "enable", "disable", "forget",
            "restore", "uninstall", "fork", "replace", "sync-dist", "init", "create", "commit", "remote", "auth",
            "info", "catalog", "catalog-update", "register", "migrate-catalog"]


@case(fixture="hello", spec=["cli/help", "cli/command-set#every-command-documented"])
def help_in_every_spelling(t: T):
    """1. `gitpm --help`, `gitpm help`, `gitpm -h` and bare `gitpm` succeed with one text naming every command."""
    before = t.snap(t.root, git=True)
    outputs = []
    for argv in (["--help"], ["help"], ["-h"], []):
        r = t.gitpm(*argv)
        t.eq(r.code, 0, f"exit of gitpm {' '.join(argv)}")
        outputs.append(r.out)
    t.eq(len(set(outputs)), 1, "distinct help texts")
    text = outputs[0]
    for command in COMMANDS:
        t.ok(any(line.split()[:1] == [command] or f"| {command}" in line for line in text.splitlines()),
             f"help describes {command}")
    t.has(text, "--root", "--catalog")
    t.same(before, "fixture")
    t.ok(not t.ws.exists(), "help created no workspace")


@case(spec=["none:harness self-check (tools in the runtime image)"])
def required_tools_present(t: T):
    """1. git, python3 and curl (README requirements) are available to the CLI."""
    for tool in ("git", "python3", "curl", "gitpm"):
        t.ok(shutil.which(tool) is not None, f"{tool} on PATH")
    t.has(t.run("git", "--version", check=True).out, "git version")


@case(spec=["cli/json-output-for-state-and-catalog", "cli/settings-come-from-flags-environment-defaults"])
def list_on_empty_explicit_root(t: T):
    """2. `list --root EMPTY` prints valid JSON with no plugins."""
    empty = t.root / "empty-root"
    r = t.gitpm_ok("--root", empty, "list")
    value = json.loads(r.out)
    t.eq(value.get("plugins"), {}, "plugins")
    t.ok(not t.ws.exists(), "GITPM_ROOT workspace untouched (explicit --root wins)")


def usage_case(name: str, doc: str, argv, needle: str, spec: str = OPT, fixture: str = "hello"):
    def fn(t: T):
        args = [str(a).replace("@REPO", str(t.repo(ID))) for a in argv]
        before = t.snap(t.root, git=True)
        r = t.gitpm(*args)
        t.ok(r.code != 0, f"gitpm {' '.join(args)} fails (exit {r.code})")
        t.has(r.err, needle)
        t.eq(r.out, "", "nothing printed on stdout")
        t.same(before, "fixture (repos, workspace, catalog)")
        t.ok(not t.ws.exists(), "no workspace created")
        t.ok(not t.catalog.exists(), "no catalog created")
    fn.__name__ = name
    fn.__doc__ = doc
    fn.__module__ = __name__
    case(fixture=fixture, spec=[spec])(fn)


for _args in [
    ("missing_install_source", "3. install without a source", ["install"], "install: expected 1 argument(s), got 0"),
    ("missing_update_id", "3. update without an ID", ["update"], "update: expected 1 argument(s), got 0"),
    ("missing_commit_version", "3. commit with only the repository", ["commit", "@REPO"],
     "commit: expected 2 argument(s), got 1"),
    ("missing_remote_url", "3. remote with only the repository", ["remote", "@REPO"],
     "remote: expected 2 argument(s), got 1"),
    ("missing_fork_id", "3. fork without an ID", ["fork"], "fork: expected 1 argument(s), got 0"),
    ("missing_fork_into", "3. fork without --into", ["fork", ID, "--ns", "mine"], "fork needs --into DIR"),
    ("missing_init_into", "3. init without --into", ["init", ID],
     "init needs --into DIR", "authoring/init-creates-a-package-with-its-first-release#init-without-into"),
    ("missing_auth_url", "3. auth check without a URL", ["auth", "check"], "auth: expected 2 argument(s), got 1"),
    ("unknown_command", "4. an unknown command", ["frobnicate"], "unknown command: frobnicate; use --help",
     "cli/command-set#removed-command"),
    ("missing_ref_value", "5. --ref without a value", ["install", "@REPO", "--ref"], "--ref requires a value"),
    ("missing_root_value", "5. --root without a value", ["list", "--root"], "--root requires a value"),
    ("unsupported_channel", "6. --channel beta", ["install", "@REPO", "--channel", "beta"],
     "channel must be stable or dev", f"{OPT}#channel-and-ref"),
    ("channel_with_ref", "7. --channel together with --ref",
     ["install", "@REPO", "--channel", "dev", "--ref", f"{ID}@0.1.0"], "--channel conflicts with --ref",
     f"{OPT}#channel-and-ref"),
    ("dry_run_outside_update_all", "9. --dry-run with catalog-update", ["catalog-update", "--dry-run"],
     "--dry-run requires update all or sync-dist", f"{OPT}#misplaced-flag"),
    ("build_flags_outside_register", "10. build metadata with install",
     ["install", "@REPO", "--build-url", "https://example.invalid/b.tar", "--build-sha256", "a" * 64],
     "build options require register", f"{OPT}#misplaced-flag"),
    ("build_url_alone", "10. --build-url without --build-sha256 (register)",
     ["register", "@REPO", "--ref", f"{ID}@0.1.0", "--build-url", "https://example.invalid/b.tar"],
     "--build-url and --build-sha256 must be provided together", "catalog/register-one-version"),
]:
    usage_case(*_args)
