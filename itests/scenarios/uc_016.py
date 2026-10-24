"""UC-016: Explicit update and version identity (P0).

Fixture hello_installed: dan/hello 0.1.0 installed directly from Git.
"""

from gitpmtest import Failure, T, case

ID = "dan/hello"


def blob(t: T, ref: str) -> str:
    return t.run("git", "-C", t.repo(ID), "show", f"{ref}:dist/index.js", quiet=True, check=True).out


def at(t: T, version: str, commit: str, previous: str | None):
    p = t.package(ID)
    t.eq(p["current"], {"version": version, "commit": commit}, "current")
    t.eq((p["previous"] or {}).get("version"), previous, "previous version")
    t.eq(t.rev(t.worktree(ID)), commit, "clone HEAD")
    t.eq((t.dist(ID) / "index.js").read_text(), blob(t, commit), "dist bytes")


def refused(t: T, *args, kind: str | None, contains=()):
    before = t.snap(t.ws, git=True)
    r = t.gitpm(*args)
    if r.code == 0:
        raise Failure(f"gitpm {' '.join(args)} succeeded, expected a refusal; now current = {t.package(ID)['current']}")
    if kind is not None:
        t.has(r.err, f"Error kind: {kind}")
    t.has(r.err, *([contains] if isinstance(contains, str) else contains))
    t.same(before, "workspace (active state, clone and dist)")
    return r


@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin"])
def update_by_tag(t: T):
    """update --ref dan/hello@0.1.1 installs the tagged revision."""
    commit = t.release(ID, "0.1.1")
    t.release(ID, "0.1.2")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.1.1")
    at(t, "0.1.1", commit, "0.1.0")


@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin"])
def update_by_commit(t: T):
    """update --ref SHA (full and abbreviated) installs that commit."""
    first = t.release(ID, "0.1.1")
    second = t.release(ID, "0.1.2")
    t.gitpm_ok("update", ID, "--ref", first)
    at(t, "0.1.1", first, "0.1.0")
    t.gitpm_ok("update", ID, "--ref", second[:12])
    at(t, "0.1.2", second, "0.1.1")


@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin"])
def explicit_older_revision_is_allowed(t: T):
    """README: --ref may select an older release; it becomes current, the newer one previous."""
    newer = t.release(ID, "0.1.1")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.1.1")
    older = t.rev(ID, f"{ID}@0.1.0")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.1.0")
    at(t, "0.1.0", older, "0.1.1")
    t.eq(t.package(ID)["previous"]["commit"], newer, "previous commit")


@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin#same-version-on-another-commit"])
def same_version_other_commit_is_accepted(t: T):
    """A new commit still saying 0.1.0: --ref SHA pins it (tags are not policed, change
    no-tag-immutability); --ref main follows the branch, which moves by commit."""
    sha = t.commit(ID, "code change without bump", {"dist/index.js": "export const greeting = 'changed';\n"})
    t.gitpm_ok("update", ID, "--ref", sha)
    at(t, "0.1.0", sha, "0.1.0")
    t.eq(t.package(ID)["mode"], {"pinned": sha}, "mode")
    t.gitpm_ok("update", ID, "--ref", "main")
    t.eq(t.package(ID)["mode"], {"branch": "main"}, "mode")


@case(fixture="hello_installed", spec=["errors/failing-operations-leave-the-workspace-unchanged"])
def id_change_by_ref_is_rejected(t: T):
    """A revision whose manifest has another ID is refused at manifest check (invalid_manifest)."""
    sha = t.release(ID, "0.1.1", extra={"id": "dan/renamed"}, tag=False)
    refused(t, "update", ID, "--ref", sha, kind="invalid_manifest", contains="id changed")
    at(t, "0.1.0", t.rev(ID, f"{ID}@0.1.0"), None)


@case(fixture="hello_installed", spec=["updates/stable-update-never-downgrades", "package-ids/broken-historical-tags-are-skipped#bad-newest-tag"])
def id_change_in_release_tag_is_rejected(t: T):
    """Release tag dan/hello@0.1.1 on a commit whose manifest says dan/renamed: stable update rejects."""
    t.release(ID, "0.1.1", extra={"id": "dan/renamed"})
    t.eq(t.git(ID, "tag", "--points-at", "HEAD"), f"{ID}@0.1.1", "tag on the renamed commit")
    refused(t, "update", ID, kind=None)
    at(t, "0.1.0", t.rev(ID, f"{ID}@0.1.0"), None)


@case(fixture="hello_installed", spec=["updates/explicit-ref-resolves-against-the-origin#same-version-on-another-commit"])
def moved_installed_release_tag_by_ref_is_accepted(t: T):
    """The installed release tag re-pointed upstream; update --ref TAG pins its new commit
    with a warning (same version, new commit)."""
    moved = t.commit(ID, "rewritten 0.1.0", {"dist/index.js": "export const greeting = 'rewritten';\n"})
    t.tag(ID, f"{ID}@0.1.0", force=True)
    r = t.gitpm_ok("update", ID, "--ref", f"{ID}@0.1.0")
    t.has(r.err, f"Warning: release tag {ID}@0.1.0 now points to {moved[:12]}")
    at(t, "0.1.0", moved, "0.1.0")
    t.eq(t.package(ID)["mode"], {"pinned": f"{ID}@0.1.0"}, "mode")


@case(fixture="hello_installed", spec=["updates/update-mirrors-the-origin-s-tags", "package-ids/tags-are-not-policed-integrity-via-protected-catalog#retagged-release"])
def moved_fetched_release_tag_is_taken(t: T):
    """0.1.2 fetched by an earlier update, then re-tagged upstream on another commit: taken
    where it points now (tags are not policed, change no-tag-immutability)."""
    first = t.release(ID, "0.1.1")
    original = t.release(ID, "0.1.2")
    t.gitpm_ok("update", ID, "--ref", f"{ID}@0.1.1")
    t.eq(t.git(t.clone(ID), "rev-parse", f"refs/tags/{ID}@0.1.2^{{commit}}"), original, "0.1.2 tag fetched")
    t.git(ID, "checkout", "-q", "-b", "rewrite", f"{ID}@0.1.1")
    moved = t.release(ID, "0.1.2", tag=False, files={"dist/index.js": "export const greeting = 'moved';\n"})
    t.tag(ID, f"{ID}@0.1.2", force=True)
    print(f"  0.1.2 moved upstream: {original} -> {moved}")
    t.git(ID, "checkout", "-q", "main")
    # The ref pinned 0.1.1: back to releases, or to the moved tag; both take the new commit.
    for args in [("update", ID, "--release"), ("update", ID, "--ref", f"{ID}@0.1.2")]:
        t.gitpm_ok(*args)
        at(t, "0.1.2", moved, "0.1.1")
        t.gitpm_ok("rollback", ID)
        at(t, "0.1.1", first, "0.1.2")
