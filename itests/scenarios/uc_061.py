"""UC-061: Identifier, tag and version edge cases (P0).

Owner decision 2026-10-16: IDs/versions given on the command line are validated before
the workspace is opened (nothing may be created); a refused direct Git install may leave
an empty workspace skeleton (state.json without packages, empty packages/ and dist/, lock)
— recorded as observed, not a failure — but never a clone, dist, state entry or .pm-txn.

Subcase 3 (letter case) targets case-insensitive APFS; gitpm refuses case variants of
installed and retained (forgotten) IDs by name, so both are checked on the container's
case-sensitive file system.
"""

import json
import time

from gitpmtest import Failure, T, case

# Loop-generated cases below share one requirement.
SPEC = ["package-ids/package-id-syntax#scoped-and-bare-ids-refused"]


# ---------------------------------------------------------------- helpers (local)

def plain_repo(t: T, dirname: str, plugin_id: str, version: str = "0.1.0", extra: dict | None = None):
    """An author repo at repos/DIRNAME whose manifest has an arbitrary (maybe invalid) ID; untagged."""
    repo = t.root / "repos" / dirname
    repo.mkdir(parents=True)
    t.git(repo, "init", "-q", "-b", "main")
    return repo, t.release(plugin_id, version, extra=extra, tag=False, repo=repo)


def nothing_installed(t: T):
    """No package clone, dist, state entry or .pm-txn; an empty skeleton is only recorded."""
    plugins = t.state().get("packages", {})
    t.eq(plugins, {}, "state entries")
    for sub in ("packages", "dist"):
        d = t.ws / sub
        files = [str(p) for p in d.rglob("*")] if d.exists() else []
        t.eq(files, [], f"contents of {sub}/")
    leftovers = [str(p) for p in t.ws.rglob("*pm-txn*")] if t.ws.exists() else []
    t.eq(leftovers, [], ".pm-txn leftovers")
    if t.ws.exists():
        print(f"  observed: empty workspace skeleton {sorted(p.name for p in t.ws.iterdir())} (accepted)")


def cli_refused_creates_nothing(t: T, argv, kind: str | None = "invalid_id", contains=()):
    before = t.snap(t.root, git=True)
    ws_existed = t.ws.exists()
    r = t.gitpm(*argv)
    t.ok(r.code != 0, f"gitpm {' '.join(map(str, argv))} refused (exit {r.code})")
    if kind:
        t.has(r.err, f"Error kind: {kind}")
    t.has(r.err, *([contains] if isinstance(contains, str) else contains))
    t.same(before, "fixture")
    if not ws_existed:
        t.ok(not t.ws.exists(), "no workspace created")
    return r


def sources(t: T, *repos) -> None:
    (t.root / "sources.txt").write_text("".join(f"{r}\n" for r in repos))


def timed(t: T, *argv, limit=60):
    start = time.time()
    r = t.gitpm_ok(*argv, timeout=limit + 30)
    elapsed = time.time() - start
    t.ok(elapsed < limit, f"gitpm {' '.join(map(str, argv))} took {elapsed:.1f}s (< {limit}s)")
    return r


# ---------------------------------------------------------------- 1. length

N80, N81 = "n" * 80, "n" * 81


@case(fixture="catalog", spec=["package-ids/package-id-syntax#uuid-accepted"])
def id_components_of_80_accepted(t: T):
    """1. namespace and name of exactly 80 characters: init, register, install-catalog, direct install."""
    name = "p" * 80
    t.gitpm_ok("init", name, "--into", t.root / "repos/long" / ("p" * 80))
    repo = t.root / "repos/long" / ("p" * 80)
    pid = t.manifest_id(repo)
    t.eq(json.loads((repo / "plugin.json").read_text())["name"], name, "alias")
    t.gitpm_ok("register", repo, "--ref", f"v0.1.0")
    t.gitpm_ok("install-catalog", pid)
    t.eq(t.package(pid)["current"]["version"], "0.1.0", "installed from catalog")
    t.ok((t.dist(pid) / "index.js").is_file(), "dist")
    t.gitpm_ok("--root", t.root / "direct", "install", repo)
    t.ok(pid in json.loads((t.root / "direct/state.json").read_text())["packages"], "direct install")


@case(fixture="catalog", spec=["package-ids/package-id-syntax", "install-uninstall/refused-input-creates-nothing"])
def id_component_of_81_refused_on_cli(t: T):
    """1. An 81-character component given on the command line creates nothing."""
    for argv in (["install-catalog", f"dan/{N81}"], ["install-catalog", f"{N81}/p"],
                 ["uninstall", f"dan/{N81}"], ["update", f"dan/{N81}"], ["enable", f"{N81}/p"]):
        cli_refused_creates_nothing(t, argv)
    cli_refused_creates_nothing(t, ["init", f"dan/{N81}", "--into", t.root / "new" / "x"])
    cli_refused_creates_nothing(t, ["init", f"{N81}/p", "--into", t.root / "new" / "y"])
    t.ok(not (t.root / "new").exists(), "init created no parent directory")


@case(fixture="catalog", spec=["package-ids/package-id-syntax", "install-uninstall/refused-input-creates-nothing"])
def id_component_of_81_refused_in_manifest(t: T):
    """1. A direct install of a manifest with an 81-character name: invalid_id, nothing installed."""
    repo, sha = plain_repo(t, "long81", f"dan/{N81}")
    t.gitpm_err("install", repo, "--ref", sha, kind="invalid_id")
    nothing_installed(t)


# ---------------------------------------------------------------- 2. character set

BAD_IDS = [
    ("scoped", "dan/p"), ("bare_alias", "p"),
    ("uppercase", "B19DABD5-0619-5414-B4D6-56C38472E88D"),
    ("missing_hyphens", "b19dabd506195414b4d656c38472e88d"),
    ("bad_hex", "b19dabd5-0619-5414-b4d6-56c38472e88g"),
    ("path", "../b19dabd5-0619-5414-b4d6-56c38472e88d"),
    ("unicode", "дан/p"), ("empty", ""),
]



def bad_id_case(name: str, plugin_id: str):
    def fn(t: T):
        if plugin_id.startswith("-"):
            # A positional starting with '-' is parsed as an option by the CLI.
            cli_refused_creates_nothing(t, ["install-catalog", plugin_id], kind=None,
                                        contains=f"unknown option: {plugin_id}")
        else:
            cli_refused_creates_nothing(t, ["install-catalog", plugin_id])
            cli_refused_creates_nothing(t, ["forget", plugin_id])
        repo, sha = plain_repo(t, f"bad-{name}", plugin_id)
        t.gitpm_err("install", repo, "--ref", sha, kind="invalid_id")
        nothing_installed(t)
    fn.__name__ = f"invalid_id_{name}"
    fn.__doc__ = f"2. ID {plugin_id!r}: refused (invalid_id) on the CLI and in a manifest; nothing installed."
    fn.__module__ = __name__
    case(fixture="catalog")(fn)


for _name, _id in BAD_IDS:
    bad_id_case(_name, _id)


# ---------------------------------------------------------------- 3. letter case

CANONICAL = "b19dabd5-0619-5414-b4d6-56c38472e88d"


def case_setup(t: T):
    t.new_package(CANONICAL, "1.0.0")
    t.gitpm_ok("install", t.repo(CANONICAL))
    return t.snap(t.worktree(CANONICAL), t.dist(CANONICAL), git=True)


@case(spec=["package-ids/ids-are-matched-exactly"])
def uppercase_uuid_is_refused_without_touching_installation(t: T):
    before = case_setup(t)
    state = t.state()
    t.gitpm_err("uninstall", CANONICAL.upper(), kind="invalid_id")
    repo, sha = plain_repo(t, "uppercase", CANONICAL.upper())
    t.gitpm_err("install", repo, "--ref", sha, kind="invalid_id")
    t.same(before, "canonical UUID installation")
    t.eq(t.state(), state, "state")


@case(spec=["package-ids/case-colliding-ids-are-refused"])
def uppercase_uuid_after_forget_is_refused(t: T):
    case_setup(t)
    t.gitpm_ok("forget", CANONICAL)
    before = t.snap(t.worktree(CANONICAL), t.dist(CANONICAL), git=True)
    repo, sha = plain_repo(t, "uppercase", CANONICAL.upper())
    t.gitpm_err("install", repo, "--ref", sha, kind="invalid_id")
    t.same(before, "retained canonical UUID clone")


# ---------------------------------------------------------------- 4-7. tags

def bad_history(t: T):
    """067f48e3-a860-5bfb-a084-b8f470924b01: tag v0.0.1 on malformed plugin.json, then a valid v1.0.0."""
    repo = t.repo("067f48e3-a860-5bfb-a084-b8f470924b01")
    repo.mkdir(parents=True)
    t.git(repo, "init", "-q", "-b", "main")
    t.commit(repo, "broken manifest", {"plugin.json": "{broken\n"})
    t.tag(repo, "v0.0.1")
    sha = t.release("067f48e3-a860-5bfb-a084-b8f470924b01", "1.0.0")
    return repo, sha


@case(spec=["package-ids/broken-historical-tags-are-skipped#one-bad-old-tag", "install-uninstall/install-the-latest-release-of-a-git-source"])
def bad_historical_tag_skipped_by_install(t: T):
    """4. Stable install picks 1.0.0 and warns about v0.0.1."""
    repo, sha = bad_history(t)
    r = t.gitpm_ok("install", repo)
    t.eq(t.package("067f48e3-a860-5bfb-a084-b8f470924b01")["current"], {"version": "1.0.0", "commit": sha}, "installed")
    t.has(r.err, "Warning", "v0.0.1")


@case(spec=["package-ids/broken-historical-tags-are-skipped#one-bad-old-tag"])
def bad_historical_tag_skipped_by_catalog_update(t: T):
    """4. catalog-update registers 1.0.0 and warns about v0.0.1."""
    repo, sha = bad_history(t)
    sources(t, repo)
    r = t.gitpm_ok("catalog-update")
    t.has(r.err, "Warning", "v0.0.1")
    index = json.loads(t.catalog_index("067f48e3-a860-5bfb-a084-b8f470924b01").read_text())
    t.eq(sorted(index["versions"]), sorted(["1.0.0"]), "catalog versions")
    t.eq(json.loads(t.catalog_release("067f48e3-a860-5bfb-a084-b8f470924b01", "1.0.0").read_text())["commit"], sha, "release commit")


def mismatched(t: T):
    """49aa9768-6c67-5163-9ed3-efd973f245db: tag v0.5.0 on a manifest that says 0.4.0, then a valid 49aa9768-6c67-5163-9ed3-efd973f245db@1.0.0."""
    t.new_package("49aa9768-6c67-5163-9ed3-efd973f245db", "0.1.0")
    t.release("49aa9768-6c67-5163-9ed3-efd973f245db", "0.4.0", tag=False)
    t.tag("49aa9768-6c67-5163-9ed3-efd973f245db", "v0.5.0")
    return t.release("49aa9768-6c67-5163-9ed3-efd973f245db", "1.0.0")


@case(spec=["package-ids/release-tag-names#tag-disagrees-with-the-manifest"])
def mismatched_tag_skipped_by_install(t: T):
    """5. A tag whose manifest version differs (below the latest) is skipped with a warning."""
    sha = mismatched(t)
    r = t.gitpm_ok("install", t.repo("49aa9768-6c67-5163-9ed3-efd973f245db"))
    t.eq(t.package("49aa9768-6c67-5163-9ed3-efd973f245db")["current"], {"version": "1.0.0", "commit": sha}, "installed")
    t.has(r.err, "Warning", "v0.5.0")


@case(spec=["package-ids/release-tag-names#tag-disagrees-with-the-manifest", "package-ids/broken-historical-tags-are-skipped"])
def mismatched_tag_skipped_by_catalog_update(t: T):
    """5. catalog-update skips the mismatched tag with a warning."""
    mismatched(t)
    sources(t, t.repo("49aa9768-6c67-5163-9ed3-efd973f245db"))
    r = t.gitpm_ok("catalog-update")
    t.has(r.err, "Warning", "v0.5.0")
    t.eq(sorted(json.loads(t.catalog_index("49aa9768-6c67-5163-9ed3-efd973f245db").read_text())["versions"]), sorted(["0.1.0", "1.0.0"]), "versions")


@case(spec=["package-manifest/unknown-keys-are-preserved"])
def unknown_manifest_field_accepted(t: T):
    """5. A release whose manifest has an unknown field installs; the field is kept as is."""
    t.new_package("d1454a1f-b5a3-5e01-8030-0b762f0006ea", "0.1.0")
    t.release("d1454a1f-b5a3-5e01-8030-0b762f0006ea", "1.0.0", extra={"bogus": True})
    t.gitpm_ok("install", t.repo("d1454a1f-b5a3-5e01-8030-0b762f0006ea"), "--ref", "v1.0.0")
    t.eq(t.package("d1454a1f-b5a3-5e01-8030-0b762f0006ea")["current"]["version"], "1.0.0", "installed version")


def off_branch(t: T):
    """a7c7069a-6d6d-5bbf-8017-a747101effe5: 1.0.0 on main; 2.0.0 tagged on branch side only."""
    t.new_package("a7c7069a-6d6d-5bbf-8017-a747101effe5", "1.0.0")
    t.git("a7c7069a-6d6d-5bbf-8017-a747101effe5", "checkout", "-q", "-b", "side")
    sha = t.release("a7c7069a-6d6d-5bbf-8017-a747101effe5", "2.0.0")
    t.git("a7c7069a-6d6d-5bbf-8017-a747101effe5", "checkout", "-q", "main")
    t.ok(t.git("a7c7069a-6d6d-5bbf-8017-a747101effe5", "branch", "--contains", sha).strip() == "side", "2.0.0 is not on main")
    return sha


@case(spec=["install-uninstall/install-an-explicit-ref-or-a-branch"])
def tag_off_default_branch_install(t: T):
    """6. install --ref of a tag not on main installs that commit."""
    sha = off_branch(t)
    t.gitpm_ok("install", t.repo("a7c7069a-6d6d-5bbf-8017-a747101effe5"), "--ref", "v2.0.0")
    t.eq(t.package("a7c7069a-6d6d-5bbf-8017-a747101effe5")["current"], {"version": "2.0.0", "commit": sha}, "installed")
    t.eq(t.rev(t.worktree("a7c7069a-6d6d-5bbf-8017-a747101effe5")), sha, "clone HEAD")
    t.has((t.dist("a7c7069a-6d6d-5bbf-8017-a747101effe5") / "index.js").read_text(), "a7c7069a-6d6d-5bbf-8017-a747101effe5 2.0.0")


@case(spec=["catalog/catalog-generated-from-git-release-tags"])
def tag_off_default_branch_catalog_update(t: T):
    """6. catalog-update registers the off-branch tag as a release."""
    sha = off_branch(t)
    sources(t, t.repo("a7c7069a-6d6d-5bbf-8017-a747101effe5"))
    t.gitpm_ok("catalog-update")
    t.eq(json.loads(t.catalog_release("a7c7069a-6d6d-5bbf-8017-a747101effe5", "2.0.0").read_text())["commit"], sha, "release commit")


@case(spec=["package-ids/release-tag-names#vversion-tag-of-a-scoped-single-package-repository"])
def scoped_id_with_v_tags(t: T):
    """7. vX.Y.Z tags are repository releases for both install and catalog-update."""
    repo = t.repo("32a6de62-aaa6-5624-8652-ba1ba4210d3a")
    repo.mkdir(parents=True)
    t.git(repo, "init", "-q", "-b", "main")
    t.release("32a6de62-aaa6-5624-8652-ba1ba4210d3a", "1.0.0", tag=False)
    t.tag(repo, "v1.0.0")
    t.release("32a6de62-aaa6-5624-8652-ba1ba4210d3a", "1.1.0", tag=False)
    t.tag(repo, "v1.1.0")
    t.gitpm_ok("install", repo)
    t.eq(t.package("32a6de62-aaa6-5624-8652-ba1ba4210d3a")["current"]["version"], "1.1.0", "latest shared version")
    sources(t, repo)
    t.gitpm_ok("catalog-update")
    t.ok(t.catalog_index("32a6de62-aaa6-5624-8652-ba1ba4210d3a").exists(), "repository releases published")


# ---------------------------------------------------------------- 8. invalid manifest versions

def invalid_version_case(name: str, version: str, tag: str):
    def fn(t: T):
        t.new_package("db02eea3-892d-5d7b-bacd-355dd00acc74", "0.1.0")
        sha = t.release("db02eea3-892d-5d7b-bacd-355dd00acc74", version, tag=False)
        t.gitpm_err("install", t.repo("db02eea3-892d-5d7b-bacd-355dd00acc74"), "--ref", sha, kind="invalid_version")
        nothing_installed(t)
        t.gitpm_err("install", t.repo("db02eea3-892d-5d7b-bacd-355dd00acc74"), "--dev", kind="invalid_version")
        nothing_installed(t)
        if t.ws.exists():
            hits = [str(p) for p in t.ws.rglob("*") if p.is_file() and version.encode() in p.read_bytes()]
            t.eq(hits, [], f"files mentioning {version}")
    fn.__name__ = f"invalid_manifest_version_{name}"
    fn.__doc__ = f"8. manifest version {version!r} via --ref SHA and --dev: invalid_version, nothing installed."
    fn.__module__ = __name__
    case(spec=[tag, "install-uninstall/refused-input-creates-nothing"])(fn)


for _name, _version, _tag in [("two_parts", "1.0", "package-ids/version-format"),
                              ("dash", "-x", "package-ids/version-format"),
                              ("prerelease", "1.0.0-beta.01", "package-ids/version-format#malformed-version-is-refused"),
                              ("build", "1.0.0+build", "package-ids/version-format#malformed-version-is-refused")]:
    invalid_version_case(_name, _version, _tag)


# ---------------------------------------------------------------- 9. many versions

@case(spec=["package-ids/version-format#numeric-ordering", "catalog/bounded-overview", "catalog/local-release-history-remains-bounded-in-execution-time#two-hundred-local-releases"])
def two_hundred_versions(t: T):
    """9. 200 releases (1.0.0 … 1.199.0): numeric maximum everywhere, each command under 60 s."""
    t.new_package("ce28dfde-11fc-5f67-936e-cb8fbae9fe9d", "1.0.0")
    for i in range(1, 200):
        t.release("ce28dfde-11fc-5f67-936e-cb8fbae9fe9d", f"1.{i}.0")
    sources(t, t.repo("ce28dfde-11fc-5f67-936e-cb8fbae9fe9d"))
    timed(t, "catalog-update")
    index = json.loads(t.catalog_index("ce28dfde-11fc-5f67-936e-cb8fbae9fe9d").read_text())
    t.eq(len(index["versions"]), 200, "catalog versions")
    out = timed(t, "--catalog", t.catalog, "info").out
    t.has(out, "ce28dfde-11fc-5f67-936e-cb8fbae9fe9d — latest: 1.199.0, versions: 200")
    timed(t, "install-catalog", "ce28dfde-11fc-5f67-936e-cb8fbae9fe9d")
    t.eq(t.package("ce28dfde-11fc-5f67-936e-cb8fbae9fe9d")["current"]["version"], "1.199.0", "install-catalog latest")
    timed(t, "--root", t.root / "direct", "install", t.repo("ce28dfde-11fc-5f67-936e-cb8fbae9fe9d"))
    t.eq(json.loads((t.root / "direct/state.json").read_text())["packages"]["ce28dfde-11fc-5f67-936e-cb8fbae9fe9d"]["current"]["version"],
         "1.199.0", "direct install latest")
