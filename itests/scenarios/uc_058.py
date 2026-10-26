"""UC-058: Required package dependencies without replacement (P0, mandatory).

Fixtures g6_core / g6_core_catalog: 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.0.0, 1.1.0, 2.0.0; 9692fc07-23c7-5f20-8779-f09374f1a571 (757fd5ca-2b6a-5b2f-8a5d-60980b028665 ^1.0),
f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^2.0), 1a620c0a-aa3c-533c-9960-3d49016fa231 (^1), 2a8af310-7783-5cbb-8218-c875e4f8a3e6 (0276fe29-bef5-5d0b-aae9-f9920c809729 ^1, not in the catalog), the cycle
d35b3151-28fd-557a-a363-b5f18ae491c8 ↔ 3a53cdea-2cb1-5653-bf22-898b679440ee, a768d335-0cf7-597a-b1ba-0eb542495aec — all registered in /fx/catalog (GITPM_CATALOG).
"""

import json

from gitpmtest import T, case

# No catalog configured: an empty GITPM_CATALOG counts as unset.
NO_CATALOG = {"GITPM_CATALOG": ""}

CORE = "757fd5ca-2b6a-5b2f-8a5d-60980b028665"


def resolved(t: T, root=None) -> dict:
    args = ("--root", root) if root else ()
    return {p["id"]: p for p in t.gitpm_ok(*args, "list").json()["resolved"]}


def registered(t: T, id: str, version: str) -> dict:
    return json.loads(t.catalog_release(id, version).read_text())


# ------------------------------------------------------------ 1. manifest validation


@case(fixture="g6_core", spec=["dependencies/requirement-syntax", "dependencies/state-keeps-dependency-edges"])
def valid_requirements_accepted(t: T):
    """1a. npm-style ranges 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0 satisfies are accepted (direct install with 757fd5ca-2b6a-5b2f-8a5d-60980b028665 installed)."""
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0")
    for i, req in enumerate(["^1.0", "1.1.0", "~1.1", ">=1.0.0 <2.0.0", "=1.1.0", ">= 1.1", "*", "^1.1.0-beta"]):
        id = f"236f5100-a0ab-5a0f-b203-742eeb73038{i:x}"
        t.new_package(id, "1.0.0", deps={CORE: req})
        t.gitpm_ok("install", t.repo(id))
        t.eq(t.package(id)["dependencies"], {CORE: req}, f"{id} dependency edge")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#bare-version-is-exact"])
def bare_version_is_exact(t: T):
    """1f. A bare version is exact: 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0 installed does not satisfy "1.0.0"."""
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0")
    t.new_package("0dfc1fa2-a78b-5d3d-a4b5-2b1d8597fa8a", "1.0.0", deps={CORE: "1.0.0"})
    t.gitpm_err("install", t.repo("0dfc1fa2-a78b-5d3d-a4b5-2b1d8597fa8a"), kind="dependency_conflict", contains=CORE)
    t.not_installed("0dfc1fa2-a78b-5d3d-a4b5-2b1d8597fa8a")


def refused(t: T, id: str, deps: dict, kind: str, *needles: str):
    t.new_package(id, "1.0.0", deps=deps)
    # An existing workspace (757fd5ca-2b6a-5b2f-8a5d-60980b028665 installed) so "nothing changed" is byte-exact; an empty
    # workspace skeleton on a refused install into a missing one is accepted by the owner.
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0")
    before = t.snap(t.ws)
    t.gitpm_err("install", t.repo(id), kind=kind, contains=needles)
    t.same(before, "workspace")
    t.not_installed(id)
    t.gitpm_err("--catalog", t.root / "check-catalog", "register", t.repo(id), "--ref", f"v1.0.0",
             kind=kind, contains=needles)


@case(fixture="g6_core", spec=["dependencies/requirement-syntax"])
def banana_requirement_refused(t: T):
    """1b. "banana" is not a requirement: invalid_manifest on install and register."""
    refused(t, "437fb969-52d2-56b9-8e77-0926367c136c", {CORE: "banana"}, "invalid_manifest", "banana")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#old-cargo-syntax"])
def cargo_syntax_refused(t: T):
    """1c. Cargo commas, wildcards, bare partial versions, || and hyphen ranges are refused."""
    for i, req in enumerate([">=1.0, <2", "1.*", "1.1", "^1 || ^2", "1.0.0 - 2.0.0"]):
        refused(t, f"0dd1434c-36ec-56c7-9810-32af84776a3{i:x}", {CORE: req}, "invalid_manifest", "invalid version requirement")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax#self-dependency"])
def self_dependency_refused(t: T):
    """1d. A package depending on itself is refused."""
    refused(t, "e6ee1820-f12d-51e4-8939-f802a1cbe9c7", {"e6ee1820-f12d-51e4-8939-f802a1cbe9c7": "^1"}, "invalid_manifest", "depends on itself")


@case(fixture="g6_core", spec=["dependencies/requirement-syntax", "package-ids/package-id-syntax#scoped-and-bare-ids-refused"])
def invalid_dependency_id_refused(t: T):
    """1e. An invalid dependency ID is refused with invalid_id."""
    refused(t, "fa24bbbe-4480-5311-abfc-7a79c11e70cc", {"Lib/../core": "^1"}, "invalid_id")


# ------------------------------------------------------------ 2–7. catalog resolution


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#dependency-pulled-from-the-catalog", "dependencies/state-keeps-dependency-edges#list-after-install"])
def catalog_install_pulls_dependency(t: T):
    """2. install-catalog 9692fc07-23c7-5f20-8779-f09374f1a571 into an empty workspace installs 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0 in the same command."""
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    state = t.state()
    t.eq(sorted(state["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", CORE]), "installed")
    core = state["packages"][CORE]
    t.eq(core["current"], {"version": "1.1.0", "commit": registered(t, CORE, "1.1.0")["commit"]},
         "757fd5ca-2b6a-5b2f-8a5d-60980b028665 current (max matching ^1.0)")
    t.eq(state["packages"]["9692fc07-23c7-5f20-8779-f09374f1a571"]["dependencies"], {CORE: "^1.0"}, "dependency edge in state")
    t.eq(t.rev(t.worktree(CORE)), core["current"]["commit"], "757fd5ca-2b6a-5b2f-8a5d-60980b028665 clone HEAD")
    t.has((t.dist(CORE) / "index.js").read_text(), "757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0")
    view = resolved(t)
    row = view["9692fc07-23c7-5f20-8779-f09374f1a571"]["dependencies"][0]
    t.eq((row["id"], row["provider"], row["version"], row["satisfied"]), (CORE, CORE, "1.1.0", True),
         "resolved dependency")
    t.eq(view[CORE].get("dependents"), ["9692fc07-23c7-5f20-8779-f09374f1a571"], "757fd5ca-2b6a-5b2f-8a5d-60980b028665 dependents")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained"])
def missing_dependency_refused(t: T):
    """3. 2a8af310-7783-5cbb-8218-c875e4f8a3e6 needs 0276fe29-bef5-5d0b-aae9-f9920c809729 (not in the catalog): dependency_conflict naming it; byte-identical."""
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "2a8af310-7783-5cbb-8218-c875e4f8a3e6", kind="dependency_conflict", contains="0276fe29-bef5-5d0b-aae9-f9920c809729")
    t.same(before, "workspace")
    t.not_installed("2a8af310-7783-5cbb-8218-c875e4f8a3e6")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained", "install-uninstall/refused-input-creates-nothing"])
def missing_dependency_refused_in_empty_workspace(t: T):
    """3'. The same into an empty workspace leaves no package, clone, dist or transaction."""
    t.gitpm_err("install-catalog", "2a8af310-7783-5cbb-8218-c875e4f8a3e6", kind="dependency_conflict", contains="0276fe29-bef5-5d0b-aae9-f9920c809729")
    t.not_installed("2a8af310-7783-5cbb-8218-c875e4f8a3e6")
    t.eq(t.state().get("packages", {}), {}, "no packages")


@case(fixture="g6_core_catalog", spec=["dependencies/conflicts-are-explained#two-consumers-disagree"])
def version_conflict_names_both_consumers(t: T):
    """4. f5984d44-92bd-51ac-8e8f-d911dd824ef3 (^2.0) while 9692fc07-23c7-5f20-8779-f09374f1a571 (^1.0) is installed: dependency_conflict naming 9692fc07-23c7-5f20-8779-f09374f1a571 and f5984d44-92bd-51ac-8e8f-d911dd824ef3."""
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", kind="dependency_conflict", contains=("9692fc07-23c7-5f20-8779-f09374f1a571", "f5984d44-92bd-51ac-8e8f-d911dd824ef3", CORE))
    t.same(before, "workspace")
    t.not_installed("f5984d44-92bd-51ac-8e8f-d911dd824ef3")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def cycle_resolves_deterministically(t: T):
    """5. d35b3151-28fd-557a-a363-b5f18ae491c8 ↔ 3a53cdea-2cb1-5653-bf22-898b679440ee resolves and installs within 60 s, identically in two workspaces."""
    r1 = t.gitpm("install-catalog", "d35b3151-28fd-557a-a363-b5f18ae491c8", timeout=60)
    r2 = t.gitpm("--root", t.root / "ws2", "install-catalog", "d35b3151-28fd-557a-a363-b5f18ae491c8", timeout=60)
    t.eq((r1.code, r2.code), (0, 0), "exit codes")
    one = t.state()["packages"]
    two = json.loads((t.root / "ws2/state.json").read_text())["packages"]
    t.eq(sorted(one), sorted(["d35b3151-28fd-557a-a363-b5f18ae491c8", "3a53cdea-2cb1-5653-bf22-898b679440ee"]), "installed")
    t.eq({k: v["current"] for k, v in one.items()}, {k: v["current"] for k, v in two.items()}, "same result")
    view = resolved(t)
    t.eq(view["d35b3151-28fd-557a-a363-b5f18ae491c8"]["dependencies"][0]["provider"], "3a53cdea-2cb1-5653-bf22-898b679440ee", "d35b3151-28fd-557a-a363-b5f18ae491c8 → 3a53cdea-2cb1-5653-bf22-898b679440ee")
    t.eq(view["3a53cdea-2cb1-5653-bf22-898b679440ee"]["dependencies"][0]["provider"], "d35b3151-28fd-557a-a363-b5f18ae491c8", "3a53cdea-2cb1-5653-bf22-898b679440ee → d35b3151-28fd-557a-a363-b5f18ae491c8")


@case(fixture="g6_core_catalog", spec=["dependencies/updates-keep-dependents-working#breaking-update"])
def update_breaking_dependent_refused(t: T):
    """6. update 757fd5ca-2b6a-5b2f-8a5d-60980b028665 (latest 2.0.0) while 9692fc07-23c7-5f20-8779-f09374f1a571 needs ^1.0: refused naming 9692fc07-23c7-5f20-8779-f09374f1a571; 1.1.0 untouched."""
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    before = t.snap(t.ws)
    t.gitpm_err("update", CORE, kind="dependency_conflict", contains="9692fc07-23c7-5f20-8779-f09374f1a571")
    t.same(before, "workspace")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "757fd5ca-2b6a-5b2f-8a5d-60980b028665 version")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies", "dependencies/state-keeps-dependency-edges"])
def shared_dependency_single_copy(t: T):
    """7. 9692fc07-23c7-5f20-8779-f09374f1a571 and 1a620c0a-aa3c-533c-9960-3d49016fa231 share one 757fd5ca-2b6a-5b2f-8a5d-60980b028665; uninstalling 9692fc07-23c7-5f20-8779-f09374f1a571 keeps it."""
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    t.gitpm_ok("install-catalog", "1a620c0a-aa3c-533c-9960-3d49016fa231")
    t.eq(sorted(t.state()["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231", CORE]), "one 757fd5ca-2b6a-5b2f-8a5d-60980b028665")
    t.eq(resolved(t)[CORE].get("dependents"), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", "1a620c0a-aa3c-533c-9960-3d49016fa231"]), "dependents")
    core = t.snap(t.worktree(CORE), t.dist(CORE))
    t.gitpm_ok("uninstall", "9692fc07-23c7-5f20-8779-f09374f1a571")
    t.not_installed("9692fc07-23c7-5f20-8779-f09374f1a571")
    t.same(core, "757fd5ca-2b6a-5b2f-8a5d-60980b028665 clone and dist")
    t.eq(resolved(t)[CORE].get("dependents"), ["1a620c0a-aa3c-533c-9960-3d49016fa231"], "dependents after uninstall")


# ------------------------------------------------------------- 8. direct Git install


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies#no-catalog"])
def direct_install_without_catalog_refused(t: T):
    """8a. No catalog configured and 757fd5ca-2b6a-5b2f-8a5d-60980b028665 not installed: refused naming 757fd5ca-2b6a-5b2f-8a5d-60980b028665 and "no catalog configured"."""
    t.gitpm_err("install", t.repo("9692fc07-23c7-5f20-8779-f09374f1a571"), env=NO_CATALOG, kind="dependency_conflict",
             contains=(CORE, "no catalog configured"))
    t.not_installed("9692fc07-23c7-5f20-8779-f09374f1a571")
    t.not_installed(CORE)


@case(fixture="g6_core_catalog", spec=["cli/commands-that-need-a-catalog-say-so"])
def missing_catalog_location_is_refused(t: T):
    """8a'. A configured catalog location that does not exist is no "no catalog": `--catalog ./missing install`
    fails with the library's invalid_argument before anything is created."""
    before = t.snap(t.root, git=True)
    t.gitpm_err("--catalog", "./missing", "install", t.repo("9692fc07-23c7-5f20-8779-f09374f1a571"), kind="invalid_argument",
             contains="catalog location ./missing does not exist")
    t.same(before, "fixture (workspace, catalog, repos)")
    t.ok(not (t.root / "missing").exists(), "no catalog created")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def direct_install_with_dependency_installed(t: T):
    """8b. 757fd5ca-2b6a-5b2f-8a5d-60980b028665 already installed (from Git), no catalog: 9692fc07-23c7-5f20-8779-f09374f1a571 installs and resolves to it."""
    before = t.snap(t.catalog)
    t.gitpm_ok("install", t.repo(CORE), "--ref", f"v1.1.0", env=NO_CATALOG)
    t.gitpm_ok("install", t.repo("9692fc07-23c7-5f20-8779-f09374f1a571"), env=NO_CATALOG)
    row = resolved(t)["9692fc07-23c7-5f20-8779-f09374f1a571"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), (CORE, True), "resolved")
    t.eq(t.package("9692fc07-23c7-5f20-8779-f09374f1a571").get("catalog"), None, "direct install records no catalog")
    t.same(before, "catalog untouched")


@case(fixture="g6_core_catalog", spec=["dependencies/install-resolves-dependencies"])
def direct_install_with_catalog(t: T):
    """8c. With the catalog: 9692fc07-23c7-5f20-8779-f09374f1a571 from Git, 757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0 pulled from the catalog."""
    t.gitpm_ok("install", t.repo("9692fc07-23c7-5f20-8779-f09374f1a571"))
    t.eq(sorted(t.state()["packages"]), sorted(["9692fc07-23c7-5f20-8779-f09374f1a571", CORE]), "installed")
    t.eq(t.package(CORE)["current"]["version"], "1.1.0", "757fd5ca-2b6a-5b2f-8a5d-60980b028665 version")
    row = resolved(t)["9692fc07-23c7-5f20-8779-f09374f1a571"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"]), (CORE, True), "resolved")


# ------------------------------------------------- 9. download, strict enable (D7, TILR-36)


@case(fixture="g6_core_catalog", spec=["install-uninstall/download-without-enabling#download-with-a-missing-dependency",
                                       "install-uninstall/download-without-enabling#enable-after-download",
                                       "dist/enable-and-disable-deploy-and-remove#refused-enable"])
def download_then_enable_in_dependency_order(t: T):
    """9a. download-catalog 9692fc07-23c7-5f20-8779-f09374f1a571 records 9692fc07-23c7-5f20-8779-f09374f1a571 and the pulled 757fd5ca-2b6a-5b2f-8a5d-60980b028665 disabled, deploys nothing and prints one
    line per package; enable 9692fc07-23c7-5f20-8779-f09374f1a571 is refused (757fd5ca-2b6a-5b2f-8a5d-60980b028665 disabled); enable 757fd5ca-2b6a-5b2f-8a5d-60980b028665 then 9692fc07-23c7-5f20-8779-f09374f1a571 works offline."""
    r = t.gitpm_ok("download-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    lines = r.out.splitlines()
    t.eq(len(lines), 2, "one line per package")
    t.ok(lines[0].startswith("downloaded 9692fc07-23c7-5f20-8779-f09374f1a571@1.0.0 from ") and lines[0].endswith(" (disabled)"), lines[0])
    t.ok(lines[1].startswith(f"downloaded {CORE}@1.1.0 from ") and lines[1].endswith(" (disabled)"), lines[1])
    for id in ["9692fc07-23c7-5f20-8779-f09374f1a571", CORE]:
        t.eq(t.package(id)["enabled"], False, f"{id} disabled")
        t.ok((t.worktree(id) / ".git").is_file(), f"{id} clone")
        t.ok(not t.dist(id).exists(), f"no dist/{id}")
    before = t.snap(t.ws)
    r = t.gitpm_err("enable", "9692fc07-23c7-5f20-8779-f09374f1a571", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.has(r.err, "Hint: `gitpm enable 9692fc07-23c7-5f20-8779-f09374f1a571 --force` enables it anyway")
    t.same(before, "workspace")
    t.gitpm_ok("enable", CORE, env=NO_CATALOG)
    t.eq(t.gitpm_ok("enable", "9692fc07-23c7-5f20-8779-f09374f1a571", env=NO_CATALOG).out, "9692fc07-23c7-5f20-8779-f09374f1a571 enabled\n", "enable line")
    t.has((t.dist("9692fc07-23c7-5f20-8779-f09374f1a571") / "index.js").read_text(), "9692fc07-23c7-5f20-8779-f09374f1a571 1.0.0")
    t.has((t.dist(CORE) / "index.js").read_text(), "757fd5ca-2b6a-5b2f-8a5d-60980b028665 1.1.0")


@case(fixture="g6_core_catalog", spec=["dependencies/enable-needs-active-dependencies#forced-enable",
                                       "dependencies/disabled-packages-and-dependencies#dependency-on-a-disabled-package-in-list"])
def forced_enable_leaves_the_consumer_broken(t: T):
    """9b. enable 9692fc07-23c7-5f20-8779-f09374f1a571 --force enables only 9692fc07-23c7-5f20-8779-f09374f1a571; list and check report its dependency on the disabled 757fd5ca-2b6a-5b2f-8a5d-60980b028665."""
    t.gitpm_ok("download-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    t.gitpm_ok("enable", "9692fc07-23c7-5f20-8779-f09374f1a571", "--force")
    t.ok((t.dist("9692fc07-23c7-5f20-8779-f09374f1a571") / "index.js").is_file(), "dist/9692fc07-23c7-5f20-8779-f09374f1a571 deployed")
    t.ok(not t.dist(CORE).exists(), "757fd5ca-2b6a-5b2f-8a5d-60980b028665 stays disabled")
    t.eq(t.package(CORE)["enabled"], False, "757fd5ca-2b6a-5b2f-8a5d-60980b028665 disabled")
    row = resolved(t)["9692fc07-23c7-5f20-8779-f09374f1a571"]["dependencies"][0]
    t.eq((row["provider"], row["satisfied"], row.get("problem")), (CORE, False, f"{CORE} is disabled"), "dependency")
    t.has(t.gitpm_ok("check").out, f"Unsatisfied dependency: 9692fc07-23c7-5f20-8779-f09374f1a571 → {CORE} ^1.0: {CORE} is disabled")


@case(fixture="g6_core_catalog", spec=["dependencies/disabled-packages-and-dependencies#install-next-to-a-disabled-dependency"])
def install_next_to_a_disabled_dependency(t: T):
    """9c. 757fd5ca-2b6a-5b2f-8a5d-60980b028665 installed but disabled: install-catalog 9692fc07-23c7-5f20-8779-f09374f1a571 and a direct install of 9692fc07-23c7-5f20-8779-f09374f1a571 are refused
    with dependency_conflict naming 757fd5ca-2b6a-5b2f-8a5d-60980b028665 as disabled; the workspace is unchanged."""
    t.gitpm_ok("install-catalog", CORE, "--version", "1.1.0")
    t.gitpm_ok("disable", CORE)
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.gitpm_err("install", t.repo("9692fc07-23c7-5f20-8779-f09374f1a571"), kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
    t.ok("9692fc07-23c7-5f20-8779-f09374f1a571" not in t.state()["packages"], "9692fc07-23c7-5f20-8779-f09374f1a571 not recorded")
    # After enabling the dependency the install goes through.
    t.gitpm_ok("enable", CORE)
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    t.ok((t.dist("9692fc07-23c7-5f20-8779-f09374f1a571") / "index.js").is_file(), "dist/9692fc07-23c7-5f20-8779-f09374f1a571 deployed")


@case(fixture="g6_core_catalog", spec=["dist/inactive-packages-change-state-and-clone-only#download-deploys-nothing",
                                       "install-uninstall/download-without-enabling"])
def direct_download_deploys_nothing(t: T):
    """9d. download GIT_SOURCE records the package disabled with its clone, prints `(disabled)` and creates no dist."""
    r = t.gitpm_ok("download", t.repo("a768d335-0cf7-597a-b1ba-0eb542495aec"))
    t.ok(r.out.startswith("downloaded a768d335-0cf7-597a-b1ba-0eb542495aec@1.0.0 from ") and r.out.endswith(" (disabled)\n"), r.out)
    t.eq(t.package("a768d335-0cf7-597a-b1ba-0eb542495aec")["enabled"], False, "disabled")
    t.ok((t.worktree("a768d335-0cf7-597a-b1ba-0eb542495aec") / ".git").is_file(), "clone")
    t.ok(not t.dist("a768d335-0cf7-597a-b1ba-0eb542495aec").exists(), "no dist")


@case(fixture="g6_core_catalog", spec=["install-uninstall/install-is-idempotent-for-the-same-source#install-over-a-download"])
def install_over_a_download_enables_it(t: T):
    """9e. install-catalog over a download is refused while 757fd5ca-2b6a-5b2f-8a5d-60980b028665 is disabled, `--force` enables only 9692fc07-23c7-5f20-8779-f09374f1a571
    (an install line, not "already installed"); a second install is "already installed"."""
    t.gitpm_ok("download-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    before = t.snap(t.ws)
    t.gitpm_err("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
    r = t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571", "--force")
    t.ok(r.out.startswith("installed 9692fc07-23c7-5f20-8779-f09374f1a571@1.0.0 from "), r.out)
    t.eq((t.package("9692fc07-23c7-5f20-8779-f09374f1a571")["enabled"], t.package(CORE)["enabled"]), (True, False), "only 9692fc07-23c7-5f20-8779-f09374f1a571 enabled")
    t.ok((t.dist("9692fc07-23c7-5f20-8779-f09374f1a571") / "index.js").is_file(), "dist/9692fc07-23c7-5f20-8779-f09374f1a571 deployed")
    t.has(t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571").out, "9692fc07-23c7-5f20-8779-f09374f1a571 already installed (1.0.0)")


@case(fixture="g6_core_catalog", spec=["install-uninstall/install-the-latest-release-of-a-git-source#forced-install-next-to-a-disabled-dependency"])
def forced_install_next_to_a_disabled_dependency(t: T):
    """9f. install --force of 9692fc07-23c7-5f20-8779-f09374f1a571 and install-catalog --force of 1a620c0a-aa3c-533c-9960-3d49016fa231 next to the disabled 757fd5ca-2b6a-5b2f-8a5d-60980b028665 enable
    and deploy them only."""
    t.gitpm_ok("install-catalog", CORE, "--version", "1.1.0")
    t.gitpm_ok("disable", CORE)
    t.gitpm_ok("install", t.repo("9692fc07-23c7-5f20-8779-f09374f1a571"), "--force")
    t.ok((t.dist("9692fc07-23c7-5f20-8779-f09374f1a571") / "index.js").is_file(), "dist/9692fc07-23c7-5f20-8779-f09374f1a571 deployed")
    t.ok(not t.dist(CORE).exists(), "757fd5ca-2b6a-5b2f-8a5d-60980b028665 stays disabled")
    t.gitpm_err("install-catalog", "1a620c0a-aa3c-533c-9960-3d49016fa231", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.gitpm_ok("install-catalog", "1a620c0a-aa3c-533c-9960-3d49016fa231", "--force")
    t.ok((t.dist("1a620c0a-aa3c-533c-9960-3d49016fa231") / "index.js").is_file(), "dist/1a620c0a-aa3c-533c-9960-3d49016fa231 deployed")


@case(fixture="g6_core_catalog", spec=["dependencies/disabled-packages-and-dependencies#rollback-and-restore-next-to-a-disabled-dependency"])
def rollback_and_restore_refuse_a_disabled_dependency(t: T):
    """9g. rollback of an active 9692fc07-23c7-5f20-8779-f09374f1a571 and restore of its enabled record refuse the disabled 757fd5ca-2b6a-5b2f-8a5d-60980b028665."""
    t.gitpm_ok("install-catalog", "9692fc07-23c7-5f20-8779-f09374f1a571")
    t.release("9692fc07-23c7-5f20-8779-f09374f1a571", "1.1.0", deps={CORE: "^1.0"})
    t.gitpm_ok("update", "9692fc07-23c7-5f20-8779-f09374f1a571")
    t.gitpm_ok("disable", CORE, "--force")
    before = t.snap(t.ws)
    t.gitpm_err("rollback", "9692fc07-23c7-5f20-8779-f09374f1a571", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
    t.gitpm_ok("forget", "9692fc07-23c7-5f20-8779-f09374f1a571")
    before = t.snap(t.ws)
    t.gitpm_err("restore", "9692fc07-23c7-5f20-8779-f09374f1a571", kind="dependency_conflict", contains=f"{CORE} is disabled; enable it")
    t.same(before, "workspace")
