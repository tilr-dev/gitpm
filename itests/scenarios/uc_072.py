"""UC-072: Multi-package repositories (P1, gitpm 0.4+).

/fx/repos/suite is made with `gitpm init 76a9b591-60f3-5c7f-9e1c-31648fc73bfd --into suite --multi` plus `init`
of f851f609-bb81-52d6-bfef-33798624782c and 955bf29f-2893-56ef-9439-369c50455ae7 into it (fixture fork_suite: gitpm.json `packages/*`, one
shared release 0.1.1). fork_suite_installed adds 76a9b591-60f3-5c7f-9e1c-31648fc73bfd and f851f609-bb81-52d6-bfef-33798624782c installed
with `install SRC ID`, sharing one clone `ws/repos/suite-<hash>`.
"""

import json
import re
from uuid import UUID

from gitpmtest import T, case

SUITE = "suite"
STORE, THEME, PANEL = "76a9b591-60f3-5c7f-9e1c-31648fc73bfd", "f851f609-bb81-52d6-bfef-33798624782c", "955bf29f-2893-56ef-9439-369c50455ae7"


def suite(t: T):
    return t.repo(SUITE)


def shared_clone(t: T):
    return t.ws / "repos" / t.package(STORE)["clone"]


@case(fixture=None, spec=["authoring/init-options#start-a-multi-package-repository-from-nothing",
                          "authoring/init-into-a-multi-package-repository"])
def init_multi_then_add_packages(t: T):
    """Generated folders use UUIDs; added packages inherit the repository version."""
    repo = t.root / "authors/suite"
    result = t.gitpm_ok("init", "store", "--into", repo, "--multi")
    store = next((repo / "packages").glob("*/plugin.json"))
    store_id = json.loads(store.read_text())["id"]
    t.eq(str(UUID(store_id)), store_id, "generated UUID")
    t.eq(store.parent.name, store_id, "UUID package folder")
    t.has(result.out, f"ID: {store_id}", "Git commit and tag v0.1.0")
    t.gitpm_ok("commit", repo, "0.2.0")
    t.gitpm_ok("init", "theme", "--into", repo)
    theme = next(path for path in (repo / "packages").glob("*/plugin.json") if path != store)
    theme_id = json.loads(theme.read_text())["id"]
    t.eq(theme.parent.name, theme_id, "second UUID folder")
    t.eq(json.loads(theme.read_text())["version"], "0.2.0", "inherited version")
    t.eq(t.git(repo, "tag", "--list").split(), ["v0.1.0", "v0.2.0"], "addition creates no tag")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean worktree")
    t.gitpm_ok("commit", repo, "0.2.1")
    for path in (store, theme):
        t.eq(json.loads(path.read_text())["version"], "0.2.1", "shared version")


@case(fixture="fork_suite", spec=["multi-package-repos/choosing-a-package-at-install#missing-package-id"])
def install_without_package_id_lists_packages(t: T):
    """`install SRC` of a repository with several packages: refused with the list; nothing created."""
    r = t.gitpm_err("install", suite(t), kind="invalid_argument",
                    contains=("repository has 3 packages at commit", "choose one by ID:",
                              f"{PANEL} (packages/panel)", f"{STORE} (packages/store)", f"{THEME} (packages/theme)"))
    t.lacks(r.out, "installed")
    t.ok(not (t.ws / "state.json").exists() or not t.state().get("packages"), "nothing installed")
    t.ok(not (t.ws / "repos").exists() or not any((t.ws / "repos").iterdir()), "no clone left")


@case(fixture="fork_suite", spec=["multi-package-repos/one-shared-clone-per-source",
                                  "multi-package-repos/choosing-a-package-at-install"])
def two_packages_share_one_clone(t: T):
    """`install SRC ID` twice: one clone repos/suite-<hash>, state records clone and path, only
    <path>/dist is deployed for each, and packages/ is absent."""
    for id in (STORE, THEME):
        r = t.gitpm_ok("install", suite(t), id)
        t.has(r.out, f"installed {id}@0.1.1 from {suite(t)}")
    clones = sorted(p.name for p in (t.ws / "repos").iterdir())
    t.eq(len(clones), 1, "one shared clone")
    t.ok(re.fullmatch(r"suite-[0-9a-f]{8,}", clones[0]) is not None, f"clone name {clones[0]} is <name>-<hash>")
    for id, folder in ((STORE, "packages/store"), (THEME, "packages/theme")):
        p = t.package(id)
        t.eq((p["clone"], p["path"]), (clones[0], folder), f"{id} clone and path")
        t.eq(t.clone(id), t.ws / "repos" / clones[0], f"{id} resolves to the shared clone")
        t.ok((t.worktree(id) / ".git").is_file(), f"{id} clone exists")
        t.eq(sorted(f.name for f in t.dist(id).iterdir()), ["index.js"], f"{id}: only its dist/ deployed")
        t.has((t.dist(id) / "index.js").read_text(), f"{folder.split('/')[-1]} 0.1.0")
    t.ok(not (t.ws / "packages").exists(), "no packages directory")
    t.ok(not t.dist(PANEL).exists(), "panel not installed")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/repository-releases"])
def repository_release_and_individual_updates(t: T):
    """Publishing a version bumps all manifests; installed packages can update separately."""
    t.write(suite(t), {"packages/store/dist/index.js": "export const greeting = 'store 0.2.0';\n"})
    r = t.gitpm_ok("commit", suite(t), "0.2.0")
    t.has(r.out, "committed 0.2.0", "tag v0.2.0")
    t.eq(t.git(suite(t), "tag", "--list").split(), ["v0.1.1", "v0.2.0"], "repository tags")
    for name in ("store", "theme", "panel"):
        t.eq(json.loads((suite(t) / "packages" / name / "plugin.json").read_text())["version"], "0.2.0", name)
    theme_before = t.snap(t.dist(THEME))
    theme_worktree_before = t.snap(t.worktree(THEME), git=True)
    t.gitpm_ok("update", STORE)
    t.eq(t.package(THEME)["current"]["version"], "0.1.1", "theme not updated yet")
    t.same(theme_worktree_before, "theme worktree unchanged by store update")
    t.gitpm_ok("update", THEME)
    t.eq(t.package(STORE)["current"]["version"], "0.2.0", "store updated")
    t.eq(t.package(THEME)["current"]["version"], "0.2.0", "theme updated")
    t.same(theme_before, "theme payload unchanged")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/one-shared-clone-per-source#uninstall-one-of-two",
                                            "multi-package-repos/shared-clone-head-handling"])
def uninstall_one_of_two_keeps_shared_clone(t: T):
    """Uninstalling 76a9b591-60f3-5c7f-9e1c-31648fc73bfd keeps the shared clone; f851f609-bb81-52d6-bfef-33798624782c keeps working; uninstalling the
    last package removes the clone."""
    clone = shared_clone(t)
    t.gitpm_ok("uninstall", STORE)
    t.ok(STORE not in t.state()["packages"] and not t.dist(STORE).exists(), "store removed")
    t.ok(clone.is_dir(), "shared clone kept")
    t.has((t.dist(THEME) / "index.js").read_text(), "theme 0.1.0")
    t.eq(t.rev(t.worktree(THEME)), t.package(THEME)["current"]["commit"], "theme worktree HEAD at installed revision")
    t.write(suite(t), {"packages/theme/dist/index.js": "export const greeting = 'theme 0.1.2';\n"})
    t.gitpm_ok("commit", suite(t), "0.1.2")
    t.gitpm_ok("update", THEME)
    t.eq(t.package(THEME)["current"]["version"], "0.1.2", "theme still updatable")
    t.gitpm_ok("uninstall", THEME)
    t.ok(not clone.exists(), "clone removed with its last package")


@case(fixture="fork_suite", spec=["catalog/register-one-version", "catalog/catalog-generated-from-git-release-tags",
                                  "multi-package-repos/repository-releases"])
def register_package_and_catalog_update(t: T):
    """`register SRC --ref TAG --package ID` adds one package's release; `catalog-update` from a
    sources list naming the repository publishes every package; install-catalog uses the shared clone."""
    single = t.root / "single-catalog"
    r = t.gitpm_ok("--catalog", single, "register", suite(t), "--ref", f"v0.1.1", "--package", THEME)
    t.eq(r.json()["manifest"]["id"], THEME, "registered manifest")
    t.ok(t.catalog_release(THEME, "0.1.1", single).is_file(), "theme release registered")
    t.ok(not t.catalog_index(STORE, single).exists(), "only the named package registered")
    (t.root / "sources.txt").write_text(f"{suite(t)}\n")
    t.gitpm_ok("catalog-update")
    for id in (STORE, THEME, PANEL):
        index = json.loads(t.catalog_index(id).read_text())
        t.eq(sorted(index["versions"]), ["0.1.1"], f"{id} published")
        release = json.loads(t.catalog_release(id, "0.1.1").read_text())
        t.eq(release["manifest"]["id"], id, f"{id} release manifest")
    t.gitpm_ok("install-catalog", STORE)
    t.gitpm_ok("install-catalog", THEME)
    t.eq(t.package(STORE)["clone"], t.package(THEME)["clone"], "catalog installs share one clone")
    t.eq((t.package(STORE)["path"], t.package(THEME)["path"]), ("packages/store", "packages/theme"), "paths")


@case(fixture="fork_suite_installed", spec=["multi-package-repos/forking-packages-of-one-repository",
                                            "fork-replacement/fork-metadata#fork-manifest",
                                            "fork-replacement/fork-isolates-source-and-destination-state#repository-starting-revision"])
def fork_one_package_of_the_repository(t: T):
    """Fork copies every package at the newest installed repository revision."""
    before = t.snap(t.ws, suite(t), git=True)
    dest = t.root / "2e255f64-6ad3-5e53-bf66-b4e1a94ab291"
    r = t.gitpm_ok("fork", THEME, "--into", dest)
    fork_theme = json.loads((dest / "packages/theme/plugin.json").read_text())["id"]
    t.has(r.out, f"{fork_theme} forked from {THEME} (packages/theme)",
          "Next steps (choose the packages to install and replace):", "NEW_VERSION", "release all packages", "vNEW_VERSION", f"gitpm replace {THEME} --with {fork_theme}")
    t.eq(json.loads((dest / "gitpm.json").read_text()), {"packages": ["packages/*"]}, "fork gitpm.json")
    t.eq(sorted(p.name for p in (dest / "packages").iterdir()), ["panel", "store", "theme"], "package folders at newest installed revision")
    t.eq(t.rev(dest, "HEAD~1"), t.package(THEME)["current"]["commit"], "newest installed revision")
    for name in ("store", "theme", "panel"):
        sibling = json.loads((dest / "packages" / name / "plugin.json").read_text())
        t.eq(str(UUID(sibling["id"])), sibling["id"], f"{name} canonical UUID")
        t.ok(sibling["id"] != {"store": STORE, "theme": THEME, "panel": PANEL}[name], "fresh fork UUID")
        current = t.package(THEME)["current"]
        t.eq(sibling["forkOf"], {"id": {"store": STORE, "theme": THEME, "panel": PANEL}[name], **current,
                               "source": t.package(THEME)["source"]}, f"{name} provenance")
    m = json.loads((dest / "packages/theme/plugin.json").read_text())
    theme = t.package(THEME)["current"]
    t.eq((m["id"], m["forkOf"], m["replaces"]),
         (fork_theme, {"id": THEME, "version": "0.1.1", "commit": theme["commit"],
                       "source": t.package(THEME)["source"]}, {"id": THEME, "version": "0.1.1"}),
         "fork manifest")
    t.same(before, "workspace")
    t.gitpm_ok("commit", dest, "0.1.2")
    t.gitpm_ok("install", dest, fork_theme)
    t.eq(t.package(fork_theme)["forkOf"]["id"], THEME, "installed fork provenance")
    t.has((t.dist(fork_theme) / "index.js").read_text(), "theme")


@case(fixture="fork_suite_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state#repository-starting-revision",
                                            "fork-replacement/fork-metadata"])
def fork_uses_all_installed_revisions_and_starting_revision_siblings(t: T):
    """An unrequested installed sibling selects the base; older installed folders are restored."""
    source = suite(t)
    store = t.package(STORE)["current"]
    store_dist = t.git(source, "show", f"{store['commit']}:packages/store/dist/index.js")
    manifests = {}
    for name in ("store", "theme", "panel"):
        path = f"packages/{name}/plugin.json"
        manifest = json.loads((source / path).read_text())
        manifest["version"] = "0.1.2"
        manifests[path] = json.dumps(manifest) + "\n"
    base = t.commit(source, "Release repository with an uninstalled sibling", {
        **manifests,
        "packages/store/dist/index.js": "newer store work\n",
        "packages/panel/dist/index.js": "panel at installed base\n",
        "base-marker.txt": "installed base\n",
    })
    t.tag(source, "v0.1.2")
    t.gitpm_ok("update", THEME)
    t.eq(t.package(THEME)["current"]["commit"], base, "newest installed sibling")
    t.commit(source, "Later source-only work", {
        "packages/panel/dist/index.js": "later panel work\n",
        "base-marker.txt": "later source head\n",
    })
    before = t.snap(t.ws, source, git=True)
    dest = t.root / "8a6a26ac-21b8-5d01-a049-a07deded6006"
    t.gitpm_ok("fork", STORE, "--into", dest)
    t.eq(t.rev(dest, "HEAD~1"), base, "base includes unrequested installed theme")
    t.eq((dest / "base-marker.txt").read_text(), "installed base\n", "root at starting revision")
    t.eq((dest / "packages/store/dist/index.js").read_text().strip(), store_dist, "older installed store restored")
    t.eq((dest / "packages/panel/dist/index.js").read_text(), "panel at installed base\n", "uninstalled panel at starting revision")
    for name, version, commit in (("store", "0.1.1", store["commit"]),
                                  ("theme", "0.1.2", base), ("panel", "0.1.2", base)):
        manifest = json.loads((dest / "packages" / name / "plugin.json").read_text())
        t.eq(str(UUID(manifest["id"])), manifest["id"], f"{name} canonical UUID")
        t.ok(manifest["id"] != {"store": STORE, "theme": THEME, "panel": PANEL}[name], "fresh fork UUID")
        t.eq(manifest["forkOf"], {"id": {"store": STORE, "theme": THEME, "panel": PANEL}[name], "version": version,
                                  "commit": commit, "source": str(source)}, f"{name} provenance")
    t.same(before, "installation and source")


@case(fixture="fork_suite_installed", spec=["fork-replacement/fork-isolates-source-and-destination-state#existing-destination"])
def existing_multi_package_fork_is_refused_unchanged(t: T):
    dest = t.root / "a7b307d0-1814-5be7-945d-9e3ca38b344d"
    t.gitpm_ok("fork", THEME, "--into", dest)
    before = t.snap(t.ws, suite(t), dest, git=True)
    t.gitpm_err("fork", STORE, "--into", dest,
                kind="invalid_argument", contains="destination already exists; pass a new folder")
    t.same(before, "existing multi-package fork, installation and source")
