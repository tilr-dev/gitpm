"""UC-025: Fork invalid destinations (P0).

dd3387ed-a5a3-5059-81ad-b1f6987a28f2 0.1.0 installed (g3_vendor_installed). `fork ID --into DIR` accepts only a new
folder; every refusal is checked for: non-zero exit, Error
kind, no destination created, and byte-identical workspace + author repositories.
"""

import os

from gitpmtest import T, case

ID = "dd3387ed-a5a3-5059-81ad-b1f6987a28f2"


def refused(t: T, args: list, kind: str, contains, created=()):
    before = t.snap(t.ws, t.root / "repos", git=True)
    t.gitpm_err("fork", *args, kind=kind, contains=contains)
    t.same(before, "workspace and author repositories")
    for path in created:
        t.ok(not os.path.lexists(path), f"{path} not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package"])
def repeated_forks_have_distinct_uuids(t: T):
    first, second = t.root / "authors/first", t.root / "authors/second"
    before = t.snap(t.ws, t.repo(ID), git=True)
    t.gitpm_ok("fork", ID, "--into", first)
    t.gitpm_ok("fork", ID, "--into", second)
    first_id, second_id = t.manifest_id(first), t.manifest_id(second)
    t.ok(len({ID, first_id, second_id}) == 3, "fresh identity for every fork")
    t.same(before, "source and installation")


@case(fixture="g3_vendor_installed", spec=["cli/option-validation"])
def namespace_flag_is_removed(t: T):
    dest = t.root / "authors/none"
    refused(t, [ID, "--ns", "author", "--into", dest], None, "unknown option: --ns", [dest])


@case(fixture="g3_vendor_installed", spec=["cli/option-validation#removed-fork-options-have-no-side-effects"])
def removed_fork_options(t: T):
    """Removed compound-operation flags fail before any files are created."""
    dest = t.root / "authors/none"
    before = t.snap(t.ws, t.root / "repos", git=True)
    for options, message in [
        (["--replace"], "unknown option: --replace"),
        (["--version", "2.0.0"], "--version is not valid for fork"),
        (["--force"], "--force is only valid with"),
    ]:
        t.gitpm_err("fork", ID, "--into", dest, *options,
                     contains=message)
        t.same(before, "workspace and author repositories")
        t.ok(not os.path.lexists(dest), f"{dest} not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-copies-an-installed-package"])
def unknown_plugin(t: T):
    dest = t.root / "authors/none"
    refused(t, ["a68376c0-a262-545a-a4f8-afe9998880a6", "--into", dest], "not_installed",
            "package a68376c0-a262-545a-a4f8-afe9998880a6 is not installed", [dest])


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def existing_destination_with_sentinel(t: T):
    dest = t.root / "authors/existing"
    t.write(dest, {"sentinel.txt": "keep me\n"})
    sentinel = t.snap(dest)
    refused(t, [ID, "--into", dest], "invalid_argument", "destination already exists; pass a new folder")
    t.same(sentinel, "destination with sentinel")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def empty_existing_destination(t: T):
    dest = t.root / "authors/empty"
    dest.mkdir(parents=True)
    refused(t, [ID, "--into", dest], "invalid_argument", "destination already exists; pass a new folder")
    t.eq(list(dest.iterdir()), [], "still empty")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def dangling_symlink_destination(t: T):
    dest = t.root / "authors/link"
    dest.parent.mkdir(parents=True)
    dest.symlink_to(t.root / "nowhere")
    refused(t, [ID, "--into", dest], "invalid_argument", "destination already exists; pass a new folder")
    t.ok(dest.is_symlink(), "destination symlink preserved")
    t.eq(os.readlink(dest), str(t.root / "nowhere"), "symlink target unchanged")
    t.ok(not (t.root / "nowhere").exists(), "symlink target not created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def source_repository_as_destination(t: T):
    """The original's source (author repository) is never a destination."""
    refused(t, [ID, "--into", t.repo(ID)], "invalid_argument",
            "is the source repository of the forked package")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#destination-inside-the-workspace"])
def installed_clone_as_destination(t: T):
    refused(t, [ID, "--into", t.clone(ID)], "invalid_argument",
            "fork destination must be outside the managed workspace")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#destination-inside-the-workspace"])
def new_path_inside_workspace(t: T):
    """Non-existing managed paths: repos/unused-0123456789abcdef, dist/author/demo, ws/6d1f6684-cf5f-573a-a60a-e28021e4fd7e."""
    for dest in [t.ws / "repos/unused-0123456789abcdef", t.ws / "dist/author/demo", t.ws / "6d1f6684-cf5f-573a-a60a-e28021e4fd7e"]:
        refused(t, [ID, "--into", dest], "invalid_argument",
                "outside the managed workspace", [dest])
    t.ok(not (t.ws / "repos/unused-0123456789abcdef").exists() and not (t.ws / "forks").exists(), "no parents created")


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination#destination-inside-the-workspace"])
def workspace_through_symlink(t: T):
    link = t.root / "ws-link"
    link.symlink_to(t.ws)
    refused(t, [ID, "--into", link / "6d1f6684-cf5f-573a-a60a-e28021e4fd7e"], "invalid_argument",
            "outside the managed workspace", [t.ws / "forks"])


@case(fixture="g3_vendor_installed", spec=["fork-replacement/fork-requires-explicit-namespace-and-destination"])
def parent_dir_component(t: T):
    dest = t.root / "authors/../escape"
    refused(t, [ID, "--into", dest], "invalid_argument", "'..'", [t.root / "escape"])
