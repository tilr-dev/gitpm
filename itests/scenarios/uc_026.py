"""UC-026: Legacy compatibility and path collisions (P0).

Legacy unscoped plugin `demo` with tags v0.1.0/v0.1.1 (g3_legacy, g3_legacy_installed).
Scoped author/demo and a legacy plugin with ID `author` live outside /fx/repos/<id> so the
legacy repository never contains the scoped one.
"""

import json

from gitpmtest import T, case

LEGACY = "demo"


def plugin_repo(t: T, path, id: str, version: str = "0.1.0", tag: str | None = None):
    path.joinpath("dist").mkdir(parents=True)
    t.git(path, "init", "-q", "-b", "main")
    t.release(id, version, repo=path, tag=False)
    t.tag(path, tag or (f"{id}@{version}" if "/" in id else f"v{version}"))
    return path


@case(fixture="g3_legacy", spec=["package-ids/release-tag-names#legacy-tag", "install-uninstall/install-the-latest-release-of-a-git-source"])
def install_latest_legacy_tag(t: T):
    """Install without --ref takes v0.1.1; the ID stays `demo` (no automatic renaming)."""
    t.gitpm_ok("install", t.repo(LEGACY))
    p = t.plugin(LEGACY)
    t.eq(p["current"], {"version": "0.1.1", "commit": t.rev(LEGACY, "v0.1.1")}, "current")
    t.eq(sorted(t.state()["plugins"]), [LEGACY], "state keys")
    t.has((t.dist(LEGACY) / "index.js").read_text(), "demo 0.1.1")


@case(fixture="g3_legacy_installed", spec=["package-ids/release-tag-names#legacy-tag", "updates/stable-update-moves-to-the-newest-release"])
def update_legacy_by_v_tag(t: T):
    """Installed at v0.1.0: update picks v0.1.1 (legacy v-tags); the ID is never renamed."""
    t.gitpm_ok("update", LEGACY)
    p = t.plugin(LEGACY)
    t.eq(p["current"], {"version": "0.1.1", "commit": t.rev(LEGACY, "v0.1.1")}, "current")
    t.eq(p["previous"]["version"], "0.1.0", "previous")
    t.eq(sorted(t.state()["plugins"]), [LEGACY], "state keys")
    t.gitpm_ok("update", LEGACY, "--ref", "v0.1.0")
    t.eq(t.plugin(LEGACY)["current"]["version"], "0.1.0", "explicit v-tag ref")


@case(fixture="g3_legacy", spec=["catalog/release-record#legacy-release-without-hashes", "install-uninstall/install-from-the-catalog"])
def register_and_install_legacy(t: T):
    """register of a legacy v-tag goes under legacy/; install-catalog works by the legacy ID."""
    t.gitpm_ok("register", t.repo(LEGACY), "--ref", "v0.1.1")
    release = t.catalog / "legacy/demo/versions/0.1.1/release.json"
    t.ok(release.is_file(), "legacy release file")
    data = json.loads(release.read_text())
    t.eq((data["manifest"]["id"], data["commit"]), (LEGACY, t.rev(LEGACY, "v0.1.1")), "release")
    t.gitpm_ok("install-catalog", LEGACY)
    t.eq(t.plugin(LEGACY)["current"]["version"], "0.1.1", "installed from catalog")


@case(fixture="g3_legacy_installed", spec=["package-ids/package-id-syntax", "workspace-layout/workspace-folders"])
def scoped_and_legacy_coexist(t: T):
    """author/demo installs next to legacy `demo`; separate clones/dist; legacy untouched."""
    repo = plugin_repo(t, t.root / "authors/author-demo", "author/demo")
    legacy = t.snap(t.clone(LEGACY), t.dist(LEGACY), git=True)
    entry = t.plugin(LEGACY)
    t.gitpm_ok("install", repo)
    t.eq(sorted(t.state()["plugins"]), ["author/demo", LEGACY], "both installed")
    t.has((t.dist("author/demo") / "index.js").read_text(), "author/demo 0.1.0")
    t.eq(t.plugin(LEGACY), entry, "legacy state")
    t.same(legacy, "legacy clone and dist")
    t.gitpm_ok("update", LEGACY)
    t.eq(t.plugin("author/demo")["current"]["version"], "0.1.0", "scoped unaffected by legacy update")


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax", "install-uninstall/refused-input-creates-nothing"])
def legacy_namespace_named_id_then_scoped(t: T):
    """Legacy `author` installed, then author/demo: refused, legacy `author` unchanged."""
    legacy = plugin_repo(t, t.root / "authors/legacy-author", "author")
    scoped = plugin_repo(t, t.root / "authors/author-demo", "author/demo")
    t.gitpm_ok("install", legacy)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install", scoped, kind="workspace", contains="namespace path")
    t.same(before, "workspace")
    t.eq(sorted(t.state()["plugins"]), ["author"], "only legacy author")


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax", "install-uninstall/refused-input-creates-nothing"])
def scoped_then_legacy_namespace_named_id(t: T):
    """Reverse order: author/demo installed, then legacy `author`: refused, author/demo unchanged."""
    legacy = plugin_repo(t, t.root / "authors/legacy-author", "author")
    scoped = plugin_repo(t, t.root / "authors/author-demo", "author/demo")
    t.gitpm_ok("install", scoped)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install", legacy, kind="workspace", contains="namespace path")
    t.same(before, "workspace")
    t.eq(sorted(t.state()["plugins"]), ["author/demo"], "only author/demo")


@case(fixture="g3_legacy", spec=["catalog/catalog-v2-is-a-tree-of-static-files"])
def legacy_and_scoped_catalog_entries_coexist(t: T):
    """register of legacy `demo` and scoped author/demo: distinct catalog entries."""
    scoped = plugin_repo(t, t.root / "authors/author-demo", "author/demo")
    t.gitpm_ok("register", t.repo(LEGACY), "--ref", "v0.1.0")
    t.gitpm_ok("register", scoped, "--ref", "author/demo@0.1.0")
    t.ok(t.catalog_index("author/demo").is_file(), "scoped index")
    t.ok((t.catalog / "legacy/demo/plugin.json").is_file(), "legacy index")
    t.has(t.gitpm_ok("--catalog", t.catalog, "info").out, "author/demo", "demo")
