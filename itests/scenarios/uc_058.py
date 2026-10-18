"""UC-058: Required package dependencies without replacement (P0, mandatory).

Fixtures g6_core / g6_core_catalog: lib/core 1.0.0, 1.1.0, 2.0.0; app/a (lib/core ^1.0),
app/b (^2.0), app/c ("1"), app/lonely (lib/missing ^1, not in the catalog), the cycle
x/a ↔ x/b, other/x — all registered in /fx/catalog (GITPM_CATALOG).
"""

import json

from gitpmtest import T, case

# No catalog configured: an empty GITPM_CATALOG counts as unset.
NO_CATALOG = {"GITPM_CATALOG": ""}

CORE = "lib/core"


def resolved(t: T, root=None) -> dict:
    args = ("--root", root) if root else ()
    return {p["id"]: p for p in t.gitpm_ok(*args, "list").json()["resolved"]}


def registered(t: T, id: str, version: str) -> dict:
    return json.loads(t.catalog_release(id, version).read_text())


# ------------------------------------------------------------ 1. manifest validation


@case(fixture="g6_core", spec=["dependencies/requirement-syntax", "dependencies/state-keeps-dependency-edges"])
def valid_requirements_accepted(t: T):
    """1a. Cargo-style requirements are accepted (direct install with lib/core installed)."""
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"{CORE}@1.1.0")
    for i, req in enumerate(["^1.0", "1.1", "~1.1", ">=1.0, <2", "=1.1.0", "1.*", "*"]):
        id = f"ok/r{i}"
        t.new_package(id, "1.0.0", deps={CORE: req})
        t.gitpm_ok("install", t.repo(id))
        t.eq(t.package(id)["dependencies"], {CORE: req}, f"{id} dependency edge")


def refused(t: T, id: str, deps: dict, kind: str, *needles: str):
    t.new_package(id, "1.0.0", deps=deps)
    # An existing workspace (lib/core installed) so "nothing changed" is byte-exact; an empty
    # workspace skeleton on a refused install into a missing one is accepted by the owner.
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"{CORE}@1.1.0")
    before = t.snap(t.ws)
    t.gitpm_err("install", t.repo(id), kind=kind, contains=needles)
    t.same(before, "workspace")
    t.not_installed(id)
    t.gitpm_err("--catalog", t.root / "check-catalog", "register", t.repo(id), "--ref", f"{id}@1.0.0",
             kind=kind, contains=needles)


@case(fixture="g6_core", spec=["dependencies/requirement-syntax"])
def banana_requirement_refused(t: T):
    """1b. "banana" is not a requirement: invalid_manifest on install and register."""
    refused(t, "bad/banana", {CORE: "banana"}, "invalid_manifest", "banana")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#pre-release-requirement"])
def prerelease_requirement_refused(t: T):
    """1c. A pre-release requirement is refused."""
    refused(t, "bad/pre", {CORE: "^1.0.0-beta"}, "invalid_manifest", "pre-release")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#self-dependency"])
def self_dependency_refused(t: T):
    """1d. A package depending on itself is refused."""
    refused(t, "bad/self", {"bad/self": "^1"}, "invalid_manifest", "depends on itself")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax", "package-ids/package-id-syntax#unsafe-components-are-refused"])
def invalid_dependency_id_refused(t: T):
    """1e. An invalid dependency ID is refused with invalid_id."""
    refused(t, "bad/id", {"Lib/../core": "^1"}, "invalid_id")


# ------------------------------------------------------------ 2–7. catalog resolution


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#dependency-pulled-from-the-catalog", "dependencies/state-keeps-dependency-edges#list-after-install"])
def catalog_install_pulls_dependency(t: T):
    """2. install-catalog app/a into an empty workspace installs lib/core 1.1.0 in the same command."""
    t.gitpm_ok("install-catalog", "app/a")
    state = t.state()
    t.eq(sorted(state["packages"]), ["app/a", CORE], "installed")
    core = state["packages"][CORE]
    t.eq(core["current"], {"version": "1.1.0", "commit": registered(t, CORE, "1.1.0")["commit"]},
         "lib/core current (max matching ^1.0)")
    t.eq(state["packages"]["app/a"]["dependencies"], {CORE: "^1.0"}, "dependency edge in state")
    t.eq(t.rev(t.clone(CORE)), core["current"]["commit"], "lib/core clone HEAD")
    t.has((t.dist(CORE) / "index.js").read_text(), "lib/core 1.1.0")
    view = resolved(t)
    row = view["app/a"]["dependencies"][0]
    t.eq((row["id"], row["provider"], row["version"], row["satisfied"]), (CORE, CORE, "1.1.0", True),
         "resolved dependency")
    t.eq(view[CORE].get("dependents"), ["app/a"], "lib/core dependents")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained"])
def missing_dependency_refused(t: T):
    """3. app/lonely needs lib/missing (not in the catalog): dependency_conflict naming it; byte-identical."""
    t.gitpm_ok("install-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "app/lonely", kind="dependency_conflict", contains="lib/missing")
    t.same(before, "workspace")
    t.not_installed("app/lonely")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained", "install-uninstall/refused-input-creates-nothing"])
def missing_dependency_refused_in_empty_workspace(t: T):
    """3'. The same into an empty workspace leaves no package, clone, dist or transaction."""
    t.gitpm_err("install-catalog", "app/lonely", kind="dependency_conflict", contains="lib/missing")
    t.not_installed("app/lonely")
    t.eq(t.state().get("packages", {}), {}, "no packages")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained#two-consumers-disagree"])
def version_conflict_names_both_consumers(t: T):
    """4. app/b (^2.0) while app/a (^1.0) is installed: dependency_conflict naming app/a and app/b."""
    t.gitpm_ok("install-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "app/b", kind="dependency_conflict", contains=("app/a", "app/b", CORE))
    t.same(before, "workspace")
    t.not_installed("app/b")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def cycle_resolves_deterministically(t: T):
    """5. x/a ↔ x/b resolves and installs within 60 s, identically in two workspaces."""
    r1 = t.gitpm("install-catalog", "x/a", timeout=60)
    r2 = t.gitpm("--root", t.root / "ws2", "install-catalog", "x/a", timeout=60)
    t.eq((r1.code, r2.code), (0, 0), "exit codes")
    one = t.state()["packages"]
    two = json.loads((t.root / "ws2/state.json").read_text())["packages"]
    t.eq(sorted(one), ["x/a", "x/b"], "installed")
    t.eq({k: v["current"] for k, v in one.items()}, {k: v["current"] for k, v in two.items()}, "same result")
    view = resolved(t)
    t.eq(view["x/a"]["dependencies"][0]["provider"], "x/b", "x/a → x/b")
    t.eq(view["x/b"]["dependencies"][0]["provider"], "x/a", "x/b → x/a")


@case(fixture="g6_core_catalog", spec=["dependencies/updates-keep-dependents-working#breaking-update"])
def update_breaking_dependent_refused(t: T):
    """6. update lib/core (latest 2.0.0) while app/a needs ^1.0: refused naming app/a; 1.1.0 untouched."""
    t.gitpm_ok("install-catalog", "app/a")
    before = t.snap(t.ws)
    t.gitpm_err("update", CORE, kind="dependency_conflict", contains="app/a")
    t.same(before, "workspace")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "lib/core version")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies", "dependencies/state-keeps-dependency-edges"])
def shared_dependency_single_copy(t: T):
    """7. app/a and app/c share one lib/core; uninstalling app/a keeps it."""
    t.gitpm_ok("install-catalog", "app/a")
    t.gitpm_ok("install-catalog", "app/c")
    t.eq(sorted(t.state()["packages"]), ["app/a", "app/c", CORE], "one lib/core")
    t.eq(resolved(t)[CORE].get("dependents"), ["app/a", "app/c"], "dependents")
    core = t.snap(t.clone(CORE), t.dist(CORE))
    t.gitpm_ok("uninstall", "app/a")
    t.not_installed("app/a")
    t.same(core, "lib/core clone and dist")
    t.eq(resolved(t)[CORE].get("dependents"), ["app/c"], "dependents after uninstall")


# ------------------------------------------------------------- 8. direct Git install


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#no-catalog"])
def direct_install_without_catalog_refused(t: T):
    """8a. No catalog configured and lib/core not installed: refused naming lib/core and "no catalog configured"."""
    t.gitpm_err("install", t.repo("app/a"), env=NO_CATALOG, kind="dependency_conflict",
             contains=(CORE, "no catalog configured"))
    t.not_installed("app/a")
    t.not_installed(CORE)


@case(fixture="g6_core_catalog", spec=["cli/commands-that-need-a-catalog-say-so"])
def missing_catalog_location_is_refused(t: T):
    """8a'. A configured catalog location that does not exist is no "no catalog": `--catalog ./missing install`
    fails with the library's invalid_argument before anything is created."""
    before = t.snap(t.root, git=True)
    t.gitpm_err("--catalog", "./missing", "install", t.repo("app/a"), kind="invalid_argument",
             contains="catalog location ./missing does not exist")
    t.same(before, "fixture (workspace, catalog, repos)")
    t.ok(not (t.root / "missing").exists(), "no catalog created")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def direct_install_with_dependency_installed(t: T):
    """8b. lib/core already installed (from Git), no catalog: app/a installs and resolves to it."""
    before = t.snap(t.catalog)
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"{CORE}@1.1.0", env=NO_CATALOG)
    t.gitpm_ok("install", t.repo("app/a"), env=NO_CATALOG)
    row = resolved(t)["app/a"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), (CORE, True), "resolved")
    t.eq(t.package("app/a").get("catalog"), None, "direct install records no catalog")
    t.same(before, "catalog untouched")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def direct_install_with_catalog(t: T):
    """8c. With the catalog: app/a from Git, lib/core 1.1.0 pulled from the catalog."""
    t.gitpm_ok("install", t.repo("app/a"))
    t.eq(sorted(t.state()["packages"]), ["app/a", CORE], "installed")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "lib/core version")
    row = resolved(t)["app/a"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), (CORE, True), "resolved")
