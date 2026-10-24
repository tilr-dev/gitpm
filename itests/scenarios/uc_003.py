"""UC-003: Namespace validation (P0).

gitpm has no default namespace and no namespace configuration: `init`/`create` take a
full NAMESPACE/NAME. An unscoped name is refused; an invalid namespace is refused with
`invalid_id` before anything is written; valid characters and an 80-character component
are accepted. Releasing a package later never renames it.
"""

import json

from gitpmtest import T, case

SYNTAX = "package-ids/package-id-syntax"


def created_id(t: T, path) -> str:
    return json.loads((t.root / path / "plugin.json").read_text())["id"]


@case(spec=["package-ids/no-default-namespace#init-without-a-namespace"])
def init_without_namespace_refused(t: T):
    """`init hello --into a/p1` (no namespace) is refused and creates nothing."""
    r = t.gitpm("init", "hello", "--into", "a/p1")
    t.ok(r.code != 0, "init refused")
    t.has(r.err, "package ID needs a namespace: use NAMESPACE/NAME, for example acme/hello")
    t.has(r.err, "Error kind: invalid_id")
    t.ok(not (t.root / "a").exists(), "nothing created")


def invalid_case(name: str, value: str, doc: str):
    def fn(t: T):
        before = t.snap(t.root, git=True)
        for command in ("init",):
            r = t.gitpm(command, f"{value}/x", "--into", "a/x")
            t.ok(r.code != 0, f"{command} {value!r}/x refused (exit {r.code})")
            if value.startswith("-"):
                # A positional starting with '-' is parsed as an option (usage error).
                t.has(r.err, f"unknown option: {value}/x")
            else:
                t.has(r.err, "Error kind: invalid_id", "1–80 ASCII letters, digits, '-' or '_'")
        t.same(before, "fixture (files)")
        t.ok(not (t.root / "a").exists(), "no package directory created")
    fn.__name__, fn.__doc__, fn.__module__ = name, doc, __name__
    tag = f"{SYNTAX}#empty-namespace" if value == "" else f"{SYNTAX}#unsafe-components-are-refused"
    case(spec=[tag])(fn)


for _name, _value, _doc in [
    ("empty_namespace_refused", "", "An empty namespace is invalid."),
    ("slash_refused", "a/b", "A slash in the namespace (three components)."),
    ("dot_dot_refused", "..", "A '..' namespace."),
    ("dot_refused", "a.b", "A dot in the namespace."),
    ("non_ascii_refused", "дан", "Non-ASCII (Cyrillic) namespace."),
    ("accented_refused", "é", "Non-ASCII (é) namespace."),
    ("space_refused", "a b", "A space in the namespace."),
    ("leading_dash_refused", "-x", "A namespace starting with '-'."),
    ("too_long_refused", "n" * 81, "An 81-character namespace."),
]:
    invalid_case(_name, _value, _doc)


@case(spec=[f"{SYNTAX}#valid-scoped-id"])
def valid_characters_accepted(t: T):
    """Underscore, hyphen, ASCII digits and an 80-character component are accepted."""
    for i, value in enumerate(["my_ns-2", "0_9", "_x", "n" * 80]):
        t.gitpm_ok("init", f"{value}/ok", "--into", f"a/ok{i}")
        t.eq(created_id(t, f"a/ok{i}"), f"{value}/ok", "ID")
        t.has(t.git(t.root / f"a/ok{i}", "tag", "--list"), f"{value}/ok@0.1.0")


@case(spec=["package-ids/no-default-namespace"])
def release_and_update_keep_the_id(t: T):
    """A package keeps its ID through install, a new release and update; nothing is renamed."""
    t.gitpm_ok("init", "first/keep", "--into", "a/keep")
    repo = t.root / "a/keep"
    t.gitpm_ok("install", repo)
    t.gitpm_ok("init", "second/new", "--into", "a/new")
    t.eq(created_id(t, "a/new"), "second/new", "new ID")
    t.gitpm_ok("commit", repo, "0.1.1")
    t.gitpm_ok("update", "first/keep")
    state = t.gitpm_ok("list").json()
    t.eq(sorted(state["packages"]), ["first/keep"], "installed IDs")
    t.eq(t.package("first/keep")["current"]["version"], "0.1.1", "updated under its own ID")
    t.ok(t.worktree("first/keep").is_dir() and not (t.ws / "packages").exists(), "clone path kept")
