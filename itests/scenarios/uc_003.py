"""UC-003: generated UUID identities and independent nonunique aliases."""

import json
from uuid import UUID

from gitpmtest import T, case

SYNTAX = "package-ids/package-id-syntax"


def created_id(t: T, path) -> str:
    return json.loads((t.root / path / "plugin.json").read_text())["id"]


@case(spec=[SYNTAX])
def init_without_namespace_generates_uuid(t: T):
    """A bare name creates a package with a canonical UUID."""
    t.gitpm_ok("init", "hello", "--into", "hello")
    id = created_id(t, "hello")
    t.eq(str(UUID(id)), id, "canonical UUID")
    t.eq(json.loads((t.root / "hello/plugin.json").read_text())["name"], "hello", "alias")


def invalid_case(name: str, value: str, doc: str):
    def fn(t: T):
        before = t.snap(t.root, git=True)
        r = t.gitpm("init", value, "--into", "invalid")
        t.ok(r.code != 0, f"invalid alias {value!r} refused")
        t.same(before, "fixture (files)")
        t.ok(not (t.root / "invalid").exists(), "nothing created")
    fn.__name__, fn.__doc__, fn.__module__ = name, doc, __name__
    case(spec=["package-manifest/known-manifest-keys"])(fn)


for _name, _value, _doc in [
    ("empty_name_refused", "", "An empty alias is invalid."),
    ("slash_refused", "acme/hello", "A scoped ID is not a name."),
    ("dot_dot_refused", "..", "A parent path is invalid."),
    ("dot_refused", "a.b", "A dotted alias is invalid."),
    ("non_ascii_refused", "дан", "Non-ASCII alias is invalid."),
    ("accented_refused", "é", "An accented alias is invalid."),
    ("space_refused", "a b", "A spaced alias is invalid."),
    ("leading_dash_refused", "-x", "A leading dash is invalid."),
    ("too_long_refused", "n" * 81, "An 81-character alias is invalid."),
]:
    invalid_case(_name, _value, _doc)


@case(spec=["package-manifest/known-manifest-keys"])
def valid_characters_accepted(t: T):
    """Safe aliases preserve their spelling beside a generated UUID."""
    for i, value in enumerate(["my_ns-2", "0_9", "_x", "n" * 80]):
        path = f"valid-{i}"
        t.gitpm_ok("init", value, "--into", path)
        id = created_id(t, path)
        t.eq(str(UUID(id)), id, "canonical UUID")
        t.eq(json.loads((t.root / path / "plugin.json").read_text())["name"], value, "alias")
        t.eq(t.git(t.root / path, "tag", "--list"), "v0.1.0", "tag")


@case(spec=["package-ids/ids-are-matched-exactly", "package-ids/case-colliding-ids-are-refused"])
def release_and_update_keep_the_id(t: T):
    """Equal aliases coexist and releasing preserves identity."""
    t.gitpm_ok("init", "keep", "--into", "first")
    t.gitpm_ok("init", "keep", "--into", "second")
    first, second = created_id(t, "first"), created_id(t, "second")
    t.ok(first != second, "distinct identities")
    for path in ("first", "second"):
        t.gitpm_ok("install", t.root / path)
    t.gitpm_ok("commit", t.root / "first", "0.1.1")
    t.eq(created_id(t, "first"), first, "immutable identity")
    t.gitpm_ok("update", first)
    t.eq(sorted(t.gitpm_ok("list").json()["packages"]), sorted([first, second]), "installed UUIDs")
    t.eq(t.package(first)["current"]["version"], "0.1.1", "updated own UUID")
    for id in (first, second):
        t.ok(t.dist(id).is_dir(), "UUID deployment")
    t.ok(t.gitpm("uninstall", "keep", "--force").code != 0, "alias does not resolve")
