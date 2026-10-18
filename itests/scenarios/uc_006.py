"""UC-006: Author release commit (P0).

`gitpm commit REPO VERSION` writes the version, stages every non-ignored addition,
modification and deletion, commits and creates the scoped tag; it never builds, pushes
or touches a catalog. Duplicate and malformed versions are refused with no partial
mutation (worktree, index, refs and the Git object store are compared).
"""

import json

from gitpmtest import T, case


ID = "dan/rel"
# Refused versions: an existing tag is never moved; malformed versions are invalid_version.
TAGS = {"existing_version_refused": "authoring/existing-tags-are-never-moved",
        "existing_future_tag_refused": "authoring/existing-tags-are-never-moved"}


def author(t: T):
    """dan/rel 0.1.0 with tracked README.md and old.txt, *.log ignored, and an empty bare origin."""
    t.new_package(ID, "0.1.0", files={"README.md": "readme v1\n", "old.txt": "old\n", ".gitignore": "*.log\n"})
    repo = t.repo(ID)
    bare = t.root / "origin.git"
    t.run("git", "init", "-q", "--bare", bare, check=True)
    t.git(repo, "remote", "add", "origin", str(bare))
    return repo, bare


def no_side_effects(t: T, bare):
    t.eq(t.git(bare, "for-each-ref"), "", "nothing pushed to origin")
    t.ok(t.catalog_empty(), "no catalog written")
    t.ok(not t.ws.exists(), "no workspace touched")


@case(spec=["authoring/commit-releases-a-version"])
def release_commits_all_changes(t: T):
    """Edits, a new file, a deleted file and an ignored file: one release commit with tag dan/rel@0.1.1."""
    repo, bare = author(t)
    parent = t.rev(repo)
    t.write(repo, {"dist/index.js": "export const greeting = 'edited';\n", "README.md": "readme v2\n",
                   "new.txt": "new\n", "old.txt": None, "debug.log": "ignored\n"})
    r = t.gitpm_ok("commit", repo, "0.1.1")
    t.has(r.out, f"{ID}@0.1.1")
    head = t.rev(repo)
    t.eq(t.rev(repo, f"{ID}@0.1.1"), head, "tag points at HEAD")
    t.eq(t.git(repo, "rev-parse", "HEAD~1"), parent, "exactly one new commit")
    t.eq(t.git(repo, "log", "-1", "--format=%s"), f"Release {ID} 0.1.1", "message")
    t.eq(json.loads(t.git(repo, "show", "HEAD:plugin.json"))["version"], "0.1.1", "committed version")
    t.eq(json.loads((repo / "plugin.json").read_text())["version"], "0.1.1", "worktree version")
    changed = sorted(t.git(repo, "diff", "--name-status", "HEAD~1", "HEAD").splitlines())
    t.eq(changed, sorted(["M\tREADME.md", "M\tdist/index.js", "A\tnew.txt", "D\told.txt", "M\tplugin.json"]),
         "committed changes")
    t.eq(t.git(repo, "show", "HEAD:dist/index.js"), "export const greeting = 'edited';", "dist committed")
    t.eq(t.git(repo, "ls-files", "debug.log"), "", "ignored file not committed")
    t.ok((repo / "debug.log").is_file(), "ignored file kept on disk")
    t.eq(t.git(repo, "status", "--porcelain"), "", "clean tree")
    t.eq(t.git(repo, "tag", "--list").split(), [f"{ID}@0.1.0", f"{ID}@0.1.1"], "tags")
    no_side_effects(t, bare)


@case(spec=["authoring/commit-releases-a-version"])
def release_after_separately_committed_payload(t: T):
    """Payload edits already committed: commit still makes a version-bump commit and tag."""
    repo, bare = author(t)
    payload = t.commit(repo, "payload", {"dist/index.js": "export const greeting = 'payload';\n"})
    t.gitpm_ok("commit", repo, "0.1.2")
    t.eq(t.git(repo, "rev-parse", "HEAD~1"), payload, "release commit on top of the payload commit")
    t.eq(t.git(repo, "diff", "--name-only", "HEAD~1", "HEAD"), "plugin.json", "only the version changes")
    t.eq(t.rev(repo, f"{ID}@0.1.2"), t.rev(repo), "tag at HEAD")
    t.eq(json.loads(t.git(repo, "show", f"{ID}@0.1.2:plugin.json"))["version"], "0.1.2", "tag manifest")
    no_side_effects(t, bare)


def refused_case(name: str, version: str, kind: str, doc: str, prepare=None):
    def fn(t: T):
        repo, bare = author(t)
        if prepare:
            prepare(t, repo)
        t.write(repo, {"dist/index.js": "pending edit\n", "untracked.txt": "x\n"})
        before = t.snap(t.root, git=True)
        status = t.git(repo, "status", "--porcelain")
        t.gitpm_err("commit", repo, version, kind=kind)
        t.same(before, "author repository, refs, objects and fixture")
        t.eq(t.git(repo, "status", "--porcelain"), status, "status unchanged")
        t.eq((repo / "dist/index.js").read_text(), "pending edit\n", "pending edit kept")
        no_side_effects(t, bare)
    fn.__name__, fn.__doc__, fn.__module__ = name, doc, __name__
    case(spec=[TAGS.get(name, "authoring/commit-releases-a-version#invalid-version")])(fn)


def _future_tag(t: T, repo):
    t.tag(repo, f"{ID}@0.3.0")


refused_case("existing_version_refused", "0.1.0", "version_conflict",
             "The current version (tag exists) is refused.")
refused_case("existing_future_tag_refused", "0.3.0", "version_conflict",
             "A version whose scoped tag already exists (made by hand) is refused.", _future_tag)
for _name, _version in [("two_parts", "0.2"), ("prerelease", "1.0.0-01"), ("build_metadata", "1.0.0+build.1"),
                        ("leading_zero", "01.0.0"), ("v_prefix", "v1.0.0"), ("four_parts", "1.0.0.0"),
                        ("text", "latest")]:
    refused_case(f"malformed_version_{_name}", _version, "invalid_version",
                 f"Malformed version {_version!r} is refused.")


# Anchor: the scenario "Release a pre-release" arrives with change pre-release-versions;
# until it is archived the case points at the requirement.
@case(spec=["authoring/commit-releases-a-version"])
def prerelease_is_released_but_not_installed_by_default(t: T):
    """A pre-release version gets its ID@VERSION tag; a plain install still takes the release."""
    repo, bare = author(t)
    t.write(repo, {"dist/index.js": "export const greeting = 'beta';\n"})
    r = t.gitpm_ok("commit", repo, "0.2.0-beta.1")
    t.has(r.out, f"{ID} committed 0.2.0-beta.1 (local commit and tag {ID}@0.2.0-beta.1)")
    t.eq(t.rev(repo, f"{ID}@0.2.0-beta.1"), t.rev(repo), "tag at HEAD")
    no_side_effects(t, bare)
    t.gitpm_ok("install", repo)
    t.ok(t.package(ID)["current"]["version"] != "0.2.0-beta.1", "the pre-release is not picked")


@case(spec=["authoring/commit-releases-a-version", "install-uninstall/install-an-explicit-ref-or-a-branch"])
def released_version_is_installable(t: T):
    """The new release is what a direct install of the tag gets."""
    repo, _ = author(t)
    t.write(repo, {"dist/index.js": "export const greeting = 'v011';\n"})
    t.gitpm_ok("commit", repo, "0.1.1")
    t.gitpm_ok("install", repo)
    t.eq(t.package(ID)["current"], {"version": "0.1.1", "commit": t.rev(repo)}, "latest release installed")
    t.eq((t.dist(ID) / "index.js").read_text(), "export const greeting = 'v011';\n", "dist")
