"""UC-026: Unscoped IDs and legacy per-package tags are refused (P0).

There is no backward compatibility before 1.0: a package ID is always namespace/name and
a release tag is always vVERSION. g3_legacy is an unscoped repository `demo` with tags
v0.1.0/v0.1.1; scoped be517c52-cf84-5615-9a18-9fae7d82c819 and an unscoped `author` live outside /fx/repos/<id>.
"""

from gitpmtest import T, case

LEGACY = "demo"


def package_repo(t: T, path, id: str, version: str = "0.1.0", tag: str | None = None):
    path.joinpath("dist").mkdir(parents=True)
    t.git(path, "init", "-q", "-b", "main")
    t.release(id, version, repo=path, tag=False)
    t.tag(path, tag or f"v{version}")
    return path


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax#scoped-and-bare-ids-refused", "install-uninstall/refused-input-creates-nothing"])
def unscoped_id_is_refused(t: T):
    """install of the unscoped `demo` fails with invalid_id and creates nothing."""
    t.gitpm_err("install", t.repo(LEGACY), kind="invalid_id", contains="UUID")
    t.ok(not t.ws.exists() or not t.state().get("packages"), "nothing installed")
    t.ok(not t.worktree(LEGACY).exists() and not t.dist(LEGACY).exists(), "no clone or dist")


@case(fixture="g3_legacy", spec=["package-ids/release-tag-names#vversion-tag-of-a-scoped-single-package-repository"])
def legacy_tag_of_a_scoped_package_is_no_release(t: T):
    """2225113b-cf36-5f79-98f2-8bca9e9ae3c7 with only 2225113b-cf36-5f79-98f2-8bca9e9ae3c7@1.0.0 on a 1.0.0 commit: install fails with no_releases."""
    repo = package_repo(t, t.root / "authors/vonly", "2225113b-cf36-5f79-98f2-8bca9e9ae3c7", "1.0.0", tag="2225113b-cf36-5f79-98f2-8bca9e9ae3c7@1.0.0")
    t.gitpm_err("install", repo, kind="no_releases", contains="vVERSION")
    t.not_installed("2225113b-cf36-5f79-98f2-8bca9e9ae3c7")


@case(fixture="g3_legacy", spec=["package-ids/release-tag-names#two-tags-for-one-version"])
def legacy_tag_next_to_a_release_is_ignored(t: T):
    """08c20d96-b858-51f8-923b-f8a0c86cc2e3@1.0.0 on another commit next to v1.0.0: only the repository tag counts, no warning."""
    repo = package_repo(t, t.root / "authors/both", "08c20d96-b858-51f8-923b-f8a0c86cc2e3", "1.0.0")
    t.commit(repo, "same version, other commit", {"dist/index.js": "other\n"})
    t.tag(repo, "08c20d96-b858-51f8-923b-f8a0c86cc2e3@1.0.0")
    r = t.gitpm_ok("install", repo)
    t.eq(t.package("08c20d96-b858-51f8-923b-f8a0c86cc2e3")["current"]["commit"], t.rev(repo, "v1.0.0"), "release commit")
    t.lacks(r.err, "08c20d96-b858-51f8-923b-f8a0c86cc2e3@1.0.0")


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax#scoped-and-bare-ids-refused"])
def unscoped_id_equal_to_an_installed_namespace(t: T):
    """be517c52-cf84-5615-9a18-9fae7d82c819 installed, then the unscoped `author`: invalid_id, be517c52-cf84-5615-9a18-9fae7d82c819 unchanged."""
    legacy = package_repo(t, t.root / "authors/legacy-author", "author", tag="v0.1.0")
    scoped = package_repo(t, t.root / "authors/author-demo", "be517c52-cf84-5615-9a18-9fae7d82c819")
    t.gitpm_ok("install", scoped)
    before = t.snap(t.ws, git=True)
    t.gitpm_err("install", legacy, kind="invalid_id")
    t.same(before, "workspace")
    t.eq(sorted(t.state()["packages"]), sorted(["be517c52-cf84-5615-9a18-9fae7d82c819"]), "only be517c52-cf84-5615-9a18-9fae7d82c819")


@case(fixture="g3_legacy", spec=["package-ids/package-id-syntax", "catalog/catalog-is-a-tree-of-static-files"])
def unscoped_id_is_never_registered(t: T):
    """register of the unscoped `demo` fails; no legacy/ folder appears in the catalog."""
    scoped = package_repo(t, t.root / "authors/author-demo", "be517c52-cf84-5615-9a18-9fae7d82c819")
    t.gitpm_err("register", t.repo(LEGACY), "--ref", "v0.1.0", kind="invalid_id")
    t.gitpm_ok("register", scoped, "--ref", "v0.1.0")
    t.ok(t.catalog_index("be517c52-cf84-5615-9a18-9fae7d82c819").is_file(), "scoped index")
    t.ok(not (t.catalog / "legacy").exists(), "no legacy folder")
