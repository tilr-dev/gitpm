"""UC-026: Unscoped IDs and vVERSION tags are refused (P0).

There is no backward compatibility before 1.0: a package ID is always namespace/name and
a release tag is always ID@VERSION. g3_legacy is an unscoped repository `demo` with tags
v0.1.0/v0.1.1; scoped author/demo and an unscoped `author` live outside /fx/repos/<id>.
"""

from gitpmtest import T, case

LEGACY = "demo"


def package_repo(t: T, path, id: str, version: str = "0.1.0", tag: str | None = None):
    path.joinpath("dist").mkdir(parents=True)
    t.git(path, "init", "-q", "-b", "main")
    t.release(id, version, repo=path, tag=False)
    t.tag(path, tag or f"{id}@{version}")
    return path


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax#unsafe-components-are-refused", "install-uninstall/refused-input-creates-nothing"])
def unscoped_id_is_refused(t: T):
    """install of the unscoped `demo` fails with invalid_id and creates nothing."""
    t.gitpm_err("install", t.repo(LEGACY), kind="invalid_id", contains="namespace/name")
    t.ok(not t.ws.exists() or not t.state().get("packages"), "nothing installed")
    t.ok(not t.clone(LEGACY).exists() and not t.dist(LEGACY).exists(), "no clone or dist")


@case(fixture="g3_legacy", spec=["package-ids/release-tag-names#vversion-tag-of-a-scoped-single-package-repository"])
def v_tag_of_a_scoped_package_is_no_release(t: T):
    """dan/vonly with only v1.0.0 on a 1.0.0 commit: install fails with no_releases."""
    repo = package_repo(t, t.root / "authors/vonly", "dan/vonly", "1.0.0", tag="v1.0.0")
    t.gitpm_err("install", repo, kind="no_releases", contains="dan/vonly@VERSION")
    t.not_installed("dan/vonly")


@case(fixture="g3_legacy", spec=["package-ids/release-tag-names#two-tags-for-one-version"])
def v_tag_next_to_a_release_is_ignored(t: T):
    """v1.0.0 on another commit next to dan/both@1.0.0: only the ID@VERSION tag counts, no warning."""
    repo = package_repo(t, t.root / "authors/both", "dan/both", "1.0.0")
    t.commit(repo, "same version, other commit", {"dist/index.js": "other\n"})
    t.tag(repo, "v1.0.0")
    r = t.gitpm_ok("install", repo)
    t.eq(t.package("dan/both")["current"]["commit"], t.rev(repo, "dan/both@1.0.0"), "release commit")
    t.lacks(r.err, "v1.0.0")


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax#legacy-id-equal-to-an-installed-namespace"])
def unscoped_id_equal_to_an_installed_namespace(t: T):
    """author/demo installed, then the unscoped `author`: invalid_id, author/demo unchanged."""
    legacy = package_repo(t, t.root / "authors/legacy-author", "author", tag="author@0.1.0")
    scoped = package_repo(t, t.root / "authors/author-demo", "author/demo")
    t.gitpm_ok("install", scoped)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install", legacy, kind="invalid_id")
    t.same(before, "workspace")
    t.eq(sorted(t.state()["packages"]), ["author/demo"], "only author/demo")


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax", "catalog/catalog-is-a-tree-of-static-files"])
def unscoped_id_is_never_registered(t: T):
    """register of the unscoped `demo` fails; no legacy/ folder appears in the catalog."""
    scoped = package_repo(t, t.root / "authors/author-demo", "author/demo")
    t.gitpm_err("register", t.repo(LEGACY), "--ref", "v0.1.0", kind="invalid_id")
    t.gitpm_ok("register", scoped, "--ref", "author/demo@0.1.0")
    t.ok(t.catalog_index("author/demo").is_file(), "scoped index")
    t.ok(not (t.catalog / "legacy").exists(), "no legacy folder")
