"""UC-063: CLI usage errors, help and output modes (P1).

gitpm usage errors: non-zero exit, a message on stderr naming the problem, and no change to
the fixture (workspace with an installed package, catalog, author repositories — the
whole /fx tree incl. Git internals). Also flag order, `info` truncation at 40 entries,
`check` colouring on a TTY vs a pipe, NO_COLOR and TERM=dumb, and help coverage.
`disable --force` is valid (owner decision) and is not tested as an error here.
"""

import json

from gitpmtest import T, case

ID = "dan/hello"
REPO = "/fx/repos/dan/hello"
OPT = "cli/option-validation"
# Every command of `gitpm --help` (main.rs COMMANDS).
COMMANDS = ["install", "install-catalog", "list", "check", "update", "rollback", "enable", "disable", "forget",
            "restore", "uninstall", "fork", "replace", "sync-dist", "init", "commit", "remote", "auth",
            "info", "catalog", "catalog-update", "register"]
# Positional arguments each command takes (main.rs `expected` - 1). install takes SOURCE [ID]
# (the message still says 1); fork takes ID [ID...] (no upper bound).
ARITY = {"list": 0, "check": 0, "sync-dist": 0, "info": 0, "catalog": 0, "catalog-update": 0,
         "commit": 2, "remote": 2, "auth": 2}
ESC = "\x1b["


def listed(command: str, text: str) -> bool:
    """A usage line starts with COMMAND, or names it after `|` (`enable ID | disable ID`)."""
    return any(line.split()[:1] == [command] or f"| {command} " in line for line in text.splitlines())


def usage_case(name: str, doc: str, argv, needle: str, spec: str):
    def fn(t: T):
        before = t.snap(t.root, git=True)
        r = t.gitpm(*argv)
        t.ok(r.code != 0, f"refused (exit {r.code})")
        t.has(r.err, needle)
        t.eq(r.out, "", "stdout")
        t.same(before, "fixture (workspace, catalog, repositories)")
    fn.__name__, fn.__doc__, fn.__module__ = name, doc, __name__
    case(fixture="g1_ws_catalog", spec=[spec])(fn)


for _name, _doc, _argv, _needle, _spec in [
    # 1. flags on the wrong command
    ("ref_with_fork", "1. --ref with fork", ["fork", ID, "--ns", "mine", "--into", "/fx/forked", "--ref", f"{ID}@0.1.0"],
     "--ref is not valid for fork", f"{OPT}#misplaced-flag"),
    ("version_with_install", "1. --version with install", ["install", REPO, "--version", "0.1.0"],
     "--version is not valid for install", f"{OPT}#misplaced-flag"),
    ("sources_with_check", "1. --sources with check", ["check", "--sources", "/fx/sources.txt"],
     "--sources requires catalog-update", f"{OPT}#misplaced-flag"),
    ("dry_run_with_catalog_update", "1. --dry-run with catalog-update", ["catalog-update", "--dry-run"],
     "--dry-run requires update all or sync-dist", f"{OPT}#misplaced-flag"),
    ("force_with_update", "1. --force with update", ["update", ID, "--force"],
     "--force is only valid with uninstall/disable/forget/replace/fork --replace", f"{OPT}#misplaced-flag"),
    ("force_with_list", "1. --force with list", ["list", "--force"],
     "--force is only valid with uninstall/disable/forget/replace/fork --replace", f"{OPT}#misplaced-flag"),
    ("with_without_replace", "1. --with outside replace", ["update", ID, "--with", "dan/other"],
     "--with/--back require replace", f"{OPT}#misplaced-flag"),
    ("multi_with_install", "1. --multi outside init", ["install", REPO, "--multi"],
     "--multi/--no-release require init", f"{OPT}#misplaced-flag"),
    ("package_with_init", "1. --package with init", ["init", "dan/x", "--into", "/fx/x", "--package", "dan/x"],
     "--package requires install/commit/register", f"{OPT}#misplaced-flag"),
    # 2. repeated / missing / unknown
    ("repeated_ref", "2. --ref a --ref b", ["update", ID, "--ref", "a", "--ref", "b"], "--ref given more than once", OPT),
    ("repeated_force", "2. --force twice", ["uninstall", ID, "--force", "--force"], "--force given more than once", OPT),
    ("missing_ref_value", "2. --ref at the end", ["update", ID, "--ref"], "--ref requires a value", OPT),
    ("unknown_flag", "2. an unknown flag", ["list", "--bogus"], "unknown option: --bogus; use --help", OPT),
    ("namespace_flag_is_unknown", "2. --namespace (removed in 0.4) is an unknown option",
     ["install", REPO, "--namespace", "dan"], "unknown option: --namespace; use --help", OPT),
    ("unknown_command", "2. an unknown command", ["upgrade", ID], "unknown command: upgrade; use --help",
     "cli/command-set#removed-command"),
    ("removed_namespace_command", "2. the removed namespace command", ["namespace", "dan"],
     "unknown command: namespace; use --help", "cli/command-set#removed-command"),
    ("removed_sync_command", "2. the removed sync command", ["sync", "--dry-run"],
     "unknown command: sync; use --help", "cli/command-set#removed-command"),
    ("unknown_auth_subcommand", "2. an unknown auth subcommand", ["auth", "login", REPO],
     "use: gitpm auth check GIT_URL", "cli/command-set"),
    # 5. --channel with --ref is no usage error any more: the library refuses dev + ref (uc_012).
    ("replace_needs_one_mode", "3. replace without --with/--back", ["replace", ID], "replace needs exactly one of", OPT),
    ("fork_needs_ns", "3. fork without --ns", ["fork", ID, "/fx/forked"], "fork needs --ns NAMESPACE", OPT),
]:
    usage_case(_name, _doc, _argv, _needle, _spec)


@case(fixture="g1_ws_catalog", spec=[OPT])
def wrong_argument_count(t: T):
    """3. Every command with too few (where required) and too many positional arguments."""
    before = t.snap(t.root, git=True)
    failures = []
    for command in COMMANDS:
        arity = ARITY.get(command, 1)
        mode = ["--back"] if command == "replace" else []
        attempts = [] if command == "fork" else [3 if command == "install" else arity + 1]
        if arity > 0:
            attempts.append(0)
            if arity == 2:
                attempts.append(1)
        for n in attempts:
            args = [f"arg{i}" for i in range(n)]
            if command == "auth" and args:
                args[0] = "check"
            r = t.gitpm(command, *args, *mode)
            problems = []
            if r.code == 0:
                problems.append("exit 0")
            if f"{command}: expected" not in r.err or f"got {n}; use --help" not in r.err:
                problems.append(f"message {r.err.strip()!r}")
            if problems:
                failures.append(f"gitpm {command} with {n} argument(s): {', '.join(problems)}")
            else:
                print(f"  ok: {command} with {n} argument(s) refused")
    t.same(before, "fixture")
    t.eq(failures, [], "argument-count failures")


@case(fixture="hello", spec=[OPT, "install-uninstall/uninstall-is-planned-and-confirmed", "cli/option-validation#flag-position"])
def uninstall_flag_order_is_irrelevant(t: T):
    """4. `uninstall ID --force` and `uninstall --force ID` both remove a dirty clone the same way."""
    results = []
    for argv in (["uninstall", ID, "--force"], ["uninstall", "--force", ID]):
        t.gitpm_ok("install", t.repo(ID))
        (t.clone(ID) / "dist/index.js").write_text("local edit\n")
        r = t.gitpm(*argv)
        t.eq(r.code, 0, f"exit of gitpm {' '.join(argv)}")
        t.not_installed(ID)
        results.append((r.code, r.out, r.err))
    t.eq(results[0], results[1], "same outcome for both orders")


def v2_catalog(t: T, name: str, n: int):
    """A v2 tree namespaces/dan/pNN with n package folders: n + 2 overview entries."""
    root = t.root / name
    ns = root / "namespaces/dan"
    ns.mkdir(parents=True)
    (root / "catalog.json").write_text(json.dumps({"schema": 2, "directories": {"namespaces": "namespaces/"}}))
    (root / "namespaces/catalog.json").write_text(json.dumps({"schema": 2, "directories": {"dan": "dan/"}}))
    dirs = {}
    for i in range(n):
        name = f"p{i:02}"
        dirs[name] = f"{name}/"
        (ns / name).mkdir()
        (ns / name / "plugin.json").write_text(json.dumps(
            {"schema": 2, "id": f"dan/{name}", "source": f"https://example.invalid/{name}.git", "versions": {}}))
    (ns / "catalog.json").write_text(json.dumps({"schema": 2, "directories": dirs}))
    return root


@case(spec=["catalog/bounded-overview"])
def info_below_the_limit_has_no_hint(t: T):
    """6. A catalog with 39 entries (2 directories, 37 packages) lists all, without the hint."""
    out = t.gitpm_ok("--catalog", v2_catalog(t, "v2-39", 37), "info").out
    t.has(out, "dan/p00", "dan/p36")
    t.lacks(out, "Overview limit")


@case(spec=["catalog/bounded-overview"])
def info_at_the_limit_shows_hint(t: T):
    """6. 40 entries (38 packages) reach the limit: all 40 shown plus the truncation hint."""
    out = t.gitpm_ok("--catalog", v2_catalog(t, "v2-40", 38), "info").out
    t.has(out, "dan/p00", "dan/p37", "Overview limit: 40 entries")


@case(spec=["catalog/bounded-overview"])
def info_v2_more_than_40_entries(t: T):
    """6. A v2 tree with 45 package folders is truncated with the hint."""
    out = t.gitpm_ok("--catalog", v2_catalog(t, "v2", 45), "info").out
    t.has(out, "dan/p00", "Overview limit: 40 entries")
    t.lacks(out, "dan/p44")


def tty(t: T, cmd: str, env: dict):
    return t.run("script", "-qec", cmd, "/dev/null", env=env, input="")


@case(fixture="g1_ws_catalog", spec=["catalog/check-compares-installed-with-the-catalog"])
def check_pipe_has_no_colour(t: T):
    """7. check into a pipe: no ANSI codes, update listed, exit 0."""
    r = t.gitpm("check", env={"TERM": "xterm-256color"})
    t.eq(r.code, 0, "exit with an update available")
    t.has(r.out, "update available", "Available updates:")
    t.lacks(r.out + r.err, ESC)


@case(fixture="g1_ws_catalog", spec=["catalog/check-compares-installed-with-the-catalog"])
def check_tty_is_coloured(t: T):
    """7. check on a TTY: the update is green; exit 0 and stable across runs."""
    codes = []
    for _ in range(2):
        r = tty(t, "gitpm check", {"TERM": "xterm-256color"})
        codes.append(r.code)
        t.has(r.out, "update available", f"{ESC}32m")
    t.eq(codes, [0, 0], "exit codes with an update available")


@case(fixture="g1_ws_catalog", spec=["catalog/check-compares-installed-with-the-catalog"])
def check_tty_no_color(t: T):
    """7. NO_COLOR=1 on a TTY: no ANSI codes."""
    r = tty(t, "gitpm check", {"TERM": "xterm-256color", "NO_COLOR": "1"})
    t.eq(r.code, 0, "exit")
    t.has(r.out, "update available")
    t.lacks(r.out, ESC)


@case(fixture="g1_ws_catalog", spec=["catalog/check-compares-installed-with-the-catalog"])
def check_tty_term_dumb(t: T):
    """7. TERM=dumb on a TTY: no ANSI codes."""
    r = tty(t, "gitpm check", {"TERM": "dumb"})
    t.eq(r.code, 0, "exit")
    t.has(r.out, "update available")
    t.lacks(r.out, ESC)


@case(fixture="g1_ws_catalog", spec=["catalog/check-compares-installed-with-the-catalog", "catalog/check-compares-installed-with-the-catalog#exit-status"])
def check_exit_code_without_updates(t: T):
    """7. The exit code is 0 with and without updates (README: updates do not change it)."""
    t.eq(t.gitpm("check").code, 0, "with an update")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.2.0")
    r = t.gitpm("check")
    t.eq(r.code, 0, "without updates")
    t.has(r.out, "Updates: 0.")


@case(spec=["cli/help", "cli/command-set#every-command-documented"])
def help_lists_every_command(t: T):
    """8. --help, -h, help and no arguments print one text listing every command."""
    texts = [t.gitpm_ok(*argv).out for argv in (["--help"], ["-h"], ["help"], [])]
    t.eq(len(set(texts)), 1, "distinct help texts")
    for command in COMMANDS:
        t.ok(listed(command, texts[0]), f"help lists {command}")


@case(spec=["cli/help#command-help"])
def command_help_prints_only_that_command(t: T):
    """8. COMMAND --help and -h print only that command's usage block, exit 0."""
    full = t.gitpm_ok("--help").out
    for command in COMMANDS:
        texts = [t.gitpm_ok(command, flag).out for flag in ("--help", "-h")]
        t.eq(texts[0], texts[1], f"{command} --help == -h")
        t.ok(texts[0].strip() and len(texts[0]) < len(full), f"{command} help is a part of the full help")
        t.ok(listed(command, texts[0]), f"{command} help names {command}")
