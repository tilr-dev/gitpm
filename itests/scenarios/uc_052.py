"""UC-052: Library API and CLI behavioral parity (P0).

The compiled gitpm-api helper (crates/gitpm/examples/api-parity.rs) calls the public
Manager API without a Rust toolchain in scenario containers. The paired lifecycle compares state and deployed bytes
against the CLI after every operation.
"""

import json

from gitpmtest import Failure, T, case

HELLO = "dan/hello"


def resolved(t: T) -> dict:
    return {p["id"]: p for p in t.gitpm_ok("list").json()["resolved"]}


def normalized(t: T, id: str) -> dict:
    """Installed entry without paths/timestamps: what must match the library's Installed."""
    p = t.package(id)
    return {k: p.get(k) for k in ("current", "previous", "enabled", "forkOf", "replaces", "dependencies")}


def rev(t: T, version: str) -> dict:
    return {"version": version, "commit": t.rev(HELLO, f"{HELLO}@{version}")}


@case(fixture="hello_releases", spec=["install-uninstall/three-removal-levels#forget-then-restore", "fork-replacement/fork-copies-an-installed-package#fork-one-package", "dist/forget-restore-and-uninstall-settle-dist"])
def lifecycle_matches_library_contract(t: T):
    """install / update / rollback / enable / disable / forget / restore / fork / uninstall."""
    t.gitpm_ok("install", t.repo(HELLO), "--ref", f"{HELLO}@0.1.0")
    t.eq(normalized(t, HELLO), {"current": rev(t, "0.1.0"), "previous": None, "enabled": True,
                                "forkOf": None, "replaces": None, "dependencies": None}, "install")
    t.gitpm_ok("update", HELLO, "--ref", f"{HELLO}@0.1.1")
    t.eq((normalized(t, HELLO)["current"], normalized(t, HELLO)["previous"]), (rev(t, "0.1.1"), rev(t, "0.1.0")),
         "update: current/previous")
    t.has((t.dist(HELLO) / "index.js").read_text(), "dan/hello 0.1.1")
    t.gitpm_ok("rollback", HELLO)
    t.eq((normalized(t, HELLO)["current"], normalized(t, HELLO)["previous"]), (rev(t, "0.1.0"), rev(t, "0.1.1")),
         "rollback swaps")
    t.has((t.dist(HELLO) / "index.js").read_text(), "dan/hello 0.1.0")
    t.gitpm_ok("disable", HELLO)
    t.eq(t.package(HELLO)["enabled"], False, "disable")
    t.gitpm_ok("enable", HELLO)
    t.eq(t.package(HELLO)["enabled"], True, "enable")
    t.gitpm_ok("forget", HELLO)
    t.ok(HELLO not in t.state()["packages"] and t.clone(HELLO).is_dir(), "forget keeps the clone")
    t.ok(not t.dist(HELLO).exists(), "forget removes dist/ (only active packages are deployed)")
    t.gitpm_ok("restore", HELLO)
    t.eq((normalized(t, HELLO)["current"], normalized(t, HELLO)["previous"]), (rev(t, "0.1.0"), None),
         "restore: no history")
    t.has((t.dist(HELLO) / "index.js").read_text(), "dan/hello 0.1.0")
    fork = t.root / "forks/hello"
    r = t.gitpm_ok("fork", HELLO, "--ns", "mine", "--into", fork)
    t.has(r.out, "mine/hello forked from dan/hello (repository root)", "Next steps (or all at once: add --replace):")
    m = json.loads((fork / "plugin.json").read_text())
    t.eq((m["id"], m["forkOf"]["id"], m["forkOf"]["commit"], m["replaces"]),
         ("mine/hello", HELLO, rev(t, "0.1.0")["commit"], {"id": HELLO, "version": "0.1.0"}), "fork manifest")
    t.eq(t.git(fork, "branch", "--show-current"), "fork", "fork branch")
    t.gitpm_ok("uninstall", HELLO)
    t.not_installed(HELLO)
    t.ok(fork.is_dir(), "uninstall leaves the fork")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#dependency-pulled-from-the-catalog"])
def catalog_install_with_dependency(t: T):
    """install-catalog app/a: the library's catalog_install_resolves_dependencies_and_records_edges outcome."""
    t.gitpm_ok("install-catalog", "app/a")
    state = t.state()
    t.eq(sorted(state["packages"]), ["app/a", "lib/core"], "ids")
    t.eq(state["packages"]["lib/core"]["current"]["version"], "1.1.0", "highest version matching ^1.0")
    t.eq(state["packages"]["app/a"]["dependencies"], {"lib/core": "^1.0"}, "edge")
    t.eq(state["packages"]["lib/core"]["catalog"], str(t.catalog / "catalog.json"), "canonical catalog origin")
    view = resolved(t)
    t.eq(view["lib/core"]["dependents"], ["app/a"], "dependents")
    row = view["app/a"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), ("lib/core", True), "provider")
    t.ok((t.dist("lib/core") / "index.js").is_file(), "dist")


@case(fixture="g6_forked", spec=["fork-replacement/replacement-is-explicit-and-global#replace-for-all-consumers", "fork-replacement/switching-back"])
def replace_and_switch_back(t: T):
    """replace / switch back: the library's fork_replacement_main_scenario outcome."""
    r = t.gitpm_ok("replace", "vendor/base", "--with", "mine/base")
    t.has(r.out, "for: app/a, app/b")  # ReplacementPlan.consumers
    view = resolved(t)
    t.eq(t.state()["replacements"], {"vendor/base": "mine/base"}, "replacements")
    t.eq(view["mine/base"]["replacing"], ["vendor/base"], "replacing")
    t.eq(view["mine/base"]["dependents"], ["app/a", "app/b"], "dependents")
    for c in ["app/a", "app/b"]:
        row = view[c]["dependencies"][0]
        t.eq((row["provider"], row["satisfied"]), ("mine/base", True), f"{c} provider")
    t.gitpm_ok("replace", "vendor/base", "--back")
    t.ok("replacements" not in t.state(), "replacements empty (skipped when empty)")
    view = resolved(t)
    t.ok(not view["mine/base"].get("dependents"), "fork has no dependents")
    for c in ["app/a", "app/b"]:
        t.eq(view[c]["dependencies"][0]["provider"], "vendor/base", f"{c} back on the original")


@case(fixture="hello", spec=["dist/a-revision-without-dist-is-refused#release-without-dist", "errors/stable-error-kinds"])
def bad_manifest_kind(t: T):
    """A release without dist/ and one with broken plugin.json: invalid_manifest, nothing installed."""
    t.commit(HELLO, "no dist", {"dist/index.js": None, "plugin.json": json.dumps({"id": HELLO, "version": "0.2.0"})})
    t.tag(HELLO, f"{HELLO}@0.2.0")
    t.gitpm_err("install", t.repo(HELLO), "--ref", f"{HELLO}@0.2.0", kind="invalid_manifest",
             contains="dist/ directory")
    t.not_installed(HELLO)
    t.commit(HELLO, "broken", {"plugin.json": '{"id": ', "dist/index.js": "x\n"})
    t.gitpm_err("install", t.repo(HELLO), "--ref", "HEAD", kind="invalid_manifest", contains="invalid plugin.json")
    t.not_installed(HELLO)


@case(fixture="hello_installed", spec=["updates/local-work-blocks-update-and-rollback", "install-uninstall/cli-confirmation-flow-for-uninstall#no-tty"])
def dirty_clone_kind(t: T):
    """A dirty clone: update and uninstall (no TTY) refuse with local_changes listing the work."""
    t.release(HELLO, "0.1.1")
    (t.clone(HELLO) / "dist/index.js").write_text("edited\n")
    before = t.snap(t.ws)
    t.gitpm_err("update", HELLO, kind="local_changes")
    t.gitpm_err("uninstall", HELLO, kind="local_changes", contains="modified: dist/index.js")
    t.same(before, "workspace")


@case(fixture="hello_installed", spec=["install-uninstall/install-is-idempotent-for-the-same-source#repeat-install"])
def repeated_install_is_idempotent(t: T):
    """Installing an installed ID again from the same source: exit 0, `ID already installed (V)`,
    nothing changes (the library's AlreadyInstalled outcome)."""
    before = t.snap(t.ws, git=True)
    r = t.gitpm_ok("install", t.repo(HELLO))
    t.has(r.out, f"{HELLO} already installed (0.1.0)")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["dependencies/dependents-guard-removal", "errors/display-is-the-complete-message#has-dependents"])
def dependent_guard_kind(t: T):
    """has_dependents with the library's message for uninstall/forget/disable of lib/core."""
    before = t.snap(t.ws)
    for action in ["uninstall", "forget", "disable"]:
        t.gitpm_err(action, "lib/core", kind="has_dependents",
                 contains=f"cannot {action} lib/core: required by app/a, app/c; doing it anyway requires force")
    t.same(before, "workspace")


@case(fixture="g6_compat_installed", spec=["fork-replacement/compatibility-is-checked-by-versions-only"])
def incompatible_replacement_kind(t: T):
    """Unconfirmed replacement leaving app/b unsatisfied: incompatible_replacement with the library message."""
    before = t.snap(t.ws)
    t.gitpm_err("replace", "vendor/base", "--with", "old/base", kind="incompatible_replacement",
             contains="old/base provides vendor/base 1.2.0, which some consumers do not accept: "
                      "app/b needs vendor/base >=1.3; confirm to apply anyway")
    t.same(before, "workspace")


@case(fixture="g6_core_deps_installed", spec=["cli/json-output-for-state-and-catalog#list", "dependencies/state-keeps-dependency-edges#list-after-install"])
def list_is_library_state_plus_resolved(t: T):
    """`list` prints Manager::state() verbatim plus Manager::list() as `resolved` — no CLI-side logic."""
    out = t.gitpm_ok("list").json()
    resolved_view = out.pop("resolved")
    t.eq(out, t.state(), "list without resolved == state.json")
    t.eq([p["id"] for p in resolved_view], sorted(t.state()["packages"]), "resolved is the sorted flat view")


@case(fixture="hello_releases", spec=["cli/json-output-for-state-and-catalog#list", "dist/dist-holds-exactly-the-active-packages"])
def side_by_side_library_run(t: T):
    """Run a lifecycle through the public API and CLI, comparing state and deployed bytes after each step."""
    paired = t.root / "api-workspace"
    steps = [
        ("install", str(t.repo(HELLO)), f"{HELLO}@0.1.0"),
        ("update", HELLO, f"{HELLO}@0.1.1"),
        ("rollback", HELLO), ("disable", HELLO), ("enable", HELLO),
        ("forget", HELLO), ("restore", HELLO),
    ]
    def files(root):
        return {str(p.relative_to(root)): p.read_bytes() for p in root.rglob("*") if p.is_file()}
    for step in steps:
        cli = list(step[:2]) + (["--ref", step[2]] if len(step) == 3 else [])
        t.gitpm_ok(*cli)
        api = t.run("gitpm-api", str(paired), *step, check=True)
        t.eq(json.loads(api.out), t.state(), f"{step[0]} state parity")
        t.eq(files(paired / "dist"), files(t.ws / "dist"), f"{step[0]} deployed bytes parity")
